//! Semantic analysis and type checking.
//!
//! Two passes: collect function signatures, then check every function
//! body against them. Also produces warnings (unreachable code) and, in
//! Beginner mode (`*mode = B;`), extra beginner-friendly notes.

use crate::ast::*;
use crate::diagnostics::{has_errors, Diagnostic};
use std::collections::{HashMap, HashSet};

/// Type-check a parsed program. Returns all diagnostics; the program is
/// valid when no diagnostic has error severity.
pub fn check(program: &Program) -> Vec<Diagnostic> {
    let mut c = Checker::new(program.mode);
    c.check_program(program);
    c.diags
}

struct FnSig {
    params: Vec<Type>,
    ret: Type,
    line: u32,
    col: u32,
}

#[derive(Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
    line: u32,
    col: u32,
}

struct Checker {
    diags: Vec<Diagnostic>,
    mode: Mode,
    sigs: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, VarInfo>>,
    /// Return type of the function currently being checked.
    ret: Type,
    loop_depth: u32,
    /// Names reassigned in the current function (for the B-mode lint).
    assigned: HashSet<String>,
    /// `var` declarations of the current function (for the B-mode lint).
    var_decls: Vec<(String, u32, u32)>,
}

impl Checker {
    fn new(mode: Mode) -> Self {
        Checker {
            diags: Vec::new(),
            mode,
            sigs: HashMap::new(),
            scopes: Vec::new(),
            ret: Type::Void,
            loop_depth: 0,
            assigned: HashSet::new(),
            var_decls: Vec::new(),
        }
    }

    fn error(&mut self, code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) {
        self.diags.push(Diagnostic::error(code, message, line, col, len));
    }

    fn warn(&mut self, code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) {
        self.diags.push(Diagnostic::warning(code, message, line, col, len));
    }

    fn check_program(&mut self, p: &Program) {
        // Pass 1: signatures.
        for f in &p.funcs {
            if f.name == "print" {
                self.error(
                    "E3016",
                    "`print` is a builtin function and cannot be redefined",
                    f.line,
                    f.col,
                    f.name.len() as u32,
                );
                continue;
            }
            if let Some(first) = self.sigs.get(&f.name) {
                let (fl, _) = (first.line, first.col);
                self.error(
                    "E3002",
                    format!("duplicate function `{}` (first defined at line {})", f.name, fl),
                    f.line,
                    f.col,
                    f.name.len() as u32,
                );
                continue;
            }
            self.sigs.insert(
                f.name.clone(),
                FnSig {
                    params: f.params.iter().map(|p| p.ty).collect(),
                    ret: f.ret,
                    line: f.line,
                    col: f.col,
                },
            );
        }

        // Pass 1b: `main` requirements.
        let main_info = self
            .sigs
            .get("main")
            .map(|m| (m.params.len(), m.ret, m.line, m.col));
        match main_info {
            None => {
                self.error("E3008", "program must define `func main()`", 1, 1, 0);
            }
            Some((nparams, ret, line, col)) => {
                if nparams != 0 {
                    self.error("E3008", "`main` must not take parameters", line, col, 4);
                }
                if ret != Type::Void {
                    self.error(
                        "E3008",
                        format!("`main` must return void, found {}", ret.as_str()),
                        line,
                        col,
                        4,
                    );
                }
            }
        }

        // Pass 2: bodies.
        for f in &p.funcs {
            self.scopes.clear();
            self.assigned.clear();
            self.var_decls.clear();
            self.ret = f.ret;
            self.loop_depth = 0;

            let mut scope = HashMap::new();
            for prm in &f.params {
                if prm.ty == Type::Void {
                    self.error(
                        "E3014",
                        format!("parameter `{}` cannot have type void", prm.name),
                        prm.line,
                        prm.col,
                        prm.name.len() as u32,
                    );
                }
                if scope.contains_key(&prm.name) {
                    self.error(
                        "E3002",
                        format!("duplicate parameter `{}`", prm.name),
                        prm.line,
                        prm.col,
                        prm.name.len() as u32,
                    );
                }
                scope.insert(
                    prm.name.clone(),
                    VarInfo { ty: prm.ty, mutable: false, line: prm.line, col: prm.col },
                );
            }
            self.scopes.push(scope);

            self.check_block(&f.body);

            if self.ret != Type::Void && !self.block_terminates(&f.body) {
                self.error(
                    "E3012",
                    format!(
                        "function `{}` must return a value of type {} on every path that reaches the end of its body",
                        f.name,
                        self.ret.as_str()
                    ),
                    f.line,
                    f.col,
                    f.name.len() as u32,
                );
            }

            // Beginner-mode lint: `var` never reassigned.
            if self.mode == Mode::B {
                for (name, line, col) in &self.var_decls {
                    if !self.assigned.contains(name) {
                        self.diags.push(Diagnostic::note(
                            "N3001",
                            format!(
                                "variable `{}` is declared `var` but never reassigned; consider `let`",
                                name
                            ),
                            *line,
                            *col,
                            name.len() as u32,
                        ));
                    }
                }
            }
        }
    }

