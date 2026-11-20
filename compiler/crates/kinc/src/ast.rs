//! The Kin 0.1 abstract syntax tree, plus a debug tree printer.

/// Programming mode selected with the `*mode = B | S | A;` directive.
///
/// Modes change how much help and technical control the language gives
/// you. In 0.1 the implemented differences are:
///
/// - `B` (Beginner): extra lint notes (for example when a `var` is never
///   reassigned) and full hints in diagnostics.
/// - `S` (Standard): the default; all diagnostics are shown.
/// - `A` (Advanced): terse output — `note`-severity diagnostics are hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    B,
    S,
    A,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::B => "B",
            Mode::S => "S",
            Mode::A => "A",
        }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Mode::S
    }
}

/// A Kin type.
///
/// `Error` is an internal poison type produced by the checker after a
/// previous error so that one mistake does not cascade into many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Error,
}

impl Type {
    pub fn as_str(&self) -> &'static str {
        match self {
            Type::Int => "int",
            Type::Float => "float",
            Type::Bool => "bool",
            Type::Str => "string",
            Type::Void => "void",
            Type::Error => "<error>",
        }
    }
}

/// A parsed program: an optional mode directive plus function declarations.
#[derive(Debug, Clone)]
pub struct Program {
    pub mode: Mode,
    /// `true` when the file contained an explicit `*mode = ...;` line.
    pub mode_declared: bool,
    pub funcs: Vec<Func>,
}

/// A function declaration.
#[derive(Debug, Clone)]
pub struct Func {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Block,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub line: u32,
    pub col: u32,
}

/// A `{ ... }` block with its own scope.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let` or `var` declaration; `ty` is `None` when inferred.
    VarDecl {
        mutable: bool,
        name: String,
        ty: Option<Type>,
        init: Expr,
        line: u32,
        col: u32,
    },
    Assign {
        name: String,
        op: AssignOp,
        value: Expr,
        line: u32,
        col: u32,
    },
    If {
        cond: Expr,
        then_branch: Block,
        /// `else if` chains are represented as a nested `Stmt::If` here.
        else_branch: Option<Box<Stmt>>,
        line: u32,
        col: u32,
    },
    While {
        cond: Expr,
        body: Block,
        line: u32,
        col: u32,
    },
    Return {
        value: Option<Expr>,
        line: u32,
        col: u32,
    },
    Break {
        line: u32,
        col: u32,
    },
    Continue {
        line: u32,
        col: u32,
    },
    /// An expression used as a statement (for example a call).
    Expr {
        expr: Expr,
        line: u32,
        col: u32,
    },
    /// A bare `{ ... }` block used as a statement.
    Block {
        block: Block,
        line: u32,
        col: u32,
    },
}

impl Stmt {
    pub fn line(&self) -> u32 {
        match self {
            Stmt::VarDecl { line, .. }
            | Stmt::Assign { line, .. }
            | Stmt::If { line, .. }
            | Stmt::While { line, .. }
            | Stmt::Return { line, .. }
            | Stmt::Break { line, .. }
            | Stmt::Continue { line, .. }
            | Stmt::Expr { line, .. }
            | Stmt::Block { line, .. } => *line,
        }
    }

    pub fn col(&self) -> u32 {
        match self {
            Stmt::VarDecl { col, .. }
            | Stmt::Assign { col, .. }
            | Stmt::If { col, .. }
            | Stmt::While { col, .. }
            | Stmt::Return { col, .. }
            | Stmt::Break { col, .. }
            | Stmt::Continue { col, .. }
            | Stmt::Expr { col, .. }
            | Stmt::Block { col, .. } => *col,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
}

impl std::fmt::Display for AssignOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssignOp::Set => write!(f, "="),
            AssignOp::Add => write!(f, "+="),
            AssignOp::Sub => write!(f, "-="),
        }
    }
}

