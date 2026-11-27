//! The dynamic web app backend (0.4.0 S6): `AppModel` → manifest +
//! templates + the MPA distribution (C.2/C.4/C.5 of
//! `docs/DYNAMIC_WEB_ARCHITECTURE.md`).
//!
//! S6 scope, honestly gated with E3071: **MPA only** (SPA routing is
//! S7), **no forms/fields** (S9), and the host builtins `param` and
//! `store_*`/`json_of`/`save_json` are rejected (S7/S9). Animation
//! names ride in the templates and the runtime's Anim layer
//! coordinates the `kinn-enter/exit/hover/press/scroll-<name>` classes
//! against `kinnrt.css` (the nine predefined visuals shipped in the
//! dist); compiler-generated CSS for declared names is the S8 step.
//!
//! The dist keeps 0.3's semantic skeleton (landmarks, skip link,
//! `aria-current`) and renders dynamic content at mount **from the
//! manifest templates** — one renderer (kinnrt.js Lifecycle) instead of
//! dual Rust/JS renderers eliminates divergence. Static `website`
//! dists are untouched and byte-identical (backend::web::generate).
//!
//! Determinism: the manifest is emitted with sorted JSON keys, the
//! binding table follows the S4 walk order (pages then components,
//! elements in view order, per element: binding expressions, handlers,
//! nested bodies — literals inlined, C.6), and rebuilding the same app
//! yields byte-identical output (tested).

use crate::appmodel::{AppElement, AppExpr, AppExprKind, AppModel, AnimAttrs, HandlerModel};
use crate::ast::NavigationMode;
use crate::backend::webkir::{self, kind_of_type};
use crate::backend::OutputFile;
use crate::diagnostics::Diagnostic;
use kinrt::kir::ValKind;
use kinrt::{FuncDef, Module};
use std::collections::BTreeMap;

// ===========================================================================
// Deterministic JSON
// ===========================================================================

#[derive(Debug, Clone)]
enum J {
    S(String),
    B(bool),
    N(i64),
    Arr(Vec<J>),
    Obj(BTreeMap<String, J>),
}