    // -- scopes --------------------------------------------------------------

    fn declare(&mut self, name: &str, info: VarInfo, line: u32, col: u32) {
        let scope = self.scopes.last_mut().expect("a scope is always open");
        if scope.contains_key(name) {
            self.error(
                "E3002",
                format!("duplicate variable `{}` in this scope", name),
                line,
                col,
                name.len() as u32,
            );
            return;
        }
        scope.insert(name.to_string(), info);
    }

    fn lookup(&self, name: &str) -> Option<VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    // -- statements ----------------------------------------------------------

    fn check_block(&mut self, b: &Block) {
        self.scopes.push(HashMap::new());
        let mut terminated = false;
        let mut reported_unreachable = false;
        for stmt in &b.stmts {
            if terminated && !reported_unreachable {
                self.warn("W3001", "unreachable code", stmt.line(), stmt.col(), 1);
                reported_unreachable = true;
            }
            self.check_stmt(stmt);
            if self.terminates(stmt) {
                terminated = true;
            }
        }
        self.scopes.pop();
    }

    fn check_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::VarDecl { mutable, name, ty, init, line, col } => {
                let init_ty = self.check_expr(init);
                let declared = match ty {
                    Some(t) => {
                        if init_ty != Type::Error && *t != init_ty {
                            self.mismatch(*t, init_ty, *line, *col, name.len() as u32);
                        }
                        *t
                    }
                    None => init_ty,
                };
                if declared == Type::Void {
                    self.error(
                        "E3015",
                        format!(
                            "variable `{}` cannot have type void{}",
                            name,
                            if ty.is_none() { " (the initializer produces no value)" } else { "" }
                        ),
                        *line,
                        *col,
                        name.len() as u32,
                    );
                }
                if *mutable {
                    self.var_decls.push((name.clone(), *line, *col));
                }
                self.declare(
                    name,
                    VarInfo { ty: declared, mutable: *mutable, line: *line, col: *col },
                    *line,
                    *col,
                );
            }
            Stmt::Assign { name, op, value, line, col } => {
                let (vty, vmutable, vline, vcol) = match self.lookup(name) {
                    Some(v) => (v.ty, v.mutable, v.line, v.col),
                    None => {
                        self.error(
                            "E3001",
                            format!("undefined variable `{}`", name),
                            *line,
                            *col,
                            name.len() as u32,
                        );
                        self.check_expr(value);
                        return;
                    }
                };
                if !vmutable {
                    self.error(
                        "E3004",
                        format!("cannot reassign immutable variable `{}` (declared with `let`)", name),
                        *line,
                        *col,
                        name.len() as u32,
                    );
                    self.diags.push(Diagnostic::note(
                        "N3002",
                        format!("`{}` declared here", name),
                        vline,
                        vcol,
                        name.len() as u32,
                    ));
                }
                let vt = self.check_expr(value);
                if vt != Type::Error && vty != Type::Error {
                    match op {
                        AssignOp::Set => {
                            if vt != vty {
                                self.mismatch(vty, vt, *line, *col, name.len() as u32);
                            }
                        }
                        AssignOp::Add | AssignOp::Sub => {
                            let symbol = if *op == AssignOp::Add { "+=" } else { "-=" };
                            if !matches!(vty, Type::Int | Type::Float) {
                                self.error(
                                    "E3003",
                                    format!("`{}` requires a numeric variable, found {}", symbol, vty.as_str()),
                                    *line,
                                    *col,
                                    2,
                                );
                            } else if vt != vty {
                                self.mismatch(vty, vt, *line, *col, 2);
                            }
                        }
                    }
                }
                self.assigned.insert(name.clone());
            }
            Stmt::If { cond, then_branch, else_branch, .. } => {
                let ct = self.check_expr(cond);
                if ct != Type::Bool && ct != Type::Error {
                    self.error(
                        "E3005",
                        format!("`if` condition must be bool, found {}", ct.as_str()),
                        cond.line,
                        cond.col,
                        2,
                    );
                }
                self.check_block(then_branch);
                if let Some(e) = else_branch {
                    self.check_stmt(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                let ct = self.check_expr(cond);
                if ct != Type::Bool && ct != Type::Error {
                    self.error(
                        "E3005",
                        format!("`while` condition must be bool, found {}", ct.as_str()),
                        cond.line,
                        cond.col,
                        5,
                    );
                }
                self.loop_depth += 1;
                self.check_block(body);
                self.loop_depth -= 1;
            }
            Stmt::Return { value, line, col } => match value {
                None => {
                    if self.ret != Type::Void {
                        self.error(
                            "E3006",
                            format!("missing return value (function returns {})", self.ret.as_str()),
                            *line,
                            *col,
                            6,
                        );
                    }
                }
                Some(v) => {
                    let vt = self.check_expr(v);
                    if self.ret == Type::Void {
                        self.error("E3006", "void function cannot return a value", *line, *col, 6);
                    } else if vt != Type::Error && vt != self.ret {
                        self.mismatch(self.ret, vt, v.line, v.col, 1);
                    }
                }
            },
            Stmt::Break { line, col } => {
                if self.loop_depth == 0 {
                    self.error("E3009", "`break` outside of a loop", *line, *col, 5);
                }
            }
            Stmt::Continue { line, col } => {
                if self.loop_depth == 0 {
                    self.error("E3009", "`continue` outside of a loop", *line, *col, 8);
                }
            }
            // A bare expression statement is fine; calling a void function
            // is the normal way to use it.
            Stmt::Expr { expr, .. } => {
                self.check_expr(expr);
            }
            Stmt::Block { block, .. } => self.check_block(block),
        }
    }

