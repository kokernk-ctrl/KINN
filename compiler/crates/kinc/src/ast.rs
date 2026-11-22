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
    /// `app "Name" { ... }` declarations (0.4.0 S2) — the dynamic
    /// sibling of `website`. Valid programs carry at most one, in the
    /// entry file; a program declares a `website` **or** an `app`, never
    /// both (E3052).
    pub apps: Vec<AppDecl>,
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

// ---------------------------------------------------------------------------
// Dynamic app module (0.4.0 S2) — docs/DYNAMIC_WEB_ARCHITECTURE.md B.2–B.9
// ---------------------------------------------------------------------------

/// `app "Name" { ... }` — the dynamic application module, sibling of
/// [`WebsiteDecl`]. All words involved (`app`, `navigation`, `state`,
/// `computed`, `component`, `view`, `when`, `each`, `on`, `form`, `field`,
/// `store`, `route`, `theme`, `animation`, `not_found`, …) are contextual
/// keywords (ADR-0019 recipe): outside these grammar positions they stay
/// ordinary identifiers.
///
/// S2 scope note: this type family is **parser/AST only**. Semantic rules
/// (B.11) arrive with S3; lowering to an `AppModel` with S4.
#[derive(Debug, Clone)]
pub struct AppDecl {
    /// Quoted application name.
    pub name: String,
    /// `navigation mpa | spa`. `None` means the default (`mpa`).
    pub navigation: Option<NavigationMode>,
    /// `not_found <Page>` — the SPA 404 page.
    pub not_found: Option<OpenTarget>,
    /// `theme { token "value" }` pairs (ADR-0027 whitelist).
    pub theme: Vec<ThemeToken>,
    /// Shared state fields, merged from one or more `state` blocks.
    pub state: Vec<StateField>,
    /// `computed name = expr` derived values, in source order.
    pub computed: Vec<ComputedDecl>,
    /// `animation { keyframes ... transition ... }` (at most one block).
    pub animation: Option<AnimationDecl>,
    /// `store "name" { key, ... }` persistence blocks.
    pub stores: Vec<StoreDecl>,
    /// `route "path/:param" -> Page` mappings.
    pub routes: Vec<RouteDecl>,
    /// `transition "name"` — named page/route transition (SPA).
    pub transition: Option<AnimRef>,
    /// `page Name { view elements }`.
    pub pages: Vec<AppPageDecl>,
    /// `component Name { prop/state/view }` declarations.
    pub components: Vec<ComponentDecl>,
    pub line: u32,
    pub col: u32,
}

/// `navigation mpa` (default) or `navigation spa` (B.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationMode {
    Mpa,
    Spa,
}

impl NavigationMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            NavigationMode::Mpa => "mpa",
            NavigationMode::Spa => "spa",
        }
    }
}

/// A reference by identifier to a page (navigation targets, routes,
/// `not_found`), with the position of the identifier for diagnostics.
#[derive(Debug, Clone)]
pub struct OpenTarget {
    pub page: String,
    pub line: u32,
    pub col: u32,
}

/// `theme { accent "#2563eb" }` — one theming token assignment. The token
/// whitelist (accent, bg, surface, text, muted, font, radius, max_width)
/// is enforced by the parser (E2034); the value is a string literal.
#[derive(Debug, Clone)]
pub struct ThemeToken {
    pub token: String,
    pub value: String,
    pub line: u32,
    pub col: u32,
}

/// `name: Type = expr` — one shared-state (B.4) or component-state field.
/// Mutable only from event handlers; the checker (S3) restricts the type
/// to scalars and one-dimensional arrays of scalars.
#[derive(Debug, Clone)]
pub struct StateField {
    pub name: String,
    pub ty: Type,
    pub init: Expr,
    pub line: u32,
    pub col: u32,
}

/// `computed name = expr` — a derived value over state/props/functions.
#[derive(Debug, Clone)]
pub struct ComputedDecl {
    pub name: String,
    pub expr: Expr,
    pub line: u32,
    pub col: u32,
}

/// `animation { keyframes "..." {...} transition "..." {...} }` (B.8).
#[derive(Debug, Clone)]
pub struct AnimationDecl {
    pub keyframes: Vec<KeyframesDecl>,
    pub transitions: Vec<TransitionDecl>,
    pub line: u32,
    pub col: u32,
}

