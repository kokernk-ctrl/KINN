//! The Kin abstract syntax tree, plus a debug tree printer.
//!
//! 0.2.0 adds: array types and literals, indexing, index assignment,
//! `for` loops over ranges and arrays, explicit casts, imports and
//! qualified (module) calls.

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
/// `[T]` is an array of `T` (arrays use reference semantics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Array(Box<Type>),
    Error,
}

impl Type {
    pub fn as_str(&self) -> String {
        match self {
            Type::Int => "int".to_string(),
            Type::Float => "float".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Str => "string".to_string(),
            Type::Void => "void".to_string(),
            Type::Array(t) => format!("[{}]", t.as_str()),
            Type::Error => "<error>".to_string(),
        }
    }
}

/// A parsed program: an optional mode directive, imports, top-level
/// `const` declarations (0.4.0 S1), function declarations, and optional
/// website declarations (0.3.0).
#[derive(Debug, Clone)]
pub struct Program {
    pub mode: Mode,
    /// `true` when the file contained an explicit `*mode = ...;` line.
    pub mode_declared: bool,
    pub imports: Vec<ImportDecl>,
    /// `const NAME: Type = expr;` top-level declarations (0.4.0 S1), in
    /// source order. Exported implicitly like functions; read qualified
    /// as `module.NAME` (the KIR-level name is `module::NAME`).
    pub consts: Vec<ConstDecl>,
    pub funcs: Vec<Func>,
    /// `website "Name" { page P { ... } }` declarations (0.3.0). Valid
    /// programs carry at most one, in the entry file.
    pub websites: Vec<WebsiteDecl>,
}

/// `const NAME: Type = expr;` — a compile-time constant (0.4.0 S1).
///
/// The initializer must be a compile-time constant expression: literals,
/// array literals, unary/binary operators, and references to consts
/// declared **earlier in the same file**. Calls, casts, indexing and
/// qualified reads are rejected by the checker (E3039).
#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: String,
    /// The declared type is part of the grammar (always present).
    pub ty: Type,
    pub init: Expr,
    pub line: u32,
    pub col: u32,
}

/// `website "Name" { page ... }` — the declarative site module. Lowered
/// to a target-independent [`crate::site::SiteModel`] for the build
/// backends (web, android). The words `website`, `page` and the element
/// names are contextual keywords: outside these declarations they remain
/// ordinary identifiers, so no 0.2 program changes meaning.
#[derive(Debug, Clone)]
pub struct WebsiteDecl {
    /// Quoted site name (checked non-empty, E3033).
    pub name: String,
    pub pages: Vec<PageDecl>,
    pub line: u32,
    pub col: u32,
}

/// `page Home { ... }` inside a website declaration.
#[derive(Debug, Clone)]
pub struct PageDecl {
    /// Page identifier; also the navigation label and the URL slug.
    pub name: String,
    pub elements: Vec<ElementDecl>,
    pub line: u32,
    pub col: u32,
}

/// One declarative element inside a page.
#[derive(Debug, Clone)]
pub struct ElementDecl {
    pub kind: ElementKind,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub enum ElementKind {
    /// `title "..."` — document title and page heading.
    Title(String),
    /// `heading "..."` — section heading.
    Heading(String),
    /// `text "..."` — paragraph.
    Text(String),
    /// `button "Label" open Page` — prominent navigation.
    Button {
        label: String,
        target: String,
        target_line: u32,
        target_col: u32,
    },
    /// `link "Label" open Page` — inline navigation.
    Link {
        label: String,
        target: String,
        target_line: u32,
        target_col: u32,
    },
    /// `image "src" alt "..."` — accessibility-checked image.
    Image {
        src: String,
        alt: String,
        alt_line: u32,
        alt_col: u32,
    },
    /// `divider` — thematic break.
    Divider,
}

impl ElementKind {
    /// Stable machine name (tests, debug output, future tooling).
    pub fn kind_name(&self) -> &'static str {
        match self {
            ElementKind::Title(_) => "title",
            ElementKind::Heading(_) => "heading",
            ElementKind::Text(_) => "text",
            ElementKind::Button { .. } => "button",
            ElementKind::Link { .. } => "link",
            ElementKind::Image { .. } => "image",
            ElementKind::Divider => "divider",
        }
    }
}