    // -- expressions ---------------------------------------------------------

    fn check_expr(&mut self, e: &Expr) -> Type {
        match &e.kind {
            ExprKind::IntLit(_) => Type::Int,
            ExprKind::FloatLit(_) => Type::Float,
            ExprKind::BoolLit(_) => Type::Bool,
            ExprKind::StrLit(_) => Type::Str,
            ExprKind::Ident(name) => match self.lookup(name) {
                Some(v) => v.ty,
                None => {
                    self.error(
                        "E3001",
                        format!("undefined variable `{}`", name),
                        e.line,
                        e.col,
                        name.len() as u32,
                    );
                    Type::Error
                }
            },
            ExprKind::Unary { op, expr } => {
                let t = self.check_expr(expr);
                match op {
                    UnOp::Not => {
                        if t != Type::Bool && t != Type::Error {
                            self.error(
                                "E3003",
                                format!("`not` requires bool, found {}", t.as_str()),
                                e.line,
                                e.col,
                                3,
                            );
                        }
                        Type::Bool
                    }
                    UnOp::Neg => match t {
                        Type::Int => Type::Int,
                        Type::Float => Type::Float,
                        Type::Error => Type::Error,
                        _ => {
                            self.error(
                                "E3003",
                                format!("unary `-` requires a number, found {}", t.as_str()),
                                e.line,
                                e.col,
                                1,
                            );
                            Type::Error
                        }
                    },
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let lt = self.check_expr(lhs);
                let rt = self.check_expr(rhs);
                if lt == Type::Error || rt == Type::Error {
                    return Type::Error;
                }
                use BinOp::*;
                match op {
                    Add => match (lt, rt) {
                        (Type::Int, Type::Int) => Type::Int,
                        (Type::Float, Type::Float) => Type::Float,
                        (Type::Str, Type::Str) => Type::Str,
                        _ => {
                            self.bin_mismatch(*op, lt, rt, e.line, e.col);
                            Type::Error
                        }
                    },
                    Sub | Mul | Div => {
                        if matches!(lt, Type::Int | Type::Float) && lt == rt {
                            lt
                        } else {
                            self.bin_mismatch(*op, lt, rt, e.line, e.col);
                            Type::Error
                        }
                    }
                    Mod => {
                        if lt == Type::Int && rt == Type::Int {
                            Type::Int
                        } else {
                            self.bin_mismatch(*op, lt, rt, e.line, e.col);
                            Type::Error
                        }
                    }
                    Lt | Gt | Le | Ge => {
                        if matches!(lt, Type::Int | Type::Float) && lt == rt {
                            Type::Bool
                        } else {
                            self.bin_mismatch(*op, lt, rt, e.line, e.col);
                            Type::Error
                        }
                    }
                    Eq | Ne => {
                        if lt == rt {
                            Type::Bool
                        } else {
                            self.bin_mismatch(*op, lt, rt, e.line, e.col);
                            Type::Error
                        }
                    }
                    And | Or => {
                        if lt == Type::Bool && rt == Type::Bool {
                            Type::Bool
                        } else {
                            self.bin_mismatch(*op, lt, rt, e.line, e.col);
                            Type::Error
                        }
                    }
                }
            }
            ExprKind::Call { name, args } => self.check_call(name, args, e.line, e.col),
        }
    }

    fn check_call(&mut self, name: &str, args: &[Expr], line: u32, col: u32) -> Type {
        if name == "print" {
            if args.len() != 1 {
                self.error(
                    "E3007",
                    format!("`print` expects 1 argument, found {}", args.len()),
                    line,
                    col,
                    name.len() as u32,
                );
                for a in args {
                    self.check_expr(a);
                }
                return Type::Void;
            }
            let t = self.check_expr(&args[0]);
            if t == Type::Void {
                self.error(
                    "E3010",
                    "cannot pass a void value to `print`",
                    line,
                    col,
                    name.len() as u32,
                );
            }
            return Type::Void;
        }
        let sig = match self.sigs.get(name) {
            Some(s) => (s.params.clone(), s.ret),
            None => {
                self.error(
                    "E3001",
                    format!("undefined function `{}`", name),
                    line,
                    col,
                    name.len() as u32,
                );
                for a in args {
                    self.check_expr(a);
                }
                return Type::Error;
            }
        };
        let (params, ret) = sig;
        if args.len() != params.len() {
            let plural = if params.len() == 1 { "" } else { "s" };
            self.error(
                "E3007",
                format!(
                    "function `{}` expects {} argument{}, found {}",
                    name,
                    params.len(),
                    plural,
                    args.len()
                ),
                line,
                col,
                name.len() as u32,
            );
        }
        for (i, a) in args.iter().enumerate() {
            let at = self.check_expr(a);
            if at == Type::Void {
                self.error(
                    "E3010",
                    format!("argument {} is void (the call produces no value)", i + 1),
                    a.line,
                    a.col,
                    1,
                );
                continue;
            }
            if at != Type::Error && i < params.len() && at != params[i] {
                self.mismatch(params[i], at, a.line, a.col, 1);
            }
        }
        ret
    }

    // -- helpers -------------------------------------------------------------

    /// Type mismatch with a dedicated message when int and float are mixed.
    fn mismatch(&mut self, expected: Type, found: Type, line: u32, col: u32, len: u32) {
        if matches!(
            (expected, found),
            (Type::Int, Type::Float) | (Type::Float, Type::Int)
        ) {
            self.error(
                "E3011",
                format!(
                    "no implicit conversion between int and float (expected {}, found {}); use matching literals like `2.0`",
                    expected.as_str(),
                    found.as_str()
                ),
                line,
                col,
                len,
            );
        } else {
            self.error(
                "E3003",
                format!("type mismatch: expected {}, found {}", expected.as_str(), found.as_str()),
                line,
                col,
                len,
            );
        }
    }

    fn bin_mismatch(&mut self, op: BinOp, lt: Type, rt: Type, line: u32, col: u32) {
        let custom = match op {
            BinOp::Mod => Some(format!(
                "`%` requires int operands, found {} and {}",
                lt.as_str(),
                rt.as_str()
            )),
            BinOp::And | BinOp::Or => Some(format!(
                "`{}` requires bool operands, found {} and {}",
                op,
                lt.as_str(),
                rt.as_str()
            )),
            BinOp::Eq | BinOp::Ne => Some(format!(
                "cannot compare {} with {} (values of different types)",
                lt.as_str(),
                rt.as_str()
            )),
            _ => None,
        };
        match custom {
            Some(msg) => self.error("E3003", msg, line, col, 1),
            None => {
                if matches!(
                    (lt, rt),
                    (Type::Int, Type::Float) | (Type::Float, Type::Int)
                ) {
                    self.error(
                        "E3011",
                        format!(
                            "no implicit conversion between int and float ({} `{}` {}); use matching literals like `2.0`",
                            lt.as_str(),
                            op,
                            rt.as_str()
                        ),
                        line,
                        col,
                        1,
                    );
                } else {
                    self.error(
                        "E3003",
                        format!(
                            "type mismatch: `{}` cannot combine {} and {}",
                            op,
                            lt.as_str(),
                            rt.as_str()
                        ),
                        line,
                        col,
                        1,
                    );
                }
            }
        }
    }

    // -- control-flow analysis -------------------------------------------------

    /// Conservative "does this statement end execution of its block?"
    /// analysis: `return`, or an `if` with an `else` where both branches
    /// terminate. `while true` is not recognized (P-002 in the registry).
    fn terminates(&self, s: &Stmt) -> bool {
        match s {
            Stmt::Return { .. } => true,
            Stmt::If { then_branch, else_branch, .. } => match else_branch.as_deref() {
                Some(e) => self.block_terminates(then_branch) && self.terminates(e),
                None => false,
            },
            Stmt::Block { block, .. } => self.block_terminates(block),
            _ => false,
        }
    }

    fn block_terminates(&self, b: &Block) -> bool {
        b.stmts.iter().any(|s| self.terminates(s))
    }
}

/// Convenience for tests and tools: run the checker and return whether the
/// program has any errors.
pub fn is_valid(program: &Program) -> bool {
    !has_errors(&check(program))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn check_src(src: &str) -> Vec<Diagnostic> {
        let (toks, _) = crate::lexer::tokenize(src);
        let prog = parse(toks).expect("test source must parse");
        check(&prog)
    }

    fn clean(src: &str) {
        let diags = check_src(src);
        let errs: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errs.is_empty(), "expected no errors, got: {:#?}", errs);
    }