impl J {
    fn obj(pairs: Vec<(&str, J)>) -> J {
        let mut m = BTreeMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v);
        }
        J::Obj(m)
    }
    fn write(&self, out: &mut String) {
        match self {
            J::S(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if (c as u32) < 0x20 => {
                            out.push_str(&format!("\\u{:04x}", c as u32))
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            J::B(b) => out.push_str(if *b { "true" } else { "false" }),
            J::N(n) => out.push_str(&n.to_string()),
            J::Arr(items) => {
                out.push('[');
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    it.write(out);
                }
                out.push(']');
            }
            J::Obj(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    J::S(k.clone()).write(out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
    fn to_json(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }
}

// ===========================================================================
// Scope gates (E3071, honest)
// ===========================================================================

fn e3071(message: &str, line: u32, hint: &str) -> Diagnostic {
    let mut d = Diagnostic::error("E3071", message.to_string(), line.max(1), 1, 0);
    d.hint = Some(hint.to_string());
    d
}

fn scope_gates(app: &AppModel) -> Result<(), Diagnostic> {
    if app.navigation == NavigationMode::Spa {
        return Err(e3071(
            "the web runtime does not implement `spa` navigation yet",
            1,
            "SPA routing arrives with S7 (0.4.0); declare `navigation mpa` for the multi-page app dist",
        ));
    }
    for p in &app.pages {
        reject_forms(&p.elements, &p.name)?;
    }
    for c in &app.components {
        reject_forms(&c.view, &c.name)?;
    }
    Ok(())
}

fn reject_forms(elements: &[AppElement], owner: &str) -> Result<(), Diagnostic> {
    for e in elements {
        match e {
            AppElement::Form { .. } | AppElement::Field { .. } => {
                return Err(e3071(
                    &format!(
                        "the web runtime does not implement `form`/`field` elements yet (found in {})",
                        owner
                    ),
                    1,
                    "form validation wiring arrives with S9 (0.4.0); use `input bind` for now",
                ));
            }
            AppElement::When { then_branch, otherwise, .. } => {
                reject_forms(then_branch, owner)?;
                if let Some(o) = otherwise {
                    reject_forms(o, owner)?;
                }
            }
            AppElement::Each { body, .. } => reject_forms(body, owner)?,
            _ => {}
        }
    }
    Ok(())
}

// ===========================================================================
// The view walk: templates + binding functions (S4 order)
// ===========================================================================

/// A literal expression rendered inline (C.6): static strings never
/// become runtime work. Returns the display string.
fn lit_str(e: &AppExpr) -> Option<String> {
    match &e.kind {
        AppExprKind::Int(v) => Some(v.to_string()),
        AppExprKind::Float(v) => Some(kinrt::Value::Float(*v).format()),
        AppExprKind::Bool(v) => Some(v.to_string()),
        AppExprKind::Str(v) => Some(v.clone()),
        _ => None,
    }
}

/// A literal as a JSON value (prop defaults).
fn lit_j(e: &AppExpr) -> Option<J> {
    match &e.kind {
        AppExprKind::Str(v) => Some(J::S(v.clone())),
        AppExprKind::Bool(v) => Some(J::B(*v)),
        AppExprKind::Int(v) => Some(J::N(*v)),
        AppExprKind::Float(_) => Some(J::S(kinrt::Value::Float(
            match &e.kind {
                AppExprKind::Float(f) => *f,
                _ => 0.0,
            },
        )
        .format())),
        _ => None,
    }
}

struct Walk<'a> {
    app: &'a AppModel,
    module: &'a Module,
    #[allow(dead_code)]
    file: &'a str,
    /// Binding functions in walk order (returned to the caller).
    fns: Vec<FuncDef>,
    /// Binding manifest entries, same order.
    meta: Vec<J>,
    /// Alignment cursor into `app.bindings` (the S4 table).
    site: usize,
}

impl<'a> Walk<'a> {
    /// One dynamic slot: literal → inline string; dynamic → `{"b": i}`
    /// plus a lowered `app:bind:<i>` function whose params are the free
    /// names resolvable at evaluation time.
    fn slot(&mut self, e: &AppExpr, shadow: &[(String, ValKind)]) -> J {
        if let Some(l) = lit_str(e) {
            return J::S(l);
        }
        let idx = self.fns.len();
        let kir = format!("app:bind:{}", idx);
        let f = webkir::binding_fn(self.app, self.module, &kir, e, shadow)
            .expect("binding lowers (the checker validated the expression)");
        let mut param_names: Vec<String> = e
            .free_names()
            .into_iter()
            .filter(|n| {
                shadow.iter().any(|(s, _)| s == n)
                    || self.app.state.iter().any(|s| &s.name == n)
                    || self.app.computed.iter().any(|c| &c.name == n)
            })
            .collect();
        param_names.sort();
        let params: Vec<J> = param_names.iter().map(|n| J::S(n.clone())).collect();
        let deps: Vec<J> = self
            .app
            .bindings
            .get(self.site)
            .map(|b| b.deps.iter().map(|d| J::S(d.clone())).collect())
            .unwrap_or_default();
        let tag = f.ret_kind.tag();
        self.fns.push(f);
        self.meta.push(J::obj(vec![
            ("d", J::Arr(deps)),
            ("fn", J::S(kir)),
            ("p", J::Arr(params)),
            ("t", J::S(tag)),
        ]));
        self.site += 1;
        J::obj(vec![("b", J::N(idx as i64))])
    }

    fn anim(a: &AnimAttrs) -> J {
        let mut m = BTreeMap::new();
        if let Some(v) = &a.enter {
            m.insert("enter".to_string(), J::S(v.clone()));
        }
        if let Some(v) = &a.exit {
            m.insert("exit".to_string(), J::S(v.clone()));
        }
        if let Some(v) = &a.hover {
            m.insert("hover".to_string(), J::S(v.clone()));
        }
        if let Some(v) = &a.press {
            m.insert("press".to_string(), J::S(v.clone()));
        }
        if let Some(v) = &a.scroll {
            m.insert("scroll".to_string(), J::S(v.clone()));
        }
        J::Obj(m)
    }

    fn handlers(&self, idxs: &[usize]) -> J {
        J::Arr(idxs.iter().map(|i| J::N(*i as i64)).collect())
    }

    fn view(
        &mut self,
        elements: &[AppElement],
        shadow: &[(String, ValKind)],
    ) -> Vec<J> {
        let mut out = Vec::new();
        for e in elements {
            out.push(self.element(e, shadow));
        }
        out
    }

    fn element(&mut self, e: &AppElement, shadow: &[(String, ValKind)]) -> J {
        match e {
            AppElement::Title { value, anim } => {
                J::obj(vec![("k", J::S("title".into())), ("a", Self::anim(anim)), ("x", self.slot(value, shadow))])
            }
            AppElement::Heading { value, anim } => J::obj(vec![
                ("k", J::S("h".into())),
                ("a", Self::anim(anim)),
                ("x", self.slot(value, shadow)),
            ]),
            AppElement::Text { value, anim } => J::obj(vec![
                ("k", J::S("text".into())),
                ("a", Self::anim(anim)),
                ("x", self.slot(value, shadow)),
            ]),
            AppElement::Button { label, open, anim, handlers } => J::obj(vec![
                ("k", J::S("button".into())),
                ("a", Self::anim(anim)),
                ("x", self.slot(label, shadow)),
                (
                    "o",
                    open.as_ref().map(|p| J::S(slug(p))).unwrap_or(J::B(false)),
                ),
                ("h", self.handlers(handlers)),
            ]),
            AppElement::Link { label, open, anim, handlers } => J::obj(vec![
                ("k", J::S("link".into())),
                ("a", Self::anim(anim)),
                ("x", self.slot(label, shadow)),
                ("o", open.as_ref().map(|p| J::S(slug(p))).unwrap_or(J::B(false))),
                ("h", self.handlers(handlers)),
            ]),
            AppElement::Image { src, alt, anim, handlers } => J::obj(vec![
                ("k", J::S("image".into())),
                ("a", Self::anim(anim)),
                ("s", self.slot(src, shadow)),
                ("l", self.slot(alt, shadow)),
                ("h", self.handlers(handlers)),
            ]),
            AppElement::Divider { anim } => {
                J::obj(vec![("k", J::S("hr".into())), ("a", Self::anim(anim))])
            }
            AppElement::Input { bind, anim, handlers } => {
                // The input's displayed value is a binding on the bound
                // field (one-way display; handlers update the state).
                let idx = self.fns.len();
                let kir = format!("app:bind:{}", idx);
                let expr = AppExpr { kind: AppExprKind::Local(bind.clone()), line: 0 };
                let f = webkir::binding_fn(self.app, self.module, &kir, &expr, &[])
                    .expect("input binding lowers");
                let deps: Vec<J> = self
                    .app
                    .bindings
                    .get(self.site)
                    .map(|b| b.deps.iter().map(|d| J::S(d.clone())).collect())
                    .unwrap_or_default();
                self.fns.push(f);
                self.meta.push(J::obj(vec![
                    ("d", J::Arr(deps)),
                    ("fn", J::S(kir)),
                    ("p", J::Arr(vec![J::S(bind.clone())])),
                    ("t", J::S(String::new())),
                ]));
                self.site += 1;
                J::obj(vec![
                    ("k", J::S("input".into())),
                    ("a", Self::anim(anim)),
                    ("n", J::S(bind.clone())),
                    ("h", self.handlers(handlers)),
                    ("x", J::obj(vec![("b", J::N(idx as i64))])),
                ])
            }
            AppElement::When { cond, then_branch, otherwise } => {
                let c = self.slot(cond, shadow);
                let t = J::Arr(self.view(then_branch, shadow));
                let o = otherwise
                    .as_ref()
                    .map(|els| J::Arr(self.view(els, shadow)))
                    .unwrap_or(J::Arr(vec![]));
                J::obj(vec![("k", J::S("when".into())), ("c", c), ("t", t), ("o", o)])
            }
            AppElement::Each { iter, var, key, body } => {
                // Binding order matches the S4 walk: iter, key, then the
                // body. The key expression is evaluated per item, so the
                // loop variable is in its scope (not the body's alone).
                let elem = match expr_kind_of(iter, self.app, self.module, shadow) {
                    ValKind::Array(e) => *e,
                    _ => ValKind::Any,
                };
                let mut inner = shadow.to_vec();
                inner.push((var.clone(), elem));
                let i = self.slot(iter, shadow);
                let y = key
                    .as_ref()
                    .map(|k| self.slot(k, &inner))
                    .unwrap_or(J::B(false));
                let b = J::Arr(self.view(body, &inner));
                J::obj(vec![
                    ("k", J::S("each".into())),
                    ("i", i),
                    ("v", J::S(var.clone())),
                    ("y", y),
                    ("b", b),
                ])
            }
            AppElement::Component { name, args, anim, handlers, .. } => {
                let comp = self
                    .app
                    .components
                    .iter()
                    .find(|c| &c.name == name)
                    .expect("checker validated the component");
                let mut props = BTreeMap::new();
                for (pname, pexpr) in args {
                    let slot = self.slot(pexpr, shadow);
                    props.insert(pname.clone(), slot);
                }
                // Prop defaults (literals only; the checker validated them).
                let mut defaults = BTreeMap::new();
                for p in &comp.props {
                    if let Some(d) = &p.default {
                        if let Some(l) = lit_j(d) {
                            defaults.insert(p.name.clone(), l);
                        }
                    }
                }
                J::obj(vec![
                    ("k", J::S("comp".into())),
                    ("a", Self::anim(anim)),
                    ("c", J::S(name.clone())),
                    ("p", J::Obj(props)),
                    ("d", J::Obj(defaults)),
                    ("h", self.handlers(handlers)),
                ])
            }
            AppElement::Form { .. } | AppElement::Field { .. } => {
                unreachable!("forms rejected by scope_gates")
            }
        }
    }
}

/// Best-effort static kind of an expression for the each-variable tag
/// (the binding function already carries the exact tag).
fn expr_kind_of(e: &AppExpr, app: &AppModel, module: &Module, shadow: &[(String, ValKind)]) -> ValKind {
    match &e.kind {
        AppExprKind::Array(items) => {
            let mut elem: Option<ValKind> = None;
            for i in items {
                let k = expr_kind_of(i, app, module, shadow);
                elem = Some(match elem {
                    None => k,
                    Some(p) if p == k => p,
                    Some(_) => ValKind::Any,
                });
            }
            ValKind::Array(Box::new(elem.unwrap_or(ValKind::Any)))
        }
        AppExprKind::Local(n) => {
            if let Some((_, k)) = shadow.iter().find(|(s, _)| s == n) {
                k.clone()
            } else if let Some(s) = app.state.iter().find(|s| &s.name == n) {
                kind_of_type(&s.ty)
            } else {
                ValKind::Any
            }
        }
        _ => ValKind::Any,
    }
}

/// URL slug (mirrors the 0.3 static backend).
pub fn slug(name: &str) -> String {
    crate::backend::web::slug(name)
}

// ===========================================================================
// Manifest
// ===========================================================================

/// Build the manifest JSON and, in the same walk, the binding functions
/// (returned through `bindings_out` — the caller appends them to the
/// generated app.js). Deterministic: sorted keys everywhere.
pub fn manifest_json(
    app: &AppModel,
    funcs: &[FuncDef],
    module: &Module,
    file: &str,
    bindings_out: &mut Vec<FuncDef>,
) -> Result<String, Diagnostic> {
    scope_gates(app)?;

    let mut w = Walk { app, module, file, fns: Vec::new(), meta: Vec::new(), site: 0 };

    // Pages (first page = home = index.html).
    let mut pages: Vec<J> = Vec::new();
    for p in &app.pages {
        let view = w.view(&p.elements, &[]);
        // Page title: the first literal `title` element (fallback: name).
        let title = p
            .elements
            .iter()
            .find_map(|e| match e {
                AppElement::Title { value, .. } => lit_str(value),
                _ => None,
            })
            .unwrap_or_else(|| p.name.clone());
        pages.push(J::obj(vec![
            ("name", J::S(p.name.clone())),
            ("slug", J::S(slug(&p.name))),
            ("title", J::S(title)),
            ("view", J::Arr(view)),
        ]));
    }

    // Components (first declaration wins — E3065 ran).
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut components: Vec<J> = Vec::new();
    for c in &app.components {
        if !seen.insert(c.name.clone()) {
            continue;
        }
        let shadow: Vec<(String, ValKind)> = c
            .props
            .iter()
            .map(|p| (p.name.clone(), kind_of_type(&p.ty)))
            .chain(c.state.iter().map(|s| (s.name.clone(), kind_of_type(&s.ty))))
            .collect();
        let view = w.view(&c.view, &shadow);
        components.push(J::obj(vec![
            ("name", J::S(c.name.clone())),
            (
                "props",
                J::Arr(
                    c.props
                        .iter()
                        .map(|p| {
                            J::obj(vec![
                                ("name", J::S(p.name.clone())),
                                ("tag", J::S(kind_of_type(&p.ty).tag())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "state",
                J::Arr(
                    c.state
                        .iter()
                        .map(|s| {
                            J::obj(vec![
                                ("name", J::S(s.name.clone())),
                                ("tag", J::S(kind_of_type(&s.ty).tag())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "init",
                if c.state.is_empty() {
                    J::B(false)
                } else {
                    J::S(format!("app:init:{}", c.name))
                },
            ),
            (
                "ip",
                if c.state.is_empty() {
                    J::Arr(vec![])
                } else {
                    J::Arr(init_prop_params(c).iter().map(|p| J::S(p.clone())).collect())
                },
            ),
            ("view", J::Arr(view)),
        ]));
    }

    // Handlers (the S4 table order).
    let handlers: Vec<J> = app
        .handlers
        .iter()
        .map(|h| handler_entry(app, module, h))
        .collect();

    // Computed values (declaration order).
    let computed: Vec<J> = app
        .computed
        .iter()
        .map(|c| {
            let params = webkir::computed_params(app, c);
            J::obj(vec![
                ("d", J::Arr(c.deps.iter().map(|d| J::S(d.clone())).collect())),
                ("fn", J::S(c.kir.clone())),
                ("name", J::S(c.name.clone())),
                ("p", J::Arr(params.iter().map(|p| J::S(p.clone())).collect())),
            ])
        })
        .collect();

    // Bindings (the walk order == the S4 BindingSite order; the count
    // must match exactly or the walk diverged from the model).
    assert_eq!(
        w.site, app.bindings.len(),
        "webapp: binding walk produced {} sites, the S4 model has {}",
        w.site,
        app.bindings.len()
    );

    let state: Vec<J> = app
        .state
        .iter()
        .map(|s| J::obj(vec![("name", J::S(s.name.clone())), ("tag", J::S(kind_of_type(&s.ty).tag()))]))
        .collect();

    let routes: Vec<J> = app
        .routes
        .iter()
        .map(|r| {
            J::obj(vec![
                ("params", J::Arr(r.params.iter().map(|p| J::S(p.clone())).collect())),
                ("path", J::S(r.path.clone())),
                ("target", J::S(r.target.clone())),
            ])
        })
        .collect();

    let stores: Vec<J> = app
        .stores
        .iter()
        .map(|st| {
            J::obj(vec![
                ("keys", J::Arr(st.keys.iter().map(|k| J::S(k.clone())).collect())),
                ("name", J::S(st.name.clone())),
            ])
        })
        .collect();

    let animations = match &app.animation {
        Some(a) => J::obj(vec![
            (
                "keyframes",
                J::Arr(a.keyframes.iter().map(|k| J::S(k.name.clone())).collect()),
            ),
            (
                "transitions",
                J::Arr(a.transitions.iter().map(|t| J::S(t.name.clone())).collect()),
            ),
        ]),
        None => J::obj(vec![
            ("keyframes", J::Arr(vec![])),
            ("transitions", J::Arr(vec![])),
        ]),
    };

    let manifest = J::obj(vec![
        ("animations", animations),
        ("bindings", J::Arr(w.meta)),
        ("components", J::Arr(components)),
        ("computed", J::Arr(computed)),
        ("file", J::S(file.to_string())),
        ("handlers", J::Arr(handlers)),
        ("name", J::S(app.name.clone())),
        ("navigation", J::S("mpa".to_string())),
        ("pages", J::Arr(pages)),
        ("routes", J::Arr(routes)),
        ("state", J::Arr(state)),
        ("stores", J::Arr(stores)),
        (
            "transition",
            app.transition.as_ref().map(|t| J::S(t.clone())).unwrap_or(J::B(false)),
        ),
        ("version", J::S(kinc_version())),
    ]);

    bindings_out.extend(w.fns);
    let _ = funcs; // the app fns are already lowered by webkir::app_funcs
    Ok(manifest.to_json())
}

fn handler_entry(app: &AppModel, module: &Module, h: &HandlerModel) -> J {
    // Param names: owner-scope names (component state/props when the
    // handler lives in a component) plus app state fields, sorted; then
    // the event builtins (fixed order).
    let mut shadow: Vec<(String, ValKind)> = Vec::new();
    if let Some(c) = app.components.iter().find(|c| c.name == h.owner) {
        shadow = c
            .props
            .iter()
            .map(|p| (p.name.clone(), kind_of_type(&p.ty)))
            .chain(c.state.iter().map(|s| (s.name.clone(), kind_of_type(&s.ty))))
            .collect();
    }
    let params = webkir::handler_params_scoped(app, h, &shadow);
    let builtins: Vec<J> = params
        .iter()
        .filter(|n| is_event_builtin(n))
        .map(|n| J::S(n.clone()))
        .collect();
    let state_params: Vec<J> = params
        .iter()
        .filter(|n| !is_event_builtin(n))
        .map(|n| J::S(n.clone()))
        .collect();
    let _ = module;
    J::obj(vec![
        ("bi", J::Arr(builtins)),
        ("event", J::S(h.event.clone())),
        ("fn", J::S(h.kir.clone())),
        ("key", h.key_filter.as_ref().map(|k| J::S(k.clone())).unwrap_or(J::B(false))),
        ("owner", J::S(h.owner.clone())),
        ("p", J::Arr(state_params)),
    ])
}

/// The props the component's state initializers read (sorted) — the
/// params of `app:init:<Comp>`.
fn init_prop_params(c: &crate::appmodel::ComponentModel) -> Vec<String> {
    let mut free = std::collections::BTreeSet::new();
    for s in &c.state {
        webkir::free_names_expr(&s.init, &mut free);
    }
    let mut names: Vec<String> = free
        .into_iter()
        .filter(|n| c.props.iter().any(|p| &p.name == n))
        .collect();
    names.sort();
    names
}

fn is_event_builtin(n: &str) -> bool {
    matches!(n, "event_value" | "event_key" | "event_x" | "event_y")
}

fn kinc_version() -> String {
    "0.4.0".to_string()
}

// ===========================================================================
// The MPA dist
// ===========================================================================

/// Generate the dynamic web distribution for a validated MPA app.
/// Files: `index.html` + `<slug>.html` per extra page + `styles.css` +
/// `kinnrt.css` (predefined animations) + `app.js` + `kinnrt.js`.
pub fn generate_app(app: &AppModel, module: &Module, file: &str) -> Result<Vec<OutputFile>, Diagnostic> {
    let mut bindings = Vec::new();
    let manifest = manifest_json(app, &[], module, file, &mut bindings)?;
    let app_js = webkir::app_js_full(app, module, file, &manifest, bindings)?;

    let mut files = Vec::new();
    for (i, p) in app.pages.iter().enumerate() {
        let path = if i == 0 {
            "index.html".to_string()
        } else {
            format!("{}.html", slug(&p.name))
        };
        files.push(OutputFile::text(&path, &page_html(app, &p.name, i)));
    }
    files.push(OutputFile::text("styles.css", &app_styles(app)));
    files.push(OutputFile::text("kinnrt.css", KINNRT_CSS));
    files.push(OutputFile::text("app.js", &app_js));
    files.push(OutputFile::text("kinnrt.js", webkir::kinnrt_dist()));
    Ok(files)
}

/// The predefined animation stylesheet (S6 Phase C): one checked-in
/// file, shipped verbatim in every dynamic app dist.
const KINNRT_CSS: &str = include_str!("kinnrt.css");

/// One MPA page: the 0.3 semantic skeleton with an empty `<main>` that
/// the runtime fills at mount (data-kpage tells it which page this is).
fn page_html(app: &AppModel, page: &str, index: usize) -> String {
    let title = app
        .pages
        .get(index)
        .and_then(|p| {
            p.elements.iter().find_map(|e| match e {
                AppElement::Title { value, .. } => lit_str(value),
                _ => None,
            })
        })
        .unwrap_or_else(|| page.to_string());
    let mut nav = String::new();
    for (i, p) in app.pages.iter().enumerate() {
        let href = if i == 0 { "index.html".to_string() } else { format!("{}.html", slug(&p.name)) };
        let current = if p.name == page { " aria-current=\"page\"" } else { "" };
        nav.push_str(&format!(
            "        <a href=\"{}\"{}>{}</a>\n",
            href,
            current,
            crate::backend::web::html_escape(&p.name)
        ));
    }
    format!(
        "<!doctype html>\n\
<html lang=\"en\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"generator\" content=\"KINN 0.4.0 dynamic web backend\">\n\
<title>{} · {}</title>\n\
<link rel=\"stylesheet\" href=\"styles.css\">\n\
<link rel=\"stylesheet\" href=\"kinnrt.css\">\n\
</head>\n\
<body>\n\
<a class=\"kinn-skip\" href=\"#main\">Skip to content</a>\n\
<header class=\"kinn-header\">\n\
      <a class=\"kinn-site\" href=\"index.html\">{}</a>\n\
    <nav aria-label=\"Pages\">\n\
{}    </nav>\n\
</header>\n\
<main class=\"kinn-main\" id=\"main\" data-kpage=\"{}\">\n\
</main>\n\
<footer class=\"kinn-footer\">\n\
    <p>Built with <a href=\"https://github.com/kokernk-ctrl/KINN\">KINN</a></p>\n\
</footer>\n\
<script src=\"kinnrt.js\" defer></script>\n\
<script src=\"app.js\" defer></script>\n\
</body>\n\
</html>\n",
        crate::backend::web::html_escape(&title),
        crate::backend::web::html_escape(&app.name),
        crate::backend::web::html_escape(&app.name),
        nav,
        crate::backend::web::html_escape(page),
    )
}

/// The app stylesheet: the 0.3 base design plus the dynamic-app rules
/// (hidden state for `when`, input styling). Theme tokens (B.2) apply
/// as custom properties.
fn app_styles(app: &AppModel) -> String {
    let mut tokens = String::new();
    for (token, value) in &app.theme {
        tokens.push_str(&format!("  --kinn-{}: {};\n", token, value));
    }
    let token_block = if tokens.is_empty() {
        String::new()
    } else {
        // The theme block re-declares the knobs the base sheet defines.
        let known = [
            ("bg", "--kinn-bg"),
            ("ink", "--kinn-ink"),
            ("muted", "--kinn-muted"),
            ("accent", "--kinn-accent"),
            ("accent_ink", "--kinn-accent-ink"),
            ("line", "--kinn-line"),
            ("card", "--kinn-card"),
            ("radius", "--kinn-radius"),
            ("max_width", "--kinn-max"),
            ("text", "--kinn-ink"),
            ("surface", "--kinn-card"),
        ];
        let mut out = String::from(":root {\n");
        for (t, v) in &app.theme {
            if let Some((_, css)) = known.iter().find(|(k, _)| k == t) {
                out.push_str(&format!("  {}: {};\n", css, v));
            } else {
                out.push_str(&format!("  --kinn-{}: {};\n", t, v));
            }
        }
        out.push_str("}\n\n");
        out
    };
    let _ = tokens;
    format!(
        "/* Generated by KINN 0.4.0 dynamic web backend — the 0.3 base\n   design plus dynamic-app rules (when/list/input). */\n{}{}",
        token_block,
        include_str!("app_base.css")
    )
}