/// `keyframes "pop" { from {...} to {...} step 50 {...} duration 200
/// easing ease_out }` — named keyframe animation. Step properties are
/// parsed as `name value` pairs; the whitelist (opacity, x, y, scale,
/// rotate, blur) is a checker rule (E3048, S3), not a parser rule.
#[derive(Debug, Clone)]
pub struct KeyframesDecl {
    pub name: String,
    pub steps: Vec<KeyframeStep>,
    pub duration: Option<Expr>,
    pub delay: Option<Expr>,
    pub easing: Option<Expr>,
    pub line: u32,
    pub col: u32,
}

/// One `from { ... }` / `to { ... }` / `step N { ... }` keyframe step.
#[derive(Debug, Clone)]
pub struct KeyframeStep {
    pub at: StepAt,
    pub props: Vec<AnimProp>,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub enum StepAt {
    From,
    To,
    /// `step <percent-expr>`.
    Step(Expr),
}

/// `opacity 0.5` — one property inside a keyframe step.
#[derive(Debug, Clone)]
pub struct AnimProp {
    pub name: String,
    pub value: Expr,
    pub line: u32,
    pub col: u32,
}

/// `transition "fade" { duration 150 delay 0 easing linear }` — named
/// transition used by `enter`/`exit` and page transitions.
#[derive(Debug, Clone)]
pub struct TransitionDecl {
    pub name: String,
    pub duration: Option<Expr>,
    pub delay: Option<Expr>,
    pub easing: Option<Expr>,
    pub line: u32,
    pub col: u32,
}

/// A reference to a named animation/transition on an element or the app
/// (`enter "pop"`); the name must be a string literal at parse time and
/// resolve to a declared animation at check time (E3047, S3).
#[derive(Debug, Clone)]
pub struct AnimRef {
    pub name: String,
    pub line: u32,
    pub col: u32,
}

/// Animation attributes available on view elements (B.8). `transition`
/// (page-level) lives on [`AppDecl::transition`] instead.
#[derive(Debug, Clone, Default)]
pub struct ElementAnim {
    pub enter: Option<AnimRef>,
    pub exit: Option<AnimRef>,
    pub hover: Option<AnimRef>,
    pub press: Option<AnimRef>,
    pub scroll: Option<AnimRef>,
}

/// `store "tasks" { items, filter }` — named set of persisted state keys
/// (B.7/B.10). Keys must name declared state fields (checker E3059, S3).
#[derive(Debug, Clone)]
pub struct StoreDecl {
    pub name: String,
    pub keys: Vec<String>,
    pub line: u32,
    pub col: u32,
}

/// `route "item/:id" -> ItemDetail` — one route mapping (B.9). Path
/// syntax (literal/`:param` segments) is validated by the checker
/// (E3055, S3); the parser only requires the string and the target.
/// `path_line`/`path_col` point at the opening quote of the path
/// string (checker diagnostics for E3055/E3056 anchor there).
#[derive(Debug, Clone)]
pub struct RouteDecl {
    pub path: String,
    pub target: OpenTarget,
    pub line: u32,
    pub col: u32,
    pub path_line: u32,
    pub path_col: u32,
}

/// `page Home { view elements }` inside an app. App pages hold dynamic
/// view elements ([`ViewElement`]), unlike the 0.3 website
/// [`PageDecl`], whose elements are string-only.
#[derive(Debug, Clone)]
pub struct AppPageDecl {
    pub name: String,
    pub elements: Vec<ViewElement>,
    pub line: u32,
    pub col: u32,
}

/// `component Name { prop... state... view {...} }` (B.3). Components are
/// template declarations — never values; the compiler instantiates each
/// use site with its prop bindings.
#[derive(Debug, Clone)]
pub struct ComponentDecl {
    pub name: String,
    pub props: Vec<PropDecl>,
    pub state: Vec<StateField>,
    pub view: Vec<ViewElement>,
    pub line: u32,
    pub col: u32,
}

/// `prop label: Str` or `prop done: Bool = false` (B.3).
#[derive(Debug, Clone)]
pub struct PropDecl {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
    pub line: u32,
    pub col: u32,
}

/// One element inside a view (page view, component view, `when`/`each`/
/// `form` bodies). Where a 0.3 element takes a string literal, an app
/// element takes a binding expression (a string literal is one).
#[derive(Debug, Clone)]
pub struct ViewElement {
    pub kind: ViewElementKind,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub enum ViewElementKind {
    /// `title <expr>`.
    Title { value: Expr, anim: ElementAnim },
    /// `heading <expr>`.
    Heading { value: Expr, anim: ElementAnim },
    /// `text <expr>`.
    Text { value: Expr, anim: ElementAnim },
    /// `button <expr> [open <Page>]` with optional handler block.
    Button { label: Expr, open: Option<OpenTarget>, anim: ElementAnim, handlers: Vec<OnEvent> },
    /// `link <expr> [open <Page>]` with optional handler block.
    Link { label: Expr, open: Option<OpenTarget>, anim: ElementAnim, handlers: Vec<OnEvent> },
    /// `image <expr> alt <expr>` with optional handler block.
    Image { src: Expr, alt: Expr, anim: ElementAnim, handlers: Vec<OnEvent> },
    /// `divider`.
    Divider { anim: ElementAnim },
    /// `input bind <state-key>` (B.7) with optional handler block.
    Input { bind: OpenTarget, anim: ElementAnim, handlers: Vec<OnEvent> },
    /// `when <cond> { ... } [otherwise { ... }]` — conditional rendering.
    When { cond: Expr, then_branch: Vec<ViewElement>, else_branch: Option<Vec<ViewElement>> },
    /// `each <array-expr> as <name> [key <expr>] { ... }` — keyed lists.
    Each { iter: Expr, var: String, key: Option<Expr>, body: Vec<ViewElement> },
    /// `<Component>(prop: expr, ...)` or `<alias>.<Component>(...)`.
    Component {
        module: Option<String>,
        name: String,
        args: Vec<PropArg>,
        anim: ElementAnim,
        handlers: Vec<OnEvent>,
    },
    /// `form <legend-expr> { fields, elements, on submit { ... } }`.
    Form { legend: Expr, body: Vec<ViewElement>, handlers: Vec<OnEvent> },
    /// `field <state-key> kind <text|number|date|email|password|textarea>
    /// { attrs..., on input/change/focus/blur { ... } }` (B.7).
    Field {
        bind: OpenTarget,
        kind: FieldKind,
        attrs: Vec<FieldAttr>,
        anim: ElementAnim,
        handlers: Vec<OnEvent>,
    },
}

impl ViewElementKind {
    /// Stable machine name (tests, debug output, future tooling).
    pub fn kind_name(&self) -> &'static str {
        match self {
            ViewElementKind::Title { .. } => "title",
            ViewElementKind::Heading { .. } => "heading",
            ViewElementKind::Text { .. } => "text",
            ViewElementKind::Button { .. } => "button",
            ViewElementKind::Link { .. } => "link",
            ViewElementKind::Image { .. } => "image",
            ViewElementKind::Divider { .. } => "divider",
            ViewElementKind::Input { .. } => "input",
            ViewElementKind::When { .. } => "when",
            ViewElementKind::Each { .. } => "each",
            ViewElementKind::Component { .. } => "component instantiation",
            ViewElementKind::Form { .. } => "form",
            ViewElementKind::Field { .. } => "field",
        }
    }
}

/// `prop: value` argument at a component instantiation site.
#[derive(Debug, Clone)]
pub struct PropArg {
    pub name: String,
    pub value: Expr,
    pub line: u32,
    pub col: u32,
}

/// `on <event> [key "..."] { statements }` (B.6). The body is parsed by
/// the ordinary statement parser; handler-only builtins
/// (`event_value`, `event_key`, `form_valid`, …) are checker concerns.
#[derive(Debug, Clone)]
pub struct OnEvent {
    pub event: EventKind,
    /// `key "Escape"` — filter for keyboard events; required for `on key`.
    pub key_filter: Option<String>,
    pub body: Block,
    pub line: u32,
    pub col: u32,
}

/// The complete 0.4.0 event set (B.6); `Key` is the sugar form
/// `on key "Escape"` (≡ `on keydown` with a required key filter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Click,
    Input,
    Change,
    Submit,
    Focus,
    Blur,
    Key,
    Keydown,
    Keyup,
    Pointerdown,
    Pointerup,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::Click => "click",
            EventKind::Input => "input",
            EventKind::Change => "change",
            EventKind::Submit => "submit",
            EventKind::Focus => "focus",
            EventKind::Blur => "blur",
            EventKind::Key => "key",
            EventKind::Keydown => "keydown",
            EventKind::Keyup => "keyup",
            EventKind::Pointerdown => "pointerdown",
            EventKind::Pointerup => "pointerup",
        }
    }

    /// Every accepted `on <word>` (used by the parser's E2031 message).
    pub const ALL: &'static [&'static str] = &[
        "click", "input", "change", "submit", "focus", "blur", "key", "keydown", "keyup",
        "pointerdown", "pointerup",
    ];
}

