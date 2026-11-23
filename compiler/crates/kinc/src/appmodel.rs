//! The app model: the target-independent, position-free representation
//! produced by the compiler front end for `app` declarations — the
//! dynamic sibling of [`crate::site::SiteModel`] (C.9 of
//! `docs/DYNAMIC_WEB_ARCHITECTURE.md`).
//!
//! Every build backend consumes a model, never the AST, so all targets
//! see identical, already-validated semantics (ADR-0018). Like the
//! site model, the app model **contains no diagnostic positions**:
//! line/col/len were fully reported by the checker before lowering
//! runs. Source **line numbers** are retained in the lowered
//! expression/statement bodies ([`AppExpr::line`], [`AppStmt::line`]) —
//! the same data `FuncDef.lines` carries — because C.7 maps runtime
//! errors back to `.kin` lines.
//!
//! Determinism is part of the contract: every table is in source
//! order, dependency sets are sorted ([`BindingSite::deps`],
//! [`ComputedModel::deps`]), and lowering the same program twice
//! yields an identical model (tested).
//!
//! S4 boundary (honest): handlers and computed values carry their
//! **KIR function names**, their event association and ordering, and
//! their validated bodies as position-free IR — the representation S5
//! lowers into `FuncDef`s (and translates to JavaScript). Emitting the
//! `FuncDef`s themselves requires the runtime ABI — how the runtime
//! passes state and event builtins into a handler — and that manifest
//! contract is S6's scope (C.2). Nothing here invents runtime
//! behavior.

use crate::ast::{AssignOp, BinOp, FieldKind, NavigationMode, Type};
use std::collections::BTreeSet;

/// A complete dynamic application, lowered from the (checked) `app`
/// declaration. Target-independent: the web dynamic path consumes it
/// from S5 on; the android backend rejects it with E3070 before any
/// generation runs (C.9).
#[derive(Debug, Clone, PartialEq)]
pub struct AppModel {
    /// Application name (non-empty, checker rule E3062).
    pub name: String,
    /// `navigation mpa` (default) or `spa`.
    pub navigation: NavigationMode,
    /// The SPA 404 page name (present only in SPA apps, checker E3057).
    pub not_found: Option<String>,
    /// Theme tokens (token, value) in declaration order.
    pub theme: Vec<(String, String)>,
    /// Shared state fields in declaration order, with validated
    /// initializers.
    pub state: Vec<AppStateField>,
    /// Computed values in declaration order, each with its static
    /// dependency set (state fields, transitively resolved) and its KIR
    /// function name.
    pub computed: Vec<ComputedModel>,
    /// The `animation` block, when the app declares one.
    pub animation: Option<AnimationTable>,
    /// The app-level page/route transition name (`transition "fade"`).
    pub transition: Option<String>,
    /// Routes in declaration order (paths are valid — E3055/E3056 ran).
    pub routes: Vec<RouteModel>,
    /// Stores in declaration order (keys are declared state fields —
    /// E3059 ran).
    pub stores: Vec<StoreModel>,
    /// Pages in declaration order; the first page is the home page.
    pub pages: Vec<AppPageModel>,
    /// Component templates in declaration order (first declaration
    /// wins after E3065 — duplicates never reach lowering).
    pub components: Vec<ComponentModel>,
    /// Every event handler in the deterministic walk order (pages then
    /// components, elements in view order, handlers in source order),
    /// each with its KIR function name and owner association.
    pub handlers: Vec<HandlerModel>,
    /// Every dynamic view binding in the same deterministic walk order,
    /// with its static dependency set (C.5: "binding → set of state
    /// fields, computed at check/lower time").
    pub bindings: Vec<BindingSite>,
}

/// One shared state field: name, declared type and validated
/// initializer.
#[derive(Debug, Clone, PartialEq)]
pub struct AppStateField {
    pub name: String,
    pub ty: Type,
    pub init: AppExpr,
}

/// One computed value: name, static dependency set (the state fields it
/// transitively reads, computed-value references resolved through the
/// acyclic graph E3044 guarantees), the KIR function name its value is
/// computed under, and the validated expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedModel {
    pub name: String,
    pub deps: BTreeSet<String>,
    /// Deterministic KIR function name (`app:computed:<name>`).
    pub kir: String,
    pub expr: AppExpr,
}

/// One route mapping: the validated path, its target page name, and the
/// `:param` names in path order.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteModel {
    pub path: String,
    pub target: String,
    pub params: Vec<String>,
}

