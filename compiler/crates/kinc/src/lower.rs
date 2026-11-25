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
use std::collections::{BTreeSet, HashMap, HashSet};

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

    // Per-file const tables (0.4.0 S1): unqualified name -> folded value.
    // The checker has already validated every initializer (constant
    // expression, declared type, earlier-consts-only references), so
    // re-evaluation here cannot fail; the values are needed once for the
    // module's `consts` record and once per read site (all reads of a
    // const share the same value — for arrays that is one shared object,
    // matching ADR-0014's reference semantics).
    let const_tables: Vec<HashMap<String, Value>> = project
        .files
        .iter()
        .map(|unit| {
            let mut t: HashMap<String, Value> = HashMap::new();
            for cd in &unit.program.consts {
                match crate::consteval::eval_const_expr(&cd.init, &const_typed(&t, &unit.program.consts), Some(&cd.ty)) {
                    Ok((v, _)) => {
                        t.insert(cd.name.clone(), v);
                    }
                    Err(e) => unreachable!(
                        "lowering bug: const `{}` failed evaluation after a clean check: {:?}",
                        cd.name, e
                    ),
                }
            }
            t
        })
        .collect();

    // The module-level const record: qualified names (`math::MAX`), files
    // in load order, consts in source order — deterministic.
    let mut consts: Vec<(String, Value)> = Vec::new();
    for (i, unit) in project.files.iter().enumerate() {
        for cd in &unit.program.consts {
            let qname = qualified_name(&unit.path, &entry_dir, i == 0, &cd.name);
            let v = const_tables[i].get(&cd.name).expect("const folded above").clone();
            consts.push((qname, v));
        }
    }

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
        // Import alias -> target file's const table, for ModConst folding.
        let mut import_consts: HashMap<String, HashMap<String, Value>> = HashMap::new();
        for r in &unit.imports {
            let target = &project.files[r.file];
            let prefix = qualified_name(&target.path, &entry_dir, false, "")
                .trim_end_matches("::")
                .to_string();
            import_prefixes.insert(r.alias.clone(), prefix);
            import_consts.insert(r.alias.clone(), const_tables[r.file].clone());
        }
        let qname = qualified_name(&unit.path, &entry_dir, *file_idx == 0, &f.name);
        funcs.push(lower_func(
            &qname,
            f,
            &index,
            &import_prefixes,
            &const_tables[*file_idx],
            &import_consts,
        ));
    }
    Module { funcs, consts }
}

/// Pair each folded const value with its declared type so the evaluator
/// can fold reads of earlier consts (it only needs the types to type the
/// result, and the values to compute it).
fn const_typed(
    values: &HashMap<String, Value>,
    decls: &[ConstDecl],
) -> HashMap<String, (Value, Type)> {
    let mut out = HashMap::new();
    for (name, v) in values {
        if let Some(cd) = decls.iter().find(|c| &c.name == name) {
            out.insert(name.clone(), (v.clone(), cd.ty.clone()));
        }
    }
    out
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
    /// This file's consts by bare name (0.4.0 S1).
    consts: HashMap<String, Value>,
    /// Import alias -> target file's consts by bare name (0.4.0 S1).
    mod_consts: HashMap<String, HashMap<String, Value>>,
    /// Counter for hidden `for` slots (names impossible in source).
    hidden: u32,
}

fn lower_func(
    qname: &str,
    f: &Func,
    index: &HashMap<String, (usize, Type)>,
    import_prefixes: &HashMap<String, String>,
    consts: &HashMap<String, Value>,
    mod_consts: &HashMap<String, HashMap<String, Value>>,
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
        consts: consts.clone(),
        mod_consts: mod_consts.clone(),
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
        param_kinds: f.params.iter().map(|p| val_kind(&p.ty)).collect(),
        ret_kind: val_kind(&f.ret),
    }
}