/// An expression with the position of its first token.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    StrLit(String),
    Ident(String),
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

impl std::fmt::Display for UnOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnOp::Not => write!(f, "not"),
            UnOp::Neg => write!(f, "-"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
            BinOp::Mod => write!(f, "%"),
            BinOp::Lt => write!(f, "<"),
            BinOp::Gt => write!(f, ">"),
            BinOp::Le => write!(f, "<="),
            BinOp::Ge => write!(f, ">="),
            BinOp::Eq => write!(f, "=="),
            BinOp::Ne => write!(f, "!="),
            BinOp::And => write!(f, "and"),
            BinOp::Or => write!(f, "or"),
        }
    }
}

// ---------------------------------------------------------------------------
// Debug tree printer (`kinn ast`)
// ---------------------------------------------------------------------------

/// Render the program as an indented tree. Expressions are fully
/// parenthesized so precedence is visible.
pub fn dump(p: &Program) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "program (mode: {}{})\n",
        p.mode.as_str(),
        if p.mode_declared { "" } else { ", default" }
    ));
    for f in &p.funcs {
        let params = f
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.ty.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!("func {}({}) -> {}\n", f.name, params, f.ret.as_str()));
        dump_block(&mut s, &f.body, 1);
    }
    s
}

fn dump_block(s: &mut String, b: &Block, depth: usize) {
    for st in &b.stmts {
        dump_stmt(s, st, depth);
    }
}

fn dump_stmt(s: &mut String, st: &Stmt, depth: usize) {
    let ind = "  ".repeat(depth);
    match st {
        Stmt::VarDecl { mutable, name, ty, init, .. } => {
            let kw = if *mutable { "var" } else { "let" };
            let ty = ty.map(|t| format!(": {}", t.as_str())).unwrap_or_default();
            s.push_str(&format!("{}{} {}{} = {}\n", ind, kw, name, ty, expr_str(init)));
        }
        Stmt::Assign { name, op, value, .. } => {
            s.push_str(&format!("{}{} {} {}\n", ind, name, op, expr_str(value)));
        }
        Stmt::If { cond, then_branch, else_branch, .. } => {
            s.push_str(&format!("{}if {}\n", ind, expr_str(cond)));
            dump_block(s, then_branch, depth + 1);
            if let Some(e) = else_branch {
                s.push_str(&format!("{}else\n", ind));
                dump_stmt(s, e, depth + 1);
            }
        }
        Stmt::While { cond, body, .. } => {
            s.push_str(&format!("{}while {}\n", ind, expr_str(cond)));
            dump_block(s, body, depth + 1);
        }
        Stmt::Return { value, .. } => match value {
            Some(v) => s.push_str(&format!("{}return {}\n", ind, expr_str(v))),
            None => s.push_str(&format!("{}return\n", ind)),
        },
        Stmt::Break { .. } => s.push_str(&format!("{}break\n", ind)),
        Stmt::Continue { .. } => s.push_str(&format!("{}continue\n", ind)),
        Stmt::Expr { expr, .. } => s.push_str(&format!("{}{}\n", ind, expr_str(expr))),
        Stmt::Block { block, .. } => dump_block(s, block, depth),
    }
}

fn expr_str(e: &Expr) -> String {
    match &e.kind {
        ExprKind::IntLit(v) => v.to_string(),
        ExprKind::FloatLit(v) => format!("{:?}", v),
        ExprKind::BoolLit(v) => v.to_string(),
        ExprKind::StrLit(v) => format!("{:?}", v),
        ExprKind::Ident(n) => n.clone(),
        ExprKind::Unary { op, expr } => format!("({} {})", op, expr_str(expr)),
        ExprKind::Binary { op, lhs, rhs } => {
            format!("({} {} {})", expr_str(lhs), op, expr_str(rhs))
        }
        ExprKind::Call { name, args } => {
            let args = args.iter().map(expr_str).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args)
        }
    }
}