/// One persistence store: name and the state keys it persists.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreModel {
    pub name: String,
    pub keys: Vec<String>,
}

/// One page: name and its position-free view tree.
#[derive(Debug, Clone, PartialEq)]
pub struct AppPageModel {
    pub name: String,
    pub elements: Vec<AppElement>,
}

/// One component template: name, validated prop signatures (with
/// optional position-free defaults), component state, and the
/// position-free view tree. A template — never a runtime value (B.3).
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentModel {
    pub name: String,
    pub props: Vec<PropModel>,
    pub state: Vec<AppStateField>,
    pub view: Vec<AppElement>,
}

/// One prop of a component template.
#[derive(Debug, Clone, PartialEq)]
pub struct PropModel {
    pub name: String,
    pub ty: Type,
    pub default: Option<AppExpr>,
}

/// The animation attributes of one element (`enter "pop"` …), position
/// free.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimAttrs {
    pub enter: Option<String>,
    pub exit: Option<String>,
    pub hover: Option<String>,
    pub press: Option<String>,
    pub scroll: Option<String>,
}

/// One view element, position free. Handlers are indices into
/// [`AppModel::handlers`] (the single flat, deterministically ordered
/// table); `bind` targets are the state-key names the checker validated
/// (E3043 ran).
#[derive(Debug, Clone, PartialEq)]
pub enum AppElement {
    Title { value: AppExpr, anim: AnimAttrs },
    Heading { value: AppExpr, anim: AnimAttrs },
    Text { value: AppExpr, anim: AnimAttrs },
    Button {
        label: AppExpr,
        open: Option<String>,
        anim: AnimAttrs,
        handlers: Vec<usize>,
    },
    Link {
        label: AppExpr,
        open: Option<String>,
        anim: AnimAttrs,
        handlers: Vec<usize>,
    },
    Image {
        src: AppExpr,
        alt: AppExpr,
        anim: AnimAttrs,
        handlers: Vec<usize>,
    },
    Divider { anim: AnimAttrs },
    Input { bind: String, anim: AnimAttrs, handlers: Vec<usize> },
    When {
        cond: AppExpr,
        then_branch: Vec<AppElement>,
        otherwise: Option<Vec<AppElement>>,
    },
    Each {
        iter: AppExpr,
        var: String,
        key: Option<AppExpr>,
        body: Vec<AppElement>,
    },
    Component {
        module: Option<String>,
        name: String,
        args: Vec<(String, AppExpr)>,
        anim: AnimAttrs,
        handlers: Vec<usize>,
    },
    Form {
        legend: AppExpr,
        body: Vec<AppElement>,
        handlers: Vec<usize>,
    },
    Field {
        bind: String,
        kind: FieldKind,
        attrs: Vec<FieldAttrModel>,
        anim: AnimAttrs,
        handlers: Vec<usize>,
    },
}

impl AppElement {
    /// Stable machine name (tests and debug output; mirrors
    /// `ast::ViewElementKind::kind_name`).
    pub fn kind_name(&self) -> &'static str {
        match self {
            AppElement::Title { .. } => "title",
            AppElement::Heading { .. } => "heading",
            AppElement::Text { .. } => "text",
            AppElement::Button { .. } => "button",
            AppElement::Link { .. } => "link",
            AppElement::Image { .. } => "image",
            AppElement::Divider { .. } => "divider",
            AppElement::Input { .. } => "input",
            AppElement::When { .. } => "when",
            AppElement::Each { .. } => "each",
            AppElement::Component { .. } => "component instantiation",
            AppElement::Form { .. } => "form",
            AppElement::Field { .. } => "field",
        }
    }
}

/// One field attribute, position free.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldAttrModel {
    Label(AppExpr),
    Placeholder(AppExpr),
    Required,
    MinLength(AppExpr),
    MaxLength(AppExpr),
    MinValue(AppExpr),
    MaxValue(AppExpr),
    Pattern(AppExpr),
}

/// The `animation` block: named keyframes and transitions (values
/// validated by E3048/E3049/E3050 — durations and delays are concrete
/// integer milliseconds, easings are concrete shapes).
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationTable {
    pub keyframes: Vec<KeyframesModel>,
    pub transitions: Vec<TransitionModel>,
}

/// One named keyframes declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframesModel {
    pub name: String,
    pub steps: Vec<KeyframeStepModel>,
    pub duration: Option<i64>,
    pub delay: Option<i64>,
    pub easing: Option<Easing>,
}

