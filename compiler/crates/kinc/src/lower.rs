//! Lowering: type-checked AST to KIR.
//!
//! Runs only on programs the checker accepted, so it can assume:
//! variables are declared before use, calls resolve, `break`/`continue`
//! appear inside loops, and stack effects are balanced.
//!
//! 0.2.0 notes:
//! - Local slots are reused after a scope exits (free-list allocator,
//!   P-003 in the registry).
//! - `for` loops are lowered directly: the counter and bounds live in
//!   hidden slots (names containing `$`, impossible in user code),
//!   `continue` jumps to the increment, and the loop variable is
//!   refreshed from the counter at the top of every iteration.
//! - Whole projects lower into one KIR module; imported functions keep
//!   qualified names (`lib/math::add`) so identical names in different
//!   files cannot collide.

use crate::ast::*;
use crate::project::{dirname, qualified_name, Project};
use crate::site::{Element, PageModel, SiteModel};
use kinrt::kir::{CastTarget, NativeId};
use kinrt::{FuncDef, Instr, Module, Value};
use std::collections::HashMap;

/// Lower a checked single-file program to a KIR module.
pub fn lower(p: &Program) -> Module {
    let project = Project {
        files: vec![crate::project::FileUnit {
            path: "<main>".to_string(),
            program: p.clone(),
            imports: Vec::new(),
        }],
        sources: std::collections::HashMap::new(),
    };
    lower_project(&project)
}

/// Lower a checked `website` declaration into a site model. Title
/// elements become the page title; a second `title` element on the same
/// page (checker warning W3036) is demoted to a section heading.
pub fn site_model(w: &WebsiteDecl) -> SiteModel {
    let mut pages = Vec::with_capacity(w.pages.len());
    for p in &w.pages {
        let mut title = String::new();
        let mut elements = Vec::new();
        for e in &p.elements {
            match &e.kind {
                ElementKind::Title(v) => {
                    if title.is_empty() {
                        title = v.clone();
                    } else {
                        elements.push(Element::Heading(v.clone()));
                    }
                }
                ElementKind::Heading(v) => elements.push(Element::Heading(v.clone())),
                ElementKind::Text(v) => elements.push(Element::Text(v.clone())),
                ElementKind::Button { label, target, .. } => {
                    elements.push(Element::Button { label: label.clone(), target: target.clone() })
                }
                ElementKind::Link { label, target, .. } => {
                    elements.push(Element::Link { label: label.clone(), target: target.clone() })
                }
                ElementKind::Image { src, alt, .. } => {
                    elements.push(Element::Image { src: src.clone(), alt: alt.clone() })
                }
                ElementKind::Divider => elements.push(Element::Divider),
            }
        }
        pages.push(PageModel { name: p.name.clone(), title, elements });
    }
    SiteModel { name: w.name.clone(), pages }
}

/// Lower a checked project (entry first, then every imported file) into
/// one linked KIR module.
pub fn lower_project(project: &Project) -> Module {
    let entry_dir = dirname(&project.files[0].path).to_string();

    // Global function index: qualified name -> (module function index,
    // return type). The entry's `main` keeps the plain name the VM looks
    // for; everything else uses qualified names.
    let mut index: HashMap<String, (usize, Type)> = HashMap::new();
    let mut order: Vec<(usize, &Func)> = Vec::new(); // (file index, func)
    for (i, unit) in project.files.iter().enumerate() {
        for f in &unit.program.funcs {
            let qname = qualified_name(&unit.path, &entry_dir, i == 0, &f.name);
            let key = if i == 0 && f.name == "main" { "main".to_string() } else { qname };
            if index.contains_key(&key) {
                // The checker rejects duplicates within a file and
                // qualified names are unique across files. Unreachable.
                continue;
            }
            index.insert(key, (order.len(), f.ret.clone()));
            order.push((i, f));
        }
    }

    let mut funcs = Vec::new();
    for (file_idx, f) in &order {
        let unit = &project.files[*file_idx];
        // Import alias -> qualified-name prefix, for ModCall resolution.
        let mut import_prefixes: HashMap<String, String> = HashMap::new();
        for r in &unit.imports {
            let target = &project.files[r.file];
            let prefix = qualified_name(&target.path, &entry_dir, false, "")
                .trim_end_matches("::")
                .to_string();
            import_prefixes.insert(r.alias.clone(), prefix);
        }
        let qname = qualified_name(&unit.path, &entry_dir, *file_idx == 0, &f.name);
        funcs.push(lower_func(&qname, f, &index, &import_prefixes));
    }
    Module { funcs }
}