    fn has(code: &str, src: &str) -> bool {
        check_src(src).iter().any(|d| d.code == code)
    }

    const HELLO: &str = "*mode = B;\nfunc main() { print(\"hi\"); let x = 1; }\n";

    #[test]
    fn valid_program_passes() {
        clean(HELLO);
        clean("*mode = S;\nfunc add(a: int, b: int) -> int { return a + b; } func main() { print(add(1, 2)); }");
    }

    #[test]
    fn missing_main_is_an_error() {
        assert!(has("E3008", "func other() { }"));
    }

    #[test]
    fn main_must_be_void_and_parameterless() {
        assert!(has("E3008", "func main(x: int) { }"));
        assert!(has("E3008", "func main() -> int { return 1; }"));
    }

    #[test]
    fn undefined_variable_and_function() {
        assert!(has("E3001", "func main() { print(x); }"));
        assert!(has("E3001", "func main() { nope(1); }"));
    }

    #[test]
    fn immutable_reassignment_is_rejected_with_note() {
        let diags = check_src("func main() { let x = 1; x = 2; }");
        assert!(diags.iter().any(|d| d.code == "E3004"));
        assert!(diags.iter().any(|d| d.code == "N3002" && d.message.contains("declared here")));
    }

    #[test]
    fn no_implicit_int_float_conversion() {
        assert!(has("E3011", "func main() { print(1 + 0.5); }"));
        assert!(has("E3011", "func main() { let f: float = 1.5; f = 2; }"));
    }