/// `import "path.kin" as alias;`
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: String,
    pub alias: String,
    pub line: u32,
    pub col: u32,
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
    /// `base[index] = value;` — element assignment (arrays are references,
    /// so the binding itself may be `let`).
    IndexAssign {
        base: Expr,
        index: Expr,
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
    /// `for var in iterable { ... }` — see [`ForIter`].
    For {
        var: String,
        iter: ForIter,
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

/// What a `for` loop iterates over.
#[derive(Debug, Clone, PartialEq)]
pub enum ForIter {
    /// `start..end` (exclusive) or `start..=end` (inclusive). Both bounds
    /// must be `int`. Ranges are only allowed in `for` headers.
    Range {
        start: Expr,
        end: Expr,
        inclusive: bool,
    },
    /// An expression that must evaluate to an array.
    Expr(Expr),
}

impl Stmt {
    pub fn line(&self) -> u32 {
        match self {
            Stmt::VarDecl { line, .. }
            | Stmt::Assign { line, .. }
            | Stmt::IndexAssign { line, .. }
            | Stmt::If { line, .. }
            | Stmt::While { line, .. }
            | Stmt::For { line, .. }
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
            | Stmt::IndexAssign { col, .. }
            | Stmt::If { col, .. }
            | Stmt::While { col, .. }
            | Stmt::For { col, .. }
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
    /// `[a, b, c]`
    ArrayLit(Vec<Expr>),
    /// `base[index]`
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// `expr as type`
    Cast {
        expr: Box<Expr>,
        ty: Type,
    },
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
    /// `module.function(args)` — a call through an import alias.
    ModCall {
        module: String,
        name: String,
        args: Vec<Expr>,
    },
    /// `module.NAME` — a qualified const read through an import alias
    /// (0.4.0 S1). Folds to a constant at lowering.
    ModConst {
        module: String,
        name: String,
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
    for im in &p.imports {
        s.push_str(&format!("import \"{}\" as {}\n", im.path, im.alias));
    }
    for c in &p.consts {
        s.push_str(&format!(
            "const {}: {} = {}  ({}:{})\n",
            c.name,
            c.ty.as_str(),
            expr_str(&c.init),
            c.line,
            c.col
        ));
    }
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
    for w in &p.websites {
        s.push_str(&format!("website {:?}\n", w.name));
        for pg in &w.pages {
            s.push_str(&format!("  page {}\n", pg.name));
            for e in &pg.elements {
                let pos = format!("{}:{}", e.line, e.col);
                match &e.kind {
                    ElementKind::Title(v) => s.push_str(&format!("    title {:?}  ({})\n", v, pos)),
                    ElementKind::Heading(v) => s.push_str(&format!("    heading {:?}  ({})\n", v, pos)),
                    ElementKind::Text(v) => s.push_str(&format!("    text {:?}  ({})\n", v, pos)),
                    ElementKind::Button { label, target, .. } => {
                        s.push_str(&format!("    button {:?} open {}  ({})\n", label, target, pos))
                    }
                    ElementKind::Link { label, target, .. } => {
                        s.push_str(&format!("    link {:?} open {}  ({})\n", label, target, pos))
                    }
                    ElementKind::Image { src, alt, .. } => {
                        s.push_str(&format!("    image {:?} alt {:?}  ({})\n", src, alt, pos))
                    }
                    ElementKind::Divider => s.push_str(&format!("    divider  ({})\n", pos)),
                }
            }
        }
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
            let ty = ty.as_ref().map(|t| format!(": {}", t.as_str())).unwrap_or_default();
            s.push_str(&format!("{}{} {}{} = {}\n", ind, kw, name, ty, expr_str(init)));
        }
        Stmt::Assign { name, op, value, .. } => {
            s.push_str(&format!("{}{} {} {}\n", ind, name, op, expr_str(value)));
        }
        Stmt::IndexAssign { base, index, value, .. } => {
            s.push_str(&format!("{}{}[{}] = {}\n", ind, expr_str(base), expr_str(index), expr_str(value)));
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
        Stmt::For { var, iter, body, .. } => {
            match iter {
                ForIter::Range { start, end, inclusive } => {
                    let dots = if *inclusive { "..=" } else { ".." };
                    s.push_str(&format!("{}for {} in {} {} {}\n", ind, var, expr_str(start), dots, expr_str(end)));
                }
                ForIter::Expr(e) => {
                    s.push_str(&format!("{}for {} in {}\n", ind, var, expr_str(e)));
                }
            }
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
        ExprKind::ArrayLit(items) => {
            let items = items.iter().map(expr_str).collect::<Vec<_>>().join(", ");
            format!("[{}]", items)
        }
        ExprKind::Index { base, index } => {
            format!("{}[{}]", expr_str(base), expr_str(index))
        }
        ExprKind::Cast { expr, ty } => format!("({} as {})", expr_str(expr), ty.as_str()),
        ExprKind::Unary { op, expr } => format!("({} {})", op, expr_str(expr)),
        ExprKind::Binary { op, lhs, rhs } => {
            format!("({} {} {})", expr_str(lhs), op, expr_str(rhs))
        }
        ExprKind::Call { name, args } => {
            let args = args.iter().map(expr_str).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args)
        }
        ExprKind::ModCall { module, name, args } => {
            let args = args.iter().map(expr_str).collect::<Vec<_>>().join(", ");
            format!("{}.{}({})", module, name, args)
        }
        ExprKind::ModConst { module, name } => format!("{}.{}", module, name),
    }
}