struct LoopCtx {
    /// Indices of `Jump` instructions emitted for `break`; patched when
    /// the loop ends.
    breaks: Vec<usize>,
    /// Indices of `Jump` instructions emitted for `continue`; patched
    /// when the loop ends (to the condition of a `while`, or to the
    /// increment of a `for`).
    continues: Vec<usize>,
    /// Where `continue` jumps in a `while` (for `for` loops this stays
    /// unused; their continues are patched to the increment).
    continue_target: usize,
}

struct FnLower {
    code: Vec<Instr>,
    lines: Vec<u32>,
    scopes: Vec<HashMap<String, u16>>,
    /// Slots allocated in each open scope (for reuse after scope exit).
    scope_slots: Vec<Vec<u16>>,
    /// Free slots available for reuse (P-003).
    free: Vec<u16>,
    /// High-water mark of slot usage == function's `nlocals`.
    max_slots: u16,
    loops: Vec<LoopCtx>,
    cur_line: u32,
    index: HashMap<String, (usize, Type)>,
    /// Import alias -> qualified-name prefix (current file).
    import_prefixes: HashMap<String, String>,
    /// Counter for hidden `for` slots (names impossible in source).
    hidden: u32,
}

fn lower_func(
    qname: &str,
    f: &Func,
    index: &HashMap<String, (usize, Type)>,
    import_prefixes: &HashMap<String, String>,
) -> FuncDef {
    let mut fl = FnLower {
        code: Vec::new(),
        lines: Vec::new(),
        scopes: Vec::new(),
        scope_slots: Vec::new(),
        free: Vec::new(),
        max_slots: f.params.len() as u16,
        loops: Vec::new(),
        cur_line: f.line,
        index: index.clone(),
        import_prefixes: import_prefixes.clone(),
        hidden: 0,
    };
    let mut scope = HashMap::new();
    let mut scope_slots = Vec::new();
    for (i, prm) in f.params.iter().enumerate() {
        scope.insert(prm.name.clone(), i as u16);
        scope_slots.push(i as u16);
    }
    fl.scopes.push(scope);
    fl.scope_slots.push(scope_slots);

    fl.lower_block(&f.body);

    // Safety net: every function ends with an explicit return of `Unit`.
    // For accepted programs this is unreachable (the checker requires a
    // trailing return in non-void functions, and void functions fall
    // through to it naturally).
    fl.cur_line = f.line;
    fl.emit(Instr::Const(Value::Unit));
    fl.emit(Instr::Return);

    FuncDef {
        name: qname.to_string(),
        nparams: f.params.len() as u16,
        nlocals: fl.max_slots,
        code: fl.code,
        lines: fl.lines,
    }
}

impl FnLower {
    fn emit(&mut self, i: Instr) {
        self.code.push(i);
        self.lines.push(self.cur_line);
    }

    fn at(&mut self, line: u32) {
        self.cur_line = line;
    }

    fn here(&self) -> usize {
        self.code.len()
    }

    /// Point the jump instruction at `idx` at the current position.
    fn set_target(&mut self, idx: usize) {
        let t = self.code.len() as u32;
        match &mut self.code[idx] {
            Instr::Jump(target) | Instr::JumpIfFalse(target) => *target = t,
            _ => unreachable!("set_target called on a non-jump instruction"),
        }
    }

    fn slot(&self, name: &str) -> u16 {
        for scope in self.scopes.iter().rev() {
            if let Some(s) = scope.get(name) {
                return *s;
            }
        }
        panic!("lowering bug: unresolved variable `{}` (checker should have caught this)", name)
    }

    /// Allocate a slot, reusing a freed one when possible (P-003).
    fn alloc(&mut self, name: &str) -> u16 {
        let slot = match self.free.pop() {
            Some(s) => s,
            None => {
                let s = self.max_slots;
                self.max_slots += 1;
                s
            }
        };
        self.scopes.last_mut().unwrap().insert(name.to_string(), slot);
        self.scope_slots.last_mut().unwrap().push(slot);
        slot
    }

    /// Allocate a slot for a compiler-generated name that cannot collide
    /// with user identifiers (`$` is not a valid Kin identifier character).
    fn alloc_hidden(&mut self, tag: &str) -> u16 {
        self.hidden += 1;
        let name = format!("${}${}", tag, self.hidden);
        self.alloc(&name)
    }

    fn lower_block(&mut self, b: &Block) {
        self.scopes.push(HashMap::new());
        self.scope_slots.push(Vec::new());
        for stmt in &b.stmts {
            self.lower_stmt(stmt);
        }
        self.scope_exit();
    }

