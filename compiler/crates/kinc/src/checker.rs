//! Semantic analysis and type checking.
//!
//! Two passes over every file of a project: collect function signatures,
//! then check each function body against them (local functions plus
//! everything reachable through imports). Also produces warnings
//! (unreachable code) and, in Beginner mode (`*mode = B;`), extra
//! beginner-friendly notes.

use crate::ast::*;
use crate::diagnostics::{has_errors, Diagnostic};
use crate::project::{FileUnit, Project};
use std::collections::{HashMap, HashSet};

/// Builtins that cannot be redefined by user functions.
const BUILTINS: [&str; 5] = ["print", "len", "abs", "min", "max"];

/// Type-check a single-file program (no imports). Returns all
/// diagnostics; the program is valid when no diagnostic has error
/// severity. Multi-file projects use [`check_project`].
pub fn check(program: &Program) -> Vec<Diagnostic> {
    let project = Project {
        files: vec![FileUnit {
            path: "<main>".to_string(),
            program: program.clone(),
            imports: Vec::new(),
        }],
        sources: HashMap::new(),
    };
    check_project(&project)
}

/// Type-check a whole loaded project. `files[0]` is the entry; only the
/// entry needs `func main()`.
pub fn check_project(project: &Project) -> Vec<Diagnostic> {
    let mut c = Checker {
        diags: Vec::new(),
        mode: project.files[0].program.mode,
        sigs: Vec::new(),
        imports: HashMap::new(),
        cur_file: 0,
        file: "<main>".to_string(),
        scopes: Vec::new(),
        ret: Type::Void,
        loop_depth: 0,
        assigned: HashSet::new(),
        var_decls: Vec::new(),
    };

    // Pass 1: signatures of every file.
    for unit in &project.files {
        let mut sigs: HashMap<String, FnSig> = HashMap::new();
        for f in &unit.program.funcs {
            if BUILTINS.contains(&f.name.as_str()) {
                c.error(
                    "E3016",
                    format!("`{}` is a builtin function and cannot be redefined", f.name),
                    f.line,
                    f.col,
                    f.name.len() as u32,
                );
                continue;
            }
            if let Some(first) = sigs.get(&f.name) {
                let fl = first.line;
                c.error(
                    "E3002",
                    format!("duplicate function `{}` (first defined at line {})", f.name, fl),
                    f.line,
                    f.col,
                    f.name.len() as u32,
                );
                continue;
            }
            sigs.insert(
                f.name.clone(),
                FnSig {
                    params: f.params.iter().map(|p| p.ty.clone()).collect(),
                    ret: f.ret.clone(),
                    line: f.line,
                    col: f.col,
                },
            );
        }
        c.sigs.push(sigs);
    }

    // Pass 1b: `main` requirements — entry file only.
    let main_info = c.sigs[0]
        .get("main")
        .map(|m| (m.params.len(), m.ret.clone(), m.line, m.col));
    match main_info {
        None => {
            c.error("E3008", "program must define `func main()`", 1, 1, 0);
        }
        Some((nparams, ret, line, col)) => {
            if nparams != 0 {
                c.error("E3008", "`main` must not take parameters", line, col, 4);
            }
            if ret != Type::Void {
                c.error(
                    "E3008",
                    format!("`main` must return void, found {}", ret.as_str()),
                    line,
                    col,
                    4,
                );
            }
        }
    }

    // Pass 2: bodies, file by file.
    for i in 0..project.files.len() {
        c.cur_file = i;
        c.file = project.files[i].path.clone();
        c.mode = project.files[i].program.mode;
        c.imports = project.files[i]
            .imports
            .iter()
            .map(|r| (r.alias.clone(), r.file))
            .collect();

        let funcs = project.files[i].program.funcs.clone();
        for f in &funcs {
            c.scopes.clear();
            c.assigned.clear();
            c.var_decls.clear();
            c.ret = f.ret.clone();
            c.loop_depth = 0;

            let mut scope = HashMap::new();
            for prm in &f.params {
                if prm.ty == Type::Void {
                    c.error(
                        "E3014",
                        format!("parameter `{}` cannot have type void", prm.name),
                        prm.line,
                        prm.col,
                        prm.name.len() as u32,
                    );
                }
                if scope.contains_key(&prm.name) {
                    c.error(
                        "E3002",
                        format!("duplicate parameter `{}`", prm.name),
                        prm.line,
                        prm.col,
                        prm.name.len() as u32,
                    );
                }
                scope.insert(
                    prm.name.clone(),
                    VarInfo { ty: prm.ty.clone(), mutable: false, line: prm.line, col: prm.col },
                );
            }
            c.scopes.push(scope);

            c.check_block(&f.body);

            if c.ret != Type::Void && !c.block_terminates(&f.body) {
                c.error(
                    "E3012",
                    format!(
                        "function `{}` must return a value of type {} on every path that reaches the end of its body",
                        f.name,
                        c.ret.as_str()
                    ),
                    f.line,
                    f.col,
                    f.name.len() as u32,
                );
            }

            // Beginner-mode lint: `var` never reassigned.
            if c.mode == Mode::B {
                for (name, line, col) in &c.var_decls.clone() {
                    if !c.assigned.contains(name) {
                        c.diags.push(
                            Diagnostic::note(
                                "N3001",
                                format!(
                                    "variable `{}` is declared `var` but never reassigned; consider `let`",
                                    name
                                ),
                                *line,
                                *col,
                                name.len() as u32,
                            )
                            .in_file(c.file.clone()),
                        );
                    }
                }
            }
        }
    }

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
    /// Signatures per project file, indexed like `project.files`.
    sigs: Vec<HashMap<String, FnSig>>,
    /// Import alias -> file index, for the file being checked.
    imports: HashMap<String, usize>,
    cur_file: usize,
    file: String,
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
    fn error(&mut self, code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) {
        self.diags
            .push(Diagnostic::error(code, message, line, col, len).in_file(self.file.clone()));
    }

    fn warn(&mut self, code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) {
        self.diags
            .push(Diagnostic::warning(code, message, line, col, len).in_file(self.file.clone()));
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
                // An empty array literal is only checkable against a
                // declared array type: `let a: [int] = [];`.
                let init_ty = match (ty, &init.kind) {
                    (Some(Type::Array(_)), ExprKind::ArrayLit(items)) if items.is_empty() => {
                        ty.clone().unwrap()
                    }
                    _ => self.check_expr(init),
                };
                let declared = match ty {
                    Some(t) => {
                        if init_ty != Type::Error && *t != init_ty {
                            self.mismatch(t, &init_ty, *line, *col, name.len() as u32);
                        }
                        t.clone()
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
                    self.diags.push(
                        Diagnostic::note(
                            "N3002",
                            format!("`{}` declared here", name),
                            vline,
                            vcol,
                            name.len() as u32,
                        )
                        .in_file(self.file.clone()),
                    );
                }
                let vt = self.check_expr(value);
                if vt != Type::Error && vty != Type::Error {
                    match op {
                        AssignOp::Set => {
                            if vt != vty {
                                self.mismatch(&vty, &vt, *line, *col, name.len() as u32);
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
                                self.mismatch(&vty, &vt, *line, *col, 2);
                            }
                        }
                    }
                }
                self.assigned.insert(name.clone());
            }
            Stmt::IndexAssign { base, index, value, line, col } => {
                let bt = self.check_expr(base);
                let it = self.check_expr(index);
                let vt = self.check_expr(value);
                match &bt {
                    Type::Array(elem) => {
                        if it != Type::Int && it != Type::Error {
                            self.error(
                                "E3023",
                                format!("array index must be an int, found {}", it.as_str()),
                                *line,
                                *col,
                                1,
                            );
                        }
                        if vt != Type::Error && vt != **elem {
                            self.mismatch(elem, &vt, *line, *col, 1);
                        }
                    }
                    Type::Error => {}
                    other => {
                        self.error(
                            "E3024",
                            format!("cannot index into {} (only arrays support `[...]`)", other.as_str()),
                            *line,
                            *col,
                            1,
                        );
                    }
                }
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
            Stmt::For { var, iter, body, line, col } => {
                // The iterable is checked in the enclosing scope; the
                // loop variable is a fresh immutable binding per iteration.
                let var_ty = match iter {
                    ForIter::Range { start, end, .. } => {
                        let st = self.check_expr(start);
                        if st != Type::Int && st != Type::Error {
                            self.error(
                                "E3027",
                                format!("range bounds must be int, found {} for the start", st.as_str()),
                                start.line,
                                start.col,
                                1,
                            );
                        }
                        let et = self.check_expr(end);
                        if et != Type::Int && et != Type::Error {
                            self.error(
                                "E3027",
                                format!("range bounds must be int, found {} for the end", et.as_str()),
                                end.line,
                                end.col,
                                1,
                            );
                        }
                        Type::Int
                    }
                    ForIter::Expr(e) => match self.check_expr(e) {
                        Type::Array(elem) => *elem,
                        Type::Error => Type::Error,
                        other => {
                            self.error(
                                "E3026",
                                format!(
                                    "`for` iterable must be a range (`a..b`) or an array, found {}",
                                    other.as_str()
                                ),
                                e.line,
                                e.col,
                                3,
                            );
                            Type::Error
                        }
                    },
                };
                let _ = (line, col);
                self.scopes.push(HashMap::new());
                self.scopes.last_mut().unwrap().insert(
                    var.clone(),
                    VarInfo { ty: var_ty, mutable: false, line: *line, col: *col },
                );
                self.loop_depth += 1;
                self.check_block(body);
                self.loop_depth -= 1;
                self.scopes.pop();
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
                        self.mismatch(&self.ret.clone(), &vt, v.line, v.col, 1);
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
            ExprKind::ArrayLit(items) => {
                if items.is_empty() {
                    self.error(
                        "E3022",
                        "cannot infer the element type of an empty array literal; add a type annotation, e.g. `let a: [int] = [];`",
                        e.line,
                        e.col,
                        2,
                    );
                    return Type::Error;
                }
                let mut elem: Option<Type> = None;
                for item in items {
                    let t = self.check_expr(item);
                    match &elem {
                        None => {
                            if t != Type::Error {
                                elem = Some(t);
                            }
                        }
                        Some(expected) => {
                            if t != Type::Error && t != *expected {
                                self.error(
                                    "E3028",
                                    format!(
                                        "array literal elements must all have the same type (first element is {}, this one is {})",
                                        expected.as_str(),
                                        t.as_str()
                                    ),
                                    item.line,
                                    item.col,
                                    1,
                                );
                            }
                        }
                    }
                }
                match elem {
                    Some(t) => Type::Array(Box::new(t)),
                    None => Type::Error,
                }
            }
            ExprKind::Index { base, index } => {
                let bt = self.check_expr(base);
                let it = self.check_expr(index);
                match bt {
                    Type::Array(elem) => {
                        if it != Type::Int && it != Type::Error {
                            self.error(
                                "E3023",
                                format!("array index must be an int, found {}", it.as_str()),
                                e.line,
                                e.col,
                                1,
                            );
                        }
                        *elem
                    }
                    Type::Error => Type::Error,
                    other => {
                        self.error(
                            "E3024",
                            format!("cannot index into {} (only arrays support `[...]`)", other.as_str()),
                            e.line,
                            e.col,
                            1,
                        );
                        Type::Error
                    }
                }
            }
            ExprKind::Cast { expr, ty } => {
                let it = self.check_expr(expr);
                if it == Type::Error {
                    return Type::Error;
                }
                if &it == ty {
                    self.error(
                        "E3025",
                        format!("cast from {} to {} is unnecessary", it.as_str(), ty.as_str()),
                        e.line,
                        e.col,
                        2,
                    );
                    return ty.clone();
                }
                let ok = matches!(
                    (&it, ty),
                    (Type::Int, Type::Float)
                        | (Type::Float, Type::Int)
                        | (Type::Int, Type::Str)
                        | (Type::Float, Type::Str)
                        | (Type::Bool, Type::Str)
                );
                if !ok {
                    self.diags.push(
                        Diagnostic::error(
                            "E3025",
                            format!(
                                "unsupported cast from {} to {} (supported: int as float, float as int, int as string, float as string, bool as string)",
                                it.as_str(),
                                ty.as_str()
                            ),
                            e.line,
                            e.col,
                            2,
                        )
                        .in_file(self.file.clone()),
                    );
                }
                ty.clone()
            }
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
                    Add => match (&lt, &rt) {
                        (Type::Int, Type::Int) => Type::Int,
                        (Type::Float, Type::Float) => Type::Float,
                        (Type::Str, Type::Str) => Type::Str,
                        _ => {
                            self.bin_mismatch(*op, &lt, &rt, e.line, e.col);
                            Type::Error
                        }
                    },
                    Sub | Mul | Div => {
                        if matches!(lt, Type::Int | Type::Float) && lt == rt {
                            lt
                        } else {
                            self.bin_mismatch(*op, &lt, &rt, e.line, e.col);
                            Type::Error
                        }
                    }
                    Mod => {
                        if lt == Type::Int && rt == Type::Int {
                            Type::Int
                        } else {
                            self.bin_mismatch(*op, &lt, &rt, e.line, e.col);
                            Type::Error
                        }
                    }
                    Lt | Gt | Le | Ge => {
                        if matches!(lt, Type::Int | Type::Float) && lt == rt {
                            Type::Bool
                        } else {
                            self.bin_mismatch(*op, &lt, &rt, e.line, e.col);
                            Type::Error
                        }
                    }
                    Eq | Ne => {
                        if lt == rt {
                            Type::Bool
                        } else {
                            self.bin_mismatch(*op, &lt, &rt, e.line, e.col);
                            Type::Error
                        }
                    }
                    And | Or => {
                        if lt == Type::Bool && rt == Type::Bool {
                            Type::Bool
                        } else {
                            self.bin_mismatch(*op, &lt, &rt, e.line, e.col);
                            Type::Error
                        }
                    }
                }
            }
            ExprKind::Call { name, args } => self.check_call(name, args, e.line, e.col),
            ExprKind::ModCall { module, name, args } => {
                self.check_mod_call(module, name, args, e.line, e.col)
            }
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
        if let Some(result) = self.check_native(name, args, line, col) {
            return result;
        }
        let sig = match self.sigs[self.cur_file].get(name) {
            Some(s) => (s.params.clone(), s.ret.clone()),
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
        self.check_args(name, &params, args, line, col);
        ret
    }

    /// Native builtins: `len`, `abs`, `min`, `max`. Returns `None` when
    /// `name` is not a native.
    fn check_native(&mut self, name: &str, args: &[Expr], line: u32, col: u32) -> Option<Type> {
        match name {
            "len" => {
                if args.len() != 1 {
                    self.error(
                        "E3007",
                        format!("`len` expects 1 argument, found {}", args.len()),
                        line,
                        col,
                        3,
                    );
                    for a in args {
                        self.check_expr(a);
                    }
                    return Some(Type::Error);
                }
                let t = self.check_expr(&args[0]);
                match &t {
                    Type::Str | Type::Array(_) | Type::Error => Some(Type::Int),
                    other => {
                        self.error(
                            "E3003",
                            format!("`len` expects an array or a string, found {}", other.as_str()),
                            args[0].line,
                            args[0].col,
                            3,
                        );
                        Some(Type::Error)
                    }
                }
            }
            "abs" => {
                if args.len() != 1 {
                    self.error(
                        "E3007",
                        format!("`abs` expects 1 argument, found {}", args.len()),
                        line,
                        col,
                        3,
                    );
                    for a in args {
                        self.check_expr(a);
                    }
                    return Some(Type::Error);
                }
                let t = self.check_expr(&args[0]);
                match t {
                    Type::Int | Type::Float | Type::Error => Some(t),
                    other => {
                        self.error(
                            "E3003",
                            format!("`abs` expects int or float, found {}", other.as_str()),
                            args[0].line,
                            args[0].col,
                            3,
                        );
                        Some(Type::Error)
                    }
                }
            }
            "min" | "max" => {
                if args.len() != 2 {
                    self.error(
                        "E3007",
                        format!("`{}` expects 2 arguments, found {}", name, args.len()),
                        line,
                        col,
                        3,
                    );
                    for a in args {
                        self.check_expr(a);
                    }
                    return Some(Type::Error);
                }
                let a = self.check_expr(&args[0]);
                let b = self.check_expr(&args[1]);
                if a == Type::Error || b == Type::Error {
                    return Some(Type::Error);
                }
                if matches!(a, Type::Int | Type::Float) && a == b {
                    Some(a)
                } else {
                    self.error(
                        "E3003",
                        format!(
                            "`{}` expects two ints or two floats, found {} and {}",
                            name,
                            a.as_str(),
                            b.as_str()
                        ),
                        line,
                        col,
                        3,
                    );
                    Some(Type::Error)
                }
            }
            _ => None,
        }
    }

    fn check_mod_call(&mut self, module: &str, name: &str, args: &[Expr], line: u32, col: u32) -> Type {
        let file = match self.imports.get(module) {
            Some(f) => *f,
            None => {
                self.error(
                    "E3020",
                    format!("unknown module `{}` (imports introduce module names — did you forget `import ... as {};`?)", module, module),
                    line,
                    col,
                    module.len() as u32,
                );
                for a in args {
                    self.check_expr(a);
                }
                return Type::Error;
            }
        };
        let sig = match self.sigs[file].get(name) {
            Some(s) => (s.params.clone(), s.ret.clone()),
            None => {
                self.error(
                    "E3021",
                    format!("module `{}` has no function `{}`", module, name),
                    line,
                    col,
                    (module.len() + name.len() + 1) as u32,
                );
                for a in args {
                    self.check_expr(a);
                }
                return Type::Error;
            }
        };
        let (params, ret) = sig;
        self.check_args(name, &params, args, line, col);
        ret
    }

    fn check_args(&mut self, name: &str, params: &[Type], args: &[Expr], line: u32, col: u32) {
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
                self.mismatch(&params[i], &at, a.line, a.col, 1);
            }
        }
    }

    // -- helpers -------------------------------------------------------------

    /// Type mismatch with a dedicated message when int and float are mixed.
    fn mismatch(&mut self, expected: &Type, found: &Type, line: u32, col: u32, len: u32) {
        if matches!(
            (expected, found),
            (Type::Int, Type::Float) | (Type::Float, Type::Int)
        ) {
            self.error(
                "E3011",
                format!(
                    "no implicit conversion between int and float (expected {}, found {}); use matching literals like `2.0` or an explicit cast",
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

    fn bin_mismatch(&mut self, op: BinOp, lt: &Type, rt: &Type, line: u32, col: u32) {
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
                            "no implicit conversion between int and float ({} `{}` {}); use matching literals like `2.0` or an explicit cast",
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
    //
    // `terminates` answers "does control ever continue past this
    // statement?" — `return`, an if/else where both branches terminate,
    // or a `while true` loop that cannot break (it diverges; code after
    // it is unreachable and the function cannot fall off its end).

    fn terminates(&self, s: &Stmt) -> bool {
        match s {
            Stmt::Return { .. } => true,
            Stmt::If { then_branch, else_branch, .. } => match else_branch.as_deref() {
                Some(e) => self.block_terminates(then_branch) && self.terminates(e),
                None => false,
            },
            Stmt::Block { block, .. } => self.block_terminates(block),
            Stmt::While { cond, body, .. } => {
                matches!(&cond.kind, ExprKind::BoolLit(true)) && !self.has_direct_break(body)
            }
            _ => false,
        }
    }

    fn block_terminates(&self, b: &Block) -> bool {
        b.stmts.iter().any(|s| self.terminates(s))
    }

    /// Does this block contain a `break` that targets the loop being
    /// scanned? Breaks inside nested loops belong to those loops and do
    /// not count; breaks inside `if` or bare blocks of *this* loop do.
    fn has_direct_break(&self, b: &Block) -> bool {
        b.stmts.iter().any(|s| self.stmt_has_direct_break(s))
    }

    fn stmt_has_direct_break(&self, s: &Stmt) -> bool {
        match s {
            Stmt::Break { .. } => true,
            Stmt::If { then_branch, else_branch, .. } => {
                self.has_direct_break(then_branch)
                    || else_branch
                        .as_deref()
                        .map(|e| self.stmt_has_direct_break(e))
                        .unwrap_or(false)
            }
            Stmt::Block { block, .. } => self.has_direct_break(block),
            _ => false, // While/For shield their own breaks; other stmts cannot break.
        }
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
    use crate::project;

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

    /// Check a two-file project: entry + one import.
    fn check_two(entry: &str, lib_src: &str) -> Vec<Diagnostic> {
        let files = [("lib.kin", lib_src)];
        let map: std::collections::HashMap<String, String> = files
            .iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect();
        let mut loader = move |path: &str| map.get(path).cloned().ok_or("not found".to_string());
        let p = project::load("main.kin", entry, &mut loader).expect("loads");
        check_project(&p)
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
        assert!(has("E3016", "func len(x: int) { } func main() { }"));
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

    // -- 0.2.0: control-flow analysis (P-002) --------------------------------

    #[test]
    fn while_true_without_break_satisfies_return() {
        // The exact case from PROBLEM_REGISTRY P-002.
        clean("func spin() -> int { while true { return 1; } } func main() { print(spin()); }");
        // Diverging loop with no return at all is also valid.
        clean("func spin() -> int { while true { print(1); } } func main() { spin(); }");
    }

    #[test]
    fn while_true_with_break_still_needs_a_return() {
        assert!(has(
            "E3012",
            "func f() -> int { while true { break; } } func main() { print(f()); }"
        ));
        // a return after the loop fixes it
        clean("func f() -> int { while true { break; } return 1; } func main() { print(f()); }");
    }

    #[test]
    fn nested_loop_break_does_not_stop_the_outer_loop() {
        // The inner `break` belongs to the inner loop, so the outer
        // `while true` still diverges — no E3012.
        clean(
            "func f() -> int {\n    while true {\n        while true { break; }\n        return 1;\n    }\n}\nfunc main() { print(f()); }",
        );
    }

    #[test]
    fn code_after_diverging_loop_is_unreachable() {
        let diags =
            check_src("func main() { while true { print(1); } print(2); }");
        assert!(diags.iter().any(|d| d.code == "W3001"));
    }

    // -- 0.2.0: arrays --------------------------------------------------------

    #[test]
    fn array_literals_type_check() {
        clean("func main() { let a = [1, 2, 3]; print(a[0]); }");
        clean("func main() { let m = [[1], [2, 3]]; print(m[1][1]); }");
        clean("func main() { let s = [\"a\", \"b\"]; print(s[1]); }");
        assert!(has("E3028", "func main() { let a = [1, 2.5]; }"));
    }

    #[test]
    fn empty_array_needs_annotation() {
        clean("func main() { let a: [int] = []; print(len(a)); }");
        assert!(has("E3022", "func main() { let a = []; }"));
    }

    #[test]
    fn indexing_rules() {
        assert!(has("E3023", "func main() { let a = [1]; print(a[true]); }"));
        assert!(has("E3024", "func main() { let x = 5; print(x[0]); }"));
        assert!(has("E3024", "func main() { let s = \"hi\"; print(s[0]); }"));
    }

    #[test]
    fn index_assignment_rules() {
        clean("func main() { let a = [1, 2]; a[0] = 9; print(a[0]); }");
        // element type must match
        assert!(has("E3003", "func main() { let a = [1]; a[0] = \"x\"; }"));
        assert!(has("E3023", "func main() { let a = [1]; a[true] = 2; }"));
        assert!(has("E3024", "func main() { let x = 1; x[0] = 2; }"));
    }

    #[test]
    fn arrays_compare_deeply_and_pass_to_functions() {
        clean("func sum(xs: [int]) -> int { return xs[0] + xs[1]; } func main() { print(sum([3, 4])); if [1] == [1] { print(1); } }");
        assert!(has("E3003", "func f(xs: [int]) { } func main() { f([1.5]); }"));
    }

    // -- 0.2.0: casts ---------------------------------------------------------

    #[test]
    fn supported_casts_type_check() {
        clean("func main() { let f = 2 as float; print(f + 0.5); }");
        clean("func main() { let i = 2.9 as int; print(i); }");
        clean("func main() { print(1 as string); print(true as string); }");
    }

    #[test]
    fn unsupported_casts_are_rejected() {
        assert!(has("E3025", "func main() { print(\"5\" as int); }"));
        assert!(has("E3025", "func main() { print(true as int); }"));
        assert!(has("E3025", "func main() { print(1 as int); }"));
    }

    #[test]
    fn cast_fixes_int_float_mismatch() {
        clean("func main() { print((1 as float) + 0.5); }");
        // A cast on the right side alone does not fix a mismatched left side.
        assert!(has("E3011", "func main() { print(1 + (0.5 as float)); }"));
    }

    // -- 0.2.0: natives -------------------------------------------------------

    #[test]
    fn native_functions_type_check() {
        clean("func main() { print(len(\"hello\") + len([1, 2, 3])); }");
        clean("func main() { print(abs(-3)); print(abs(-2.5) + 0.5); }");
        clean("func main() { print(min(1, 2)); print(max(1.5, 2.5)); }");
        assert!(has("E3003", "func main() { print(len(5)); }"));
        assert!(has("E3003", "func main() { print(abs(\"x\")); }"));
        assert!(has("E3003", "func main() { print(min(1, 2.5)); }"));
        assert!(has("E3007", "func main() { print(len()); }"));
        assert!(has("E3007", "func main() { print(min(1)); }"));
    }

    // -- 0.2.0: for loops -----------------------------------------------------

    #[test]
    fn for_over_ranges_type_checks() {
        clean("func main() { for i in 0..5 { print(i); } for i in 0..=5 { print(i); } }");
        assert!(has("E3027", "func main() { for i in 0.5..5 { print(i); } }"));
        assert!(has("E3027", "func main() { for i in 0..\"x\" { print(i); } }"));
    }

    #[test]
    fn for_over_arrays_type_checks() {
        clean("func main() { let xs = [1, 2]; for x in xs { print(x); } }");
        clean("func main() { for s in [\"a\", \"b\"] { print(s); } }");
        assert!(has("E3026", "func main() { for x in 5 { print(x); } }"));
        assert!(has("E3026", "func main() { for x in \"abc\" { print(x); } }"));
    }

    #[test]
    fn for_loop_variable_is_immutable_and_scoped() {
        assert!(has("E3004", "func main() { for i in 0..3 { i = 5; } }"));
        assert!(has("E3001", "func main() { for i in 0..3 { print(1); } print(i); }"));
    }

    #[test]
    fn break_and_continue_work_in_for_loops() {
        clean("func main() { for i in 0..10 { if i == 3 { break; } if i == 1 { continue; } print(i); } }");
    }

    #[test]
    fn for_iterable_may_use_calls() {
        clean("func main() { let a = [1, 2, 3]; for i in 0..len(a) { print(a[i]); } }");
    }

    // -- 0.2.0: modules -------------------------------------------------------

    #[test]
    fn qualified_calls_resolve_across_files() {
        let diags = check_two(
            "import \"lib.kin\" as math;\nfunc main() { print(math.add(1, 2)); }",
            "func add(a: int, b: int) -> int { return a + b; }",
        );
        let errs: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errs.is_empty(), "{:#?}", errs);
    }

    #[test]
    fn unknown_module_and_function_are_errors() {
        let diags = check_two(
            "import \"lib.kin\" as math;\nfunc main() { print(nope.f(1)); }",
            "func f(x: int) { }",
        );
        assert!(diags.iter().any(|d| d.code == "E3020"));

        let diags = check_two(
            "import \"lib.kin\" as math;\nfunc main() { print(math.g(1)); }",
            "func f(x: int) { }",
        );
        assert!(diags.iter().any(|d| d.code == "E3021"));
    }

    #[test]
    fn imported_files_do_not_need_main() {
        let diags = check_two(
            "import \"lib.kin\" as math;\nfunc main() { math.f(1); }",
            "func f(x: int) { }",
        );
        assert!(!diags.iter().any(|d| d.code == "E3008"));
    }

    #[test]
    fn module_function_arguments_are_checked() {
        let diags = check_two(
            "import \"lib.kin\" as math;\nfunc main() { print(math.add(1, \"x\")); }",
            "func add(a: int, b: int) -> int { return a + b; }",
        );
        assert!(diags.iter().any(|d| d.code == "E3003"));
    }

    #[test]
    fn modules_compose_transitively() {
        let files = [
            ("a.kin", "import \"b.kin\" as b;\nfunc fa() -> int { return b.fb(); }"),
            ("b.kin", "func fb() -> int { return 7; }"),
        ];
        let map: std::collections::HashMap<String, String> = files
            .iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect();
        let mut loader = move |path: &str| map.get(path).cloned().ok_or("not found".to_string());
        let p = project::load(
            "main.kin",
            "import \"a.kin\" as a;\nfunc main() { print(a.fa()); }",
            &mut loader,
        )
        .expect("loads");
        let diags = check_project(&p);
        let errs: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errs.is_empty(), "{:#?}", errs);
    }

    #[test]
    fn same_name_in_two_files_is_not_a_duplicate() {
        let diags = check_two(
            "import \"lib.kin\" as l;\nfunc f() -> int { return 1; }\nfunc main() { print(f() + l.f()); }",
            "func f() -> int { return 2; }",
        );
        let errs: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errs.is_empty(), "{:#?}", errs);
    }
}