/// One `from`/`to`/`step N` keyframe step.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeStepModel {
    pub at: StepAtModel,
    pub props: Vec<(String, AppExpr)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepAtModel {
    From,
    To,
    /// `step <percent>` (semantics of the percent value are S8's CSS
    /// contract).
    Step(AppExpr),
}

/// One named transition declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionModel {
    pub name: String,
    pub duration: Option<i64>,
    pub delay: Option<i64>,
    pub easing: Option<Easing>,
}

/// A validated easing (E3049): a named curve or `cubic(a, b, c, d)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    Cubic([f64; 4]),
}

/// One event handler: its KIR function name, where it hangs (owner +
/// element path + source index), its event association, and its
/// validated body as position-free statements (source lines retained).
/// Association and ordering are the S4 contract (B.6); the runtime ABI
/// is S6's manifest contract.
#[derive(Debug, Clone, PartialEq)]
pub struct HandlerModel {
    /// Deterministic KIR function name: `app:<owner>:<event>:<n>` with
    /// `n` a per-(owner, event) ordinal in walk order.
    pub kir: String,
    /// The page or component name the handler's element lives in.
    pub owner: String,
    /// Element path from the owner root: indices into the view tree,
    /// one index per nesting level.
    pub path: Vec<u32>,
    /// Event word (`click`, `key`, …) — the E3046 comparison unit.
    pub event: String,
    /// `on key "Escape"` filter, when present.
    pub key_filter: Option<String>,
    /// The handler's source index among its element's handlers.
    pub index: u32,
    /// Validated body, position free. `return` never appears (E3045).
    pub body: Vec<AppStmt>,
}

/// One dynamic view binding: where it renders (owner + element path +
/// role) and its static dependency set — the **state field names** this
/// binding transitively depends on (computed-value references resolved
/// through the computed graph; sorted, BTreeSet). Component-scope
/// references (props, component state) are per-instance values, not
/// shared-state dependencies; their reactivity is S6's instance model.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingSite {
    pub owner: String,
    pub path: Vec<u32>,
    pub role: String,
    pub deps: BTreeSet<String>,
}

// ---------------------------------------------------------------------------
// Position-free expression and statement IR
// ---------------------------------------------------------------------------

/// A lowered expression: no columns, no lengths — the source line
/// stays (C.7 runtime-error mapping; the same data `FuncDef.lines`
/// carries). Identifiers are [`AppExprKind::Local`]: classification
/// (state field, computed value, const, each variable, function) is
/// the checker's already-validated knowledge; consumers that need it
/// reclassify by name against the model's tables.
#[derive(Debug, Clone, PartialEq)]
pub struct AppExpr {
    pub kind: AppExprKind,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    /// A name as written (variable, each variable, state field,
    /// computed value, const, function reference).
    Local(String),
    Array(Vec<AppExpr>),
    Index(Box<AppExpr>, Box<AppExpr>),
    Cast {
        expr: Box<AppExpr>,
        ty: Type,
    },
    Neg(Box<AppExpr>),
    Not(Box<AppExpr>),
    Binary(BinOp, Box<AppExpr>, Box<AppExpr>),
    Call {
        name: String,
        args: Vec<AppExpr>,
    },
    ModCall {
        module: String,
        name: String,
        args: Vec<AppExpr>,
    },
    ModConst {
        module: String,
        name: String,
    },
}