    /// End the innermost scope and release its slots for reuse.
    fn scope_exit(&mut self) {
        let slots = self.scope_slots.pop().expect("scope open");
        self.free.extend(slots);
        self.scopes.pop();
    }

    fn lower_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::VarDecl { name, init, line, .. } => {
                let leaves = self.lower_expr(init);
                debug_assert!(leaves, "void initializers are rejected by the checker");
                self.at(*line);
                let slot = self.alloc(name);
                self.emit(Instr::Store(slot));
            }
            Stmt::Assign { name, op, value, line, .. } => {
                let slot = self.slot(name);
                match op {
                    AssignOp::Set => {
                        self.lower_expr(value);
                        self.at(*line);
                        self.emit(Instr::Store(slot));
                    }
                    AssignOp::Add | AssignOp::Sub => {
                        self.emit(Instr::Load(slot));
                        self.lower_expr(value);
                        self.at(*line);
                        self.emit(match op {
                            AssignOp::Add => Instr::Add,
                            _ => Instr::Sub,
                        });
                        self.emit(Instr::Store(slot));
                    }
                }
            }
            Stmt::IndexAssign { base, index, value, line, .. } => {
                self.lower_expr(base);
                self.lower_expr(index);
                self.lower_expr(value);
                self.at(*line);
                self.emit(Instr::ArrSet);
            }
            Stmt::If { cond, then_branch, else_branch, line, .. } => {
                self.lower_expr(cond);
                let jf = {
                    self.at(*line);
                    self.emit(Instr::JumpIfFalse(0));
                    self.code.len() - 1
                };
                self.lower_block(then_branch);
                if let Some(e) = else_branch {
                    let jend = {
                        self.at(*line);
                        self.emit(Instr::Jump(0));
                        self.code.len() - 1
                    };
                    self.set_target(jf);
                    self.lower_stmt(e);
                    self.set_target(jend);
                } else {
                    self.set_target(jf);
                }
            }
            Stmt::While { cond, body, line, .. } => {
                let start = self.here();
                self.lower_expr(cond);
                let jf = {
                    self.at(*line);
                    self.emit(Instr::JumpIfFalse(0));
                    self.code.len() - 1
                };
                self.loops.push(LoopCtx {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    continue_target: start,
                });
                self.lower_block(body);
                let lp = self.loops.pop().expect("loop context pushed above");
                self.at(*line);
                self.emit(Instr::Jump(start as u32));
                self.set_target(jf);
                let after = self.code.len() as u32;
                self.patch_loop_end(lp, after, None);
            }
            Stmt::For { var, iter, body, line, .. } => {
                self.lower_for(var, iter, body, *line);
            }
            Stmt::Return { value, line, .. } => {
                match value {
                    Some(v) => {
                        self.lower_expr(v);
                    }
                    None => {
                        self.at(*line);
                        self.emit(Instr::Const(Value::Unit));
                    }
                }
                self.at(*line);
                self.emit(Instr::Return);
            }
            Stmt::Break { line, .. } => {
                self.at(*line);
                self.emit(Instr::Jump(0));
                let idx = self.code.len() - 1;
                self.loops.last_mut().expect("`break` outside a loop (checker bug)").breaks.push(idx);
            }
            Stmt::Continue { line, .. } => {
                self.at(*line);
                self.emit(Instr::Jump(0)); // patched at loop end
                let idx = self.code.len() - 1;
                self.loops
                    .last_mut()
                    .expect("`continue` outside a loop (checker bug)")
                    .continues
                    .push(idx);
            }
            Stmt::Expr { expr, .. } => {
                if self.lower_expr(expr) {
                    self.emit(Instr::Pop);
                }
            }
            Stmt::Block { block, .. } => self.lower_block(block),
        }
    }

    /// Patch the break/continue jumps of a finished loop: breaks and the
    /// condition-exit jump land on `after` (the code following the loop);
    /// continues land on `lp.continue_target` (while) or `inc` (for).
    fn patch_loop_end(&mut self, lp: LoopCtx, after: u32, inc: Option<u32>) {
        for b in &lp.breaks {
            if let Instr::Jump(t) = &mut self.code[*b] {
                *t = after;
            }
        }
        let target = inc.unwrap_or(lp.continue_target as u32);
        for c in &lp.continues {
            if let Instr::Jump(t) = &mut self.code[*c] {
                *t = target;
            }
        }
    }

    /// Lower a `for` loop. Hidden slots (`$it$N`, impossible user names)
    /// hold the counter and bounds; the loop variable is refreshed from
    /// the counter at the top of every iteration.
    fn lower_for(&mut self, var: &str, iter: &ForIter, body: &Block, line: u32) {
        // Scope for the hidden machinery + the loop variable.
        self.scopes.push(HashMap::new());
        self.scope_slots.push(Vec::new());

        match iter {
            ForIter::Range { start, end, inclusive } => {
                // { $it = start; $end = end;
                //   while $it (<|<=) $end { x = $it; body; $it += 1; } }
                let it = self.alloc_hidden("it");
                let end_slot = self.alloc_hidden("end");
                self.lower_expr(start);
                self.at(line);
                self.emit(Instr::Store(it));
                self.lower_expr(end);
                self.at(line);
                self.emit(Instr::Store(end_slot));

                let cond_at = self.here();
                self.emit(Instr::Load(it));
                self.emit(Instr::Load(end_slot));
                self.emit(if *inclusive { Instr::Le } else { Instr::Lt });
                let jf = {
                    self.at(line);
                    self.emit(Instr::JumpIfFalse(0));
                    self.code.len() - 1
                };

                // Refresh the loop variable from the counter.
                let var_slot = self.alloc(var);
                self.emit(Instr::Load(it));
                self.emit(Instr::Store(var_slot));

                self.loops.push(LoopCtx {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    continue_target: 0,
                });
                self.lower_block(body);

                // `continue` lands here: increment, then loop.
                let inc_at = self.here() as u32;
                self.emit(Instr::Load(it));
                self.emit(Instr::Const(Value::Int(1)));
                self.emit(Instr::Add);
                self.emit(Instr::Store(it));
                self.emit(Instr::Jump(cond_at as u32));

                let lp = self.loops.pop().expect("for loop context");
                self.set_target(jf);
                let after = self.code.len() as u32;
                self.patch_loop_end(lp, after, Some(inc_at));
            }
            ForIter::Expr(e) => {
                // { $arr = e; $i = 0; $len = len($arr);
                //   while $i < $len { x = $arr[$i]; body; $i += 1; } }
                let arr = self.alloc_hidden("arr");
                let it = self.alloc_hidden("i");
                let len_slot = self.alloc_hidden("len");
                self.lower_expr(e);
                self.at(line);
                self.emit(Instr::Store(arr));
                self.emit(Instr::Const(Value::Int(0)));
                self.emit(Instr::Store(it));
                self.emit(Instr::Load(arr));
                self.emit(Instr::Native(NativeId::Len));
                self.emit(Instr::Store(len_slot));

                let cond_at = self.here();
                self.emit(Instr::Load(it));
                self.emit(Instr::Load(len_slot));
                self.emit(Instr::Lt);
                let jf = {
                    self.at(line);
                    self.emit(Instr::JumpIfFalse(0));
                    self.code.len() - 1
                };

                let var_slot = self.alloc(var);
                self.emit(Instr::Load(arr));
                self.emit(Instr::Load(it));
                self.emit(Instr::ArrGet);
                self.emit(Instr::Store(var_slot));

                self.loops.push(LoopCtx {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    continue_target: 0,
                });
                self.lower_block(body);

                let inc_at = self.here() as u32;
                self.emit(Instr::Load(it));
                self.emit(Instr::Const(Value::Int(1)));
                self.emit(Instr::Add);
                self.emit(Instr::Store(it));
                self.emit(Instr::Jump(cond_at as u32));

                let lp = self.loops.pop().expect("for loop context");
                self.set_target(jf);
                let after = self.code.len() as u32;
                self.patch_loop_end(lp, after, Some(inc_at));
            }
        }

        self.scope_exit();
    }

    /// Lower an expression; returns `true` when it leaves a value on the
    /// stack (`false` for the void builtin `print`).
    fn lower_expr(&mut self, e: &Expr) -> bool {
        self.at(e.line);
        match &e.kind {
            ExprKind::IntLit(v) => {
                self.emit(Instr::Const(Value::Int(*v)));
                true
            }
            ExprKind::FloatLit(v) => {
                self.emit(Instr::Const(Value::Float(*v)));
                true
            }
            ExprKind::BoolLit(v) => {
                self.emit(Instr::Const(Value::Bool(*v)));
                true
            }
            ExprKind::StrLit(v) => {
                self.emit(Instr::Const(Value::Str(v.clone())));
                true
            }
            ExprKind::Ident(name) => {
                let slot = self.slot(name);
                self.emit(Instr::Load(slot));
                true
            }
            ExprKind::ArrayLit(items) => {
                for item in items {
                    self.lower_expr(item);
                }
                self.at(e.line);
                self.emit(Instr::ArrNew(items.len() as u16));
                true
            }
            ExprKind::Index { base, index } => {
                self.lower_expr(base);
                self.lower_expr(index);
                self.at(e.line);
                self.emit(Instr::ArrGet);
                true
            }
            ExprKind::Cast { expr, ty } => {
                self.lower_expr(expr);
                self.at(e.line);
                let target = match ty {
                    Type::Int => CastTarget::Int,
                    Type::Float => CastTarget::Float,
                    Type::Str => CastTarget::Str,
                    _ => unreachable!("the checker only accepts casts to int, float and string"),
                };
                self.emit(Instr::Cast(target));
                true
            }
            ExprKind::Unary { op, expr } => {
                self.lower_expr(expr);
                self.at(e.line);
                self.emit(match op {
                    UnOp::Not => Instr::Not,
                    UnOp::Neg => Instr::Neg,
                });
                true
            }
            ExprKind::Binary { op, lhs, rhs } => {
                // `and` / `or` short-circuit, so they are lowered with
                // jumps instead of a single arithmetic instruction.
                match op {
                    BinOp::And => {
                        self.lower_expr(lhs);
                        let jf = {
                            self.at(e.line);
                            self.emit(Instr::JumpIfFalse(0));
                            self.code.len() - 1
                        };
                        self.lower_expr(rhs);
                        let jend = {
                            self.at(e.line);
                            self.emit(Instr::Jump(0));
                            self.code.len() - 1
                        };
                        self.set_target(jf);
                        self.emit(Instr::Const(Value::Bool(false)));
                        self.set_target(jend);
                        true
                    }
                    BinOp::Or => {
                        self.lower_expr(lhs);
                        let jf = {
                            self.at(e.line);
                            self.emit(Instr::JumpIfFalse(0));
                            self.code.len() - 1
                        };
                        self.emit(Instr::Const(Value::Bool(true)));
                        let jend = {
                            self.at(e.line);
                            self.emit(Instr::Jump(0));
                            self.code.len() - 1
                        };
                        self.set_target(jf);
                        self.lower_expr(rhs);
                        self.set_target(jend);
                        true
                    }
                    _ => {
                        self.lower_expr(lhs);
                        self.lower_expr(rhs);
                        self.at(e.line);
                        self.emit(match op {
                            BinOp::Add => Instr::Add,
                            BinOp::Sub => Instr::Sub,
                            BinOp::Mul => Instr::Mul,
                            BinOp::Div => Instr::Div,
                            BinOp::Mod => Instr::Mod,
                            BinOp::Lt => Instr::Lt,
                            BinOp::Gt => Instr::Gt,
                            BinOp::Le => Instr::Le,
                            BinOp::Ge => Instr::Ge,
                            BinOp::Eq => Instr::Eq,
                            BinOp::Ne => Instr::Ne,
                            BinOp::And | BinOp::Or => unreachable!("handled above"),
                        });
                        true
                    }
                }
            }
            ExprKind::Call { name, args } => {
                if name == "print" {
                    if let Some(a) = args.first() {
                        self.lower_expr(a);
                    }
                    self.at(e.line);
                    self.emit(Instr::Print);
                    return false;
                }
                if let Some(native) = native_id(name) {
                    for a in args {
                        self.lower_expr(a);
                    }
                    self.at(e.line);
                    self.emit(Instr::Native(native));
                    return true;
                }
                let (idx, ret) = self
                    .index
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| panic!("lowering bug: unresolved function `{}`", name));
                for a in args {
                    self.lower_expr(a);
                }
                self.at(e.line);
                self.emit(Instr::Call(idx as u16));
                ret != Type::Void
            }
            ExprKind::ModCall { module, name, args } => {
                let prefix = self
                    .import_prefixes
                    .get(module)
                    .cloned()
                    .unwrap_or_else(|| panic!("lowering bug: unresolved module `{}`", module));
                let qualified = format!("{}::{}", prefix, name);
                let (idx, ret) = self
                    .index
                    .get(&qualified)
                    .cloned()
                    .unwrap_or_else(|| panic!("lowering bug: unresolved function `{}`", qualified));
                for a in args {
                    self.lower_expr(a);
                }
                self.at(e.line);
                self.emit(Instr::Call(idx as u16));
                ret != Type::Void
            }
        }
    }
}

/// Map a builtin name to its native instruction.
fn native_id(name: &str) -> Option<NativeId> {
    match name {
        "len" => Some(NativeId::Len),
        "abs" => Some(NativeId::Abs),
        "min" => Some(NativeId::Min),
        "max" => Some(NativeId::Max),
        _ => None,
    }
}