    #[test]
    fn matching_float_arithmetic_is_fine() {
        clean("func main() { print(1.5 + 0.5); }");
    }

    #[test]
    fn conditions_must_be_bool() {
        assert!(has("E3005", "func main() { if 1 { print(2); } }"));
        assert!(has("E3005", "func main() { while 0 { print(2); } }"));
    }

    #[test]
    fn return_rules() {
        assert!(has("E3006", "func f() -> int { return; } func main() { print(f()); }"));
        assert!(has("E3006", "func f() { return 1; } func main() { f(); }"));
        assert!(has("E3003", "func f() -> int { return \"x\"; } func main() { print(f()); }"));
    }

    #[test]
    fn missing_return_is_detected() {
        assert!(has("E3012", "func f() -> int { print(1); } func main() { print(f()); }"));
        // if/else with both branches returning is fine
        clean("func f(n: int) -> int { if n > 0 { return 1; } else { return 2; } } func main() { print(f(1)); }");
    }

    #[test]
    fn break_and_continue_need_loops() {
        assert!(has("E3009", "func main() { break; }"));
        assert!(has("E3009", "func main() { continue; }"));
        clean("func main() { while true { break; } }");
    }

    #[test]
    fn void_values_cannot_be_consumed() {
        assert!(has("E3015", "func main() { let x = print(\"a\"); }"));
        assert!(has("E3010", "func main() { print(print(\"a\")); }"));
    }