/// Map a declared Kin type to the runtime's static value kind. The
/// JS code generator (S5) seeds its type analysis from this; the VM
/// ignores it. `Error` never survives the checker.
fn val_kind(t: &Type) -> kinrt::kir::ValKind {
    match t {
        Type::Int => kinrt::kir::ValKind::Int,
        Type::Float => kinrt::kir::ValKind::Float,
        Type::Bool => kinrt::kir::ValKind::Bool,
        Type::Str => kinrt::kir::ValKind::Str,
        Type::Void => kinrt::kir::ValKind::Unit,
        Type::Array(e) => kinrt::kir::ValKind::Array(Box::new(val_kind(e))),
        Type::Error => kinrt::kir::ValKind::Any,
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

    /// Like [`slot`] but `None` when the name is not a local/parameter.
    fn try_slot(&self, name: &str) -> Option<u16> {
        for scope in self.scopes.iter().rev() {
            if let Some(s) = scope.get(name) {
                return Some(*s);
            }
        }
        None
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
                // Locals win over consts; the checker guarantees exactly
                // one of the two resolves (a bare const read folds to a
                // constant — 0.4.0 S1).
                match self.try_slot(name) {
                    Some(slot) => {
                        self.emit(Instr::Load(slot));
                    }
                    None => {
                        let v = self
                            .consts
                            .get(name)
                            .expect("lowering bug: unresolved name (checker should have caught this)");
                        self.emit(Instr::Const(v.clone()));
                    }
                }
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
            ExprKind::ModConst { module, name } => {
                // Qualified const read (0.4.0 S1): folds to the target
                // module's constant value.
                let v = self
                    .mod_consts
                    .get(module)
                    .and_then(|t| t.get(name))
                    .cloned()
                    .unwrap_or_else(|| {
                        panic!("lowering bug: unresolved module const `{}.{}`", module, name)
                    });
                self.at(e.line);
                self.emit(Instr::Const(v));
                true
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


// ---------------------------------------------------------------------------
// S4: AppModel lowering (0.4.0) — the position-free, target-independent
// model for `app` programs (docs/DYNAMIC_WEB_ARCHITECTURE.md C.9, Part D
// S4). Runs only on programs the checker accepted, so E3040–E3061 and
// W3040–W3044 already hold; no semantic rule is re-implemented here.
// ---------------------------------------------------------------------------

/// Lower a checked `app` declaration into the [`AppModel`]
/// (`kinc::appmodel`).
///
/// Determinism is the contract: tables keep source order, handlers and
/// bindings are enumerated in one documented walk (pages then
/// components, elements in view order, per element: its binding
/// expressions, then its handlers, then nested bodies), dependency sets
/// are sorted, and re-lowering the same program yields an identical
/// model (tested).
///
/// The binding graph's dependency sets are **state field names**,
/// transitively resolved through the computed graph (E3044 guarantees
/// acyclicity). Component-scope reads (props, component state) and
/// `each` loop variables shadow the app layer exactly as the resolution
/// order does, so they are never shared-state dependencies.
pub fn app_model(a: &AppDecl) -> crate::appmodel::AppModel {
    // Names the dependency extraction classifies against.
    let state_names: Vec<String> = a.state.iter().map(|f| f.name.clone()).collect();
    let computed_names: Vec<String> = a.computed.iter().map(|c| c.name.clone()).collect();

    // Transitive state-field dependencies per computed value: direct
    // state reads plus the fixpoint through other computed values. The
    // graph is acyclic (E3044), so the iteration terminates.
    let mut computed_transitive: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut changed = true;
    while changed {
        changed = false;
        for cd in &a.computed {
            let mut deps = BTreeSet::new();
            collect_state_deps(
                &cd.expr,
                &state_names,
                &computed_names,
                &computed_transitive,
                &mut deps,
            );
            let entry = computed_transitive.entry(cd.name.clone()).or_default();
            if *entry != deps {
                *entry = deps;
                changed = true;
            }
        }
    }

    // One deterministic walk for handlers and bindings, per container.
    let mut handlers: Vec<crate::appmodel::HandlerModel> = Vec::new();
    let mut bindings: Vec<crate::appmodel::BindingSite> = Vec::new();

    let mut pages = Vec::with_capacity(a.pages.len());
    for p in &a.pages {
        let shadow: Vec<String> = Vec::new();
        let elements = lower_view(
            &p.name,
            &p.elements,
            &shadow,
            &state_names,
            &computed_names,
            &computed_transitive,
            &mut Vec::new(),
            &mut handlers,
            &mut bindings,
        );
        pages.push(crate::appmodel::AppPageModel { name: p.name.clone(), elements });
    }

    let mut components = Vec::new();
    let mut seen_components: HashSet<&str> = HashSet::new();
    for comp in &a.components {
        if !seen_components.insert(comp.name.as_str()) {
            continue; // E3065 duplicates never reach lowering (first wins)
        }
        let shadow: Vec<String> = comp
            .props
            .iter()
            .map(|p| p.name.clone())
            .chain(comp.state.iter().map(|f| f.name.clone()))
            .collect();
        let view = lower_view(
            &comp.name,
            &comp.view,
            &shadow,
            &state_names,
            &computed_names,
            &computed_transitive,
            &mut Vec::new(),
            &mut handlers,
            &mut bindings,
        );
        components.push(crate::appmodel::ComponentModel {
            name: comp.name.clone(),
            props: comp
                .props
                .iter()
                .map(|p| crate::appmodel::PropModel {
                    name: p.name.clone(),
                    ty: p.ty.clone(),
                    default: p.default.as_ref().map(lower_expr),
                })
                .collect(),
            state: comp
                .state
                .iter()
                .map(|f| crate::appmodel::AppStateField {
                    name: f.name.clone(),
                    ty: f.ty.clone(),
                    init: lower_expr(&f.init),
                })
                .collect(),
            view,
        });
    }

    let computed = a
        .computed
        .iter()
        .map(|cd| crate::appmodel::ComputedModel {
            name: cd.name.clone(),
            deps: computed_transitive.get(&cd.name).cloned().unwrap_or_default(),
            kir: format!("app:computed:{}", cd.name),
            expr: lower_expr(&cd.expr),
        })
        .collect();

    let routes = a
        .routes
        .iter()
        .map(|r| crate::appmodel::RouteModel {
            path: r.path.clone(),
            target: r.target.page.clone(),
            params: r
                .path
                .split('/')
                .filter_map(|seg| seg.strip_prefix(':').map(|name| name.to_string()))
                .collect(),
        })
        .collect();

    let stores = a
        .stores
        .iter()
        .map(|s| crate::appmodel::StoreModel {
            name: s.name.clone(),
            keys: s.keys.iter().map(|k| k.name.clone()).collect(),
        })
        .collect();

    let animation = a.animation.as_ref().map(|anim| crate::appmodel::AnimationTable {
        keyframes: anim
            .keyframes
            .iter()
            .map(|k| crate::appmodel::KeyframesModel {
                name: k.name.clone(),
                steps: k
                    .steps
                    .iter()
                    .map(|st| crate::appmodel::KeyframeStepModel {
                        at: match &st.at {
                            StepAt::From => crate::appmodel::StepAtModel::From,
                            StepAt::To => crate::appmodel::StepAtModel::To,
                            StepAt::Step(e) => crate::appmodel::StepAtModel::Step(lower_expr(e)),
                        },
                        props: st
                            .props
                            .iter()
                            .map(|p| (p.name.clone(), lower_expr(&p.value)))
                            .collect(),
                    })
                    .collect(),
                duration: k.duration.as_ref().and_then(int_literal),
                delay: k.delay.as_ref().and_then(int_literal),
                easing: k.easing.as_ref().and_then(lower_easing),
            })
            .collect(),
        transitions: anim
            .transitions
            .iter()
            .map(|t| crate::appmodel::TransitionModel {
                name: t.name.clone(),
                duration: t.duration.as_ref().and_then(int_literal),
                delay: t.delay.as_ref().and_then(int_literal),
                easing: t.easing.as_ref().and_then(lower_easing),
            })
            .collect(),
    });

    crate::appmodel::AppModel {
        name: a.name.clone(),
        navigation: a.navigation.unwrap_or(NavigationMode::Mpa),
        not_found: a.not_found.as_ref().map(|nf| nf.page.clone()),
        theme: a.theme.iter().map(|t| (t.token.clone(), t.value.clone())).collect(),
        state: a
            .state
            .iter()
            .map(|f| crate::appmodel::AppStateField {
                name: f.name.clone(),
                ty: f.ty.clone(),
                init: lower_expr(&f.init),
            })
            .collect(),
        computed,
        animation,
        transition: a.transition.as_ref().map(|t| t.name.clone()),
        routes,
        stores,
        pages,
        components,
        handlers,
        bindings,
    }
}

/// The integer literal of an animation option (E3050 guarantees int
/// literals; a negated literal keeps its sign).
fn int_literal(e: &Expr) -> Option<i64> {
    match &e.kind {
        ExprKind::IntLit(v) => Some(*v),
        ExprKind::Unary { op: UnOp::Neg, expr } => match &expr.kind {
            ExprKind::IntLit(v) => Some(-v),
            _ => None,
        },
        _ => None,
    }
}

/// The validated easing expression as its concrete shape (E3049 ran).
fn lower_easing(e: &Expr) -> Option<crate::appmodel::Easing> {
    use crate::appmodel::Easing;
    match &e.kind {
        ExprKind::Ident(word) => match word.as_str() {
            "linear" => Some(Easing::Linear),
            "ease" => Some(Easing::Ease),
            "ease_in" => Some(Easing::EaseIn),
            "ease_out" => Some(Easing::EaseOut),
            "ease_in_out" => Some(Easing::EaseInOut),
            _ => None,
        },
        ExprKind::Call { name, args } if name == "cubic" && args.len() == 4 => {
            let mut vals = [0.0; 4];
            for (i, a) in args.iter().enumerate() {
                vals[i] = match &a.kind {
                    ExprKind::IntLit(v) => *v as f64,
                    ExprKind::FloatLit(v) => *v,
                    ExprKind::Unary { op: UnOp::Neg, expr } => match &expr.kind {
                        ExprKind::IntLit(v) => -(*v as f64),
                        ExprKind::FloatLit(v) => -*v,
                        _ => return None,
                    },
                    _ => return None,
                };
            }
            Some(Easing::Cubic(vals))
        }
        _ => None,
    }
}

/// AST expression → position-free [`crate::appmodel::AppExpr`] (columns
/// dropped, the source line kept for C.7 error mapping).
fn lower_expr(e: &Expr) -> crate::appmodel::AppExpr {
    use crate::appmodel::{AppExpr, AppExprKind};
    let line = e.line;
    let kind = match &e.kind {
        ExprKind::IntLit(v) => AppExprKind::Int(*v),
        ExprKind::FloatLit(v) => AppExprKind::Float(*v),
        ExprKind::BoolLit(v) => AppExprKind::Bool(*v),
        ExprKind::StrLit(v) => AppExprKind::Str(v.clone()),
        ExprKind::Ident(name) => AppExprKind::Local(name.clone()),
        ExprKind::ArrayLit(items) => AppExprKind::Array(items.iter().map(lower_expr).collect()),
        ExprKind::Index { base, index } => {
            AppExprKind::Index(Box::new(lower_expr(base)), Box::new(lower_expr(index)))
        }
        ExprKind::Cast { expr, ty } => {
            AppExprKind::Cast { expr: Box::new(lower_expr(expr)), ty: ty.clone() }
        }
        ExprKind::Unary { op, expr } => match op {
            UnOp::Neg => AppExprKind::Neg(Box::new(lower_expr(expr))),
            UnOp::Not => AppExprKind::Not(Box::new(lower_expr(expr))),
        },
        ExprKind::Binary { op, lhs, rhs } => {
            AppExprKind::Binary(*op, Box::new(lower_expr(lhs)), Box::new(lower_expr(rhs)))
        }
        ExprKind::Call { name, args } => AppExprKind::Call {
            name: name.clone(),
            args: args.iter().map(lower_expr).collect(),
        },
        ExprKind::ModCall { module, name, args } => AppExprKind::ModCall {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(lower_expr).collect(),
        },
        ExprKind::ModConst { module, name } => {
            AppExprKind::ModConst { module: module.clone(), name: name.clone() }
        }
    };
    AppExpr { kind, line }
}

/// AST statements → position-free statements.
fn lower_stmts(b: &Block) -> Vec<crate::appmodel::AppStmt> {
    b.stmts.iter().map(lower_stmt).collect()
}

/// One statement → its position-free form.
fn lower_stmt(s: &Stmt) -> crate::appmodel::AppStmt {
    use crate::appmodel::{AppStmt, AppStmtKind, ForIterModel};
    let line = s.line();
    let kind = match s {
        Stmt::VarDecl { mutable, name, ty, init, .. } => AppStmtKind::VarDecl {
            mutable: *mutable,
            name: name.clone(),
            ty: ty.clone(),
            init: lower_expr(init),
        },
        Stmt::Assign { name, op, value, .. } => AppStmtKind::Assign {
            name: name.clone(),
            op: *op,
            value: lower_expr(value),
        },
        Stmt::IndexAssign { base, index, value, .. } => AppStmtKind::IndexAssign {
            base: lower_expr(base),
            index: lower_expr(index),
            value: lower_expr(value),
        },
        Stmt::If { cond, then_branch, else_branch, .. } => AppStmtKind::If {
            cond: lower_expr(cond),
            then_branch: lower_stmts(then_branch),
            else_branch: else_branch.as_ref().map(|st| lower_stmt_boxed(st)),
        },
        Stmt::While { cond, body, .. } => {
            AppStmtKind::While { cond: lower_expr(cond), body: lower_stmts(body) }
        }
        Stmt::For { var, iter, body, .. } => AppStmtKind::For {
            var: var.clone(),
            iter: match iter {
                ForIter::Range { start, end, inclusive } => ForIterModel::Range {
                    start: lower_expr(start),
                    end: lower_expr(end),
                    inclusive: *inclusive,
                },
                ForIter::Expr(e) => ForIterModel::Expr(lower_expr(e)),
            },
            body: lower_stmts(body),
        },
        Stmt::Return { value, .. } => AppStmtKind::Return { value: value.as_ref().map(lower_expr) },
        Stmt::Break { .. } => AppStmtKind::Break,
        Stmt::Continue { .. } => AppStmtKind::Continue,
        Stmt::Expr { expr, .. } => AppStmtKind::Expr(lower_expr(expr)),
        Stmt::Block { block, .. } => AppStmtKind::Block(lower_stmts(block)),
    };
    AppStmt { kind, line }
}

/// One nested `else if` statement (the boxed branch of an `If`).
fn lower_stmt_boxed(s: &Stmt) -> Box<crate::appmodel::AppStmt> {
    Box::new(lower_stmt(s))
}

/// The state fields one expression reads, given the shadow set (each
/// variables, component props/state — names that resolve to something
/// other than shared state) and the computed graph's transitive sets.
/// Computed references contribute their transitive state dependencies;
/// consts, functions and builtins contribute nothing.
fn collect_state_deps(
    e: &Expr,
    state_names: &[String],
    computed_names: &[String],
    computed_transitive: &HashMap<String, BTreeSet<String>>,
    out: &mut BTreeSet<String>,
) {
    match &e.kind {
        ExprKind::Ident(name) => {
            if computed_names.iter().any(|n| n == name) {
                if let Some(deps) = computed_transitive.get(name) {
                    out.extend(deps.iter().cloned());
                }
            } else if state_names.iter().any(|n| n == name) {
                out.insert(name.clone());
            }
        }
        ExprKind::ArrayLit(items) => {
            for it in items {
                collect_state_deps(it, state_names, computed_names, computed_transitive, out);
            }
        }
        ExprKind::Index { base, index } => {
            collect_state_deps(base, state_names, computed_names, computed_transitive, out);
            collect_state_deps(index, state_names, computed_names, computed_transitive, out);
        }
        ExprKind::Cast { expr, .. } | ExprKind::Unary { expr, .. } => {
            collect_state_deps(expr, state_names, computed_names, computed_transitive, out);
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_state_deps(lhs, state_names, computed_names, computed_transitive, out);
            collect_state_deps(rhs, state_names, computed_names, computed_transitive, out);
        }
        ExprKind::Call { args, .. } | ExprKind::ModCall { args, .. } => {
            for a in args {
                collect_state_deps(a, state_names, computed_names, computed_transitive, out);
            }
        }
        _ => {}
    }
}

/// The deterministic view walk: converts the element tree, records
/// every dynamic binding site (owner + element path + role + static
/// dependency set) and every handler (owner + element path + event +
/// KIR name + body) in the shared, ordered sinks. `path` is the index
/// chain from the container root; `shadow` holds names that resolve to
/// component-scope or loop-local bindings inside this container.
#[allow(clippy::too_many_arguments)]
fn lower_view(
    owner: &str,
    elements: &[ViewElement],
    shadow: &[String],
    state_names: &[String],
    computed_names: &[String],
    computed_transitive: &HashMap<String, BTreeSet<String>>,
    path: &mut Vec<u32>,
    handlers: &mut Vec<crate::appmodel::HandlerModel>,
    bindings: &mut Vec<crate::appmodel::BindingSite>,
) -> Vec<crate::appmodel::AppElement> {
    use crate::appmodel::{AppElement, BindingSite, FieldAttrModel};
    use ast_to_model::anim_attrs;

    // Local helpers binding the shared context.
    fn record_binding(
        owner: &str,
        path: &[u32],
        role: &str,
        e: &Expr,
        shadow: &[String],
        state_names: &[String],
        computed_names: &[String],
        computed_transitive: &HashMap<String, BTreeSet<String>>,
        bindings: &mut Vec<BindingSite>,
    ) {
        if is_literal(e) {
            return; // static content is inlined into templates (C.6)
        }
        let mut deps = BTreeSet::new();
        if !shadow.iter().any(|s| expr_mentions(e, s)) {
            collect_state_deps(e, state_names, computed_names, computed_transitive, &mut deps);
        }
        bindings.push(BindingSite {
            owner: owner.to_string(),
            path: path.to_vec(),
            role: role.to_string(),
            deps,
        });
    }

    elements
        .iter()
        .enumerate()
        .map(|(idx, e)| {
            path.push(idx as u32);
            let element = match &e.kind {
                ViewElementKind::Title { value, anim } => {
                    record_binding(owner, path, "title", value, shadow, state_names, computed_names, computed_transitive, bindings);
                    AppElement::Title { value: lower_expr(value), anim: anim_attrs(anim) }
                }
                ViewElementKind::Heading { value, anim } => {
                    record_binding(owner, path, "heading", value, shadow, state_names, computed_names, computed_transitive, bindings);
                    AppElement::Heading { value: lower_expr(value), anim: anim_attrs(anim) }
                }
                ViewElementKind::Text { value, anim } => {
                    record_binding(owner, path, "text", value, shadow, state_names, computed_names, computed_transitive, bindings);
                    AppElement::Text { value: lower_expr(value), anim: anim_attrs(anim) }
                }
                ViewElementKind::Button { label, open, anim, handlers: hs } => {
                    record_binding(owner, path, "button label", label, shadow, state_names, computed_names, computed_transitive, bindings);
                    let hidx = lower_handlers(owner, path, hs, handlers);
                    AppElement::Button {
                        label: lower_expr(label),
                        open: open.as_ref().map(|t| t.page.clone()),
                        anim: anim_attrs(anim),
                        handlers: hidx,
                    }
                }
                ViewElementKind::Link { label, open, anim, handlers: hs } => {
                    record_binding(owner, path, "link label", label, shadow, state_names, computed_names, computed_transitive, bindings);
                    let hidx = lower_handlers(owner, path, hs, handlers);
                    AppElement::Link {
                        label: lower_expr(label),
                        open: open.as_ref().map(|t| t.page.clone()),
                        anim: anim_attrs(anim),
                        handlers: hidx,
                    }
                }
                ViewElementKind::Image { src, alt, anim, handlers: hs } => {
                    record_binding(owner, path, "image src", src, shadow, state_names, computed_names, computed_transitive, bindings);
                    record_binding(owner, path, "image alt", alt, shadow, state_names, computed_names, computed_transitive, bindings);
                    let hidx = lower_handlers(owner, path, hs, handlers);
                    AppElement::Image {
                        src: lower_expr(src),
                        alt: lower_expr(alt),
                        anim: anim_attrs(anim),
                        handlers: hidx,
                    }
                }
                ViewElementKind::Divider { anim } => AppElement::Divider { anim: anim_attrs(anim) },
                ViewElementKind::Input { bind, anim, handlers: hs } => {
                    // A two-way bind renders the field's value: a
                    // dependency on itself.
                    let mut deps = BTreeSet::new();
                    deps.insert(bind.page.clone());
                    bindings.push(BindingSite {
                        owner: owner.to_string(),
                        path: path.to_vec(),
                        role: "input bind".to_string(),
                        deps,
                    });
                    let hidx = lower_handlers(owner, path, hs, handlers);
                    AppElement::Input {
                        bind: bind.page.clone(),
                        anim: anim_attrs(anim),
                        handlers: hidx,
                    }
                }
                ViewElementKind::When { cond, then_branch, else_branch } => {
                    record_binding(owner, path, "when cond", cond, shadow, state_names, computed_names, computed_transitive, bindings);
                    let then_view = lower_view(owner, then_branch, shadow, state_names, computed_names, computed_transitive, path, handlers, bindings);
                    let otherwise = else_branch
                        .as_ref()
                        .map(|els| lower_view(owner, els, shadow, state_names, computed_names, computed_transitive, path, handlers, bindings));
                    AppElement::When {
                        cond: lower_expr(cond),
                        then_branch: then_view,
                        otherwise,
                    }
                }
                ViewElementKind::Each { iter, var, key, body } => {
                    record_binding(owner, path, "each iter", iter, shadow, state_names, computed_names, computed_transitive, bindings);
                    if let Some(k) = key {
                        record_binding(owner, path, "each key", k, shadow, state_names, computed_names, computed_transitive, bindings);
                    }
                    // The loop variable shadows the app layer inside the body.
                    let mut body_shadow = shadow.to_vec();
                    body_shadow.push(var.clone());
                    let body_view = lower_view(owner, body, &body_shadow, state_names, computed_names, computed_transitive, path, handlers, bindings);
                    AppElement::Each {
                        iter: lower_expr(iter),
                        var: var.clone(),
                        key: key.as_ref().map(lower_expr),
                        body: body_view,
                    }
                }
                ViewElementKind::Component { module, name, args, anim, handlers: hs } => {
                    for arg in args {
                        record_binding(owner, path, &format!("prop {}", arg.name), &arg.value, shadow, state_names, computed_names, computed_transitive, bindings);
                    }
                    let hidx = lower_handlers(owner, path, hs, handlers);
                    AppElement::Component {
                        module: module.clone(),
                        name: name.clone(),
                        args: args.iter().map(|a| (a.name.clone(), lower_expr(&a.value))).collect(),
                        anim: anim_attrs(anim),
                        handlers: hidx,
                    }
                }
                ViewElementKind::Form { legend, body, handlers: hs } => {
                    record_binding(owner, path, "form legend", legend, shadow, state_names, computed_names, computed_transitive, bindings);
                    let hidx = lower_handlers(owner, path, hs, handlers);
                    let body_view = lower_view(owner, body, shadow, state_names, computed_names, computed_transitive, path, handlers, bindings);
                    AppElement::Form {
                        legend: lower_expr(legend),
                        body: body_view,
                        handlers: hidx,
                    }
                }
                ViewElementKind::Field { bind, kind, attrs, anim, handlers: hs, .. } => {
                    let mut deps = BTreeSet::new();
                    deps.insert(bind.page.clone());
                    bindings.push(BindingSite {
                        owner: owner.to_string(),
                        path: path.to_vec(),
                        role: "field bind".to_string(),
                        deps,
                    });
                    let attr_role = |w: &str| format!("field {}", w);
                    for attr in attrs {
                        match attr {
                            FieldAttr::Label(v) => record_binding(owner, path, &attr_role("label"), v, shadow, state_names, computed_names, computed_transitive, bindings),
                            FieldAttr::Placeholder(v) => record_binding(owner, path, &attr_role("placeholder"), v, shadow, state_names, computed_names, computed_transitive, bindings),
                            FieldAttr::Pattern(v) => record_binding(owner, path, &attr_role("pattern"), v, shadow, state_names, computed_names, computed_transitive, bindings),
                            FieldAttr::MinLength(v) => record_binding(owner, path, &attr_role("min_length"), v, shadow, state_names, computed_names, computed_transitive, bindings),
                            FieldAttr::MaxLength(v) => record_binding(owner, path, &attr_role("max_length"), v, shadow, state_names, computed_names, computed_transitive, bindings),
                            FieldAttr::MinValue(v) => record_binding(owner, path, &attr_role("min_value"), v, shadow, state_names, computed_names, computed_transitive, bindings),
                            FieldAttr::MaxValue(v) => record_binding(owner, path, &attr_role("max_value"), v, shadow, state_names, computed_names, computed_transitive, bindings),
                            FieldAttr::Required => {}
                        }
                    }
                    let hidx = lower_handlers(owner, path, hs, handlers);
                    AppElement::Field {
                        bind: bind.page.clone(),
                        kind: *kind,
                        attrs: attrs
                            .iter()
                            .map(|a| match a {
                                FieldAttr::Label(v) => FieldAttrModel::Label(lower_expr(v)),
                                FieldAttr::Placeholder(v) => FieldAttrModel::Placeholder(lower_expr(v)),
                                FieldAttr::Required => FieldAttrModel::Required,
                                FieldAttr::MinLength(v) => FieldAttrModel::MinLength(lower_expr(v)),
                                FieldAttr::MaxLength(v) => FieldAttrModel::MaxLength(lower_expr(v)),
                                FieldAttr::MinValue(v) => FieldAttrModel::MinValue(lower_expr(v)),
                                FieldAttr::MaxValue(v) => FieldAttrModel::MaxValue(lower_expr(v)),
                                FieldAttr::Pattern(v) => FieldAttrModel::Pattern(lower_expr(v)),
                            })
                            .collect(),
                        anim: anim_attrs(anim),
                        handlers: hidx,
                    }
                }
            };
            path.pop();
            element
        })
        .collect()
}

/// Lower one element's handlers into the shared sink; returns their
/// indices in [`AppModel::handlers`]. KIR names are deterministic:
/// `app:<owner>:<event>:<n>` with `n` a per-(owner, event) ordinal in
/// walk order.
fn lower_handlers(
    owner: &str,
    path: &[u32],
    hs: &[OnEvent],
    sink: &mut Vec<crate::appmodel::HandlerModel>,
) -> Vec<usize> {
    // Per-(owner, event) ordinals derive from what is already in the
    // sink (the walk is sequential, so counting there is the walk
    // order).
    let mut indices = Vec::with_capacity(hs.len());
    for (i, on) in hs.iter().enumerate() {
        let ordinal = sink
            .iter()
            .filter(|h| h.owner == owner && h.event == on.event.as_str())
            .count() as u32;
        sink.push(crate::appmodel::HandlerModel {
            kir: format!("app:{}:{}:{}", owner, on.event.as_str(), ordinal),
            owner: owner.to_string(),
            path: path.to_vec(),
            event: on.event.as_str().to_string(),
            key_filter: on.key_filter.clone(),
            index: i as u32,
            body: lower_stmts(&on.body),
        });
        indices.push(sink.len() - 1);
    }
    indices
}

/// Whether an expression is a plain literal (static content is inlined
/// into templates — C.6); everything else is a dynamic binding site.
fn is_literal(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_) | ExprKind::StrLit(_)
    )
}

/// Whether an expression mentions the given identifier anywhere (the
/// shadow test for the dependency walk).
fn expr_mentions(e: &Expr, name: &str) -> bool {
    match &e.kind {
        ExprKind::Ident(n) => n == name,
        ExprKind::ArrayLit(items) => items.iter().any(|i| expr_mentions(i, name)),
        ExprKind::Index { base, index } => expr_mentions(base, name) || expr_mentions(index, name),
        ExprKind::Cast { expr, .. } | ExprKind::Unary { expr, .. } => expr_mentions(expr, name),
        ExprKind::Binary { lhs, rhs, .. } => expr_mentions(lhs, name) || expr_mentions(rhs, name),
        ExprKind::Call { args, .. } | ExprKind::ModCall { args, .. } => {
            args.iter().any(|a| expr_mentions(a, name))
        }
        _ => false,
    }
}

/// AST→model conversion helpers that carry no walk state.
mod ast_to_model {
    /// Element animation attributes, position free.
    pub fn anim_attrs(a: &crate::ast::ElementAnim) -> crate::appmodel::AnimAttrs {
        crate::appmodel::AnimAttrs {
            enter: a.enter.as_ref().map(|r| r.name.clone()),
            exit: a.exit.as_ref().map(|r| r.name.clone()),
            hover: a.hover.as_ref().map(|r| r.name.clone()),
            press: a.press.as_ref().map(|r| r.name.clone()),
            scroll: a.scroll.as_ref().map(|r| r.name.clone()),
        }
    }
}