/// A lowered statement: no columns — the source line stays.
#[derive(Debug, Clone, PartialEq)]
pub struct AppStmt {
    pub kind: AppStmtKind,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppStmtKind {
    VarDecl {
        mutable: bool,
        name: String,
        ty: Option<Type>,
        init: AppExpr,
    },
    Assign {
        name: String,
        op: AssignOp,
        value: AppExpr,
    },
    IndexAssign {
        base: AppExpr,
        index: AppExpr,
        value: AppExpr,
    },
    If {
        cond: AppExpr,
        then_branch: Vec<AppStmt>,
        /// `else if` chains stay nested statements.
        else_branch: Option<Box<AppStmt>>,
    },
    While {
        cond: AppExpr,
        body: Vec<AppStmt>,
    },
    For {
        var: String,
        iter: ForIterModel,
        body: Vec<AppStmt>,
    },
    /// Present for IR totality; handlers never contain it (E3045).
    Return {
        value: Option<AppExpr>,
    },
    Break,
    Continue,
    Expr(AppExpr),
    Block(Vec<AppStmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForIterModel {
    Range {
        start: AppExpr,
        end: AppExpr,
        inclusive: bool,
    },
    Expr(AppExpr),
}

// ---------------------------------------------------------------------------
// Tests: structural shapes, determinism, the binding graph, handler
// association, and the E3070 capability gate.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::NavigationMode;

    const APP: &str = r##"app "Shop" {
    navigation spa
    not_found Missing
    transition "fade"

    theme { accent "#2563eb" }

    state {
        items: [string] = []
        count: int = 0
        label: string = "hi"
    }

    computed total = len(items) + count
    computed summary = str(total) + " items"

    animation {
        keyframes "pop" {
            from { opacity 0 scale 0.96 }
            to { opacity 1 scale 1 }
            duration 200
            easing ease_out
        }
        transition "fade" { duration 150 easing cubic(0.4, 0.1, 0.2, 1) }
    }

    store "cart" { items, label }

    route "item/:id" -> Item
    route "about" -> About

    component Card {
        prop title: string
        prop highlighted: bool = false
        state open: bool = false
        view {
            text title
            button "Open" { on click { open = not open; } }
        }
    }

    page Home {
        heading "Shop" enter "pop"
        text "Count: " + str(count)
        text summary
        each items as item key item {
            Card(title: item, highlighted: true)
        }
        when count == 0 {
            text "empty"
        }
        otherwise {
            text str(count)
        }
        input bind label
        button "Go" { on click { count = count + 1; } on key "Escape" { count = 0; } }
    }

    page Item {
        text param("id")
        link "Back" open Home
    }

    page About {
        text "about"
    }

    page Missing {
        text "404"
    }
}
"##;

    fn model() -> AppModel {
        let (toks, _) = crate::lexer::tokenize(APP);
        let prog = crate::parser::parse(toks).expect("parses");
        let diags = crate::checker::check(&prog);
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "fixture must check clean: {:#?}", errors);
        crate::lower::app_model(prog.apps.first().expect("app present"))
    }

    #[test]
    fn app_model_is_deterministic() {
        assert_eq!(model(), model());
    }