    #[test]
    fn void_params_and_vars_are_rejected() {
        assert!(has("E3014", "func f(x: void) { } func main() { f(); }"));
        assert!(has("E3015", "func main() { let x: void = 1; }"));
    }

    #[test]
    fn wrong_argument_count_is_rejected() {
        assert!(has("E3007", "func f(a: int) { } func main() { f(); }"));
        assert!(has("E3007", "func f(a: int) { } func main() { f(1, 2); }"));
        assert!(has("E3007", "func main() { print(1, 2); }"));
    }

    #[test]
    fn argument_types_are_checked() {
        assert!(has("E3003", "func f(a: int) { } func main() { f(\"x\"); }"));
        assert!(has("E3011", "func f(a: float) { } func main() { f(1); }"));
    }

    #[test]
    fn duplicate_definitions_are_rejected() {
        assert!(has("E3002", "func f() { } func f() { } func main() { f(); }"));
        assert!(has("E3002", "func main() { let x = 1; let x = 2; }"));
        assert!(has("E3002", "func f(a: int, a: int) { } func main() { f(1, 2); }"));
    }

    #[test]
    fn shadowing_in_inner_scope_is_allowed() {
        clean("func main() { let x = 1; { let x = 2.5; print(x); } print(x); }");
    }

    #[test]
    fn print_cannot_be_redefined() {
        assert!(has("E3016", "func print(x: int) { } func main() { }"));
    }

    #[test]
    fn string_rules() {
        clean("func main() { print(\"a\" + \"b\"); if \"a\" == \"a\" { print(1); } }");
        assert!(has("E3003", "func main() { print(\"a\" + 1); }"));
    }

    #[test]
    fn unreachable_code_is_a_warning_not_an_error() {
        let diags = check_src("func f() -> int { return 1; print(2); } func main() { print(f()); }");
        assert!(diags.iter().any(|d| d.code == "W3001" && !d.is_error()));
    }

    #[test]
    fn beginner_mode_lints_unnecessary_var() {
        let b = check_src("*mode = B;\nfunc main() { var x = 1; print(x); }");
        assert!(b.iter().any(|d| d.code == "N3001"));
        let s = check_src("*mode = S;\nfunc main() { var x = 1; print(x); }");
        assert!(!s.iter().any(|d| d.code == "N3001"));
        // reassigned vars are not linted
        let b2 = check_src("*mode = B;\nfunc main() { var x = 1; x = 2; print(x); }");
        assert!(!b2.iter().any(|d| d.code == "N3001"));
    }

    #[test]
    fn logical_operators_require_bools() {
        assert!(has("E3003", "func main() { print(1 and 2); }"));
        clean("func main() { print(true and false or not true); }");
    }

    #[test]
    fn modulo_is_int_only() {
        assert!(has("E3003", "func main() { print(1.5 % 2.0); }"));
        clean("func main() { print(7 % 3); }");
    }
}