/// `field <key> kind <word>` input kinds (B.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Number,
    Date,
    Email,
    Password,
    Textarea,
}

impl FieldKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FieldKind::Text => "text",
            FieldKind::Number => "number",
            FieldKind::Date => "date",
            FieldKind::Email => "email",
            FieldKind::Password => "password",
            FieldKind::Textarea => "textarea",
        }
    }

    /// Every accepted `kind <word>` (used by the parser's E2035 message).
    pub const ALL: &'static [&'static str] =
        &["text", "number", "date", "email", "password", "textarea"];
}

/// One attribute inside a `field` block (B.7).
#[derive(Debug, Clone)]
pub enum FieldAttr {
    /// `label <expr>`.
    Label(Expr),
    /// `placeholder <expr>`.
    Placeholder(Expr),
    /// `required`.
    Required,
    /// `min_length <expr>`.
    MinLength(Expr),
    /// `max_length <expr>`.
    MaxLength(Expr),
    /// `min_value <expr>`.
    MinValue(Expr),
    /// `max_value <expr>`.
    MaxValue(Expr),
    /// `pattern <expr>`.
    Pattern(Expr),
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
    for a in &p.apps {
        dump_app(&mut s, a);
    }
    s
}

/// Dump one `app` declaration (`kinn ast`, 0.4.0 S2).
fn dump_app(s: &mut String, a: &AppDecl) {
    s.push_str(&format!("app {:?}  ({}:{})\n", a.name, a.line, a.col));
    if let Some(nav) = a.navigation {
        s.push_str(&format!("  navigation {}\n", nav.as_str()));
    }
    if let Some(nf) = &a.not_found {
        s.push_str(&format!("  not_found {}  ({}:{})\n", nf.page, nf.line, nf.col));
    }
    if let Some(t) = &a.transition {
        s.push_str(&format!("  transition {:?}  ({}:{})\n", t.name, t.line, t.col));
    }
    if !a.theme.is_empty() {
        s.push_str("  theme\n");
        for t in &a.theme {
            s.push_str(&format!("    {} {:?}  ({}:{})\n", t.token, t.value, t.line, t.col));
        }
    }
    if !a.state.is_empty() {
        s.push_str("  state\n");
        for f in &a.state {
            s.push_str(&format!(
                "    {}: {} = {}  ({}:{})\n",
                f.name,
                f.ty.as_str(),
                expr_str(&f.init),
                f.line,
                f.col
            ));
        }
    }
    for c in &a.computed {
        s.push_str(&format!("  computed {} = {}  ({}:{})\n", c.name, expr_str(&c.expr), c.line, c.col));
    }
    if let Some(an) = &a.animation {
        s.push_str(&format!("  animation  ({}:{})\n", an.line, an.col));
        for k in &an.keyframes {
            s.push_str(&format!("    keyframes {:?}", k.name));
            let mut opts = Vec::new();
            if let Some(d) = &k.duration {
                opts.push(format!("duration {}", expr_str(d)));
            }
            if let Some(d) = &k.delay {
                opts.push(format!("delay {}", expr_str(d)));
            }
            if let Some(e) = &k.easing {
                opts.push(format!("easing {}", expr_str(e)));
            }
            if opts.is_empty() {
                s.push('\n');
            } else {
                s.push_str(&format!(" ({})\n", opts.join(", ")));
            }
            for st in &k.steps {
                let at = match &st.at {
                    StepAt::From => "from".to_string(),
                    StepAt::To => "to".to_string(),
                    StepAt::Step(e) => format!("step {}", expr_str(e)),
                };
                let props = st
                    .props
                    .iter()
                    .map(|pr| format!("{} {}", pr.name, expr_str(&pr.value)))
                    .collect::<Vec<_>>()
                    .join(", ");
                s.push_str(&format!("      {} {{ {} }}\n", at, props));
            }
        }
        for t in &an.transitions {
            s.push_str(&format!("    transition {:?}", t.name));
            let mut opts = Vec::new();
            if let Some(d) = &t.duration {
                opts.push(format!("duration {}", expr_str(d)));
            }
            if let Some(d) = &t.delay {
                opts.push(format!("delay {}", expr_str(d)));
            }
            if let Some(e) = &t.easing {
                opts.push(format!("easing {}", expr_str(e)));
            }
            if opts.is_empty() {
                s.push('\n');
            } else {
                s.push_str(&format!(" ({})\n", opts.join(", ")));
            }
        }
    }
    for st in &a.stores {
        s.push_str(&format!("  store {:?} ({})  ({}:{})\n", st.name, st.keys.join(", "), st.line, st.col));
    }
    for r in &a.routes {
        s.push_str(&format!(
            "  route {:?} -> {}  ({}:{})\n",
            r.path, r.target.page, r.line, r.col
        ));
    }
    for pg in &a.pages {
        s.push_str(&format!("  page {}  ({}:{})\n", pg.name, pg.line, pg.col));
        dump_view_elements(s, &pg.elements, 2);
    }
    for c in &a.components {
        s.push_str(&format!("  component {}  ({}:{})\n", c.name, c.line, c.col));
        for pr in &c.props {
            match &pr.default {
                Some(d) => s.push_str(&format!(
                    "    prop {}: {} = {}  ({}:{})\n",
                    pr.name,
                    pr.ty.as_str(),
                    expr_str(d),
                    pr.line,
                    pr.col
                )),
                None => s.push_str(&format!(
                    "    prop {}: {}  ({}:{})\n",
                    pr.name,
                    pr.ty.as_str(),
                    pr.line,
                    pr.col
                )),
            }
        }
        for f in &c.state {
            s.push_str(&format!(
                "    state {}: {} = {}  ({}:{})\n",
                f.name,
                f.ty.as_str(),
                expr_str(&f.init),
                f.line,
                f.col
            ));
        }
        s.push_str("    view\n");
        dump_view_elements(s, &c.view, 3);
    }
}