    #[test]
    fn app_model_tables_keep_source_order() {
        let m = model();
        assert_eq!(m.name, "Shop");
        assert_eq!(m.navigation, NavigationMode::Spa);
        assert_eq!(m.not_found.as_deref(), Some("Missing"));
        assert_eq!(m.transition.as_deref(), Some("fade"));
        assert_eq!(m.theme, vec![("accent".to_string(), "#2563eb".to_string())]);
        assert_eq!(
            m.state.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            vec!["items", "count", "label"]
        );
        assert_eq!(
            m.routes.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
            vec!["item/:id", "about"]
        );
        assert_eq!(m.routes[0].target, "Item");
        assert_eq!(m.routes[0].params, vec!["id"]);
        assert_eq!(m.routes[1].params, Vec::<String>::new());
        assert_eq!(m.stores.len(), 1);
        assert_eq!(m.stores[0].name, "cart");
        assert_eq!(m.stores[0].keys, vec!["items", "label"]);
        assert_eq!(m.pages.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Home", "Item", "About", "Missing"]);
        assert_eq!(m.components.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Card"]);
    }

    #[test]
    fn computed_dependency_sets_are_exact() {
        let m = model();
        assert_eq!(m.computed.len(), 2);
        // Direct state reads.
        let total = &m.computed[0];
        assert_eq!(total.name, "total");
        let total_deps: Vec<_> = total.deps.iter().map(|s| s.as_str()).collect();
        assert_eq!(total_deps, vec!["count", "items"]);
        assert_eq!(total.kir, "app:computed:total");
        // Indirect through another computed value.
        let summary = &m.computed[1];
        assert_eq!(summary.name, "summary");
        let summary_deps: Vec<_> = summary.deps.iter().map(|s| s.as_str()).collect();
        assert_eq!(summary_deps, vec!["count", "items"]);
        assert_eq!(summary.kir, "app:computed:summary");
    }

    #[test]
    fn binding_graph_covers_direct_indirect_and_independent_deps() {
        let m = model();
        // Walk order: page Home's elements in view order. Element
        // indices (0-based): 0 heading (literal — no binding), 1 text,
        // 2 text, 3 each (iter + key + one prop arg in the body),
        // 4 when (cond + otherwise body), 5 input bind, 6 button
        // (literal label — no binding).
        let home: Vec<&BindingSite> = m.bindings.iter().filter(|b| b.owner == "Home").collect();
        assert_eq!(home.len(), 8, "{:#?}", home.iter().map(|b| (&b.role, &b.path, &b.deps)).collect::<Vec<_>>());
        // Direct dependency.
        assert_eq!(home[0].role, "text");
        assert_eq!(home[0].path, vec![1u32]);
        let deps0: Vec<_> = home[0].deps.iter().map(|s| s.as_str()).collect();
        assert_eq!(deps0, vec!["count"]);
        // Indirect: through the computed graph.
        assert_eq!(home[1].role, "text");
        assert_eq!(home[1].path, vec![2u32]);
        let deps1: Vec<_> = home[1].deps.iter().map(|s| s.as_str()).collect();
        assert_eq!(deps1, vec!["count", "items"]);
        // Each iter — direct on items; the key is a binding too (role
        // "each key") with empty deps: the loop variable is not a
        // state field here.
        assert_eq!(home[2].role, "each iter");
        assert_eq!(home[2].path, vec![3u32]);
        let deps2: Vec<_> = home[2].deps.iter().map(|s| s.as_str()).collect();
        assert_eq!(deps2, vec!["items"]);
        assert_eq!(home[3].role, "each key");
        assert_eq!(home[3].path, vec![3u32]);
        assert!(home[3].deps.is_empty(), "{:#?}", home[3].deps);
        // The each body's prop arg reads the loop variable — a binding
        // site feeding the Card template, empty deps (shadowed).
        assert_eq!(home[4].role, "prop title");
        assert_eq!(home[4].path, vec![3u32, 0u32]);
        assert!(home[4].deps.is_empty(), "{:#?}", home[4].deps);
        // When cond — direct.
        assert_eq!(home[5].role, "when cond");
        assert_eq!(home[5].path, vec![4u32]);
        let deps5: Vec<_> = home[5].deps.iter().map(|s| s.as_str()).collect();
        assert_eq!(deps5, vec!["count"]);
        // Otherwise body's text str(count) — nested under the when.
        assert_eq!(home[6].role, "text");
        assert_eq!(home[6].path, vec![4u32, 0u32]);
        let deps6: Vec<_> = home[6].deps.iter().map(|s| s.as_str()).collect();
        assert_eq!(deps6, vec!["count"]);
        // Input bind — dep on itself (a two-way binding renders it).
        assert_eq!(home[7].role, "input bind");
        assert_eq!(home[7].path, vec![5u32]);
        let deps7: Vec<_> = home[7].deps.iter().map(|s| s.as_str()).collect();
        assert_eq!(deps7, vec!["label"]);
    }

    #[test]
    fn each_loop_variable_shadows_state_fields() {
        // A state field named like the loop variable must not become a
        // false dependency of bindings that reference the loop variable.
        let src = r#"app "A" {
    state { item: int = 0, items: [string] = [] }
    page Home {
        each items as item key item { text item }
    }
}
"#;
        let (toks, _) = crate::lexer::tokenize(src);
        let prog = crate::parser::parse(toks).expect("parses");
        assert!(crate::checker::check(&prog).iter().filter(|d| d.is_error()).count() == 0);
        let m = crate::lower::app_model(prog.apps.first().unwrap());
        let text = m.bindings.iter().find(|b| b.path == vec![0u32, 0u32]).expect("text binding");
        assert!(text.deps.is_empty(), "the loop variable shadows the state field: {:#?}", text.deps);
    }

    #[test]
    fn component_scope_reads_are_not_shared_state_deps() {
        // Inside a component view, a read of the component's prop/state
        // is per-instance — not a shared-state dependency.
        let m = model();
        let card: Vec<&BindingSite> = m.bindings.iter().filter(|b| b.owner == "Card").collect();
        assert_eq!(card.len(), 1);
        assert_eq!(card[0].role, "text");
        assert!(card[0].deps.is_empty(), "the prop shadows app state: {:#?}", card[0].deps);
    }

    #[test]
    fn handlers_keep_association_ordering_and_names() {
        let m = model();
        assert_eq!(m.handlers.len(), 3, "{:#?}", m.handlers);
        // Component Card's handler is walked before the page's (the
        // component declarations come after pages? No — the walk is
        // pages first, then components).
        // Page Home, element 9 (button "Go"): two handlers, source order.
        let by_kir = |k: &str| m.handlers.iter().find(|h| h.kir == k).expect(k);
        let go_click = by_kir("app:Home:click:0");
        assert_eq!(go_click.path, vec![6u32]);
        assert_eq!(go_click.event, "click");
        assert_eq!(go_click.index, 0);
        assert!(go_click.key_filter.is_none());
        assert_eq!(go_click.body.len(), 1);
        let go_key = by_kir("app:Home:key:0");
        assert_eq!(go_key.path, vec![6u32]);
        assert_eq!(go_key.event, "key");
        assert_eq!(go_key.key_filter.as_deref(), Some("Escape"));
        assert_eq!(go_key.index, 1);
        let open_click = by_kir("app:Card:click:0");
        assert_eq!(open_click.owner, "Card");
        assert_eq!(open_click.path, vec![1u32]);
        // A second click handler on the same element would be E3046 —
        // the model never sees one; a second element's handler gets
        // ordinal 1.
        assert_eq!(m.handlers.iter().filter(|h| h.event == "click").count(), 2);
    }

    #[test]
    fn bodies_are_position_free_but_line_carrying() {
        let m = model();
        let total = &m.computed[0];
        assert_eq!(total.expr.line, 14);
        // Handler bodies keep their source lines for C.7 mapping.
        let go = m.handlers.iter().find(|h| h.kir == "app:Home:click:0").unwrap();
        assert_eq!(go.body[0].line, 56);
    }

    #[test]
    fn animation_values_arrive_concrete() {
        let m = model();
        let anim = m.animation.as_ref().expect("animation block");
        assert_eq!(anim.keyframes.len(), 1);
        let k = &anim.keyframes[0];
        assert_eq!(k.name, "pop");
        assert_eq!(k.duration, Some(200));
        assert_eq!(k.easing, Some(Easing::EaseOut));
        assert_eq!(anim.transitions[0].duration, Some(150));
        assert_eq!(
            anim.transitions[0].easing,
            Some(Easing::Cubic([0.4, 0.1, 0.2, 1.0]))
        );
    }

    #[test]
    fn state_defaults_are_position_free_values() {
        let m = model();
        assert_eq!(m.state[0].name, "items");
        assert_eq!(m.state[0].ty, crate::ast::Type::Array(Box::new(crate::ast::Type::Str)));
        assert_eq!(m.state[0].init.kind, crate::appmodel::AppExprKind::Array(Vec::new()));
        assert_eq!(m.state[1].init.kind, crate::appmodel::AppExprKind::Int(0));
        assert_eq!(
            m.state[2].init.kind,
            crate::appmodel::AppExprKind::Str("hi".to_string())
        );
    }

    #[test]
    fn component_templates_preserve_validated_props() {
        let m = model();
        let card = &m.components[0];
        assert_eq!(card.props.len(), 2);
        assert_eq!(card.props[0].name, "title");
        assert!(card.props[0].default.is_none());
        assert_eq!(card.props[1].name, "highlighted");
        assert_eq!(
            card.props[1].default.as_ref().unwrap().kind,
            crate::appmodel::AppExprKind::Bool(false)
        );
        assert_eq!(card.state.len(), 1);
        assert_eq!(card.state[0].name, "open");
    }

    #[test]
    fn e3070_the_capability_gate_rejects_apps_and_passes_sites() {
        // A model exists after lowering — the android gate must refuse
        // it before any generation runs (C.9).
        let m = model();
        let err = crate::backend::android_gate(None, Some(&m)).expect_err("apps are rejected");
        assert_eq!(err.code, "E3070");
        assert!(err.message.contains("android"), "{}", err.message);
        // A website program passes; nothing about it changed.
        let (toks, _) = crate::lexer::tokenize("website \"S\" { page Home { text \"x\" } }");
        let prog = crate::parser::parse(toks).unwrap();
        assert!(crate::backend::android_gate(Some(&crate::lower::site_model(prog.websites.first().unwrap())), None).is_ok());
        // Console programs have neither model.
        assert!(crate::backend::android_gate(None, None).is_ok());
    }

    #[test]
    fn cycles_never_reach_lowering() {
        let src = "app \"A\" {\n    computed a = b + 1\n    computed b = a + 1\n    page Home { text \"x\" }\n}\n";
        let (toks, _) = crate::lexer::tokenize(src);
        let prog = crate::parser::parse(toks).unwrap();
        let diags = crate::checker::check(&prog);
        assert!(crate::diagnostics::has_errors(&diags), "E3044 must reject the cycle");
    }
}
