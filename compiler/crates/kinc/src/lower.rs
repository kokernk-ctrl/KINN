//! Lowering: type-checked AST to KIR.
//!
//! Runs only on programs the checker accepted, so it can assume:
//! variables are declared before use, calls resolve, `break`/`continue`
//! appear inside loops, and stack effects are balanced.

use crate::ast::*;
use kinrt::{FuncDef, Instr, Module, Value};
use std::collections::HashMap;

/// Lower a checked program to a KIR module.
pub fn lower(p: &Program) -> Module {
    let mut index: HashMap<String, (usize, Type)> = HashMap::new();
    for (i, f) in p.funcs.iter().enumerate() {
        index.insert(f.name.clone(), (i, f.ret));
    }
    Module { funcs: p.funcs.iter().map(|f| lower_func(f, &index)).collect() }
}

struct LoopCtx {
    /// Indices of `Jump` instructions emitted for `break`; patched when
    /// the loop ends.
    breaks: Vec<usize>,
    /// Code index of the loop condition (target of `continue`).
    start: usize,
}

struct FnLower {
    code: Vec<Instr>,
    lines: Vec<u32>,
    scopes: Vec<HashMap<String, u16>>,
    nlocals: u16,
    loops: Vec<LoopCtx>,
    cur_line: u32,
    index: HashMap<String, (usize, Type)>,
}

fn lower_func(f: &Func, index: &HashMap<String, (usize, Type)>) -> FuncDef {
    let mut fl = FnLower {
        code: Vec::new(),
        lines: Vec::new(),
        scopes: Vec::new(),
        nlocals: f.params.len() as u16,
        loops: Vec::new(),
        cur_line: f.line,
        index: index.clone(),
    };
    let mut scope = HashMap::new();
    for (i, prm) in f.params.iter().enumerate() {
        scope.insert(prm.name.clone(), i as u16);
    }
    fl.scopes.push(scope);

    fl.lower_block(&f.body);

    // Safety net: every function ends with an explicit return of `Unit`.
    // For accepted programs this is unreachable (the checker requires a
    // trailing return in non-void functions, and void functions fall
    // through to it naturally).
    fl.cur_line = f.line;
    fl.emit(Instr::Const(Value::Unit));
    fl.emit(Instr::Return);

    FuncDef {
        name: f.name.clone(),
        nparams: f.params.len() as u16,
        nlocals: fl.nlocals,
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

    fn alloc(&mut self, name: &str) -> u16 {
        let slot = self.nlocals;
        self.nlocals += 1;
        self.scopes.last_mut().unwrap().insert(name.to_string(), slot);
        slot
    }

    fn lower_block(&mut self, b: &Block) {
        self.scopes.push(HashMap::new());
        for stmt in &b.stmts {
            self.lower_stmt(stmt);
        }
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
                self.loops.push(LoopCtx { breaks: Vec::new(), start });
                self.lower_block(body);
                let lp = self.loops.pop().expect("loop context pushed above");
                self.at(*line);
                self.emit(Instr::Jump(start as u32));
                self.set_target(jf);
                for b in lp.breaks {
                    let end = self.code.len() as u32;
                    if let Instr::Jump(t) = &mut self.code[b] {
                        *t = end;
                    }
                }
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
                let start = self.loops.last().expect("`continue` outside a loop (checker bug)").start;
                self.emit(Instr::Jump(start as u32));
            }
            Stmt::Expr { expr, .. } => {
                if self.lower_expr(expr) {
                    self.emit(Instr::Pop);
                }
            }
            Stmt::Block { block, .. } => self.lower_block(block),
        }
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
                let (idx, ret) = *self
                    .index
                    .get(name)
                    .unwrap_or_else(|| panic!("lowering bug: unresolved function `{}`", name));
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