/// Dump a nested view element list (pages at depth 2, deeper per nesting).
fn dump_view_elements(s: &mut String, elems: &[ViewElement], depth: usize) {
    let ind = "  ".repeat(depth);
    for e in elems {
        let pos = format!("{}:{}", e.line, e.col);
        match &e.kind {
            ViewElementKind::Title { value, anim } => {
                s.push_str(&format!("{}title {}{}  ({})\n", ind, expr_str(value), anim_suffix(anim), pos))
            }
            ViewElementKind::Heading { value, anim } => {
                s.push_str(&format!("{}heading {}{}  ({})\n", ind, expr_str(value), anim_suffix(anim), pos))
            }
            ViewElementKind::Text { value, anim } => {
                s.push_str(&format!("{}text {}{}  ({})\n", ind, expr_str(value), anim_suffix(anim), pos))
            }
            ViewElementKind::Button { label, open, anim, handlers } => {
                let open = open.as_ref().map(|o| format!(" open {}", o.page)).unwrap_or_default();
                s.push_str(&format!(
                    "{}button {}{}{}  ({})\n",
                    ind,
                    expr_str(label),
                    open,
                    anim_suffix(anim),
                    pos
                ));
                dump_handlers(s, handlers, depth);
            }
            ViewElementKind::Link { label, open, anim, handlers } => {
                let open = open.as_ref().map(|o| format!(" open {}", o.page)).unwrap_or_default();
                s.push_str(&format!(
                    "{}link {}{}{}  ({})\n",
                    ind,
                    expr_str(label),
                    open,
                    anim_suffix(anim),
                    pos
                ));
                dump_handlers(s, handlers, depth);
            }
            ViewElementKind::Image { src, alt, anim, handlers } => {
                s.push_str(&format!(
                    "{}image {} alt {}{}  ({})\n",
                    ind,
                    expr_str(src),
                    expr_str(alt),
                    anim_suffix(anim),
                    pos
                ));
                dump_handlers(s, handlers, depth);
            }
            ViewElementKind::Divider { anim } => {
                s.push_str(&format!("{}divider{}  ({})\n", ind, anim_suffix(anim), pos))
            }
            ViewElementKind::Input { bind, anim, handlers } => {
                s.push_str(&format!(
                    "{}input bind {}{}  ({})\n",
                    ind,
                    bind.page,
                    anim_suffix(anim),
                    pos
                ));
                dump_handlers(s, handlers, depth);
            }
            ViewElementKind::When { cond, then_branch, else_branch } => {
                s.push_str(&format!("{}when {}  ({})\n", ind, expr_str(cond), pos));
                dump_view_elements(s, then_branch, depth + 1);
                if let Some(els) = else_branch {
                    s.push_str(&format!("{}otherwise\n", ind));
                    dump_view_elements(s, els, depth + 1);
                }
            }
            ViewElementKind::Each { iter, var, key, body } => {
                let key = key.as_ref().map(|k| format!(" key {}", expr_str(k))).unwrap_or_default();
                s.push_str(&format!("{}each {} as {}{}  ({})\n", ind, expr_str(iter), var, key, pos));
                dump_view_elements(s, body, depth + 1);
            }
            ViewElementKind::Component { module, name, args, anim, handlers } => {
                let args = args
                    .iter()
                    .map(|a| format!("{}: {}", a.name, expr_str(&a.value)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let qualified = module.as_ref().map(|m| format!("{}.", m)).unwrap_or_default();
                s.push_str(&format!(
                    "{}{}{}({}){}  ({})\n",
                    ind,
                    qualified,
                    name,
                    args,
                    anim_suffix(anim),
                    pos
                ));
                dump_handlers(s, handlers, depth);
            }
            ViewElementKind::Form { legend, body, handlers } => {
                s.push_str(&format!("{}form {}  ({})\n", ind, expr_str(legend), pos));
                dump_view_elements(s, body, depth + 1);
                dump_handlers(s, handlers, depth);
            }
            ViewElementKind::Field { bind, kind, attrs, anim, handlers } => {
                s.push_str(&format!(
                    "{}field {} kind {}{}  ({})\n",
                    ind,
                    bind.page,
                    kind.as_str(),
                    anim_suffix(anim),
                    pos
                ));
                let aind = "  ".repeat(depth + 1);
                for attr in attrs {
                    match attr {
                        FieldAttr::Label(e) => s.push_str(&format!("{}label {}\n", aind, expr_str(e))),
                        FieldAttr::Placeholder(e) => {
                            s.push_str(&format!("{}placeholder {}\n", aind, expr_str(e)))
                        }
                        FieldAttr::Required => s.push_str(&format!("{}required\n", aind)),
                        FieldAttr::MinLength(e) => {
                            s.push_str(&format!("{}min_length {}\n", aind, expr_str(e)))
                        }
                        FieldAttr::MaxLength(e) => {
                            s.push_str(&format!("{}max_length {}\n", aind, expr_str(e)))
                        }
                        FieldAttr::MinValue(e) => {
                            s.push_str(&format!("{}min_value {}\n", aind, expr_str(e)))
                        }
                        FieldAttr::MaxValue(e) => {
                            s.push_str(&format!("{}max_value {}\n", aind, expr_str(e)))
                        }
                        FieldAttr::Pattern(e) => s.push_str(&format!("{}pattern {}\n", aind, expr_str(e))),
                    }
                }
                dump_handlers(s, handlers, depth);
            }
        }
    }
}

/// ` enter "pop" exit "fade"` suffix for element dump lines.
fn anim_suffix(a: &ElementAnim) -> String {
    let mut s = String::new();
    for (word, r) in [
        ("enter", &a.enter),
        ("exit", &a.exit),
        ("hover", &a.hover),
        ("press", &a.press),
        ("scroll", &a.scroll),
    ] {
        if let Some(r) = r {
            s.push_str(&format!(" {} {:?}", word, r.name));
        }
    }
    s
}

/// Dump `on ...` handler blocks of one element.
fn dump_handlers(s: &mut String, handlers: &[OnEvent], depth: usize) {
    let ind = "  ".repeat(depth + 1);
    for h in handlers {
        match &h.key_filter {
            Some(k) => s.push_str(&format!("{}on {} {:?}  ({}:{})\n", ind, h.event.as_str(), k, h.line, h.col)),
            None => s.push_str(&format!("{}on {}  ({}:{})\n", ind, h.event.as_str(), h.line, h.col)),
        }
        dump_block(s, &h.body, depth + 2);
    }
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
            let kw = if *mutable { "var" } else { "fin" };
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
