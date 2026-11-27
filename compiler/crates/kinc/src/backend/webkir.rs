//! The KIR → JavaScript translator (0.4.0 S5, C.3 of
//! `docs/DYNAMIC_WEB_ARCHITECTURE.md`).
//!
//! A second execution carrier without a second semantics: the same KIR
//! the Rust VM executes is translated to plain ES2020 JavaScript that
//! calls the helpers in `kinnrt.js` (the shipped build artifact; the
//! readable source is `kinnrt.src.js`, see `KINNRT_SRC_JS` below).
//! Checked i64 arithmetic, `Value::format` printing parity, deep array
//! equality and the B.10 string helpers are differential-tested against
//! `kinrt::run` — stdout must match byte-for-byte (tests/webkir_diff.rs).
//!
//! Generated shape (one threaded `switch`, one `case` per instruction —
//! exact jump parity for every pattern the lowerings can emit, trivial
//! determinism, and one auditable case per instruction):
//!
//! ```js
//! "use strict";
//! (function (K) {
//!   var F = K.fns;
//!   var C0 = [1, 2];                      // hoisted array consts (one
//!   K.reg(0, "main", "f.kin", function () {   // shared object = Rc refs)
//!     var L = []; L.length = 2;
//!     var S = [];
//!     var ip = 0;
//!     for (;;) {
//!       switch (ip) {
//!       case 0: /*@kinn:f.kin:3*/ S.push(41); ip = 1; break;
//!       case 1: K.out(K.h[10](S.pop(), "i")); ip = 2; break;
//!       case 2: return;
//!       default: throw new K.KinRTError("E4999", "...", 3);
//!       }
//!     }
//!   });
//!   K.call("main", []);
//! })(globalThis.KinnRT);
//! ```
//!
//! JS numbers cannot distinguish Kin int `2` from float `2.0`, so the
//! generator runs a worklist abstract interpretation per function
//! (stack + slot kinds, seeded from the new `FuncDef::param_kinds`)
//! and picks the typed arm statically. Kin has no conditional
//! expressions and statement boundaries keep the value stack empty, so
//! the stack shape at any instruction index is path-independent; slot
//! kinds merge with a join (sibling scopes reuse slots for different
//! variables — P-003), stack shapes assert equal.
//!
//! This module is deterministic: functions, constants and manifest keys
//! are emitted in fixed order and rebuilding the same input yields
//! byte-identical output (tested).

use crate::appmodel::{
    AppExpr, AppExprKind, AppModel, AppStmt, AppStmtKind, ComputedModel, ForIterModel,
    HandlerModel,
};
use crate::ast::{AssignOp, BinOp, NavigationMode, Type};
use crate::diagnostics::Diagnostic;
use kinrt::kir::{CastTarget, Instr, NativeId, ValKind};
use kinrt::{FuncDef, Module, Value};
use std::collections::{BTreeSet, HashMap};

/// The readable runtime source. `kinnrt.src.js` is hand-written and
/// commented; it is NEVER shipped — it exists so the shipped bytes stay
/// reviewable and diffable. The build artifact is regenerated from it
/// with `KINNRT_WRITE=1 cargo test -p kinc kinnrt` (bless-style; the
/// byte-identity test below is the gate).
pub const KINNRT_SRC_JS: &str = include_str!("kinnrt.src.js");

/// The shipped runtime: `kinnrt.js` is a **checked-in build artifact** —
/// the deterministic output of `js_minify(KINNRT_SRC_JS)` (comment/
/// whitespace removal plus scope-safe identifier renaming). This is the
/// exact blob the GitHub API reports and the exact file every web dist
/// embeds; the artifact tests assert byte-identity with the source and
/// the size budget below.
pub const KINNRT_JS: &str = include_str!("kinnrt.js");

/// Hard budget for the delivered runtime (C.6 mobile constraint),
/// asserted against the checked-in artifact `kinnrt.js` by test AND by
/// a dedicated CI job (`.github/workflows/ci.yml: kinnrt-size-gate`).
pub const KINNRT_BUDGET: usize = 16 * 1024;

// ===========================================================================
// Entry points
// ===========================================================================

/// Translate a whole console module: every function plus the `main`
/// auto-run call. `file` labels the source for `//@kinn:` markers and
/// error envelopes.
pub fn js_module(module: &Module, file: &str) -> String {
    let mut js = String::new();
    js.push_str("\"use strict\";\n(function (K) {\nvar F = K.fns;\n");
    // Hoisted array constants, first-use order across the module.
    for (i, v) in array_consts(module).iter().enumerate() {
        js.push_str(&format!("var C{} = {};\n", i, js_value(v)));
    }
    for (i, _f) in module.funcs.iter().enumerate() {
        js.push_str(&gen_func(module, i, file));
    }
    if module.find("main").is_some() {
        js.push_str("K.call(\"main\", []);\n");
    }
    js.push_str("})(globalThis.KinnRT);\n");
    js
}


/// Host builtins the web runtime does not implement yet (E3071 scope
/// gates, S9): the store/JSON set. `param` left this table in S7 —
/// it lowers to `Instr::Native(NativeId::Param)` and reads the live
/// route table in kinnrt.js (K.h[23]).
const HOST_BUILTINS: [(&str, &str, &str); 5] = [
    ("store_save", "S9", "store persistence arrives with S9"),
    ("store_load", "S9", "store persistence arrives with S9"),
    ("store_clear", "S9", "store persistence arrives with S9"),
    ("json_of", "S9", "JSON host builtins arrive with S9"),
    ("save_json", "S9", "JSON host builtins arrive with S9"),
];

/// Reject host-builtins in one expression tree (honest E3071).
fn reject_host_builtins(e: &AppExpr) -> Result<(), Diagnostic> {
    fn walk(e: &AppExpr) -> Option<(&'static str, &'static str, &'static str, u32)> {
        match &e.kind {
            AppExprKind::Call { name, .. } => {
                for (n, step, hint) in HOST_BUILTINS {
                    if name == n {
                        return Some((n, step, hint, e.line));
                    }
                }
                None
            }
            AppExprKind::Array(items) => items.iter().find_map(walk),
            AppExprKind::Index(b, i) => walk(b).or_else(|| walk(i)),
            AppExprKind::Cast { expr, .. } => walk(expr),
            AppExprKind::Neg(i) | AppExprKind::Not(i) => walk(i),
            AppExprKind::Binary(_, l, r) => walk(l).or_else(|| walk(r)),
            AppExprKind::ModCall { args, .. } => args.iter().find_map(walk),
            _ => None,
        }
    }
    if let Some((n, step, hint, line)) = walk(e) {
        return Err(e3071(
            &format!(
                "the web runtime does not implement the `{}` builtin yet ({} scope)",
                n, step
            ),
            line,
            hint,
        ));
    }
    Ok(())
}

/// Lower one binding expression into `app:bind:<i>`: params are the free
/// names resolvable at evaluation time (state fields, computed values,
/// each-variables, component props/state), sorted; the body returns the
/// expression value. The caller (webapp) supplies the shadow kinds.
pub fn binding_fn(
    app: &AppModel,
    module: &Module,
    kir_name: &str,
    expr: &AppExpr,
    shadow: &[(String, ValKind)],
) -> Result<FuncDef, Diagnostic> {
    reject_host_builtins(expr)?;
    let ctx = AppCtx::new(app, module);
    let free = expr.free_names();
    let mut params: Vec<(String, ValKind)> = Vec::new();
    for n in &free {
        let shadow_kind = shadow.iter().find(|(s, _)| s == n).map(|(_, k)| k.clone());
        if let Some(k) = shadow_kind {
            params.push((n.clone(), k));
        } else if let Some(k) = ctx.state_kind(n) {
            params.push((n.clone(), k));
        } else if ctx.is_computed(n) {
            params.push((n.clone(), ctx.computed_kind(n)));
        }
        // Consts fold; anything else cannot appear (the checker resolved it).
    }
    params.sort_by(|a, b| a.0.cmp(&b.0));
    let mut lo = FnAppLower::new(&ctx, kir_name, params.len() as u16);
    lo.bind_params(&params);
    let kind = lo.expr_kind(expr);
    lo.lower_expr(expr);
    lo.emit(Instr::Return, expr.line);
    Ok(lo.finish(params.iter().map(|(_, k)| k.clone()).collect(), kind))
}

/// Reject host-builtins across a statement tree.
pub fn reject_host_builtins_stmt(st: &AppStmt) -> Result<(), Diagnostic> {
    fn walk_stmt(st: &AppStmt) -> Result<(), Diagnostic> {
        let exprs: Vec<&AppExpr> = match &st.kind {
            AppStmtKind::VarDecl { init, .. } => vec![init],
            AppStmtKind::Assign { value, .. } => vec![value],
            AppStmtKind::IndexAssign { base, index, value } => vec![base, index, value],
            AppStmtKind::If { cond, then_branch, else_branch } => {
                reject_host_builtins(cond)?;
                for t in then_branch {
                    walk_stmt(t)?;
                }
                if let Some(e) = else_branch {
                    walk_stmt(e)?;
                }
                return Ok(());
            }
            AppStmtKind::While { cond, body } => {
                reject_host_builtins(cond)?;
                for b in body {
                    walk_stmt(b)?;
                }
                return Ok(());
            }
            AppStmtKind::For { iter, body, .. } => {
                match iter {
                    ForIterModel::Range { start, end, .. } => {
                        reject_host_builtins(start)?;
                        reject_host_builtins(end)?;
                    }
                    ForIterModel::Expr(e) => reject_host_builtins(e)?,
                }
                for b in body {
                    walk_stmt(b)?;
                }
                return Ok(());
            }
            AppStmtKind::Return { value } => value.as_ref().map(|v| vec![v]).unwrap_or_default(),
            AppStmtKind::Expr(e) => vec![e],
            AppStmtKind::Block(b) => {
                for s in b {
                    walk_stmt(s)?;
                }
                return Ok(());
            }
            AppStmtKind::Break | AppStmtKind::Continue => Vec::new(),
        };
        for e in exprs {
            reject_host_builtins(e)?;
        }
        Ok(())
    }
    walk_stmt(st)
}

/// The handler ABI param names: owner-scope names (component state/props
/// when the handler lives in a component view) plus app state fields,
/// sorted; then the event builtins it uses (fixed order).
pub fn handler_params_scoped(
    app: &AppModel,
    h: &HandlerModel,
    shadow: &[(String, ValKind)],
) -> Vec<String> {
    let free = free_names_stmts(&h.body);
    let mut names: Vec<String> = free
        .iter()
        .filter(|n| {
            shadow.iter().any(|(s, _)| s == *n) || app.state.iter().any(|s| s.name.as_str() == n.as_str())
        })
        .cloned()
        .collect();
    names.sort();
    names.dedup();
    let mut out = names;
    for (n, _, _) in event_builtins() {
        if free.contains(n) {
            out.push(n.to_string());
        }
    }
    out
}

/// The computed ABI param names (free names that are state fields or
/// other computeds), sorted.
pub fn computed_params(app: &AppModel, c: &ComputedModel) -> Vec<String> {
    let mut params: Vec<String> = c
        .expr
        .free_names()
        .into_iter()
        .filter(|n| {
            app.state.iter().any(|s| &s.name == n) || app.computed.iter().any(|c| &c.name == n)
        })
        .collect();
    params.sort();
    params
}

/// The generated `app.js`: every function (user + app + binding) and the
/// manifest, ending in `K.mount`. `bindings` are the walk-order binding
/// functions webapp produced alongside the manifest.
pub fn app_js_full(
    app: &AppModel,
    module: &Module,
    file: &str,
    manifest: &str,
    bindings: Vec<FuncDef>,
) -> Result<String, Diagnostic> {
    let funcs = app_funcs(app, module, file)?;
    let mut js = String::new();
    js.push_str("\"use strict\";\n(function (K) {\nvar F = K.fns;\n");
    // Hoisted array constants from the FULL function set (user + app +
    // bindings): every read of a const array shares one object (Rc parity).
    let mut combined: Vec<FuncDef> = module.funcs.clone();
    combined.extend(funcs);
    combined.extend(bindings);
    let combined_module = Module { funcs: combined, consts: module.consts.clone() };
    for (i, v) in array_consts(&combined_module).iter().enumerate() {
        js.push_str(&format!("var C{} = {};\n", i, js_value(v)));
    }
    for i in 0..combined_module.funcs.len() {
        js.push_str(&gen_func(&combined_module, i, file));
    }
    // The SPA glue (S7): router-owned clicks + the title/aria-current
    // truthing on every page swap. Generated per SPA app (app.js has no
    // size budget; kinnrt.js keeps only the router core behind the
    // public K.navigate/K.onpage surface). Registered BEFORE mount so
    // the first page swap already runs through it. MPA apps emit
    // nothing — their navigation is plain files.
    if app.navigation == NavigationMode::Spa {
        js.push_str(&spa_glue(app));
    }
    js.push_str(&format!("K.mount({});\n", manifest));
    js.push_str("})(globalThis.KinnRT);\n");
    Ok(js)
}

/// The generated SPA glue (S7): the document's URL adapter + the
/// router-owned click interceptor + the page-swap truthing. kinnrt.js
/// owns route matching, `param()`, and the page swap; this block (in
/// app.js, which carries the per-app code) owns everything
/// document-specific: the history API (pushState/replaceState +
/// popstate) with the hash fallback for non-http documents (file://,
/// about:srcdoc, blob:), the base-path analysis (the document's
/// directory, or the shortest pathname prefix whose remainder matches
/// a route — what a server-rewritten deep link looks like), the
/// sessionStorage route save/restore, and the `aria-current`/title
/// truthing after every swap. Deterministic: a fixed block plus the
/// JSON-escaped app name literal.
fn spa_glue(app: &AppModel) -> String {
    let name = crate::backend::web::html_escape(&app.name);
    format!(
        "/* S7 SPA glue (generated): the URL adapter. kinnrt.js owns\n\
   matching, param() and the page swap; this block owns the URL. */\n\
(function () {{\n\
  var W = window, D = document;\n\
  var HASH = W.location.protocol !== \"http:\" && W.location.protocol !== \"https:\";\n\
  var PN = W.location.pathname;\n\
  var BASE = \"/\"; /* resolved at boot (below), once M exists */\n\
  /* Base path: the document's directory, or the shortest prefix whose\n\
     remainder matches a route (a server-rewritten deep link). */\n\
  function baseOf() {{\n\
    if (HASH) return \"/\";\n\
    if (PN.endsWith(\"index.html\")) {{\n\
      return PN.slice(0, PN.length - 10);\n\
    }}\n\
    var base = PN.slice(0, PN.lastIndexOf(\"/\") + 1);\n\
    var cuts = [1], q = PN.indexOf(\"/\", 1);\n\
    while (q !== -1) {{ cuts.push(q + 1); q = PN.indexOf(\"/\", q + 1); }}\n\
    for (var k = 0; k < cuts.length; k++) {{\n\
      if (K.match(PN.slice(cuts[k]))) {{ base = PN.slice(0, cuts[k]); break; }}\n\
    }}\n\
    return base;\n\
  }}\n\
  function curPath() {{\n\
    if (HASH) {{\n\
      var h = W.location.hash;\n\
      return h.length > 1 ? h.replace(/^#\\/?/, \"\") : \"\";\n\
    }}\n\
    /* The document's own `index.html` tail is the root route, not a\n\
       path segment. */\n\
    var p = W.location.pathname;\n\
    if (p.endsWith(\"index.html\")) p = p.slice(0, p.length - 10);\n\
    return p.length > BASE.length ? p.slice(BASE.length) : \"\";\n\
  }}\n\
  /* URL writes + the route save (the runtime calls this on every\n\
     navigation: mode 1 = push, 2 = replace, 0 = the URL is already\n\
     right). */\n\
  K.onurl = function (path, mode) {{\n\
    try {{ W.sessionStorage.setItem(\"kinn:r:NAME\", path); }} catch (e) {{}}\n\
    if (HASH) {{\n\
      if (mode && W.location.hash !== \"/\" + path) W.location.hash = \"/\" + path;\n\
    }} else if (mode === 1) {{\n\
      W.history.pushState(null, \"\", BASE + path);\n\
    }} else if (mode === 2) {{\n\
      W.history.replaceState(null, \"\", BASE + path);\n\
    }}\n\
  }};\n\
  /* Boot: back/forward listeners, then the first route — a deep link\n\
     wins over the saved route; the saved route restores a reload. */\n\
  K.onboot = function () {{\n\
    BASE = baseOf(); /* K.match needs the manifest — M exists now */\n\
    W.addEventListener(\"popstate\", function () {{ K.route(curPath(), 0); }});\n\
    if (HASH) W.addEventListener(\"hashchange\", function () {{ K.route(curPath(), 0); }});\n\
    var saved = \"\";\n\
    try {{ saved = W.sessionStorage.getItem(\"kinn:r:NAME\") || \"\"; }} catch (e) {{}}\n\
    var live = curPath();\n\
    K.route(live || saved, live ? 0 : 2);\n\
  }};\n\
  /* Router-owned clicks: [data-open] targets navigate through the\n\
     router instead of their href (kinnrt's own click dispatch runs\n\
     first, so user handlers write state before the view re-mounts). */\n\
  D.addEventListener(\"click\", function (ev) {{\n\
    var t = ev.target;\n\
    t = t && t.closest ? t.closest(\"[data-open]\") : null;\n\
    if (!t) return;\n\
    var p = t.getAttribute(\"data-open\");\n\
    if (p === null) return;\n\
    ev.preventDefault();\n\
    K.route(p, 1);\n\
  }});\n\
  /* Page-swap truthing: the document title and the shell nav's\n\
     aria-current. */\n\
  K.onpage = function (page) {{\n\
    D.title = page.title + \" · \" + \"NAME\";\n\
    var ls = D.querySelectorAll(\"[data-nav]\");\n\
    for (var i = 0; i < ls.length; i++) {{\n\
      if (ls[i].getAttribute(\"data-nav\") === page.name) ls[i].setAttribute(\"aria-current\", \"page\");\n\
      else ls[i].removeAttribute(\"aria-current\");\n\
    }}\n\
  }};\n\
}})();\n\
",
    )
    .replace("kinn:r:NAME", &format!("kinn:r:{}", name))
    .replace(" + \"NAME\"", &format!(" + \"{}\"", name))
}

/// The app program pieces S5 emits: the app-side KIR functions (`app:init`,
/// every computed, every handler, component state inits) ready to append
/// to the user module, in deterministic order. The manifest emission (S6)
/// consumes the same functions. `file` is the app source label.
pub fn app_funcs(app: &AppModel, module: &Module, file: &str) -> Result<Vec<FuncDef>, Diagnostic> {
    let ctx = AppCtx::new(app, module);
    let mut out = Vec::new();
    out.push(ctx.init_func()?);
    for c in &app.computed {
        out.push(ctx.computed_func(c)?);
    }
    for h in &app.handlers {
        out.push(ctx.handler_func(h)?);
    }
    for c in &app.components {
        if !c.state.is_empty() {
            out.push(ctx.component_init_func(c, file)?);
        }
    }
    Ok(out)
}

// ===========================================================================
// Static kinds
// ===========================================================================

/// Map an ast type to the runtime kind (shared with lower.rs's private
/// copy through behavior — kept local so webkir stays self-contained).
pub fn kind_of_type(t: &Type) -> ValKind {
    match t {
        Type::Int => ValKind::Int,
        Type::Float => ValKind::Float,
        Type::Bool => ValKind::Bool,
        Type::Str => ValKind::Str,
        Type::Void => ValKind::Unit,
        Type::Array(e) => ValKind::Array(Box::new(kind_of_type(e))),
        Type::Error => ValKind::Any,
    }
}

fn kind_of_value(v: &Value) -> ValKind {
    match v {
        Value::Int(_) => ValKind::Int,
        Value::Float(_) => ValKind::Float,
        Value::Bool(_) => ValKind::Bool,
        Value::Str(_) => ValKind::Str,
        Value::Array(a) => {
            let mut elem: Option<ValKind> = None;
            for e in a.borrow().iter() {
                let k = kind_of_value(e);
                elem = Some(match elem {
                    None => k,
                    Some(prev) if prev == k => prev,
                    Some(_) => ValKind::Any,
                });
            }
            ValKind::Array(Box::new(elem.unwrap_or(ValKind::Any)))
        }
        Value::Unit => ValKind::Unit,
    }
}

fn join(a: &ValKind, b: &ValKind) -> ValKind {
    if a == b {
        a.clone()
    } else if *a == ValKind::Unit {
        b.clone()
    } else if *b == ValKind::Unit {
        a.clone()
    } else if let (ValKind::Array(x), ValKind::Array(y)) = (a, b) {
        // Two arrays join to an array of the joined element kind: the
        // (Array, Array) concat arm is element-agnostic, so the join
        // keeps the arithmetic arm precise while widening the element.
        ValKind::Array(Box::new(join(x, y)))
    } else {
        ValKind::Any
    }
}

fn elem_kind(k: &ValKind) -> ValKind {
    match k {
        ValKind::Array(e) => (**e).clone(),
        _ => ValKind::Any,
    }
}


// ===========================================================================
// Deterministic minifier: the generator of the shipped kinnrt.js
// artifact (16 KB budget, C.6)
// ===========================================================================

/// The bytes every web dist ships and every browser loads: the
/// checked-in `kinnrt.js` artifact. Kept as a function so consumers have
/// one entry point; `kinnrt.js == js_minify(kinnrt.src.js)` byte-for-byte
/// (asserted by `dist_tests::kinnrt_artifact_byte_identity`), so the
/// minifier is exercised on every CI run without being on the hot path.
pub fn kinnrt_dist() -> &'static str {
    KINNRT_JS
}

/// Regenerate the checked-in artifact from the readable source
/// (bless-style). Run `KINNRT_WRITE=1 cargo test -p kinc kinnrt` from the
/// repository root after editing `kinnrt.src.js`; plain `cargo test`
/// (what CI does) only asserts equality and fails on drift.
fn kinnrt_write_artifact(bytes: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/backend/kinnrt.js");
    if let Err(e) = std::fs::write(&path, bytes) {
        panic!("KINNRT_WRITE: cannot write {}: {}", path.display(), e);
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Str(String),
    Regex(String),
    Ident(String),
    Num(String),
    Punct(String),
    Space,
}

fn js_tokenize(src: &str) -> Vec<Tok> {
    let b: Vec<char> = src.chars().collect();
    let n = b.len();
    let mut toks: Vec<Tok> = Vec::new();
    let mut i = 0;
    let mut buf: Vec<char> = Vec::new();
    let mut last_meaning: Option<String> = None;
    fn is_ident_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || c == '$'
    }
    fn flush(toks: &mut Vec<Tok>, buf: &mut Vec<char>) {
        if buf.is_empty() {
            return;
        }
        let w: String = buf.iter().collect();
        let kind = if is_ident_char(buf[0]) && !buf[0].is_ascii_digit() {
            Tok::Ident(w)
        } else {
            Tok::Num(w)
        };
        toks.push(kind);
        buf.clear();
    }
    while i < n {
        let c = b[i];
        // string literals
        if c == '"' || c == '\'' || c == '`' {
            flush(&mut toks, &mut buf);
            let quote = c;
            let mut s = vec![c];
            i += 1;
            let mut esc = false;
            while i < n {
                let d = b[i];
                s.push(d);
                if esc {
                    esc = false;
                } else if d == '\\' {
                    esc = true;
                } else if d == quote {
                    break;
                }
                i += 1;
            }
            i += 1;
            let text: String = s.iter().collect();
            toks.push(Tok::Str(text));
            last_meaning = Some("STR".to_string());
            continue;
        }
        // comments
        if c == '/' && i + 1 < n && b[i + 1] == '/' {
            flush(&mut toks, &mut buf);
            while i < n && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < n && b[i + 1] == '*' {
            flush(&mut toks, &mut buf);
            i += 2;
            while i + 1 < n && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        // regex literal: / after an operator-ish token
        if c == '/'
            && matches!(
                last_meaning.as_deref(),
                Some("=") | Some("(") | Some(",") | Some("[") | Some("!") | Some("?")
                    | Some(":") | Some(";") | Some("&&") | Some("||") | Some("return")
            )
        {
            flush(&mut toks, &mut buf);
            let mut j = i + 1;
            let mut in_cls = false;
            while j < n {
                let d = b[j];
                if d == '\\' {
                    j += 2;
                    continue;
                }
                if d == '[' {
                    in_cls = true;
                } else if d == ']' {
                    in_cls = false;
                } else if d == '/' && !in_cls {
                    break;
                } else if d == '\n' {
                    break;
                }
                j += 1;
            }
            let mut k = j + 1;
            while k < n && b[k].is_ascii_alphabetic() {
                k += 1;
            }
            let text: String = b[i..k].iter().collect();
            toks.push(Tok::Regex(text));
            last_meaning = Some("REGEX".to_string());
            i = k;
            continue;
        }
        // whitespace keeps the previous context
        if c.is_whitespace() {
            flush(&mut toks, &mut buf);
            toks.push(Tok::Space);
            i += 1;
            continue;
        }
        // identifiers / numbers end operator context (division follows)
        if is_ident_char(c) {
            buf.push(c);
            last_meaning = Some("ID".to_string());
            i += 1;
            continue;
        }
        flush(&mut toks, &mut buf);
        // punctuation (longest first)
        let three: String = b[i..(i + 3).min(n)].iter().collect();
        // `...` (spread) must be ONE token: three separate dots would
        // make the emitter treat the following identifier as a property
        // (`.s` member access) and skip its rename.
        if three == "..." {
            toks.push(Tok::Punct(three.clone()));
            last_meaning = Some(three);
            i += 3;
            continue;
        }
        if three == "===" || three == "!==" || three == "<<=" || three == ">>=" {
            toks.push(Tok::Punct(three));
            last_meaning = Some("===".to_string());
            i += 3;
            continue;
        }
        let two: String = b[i..(i + 2).min(n)].iter().collect();
        if [
            "==", "!=", "&&", "||", "<=", ">=", "+=", "-=", "*=", "/=", "%=", "=>", "++", "--",
            "<<", ">>",
        ]
        .contains(&two.as_str())
        {
            toks.push(Tok::Punct(two.clone()));
            last_meaning = Some(two);
            i += 2;
            continue;
        }
        let one = c.to_string();
        toks.push(Tok::Punct(one.clone()));
        last_meaning = Some(one);
        i += 1;
    }
    flush(&mut toks, &mut buf);
    toks
}

const JS_KEYWORDS: [&str; 33] = [
    "var", "function", "return", "if", "else", "for", "while", "do", "switch", "case", "default",
    "break", "continue", "new", "typeof", "delete", "in", "of", "try", "catch", "finally",
    "throw", "this", "true", "false", "null", "undefined", "instanceof", "void", "const", "let",
    "class", "extends",
];

/// Collect declared names (var locals, function names, parameters).
fn js_decls(toks: &[Tok]) -> Vec<String> {
    let mut decls: Vec<String> = Vec::new();
    let compact: Vec<&Tok> = toks.iter().filter(|t| !matches!(t, Tok::Space)).collect();
    let n = compact.len();
    let mut i = 0;
    let text = |t: &Tok| -> String {
        match t {
            Tok::Ident(s) | Tok::Num(s) | Tok::Punct(s) | Tok::Str(s) | Tok::Regex(s) => s.clone(),
            Tok::Space => String::new(),
        }
    };
    while i < n {
        let t = compact[i];
        if let Tok::Ident(w) = t {
            if w == "var" {
                let mut j = i + 1;
                while j < n {
                    if let Tok::Ident(name) = compact[j] {
                        if JS_KEYWORDS.contains(&name.as_str()) {
                            break;
                        }
                        if !decls.contains(&name) {
                            decls.push(name.clone());
                        }
                        // skip the initializer
                        j += 1;
                        let mut depth = 0;
                        while j < n {
                            match compact[j] {
                                Tok::Punct(p) => {
                                    if p == "(" || p == "[" || p == "{" {
                                        depth += 1;
                                    } else if p == ")" || p == "]" || p == "}" {
                                        if depth == 0 {
                                            break;
                                        }
                                        depth -= 1;
                                    } else if (p == "," || p == ";") && depth == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            j += 1;
                        }
                        if j < n && text(compact[j]) == "," {
                            j += 1;
                            continue;
                        }
                        break;
                    }
                    break;
                }
                i = j.max(i + 1);
                continue;
            }
            if w == "function" {
                let mut j = i + 1;
                if j < n {
                    if let Tok::Ident(name) = compact[j] {
                        if !JS_KEYWORDS.contains(&name.as_str()) && !decls.contains(&name) {
                            decls.push(name.clone());
                        }
                        j += 1;
                    }
                }
                if j < n && text(compact[j]) == "(" {
                    j += 1;
                    while j < n && text(compact[j]) != ")" {
                        if let Tok::Ident(p) = compact[j] {
                            if !JS_KEYWORDS.contains(&p.as_str()) && !decls.contains(&p) {
                                decls.push(p.clone());
                            }
                        }
                        j += 1;
                    }
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    decls
}

/// Deterministic short-name mapping (longest names get the shortest
/// slots); candidates must not appear as any word in the source and
/// must not be a JavaScript reserved word (the 1–2 character candidate
/// space contains `do`, `if`, `in` and `of`).
fn js_name_map(decls: &[String], src: &str) -> std::collections::BTreeMap<String, String> {
    const RESERVED: [&str; 38] = [
        "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete",
        "do", "else", "enum", "export", "extends", "false", "finally", "for", "function", "if",
        "import", "in", "instanceof", "let", "new", "null", "return", "static", "super", "switch",
        "this", "throw", "true", "try", "typeof", "var", "void", "while", "with",
    ];
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in src.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            cur.push(c);
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    let mut decls_sorted: Vec<String> = decls.to_vec();
    decls_sorted.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    let mut map = std::collections::BTreeMap::new();
    let alphabet: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().collect();
    let mut cands: Vec<String> = Vec::new();
    for c1 in &alphabet {
        cands.push(c1.to_string());
    }
    for c1 in &alphabet {
        for c2 in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars() {
            cands.push(format!("{}{}", c1, c2));
        }
    }
    let mut cand_i = 0;
    for d in decls_sorted {
        while cand_i < cands.len()
            && (words.contains(&cands[cand_i]) || RESERVED.contains(&cands[cand_i].as_str()))
        {
            cand_i += 1;
        }
        if cand_i >= cands.len() {
            map.insert(d.clone(), d); // no short name left: keep
            continue;
        }
        map.insert(d.clone(), cands[cand_i].clone());
        words.push(cands[cand_i].clone());
        cand_i += 1;
    }
    map
}

/// Emit: drop whitespace, rename identifiers (never property positions
/// or object-literal keys), join with a single space only between two
/// identifier-ish tokens.
fn js_emit(toks: &[Tok], map: &std::collections::BTreeMap<String, String>) -> String {
    let compact: Vec<&Tok> = toks.iter().filter(|t| !matches!(t, Tok::Space)).collect();
    let mut out = String::new();
    let text = |t: &Tok| -> String {
        match t {
            Tok::Ident(s) | Tok::Num(s) | Tok::Punct(s) | Tok::Str(s) | Tok::Regex(s) => s.clone(),
            Tok::Space => String::new(),
        }
    };
    for (i, t) in compact.iter().enumerate() {
        let mut piece = text(t);
        if let Tok::Ident(w) = t {
            let is_prop = i > 0 && text(compact[i - 1]) == ".";
            let is_key = i + 1 < compact.len()
                && text(compact[i + 1]) == ":"
                && i > 0
                && matches!(text(compact[i - 1]).as_str(), "{" | ",");
            if !is_prop && !is_key {
                if let Some(new) = map.get(w) {
                    piece = new.clone();
                }
            }
        }
        if !out.is_empty() {
            let last = out.chars().last().unwrap();
            let first = piece.chars().next().unwrap();
            let identy = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
            if identy(last) && identy(first) {
                out.push(' ');
            }
        }
        out.push_str(&piece);
    }
    out
}

fn js_minify(src: &str) -> String {
    let toks = js_tokenize(src);
    let decls = js_decls(&toks);
    let map = js_name_map(&decls, src);
    let out = js_emit(&toks, &map);
    // Optional catch binding (ES2019): `catch (e) {}` with an unused
    // binding becomes `catch {}`. Only empty bodies match, so a used
    // binding (k_call's envelope enrichment) is never touched.
    // Statement-final semicolons before `}` are dead weight (148 in
    // the current artifact) — the source has no `;}` inside a string
    // or regex literal (verified by test), so the textual pass is
    // exact.
    js_drop_empty_catch(&out).replace(";}", "}")
}

/// `catch (<ident>) {}` → `catch {}` (the binding is provably unused).
fn js_drop_empty_catch(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let b: Vec<char> = src.chars().collect();
    let n = b.len();
    let mut i = 0;
    while i < n {
        if b[i] == 'c'
            && b[i + 1..].starts_with("atch(".chars().collect::<Vec<char>>().as_slice())
        {
            let mut k = i + 6; // after "catch("
            while k < n && (b[k].is_ascii_alphanumeric() || b[k] == '_' || b[k] == '$') {
                k += 1;
            }
            if k < n && b[k] == ')' && k + 1 < n && b[k + 1] == '{' && k + 2 < n && b[k + 2] == '}' {
                out.push_str("catch{}");
                i = k + 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

// ===========================================================================
// Worklist abstract interpretation (per function)
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
struct TypeState {
    stack: Vec<ValKind>,
    slots: Vec<ValKind>,
}

impl TypeState {
    fn initial(f: &FuncDef) -> TypeState {
        let slots = (0..f.nlocals as usize)
            .map(|i| {
                f.param_kinds
                    .get(i)
                    .cloned()
                    .unwrap_or(if i < f.nparams as usize { ValKind::Any } else { ValKind::Unit })
            })
            .collect();
        TypeState { stack: Vec::new(), slots }
    }
}

/// Compute the type state **before** each instruction. See the module
/// docs for the merge rules; a stack-shape conflict is a compiler bug and
/// panics with the offending ip for triage.
fn analyze(f: &FuncDef, module: &Module) -> Vec<TypeState> {
    let n = f.code.len();
    let mut states: Vec<Option<TypeState>> = vec![None; n];
    let mut work: Vec<usize> = Vec::new();
    if n > 0 {
        states[0] = Some(TypeState::initial(f));
        work.push(0);
    }
    while let Some(ip) = work.pop() {
        let st = states[ip].clone().expect("state exists");
        let instr = &f.code[ip];
        let mut next = st.clone();
        let mut targets: Vec<usize> = Vec::new();
        match instr {
            Instr::Const(v) => {
                next.stack.push(kind_of_value(v));
                targets.push(ip + 1);
            }
            Instr::Load(n) => {
                let k = next.slots.get(*n as usize).cloned().unwrap_or(ValKind::Any);
                next.stack.push(k);
                targets.push(ip + 1);
            }
            Instr::Store(n) => {
                let k = next.stack.pop().unwrap_or(ValKind::Any);
                if let Some(s) = next.slots.get_mut(*n as usize) {
                    *s = k;
                }
                targets.push(ip + 1);
            }
            Instr::Add => {
                let (a, b) = pop2k(&mut next);
                next.stack.push(join_add(&a, &b));
                targets.push(ip + 1);
            }
            Instr::Sub | Instr::Mul | Instr::Div | Instr::Mod => {
                let (a, b) = pop2k(&mut next);
                next.stack.push(join_num(&a, &b));
                targets.push(ip + 1);
            }
            Instr::Lt | Instr::Gt | Instr::Le | Instr::Ge | Instr::Eq | Instr::Ne => {
                pop2k(&mut next);
                next.stack.push(ValKind::Bool);
                targets.push(ip + 1);
            }
            Instr::Neg => {
                let a = next.stack.pop().unwrap_or(ValKind::Any);
                next.stack.push(a);
                targets.push(ip + 1);
            }
            Instr::Not => {
                next.stack.pop();
                next.stack.push(ValKind::Bool);
                targets.push(ip + 1);
            }
            Instr::Jump(t) => targets.push(*t as usize),
            Instr::JumpIfFalse(t) => {
                next.stack.pop();
                targets.push(ip + 1);
                targets.push(*t as usize);
            }
            Instr::Call(c) => {
                let callee = &module.funcs[*c as usize];
                for _ in 0..callee.nparams {
                    next.stack.pop();
                }
                next.stack.push(callee.ret_kind.clone());
                targets.push(ip + 1);
            }
            Instr::Return => { /* terminates */ }
            Instr::Pop => {
                next.stack.pop();
                targets.push(ip + 1);
            }
            Instr::Print => {
                next.stack.pop();
                targets.push(ip + 1);
            }
            Instr::ArrNew(count) => {
                let c = *count as usize;
                let mut elem: Option<ValKind> = None;
                for _ in 0..c {
                    let k = next.stack.pop().unwrap_or(ValKind::Any);
                    elem = Some(match elem {
                        None => k,
                        Some(p) if p == k => p,
                        Some(_) => ValKind::Any,
                    });
                }
                next.stack.push(ValKind::Array(Box::new(elem.unwrap_or(ValKind::Any))));
                targets.push(ip + 1);
            }
            Instr::ArrGet => {
                let (a, _i) = pop2k(&mut next);
                next.stack.push(elem_kind(&a));
                targets.push(ip + 1);
            }
            Instr::ArrSet => {
                for _ in 0..3 {
                    next.stack.pop();
                }
                targets.push(ip + 1);
            }
            Instr::Native(id) => {
                let arity = match id {
                    NativeId::Len | NativeId::Abs | NativeId::Str | NativeId::Upper
                    | NativeId::Lower | NativeId::Trim | NativeId::Param => 1,
                    _ => 2,
                };
                let mut operand: Option<ValKind> = None;
                for _ in 0..arity {
                    let k = next.stack.pop().unwrap_or(ValKind::Any);
                    operand = Some(k);
                }
                let result = match id {
                    NativeId::Len => ValKind::Int,
                    NativeId::Abs => operand.unwrap_or(ValKind::Any),
                    NativeId::Min | NativeId::Max => operand.unwrap_or(ValKind::Any),
                    NativeId::Str | NativeId::Upper | NativeId::Lower | NativeId::Trim => {
                        ValKind::Str
                    }
                    NativeId::Contains => ValKind::Bool,
                    NativeId::Split => ValKind::Array(Box::new(ValKind::Str)),
                    NativeId::Join => ValKind::Str,
                    NativeId::Param => ValKind::Str,
                };
                next.stack.push(result);
                targets.push(ip + 1);
            }
            Instr::Cast(t) => {
                next.stack.pop();
                next.stack.push(match t {
                    CastTarget::Int => ValKind::Int,
                    CastTarget::Float => ValKind::Float,
                    CastTarget::Str => ValKind::Str,
                });
                targets.push(ip + 1);
            }
        }
        for t in targets {
            if t >= n {
                continue; // past-the-end jump: VM errors E4999; unreachable
            }
            if merge_state(&mut states, t, next.clone(), f, ip) {
                work.push(t);
            }
        }
    }
    states
        .into_iter()
        .enumerate()
        .map(|(ip, s)| s.unwrap_or_else(|| {
            // Unreachable code: never executed by either engine; give the
            // codegen the entry state so it can still emit a syntactic case.
            if ip == 0 {
                TypeState::initial(f)
            } else {
                states_default(ip, f)
            }
        }))
        .collect()
}

fn states_default(_ip: usize, f: &FuncDef) -> TypeState {
    TypeState::initial(f)
}

fn pop2k(st: &mut TypeState) -> (ValKind, ValKind) {
    let b = st.stack.pop().unwrap_or(ValKind::Any);
    let a = st.stack.pop().unwrap_or(ValKind::Any);
    (a, b)
}

fn join_add(a: &ValKind, b: &ValKind) -> ValKind {
    match (a, b) {
        (ValKind::Int, ValKind::Int) => ValKind::Int,
        (ValKind::Float, ValKind::Float) => ValKind::Float,
        (ValKind::Str, ValKind::Str) => ValKind::Str,
        (ValKind::Array(x), ValKind::Array(y)) => ValKind::Array(Box::new(join(x, y))),
        _ => ValKind::Any,
    }
}

fn join_num(a: &ValKind, b: &ValKind) -> ValKind {
    match (a, b) {
        (ValKind::Int, ValKind::Int) => ValKind::Int,
        (ValKind::Float, ValKind::Float) => ValKind::Float,
        _ => ValKind::Any,
    }
}

fn merge_state(
    states: &mut [Option<TypeState>],
    ip: usize,
    incoming: TypeState,
    f: &FuncDef,
    from: usize,
) -> bool {
    match &states[ip] {
        None => {
            states[ip] = Some(incoming);
            true
        }
        Some(existing) => {
            // Stack HEIGHTS at one ip are structural (statement
            // boundaries keep them equal per the design): different
            // heights are a real compiler bug. Element KINDS, however,
            // can legitimately differ between paths — a slot written
            // through an empty-array literal + concat chain widens to
            // Array(Any) while the param path still carries Array(Str)
            // — so kinds join (monotone widening) instead of panicking.
            if existing.stack.len() != incoming.stack.len() {
                panic!(
                    "webkir: stack height conflict in `{}` at ip {} (from {}): {:?} vs {:?}",
                    f.name, ip, from, existing.stack, incoming.stack
                );
            }
            let mut changed = false;
            let mut merged = existing.clone();
            for (i, k) in incoming.stack.iter().enumerate() {
                let j = join(&merged.stack[i], k);
                if j != merged.stack[i] {
                    changed = true;
                    merged.stack[i] = j;
                }
            }
            for (i, k) in incoming.slots.iter().enumerate() {
                if let Some(s) = merged.slots.get_mut(i) {
                    let j = join(s, k);
                    if j != *s {
                        changed = true;
                        *s = j;
                    }
                }
            }
            states[ip] = Some(merged);
            changed
        }
    }
}

// ===========================================================================
// JS code generation
// ===========================================================================

/// Array constants used by `Const` instructions, in first-use order,
/// deduplicated structurally (each read pushes the SAME object, matching
/// the VM's shared `Rc`).
fn array_consts(module: &Module) -> Vec<Value> {
    let mut seen: Vec<Value> = Vec::new();
    for f in &module.funcs {
        for i in &f.code {
            if let Instr::Const(v) = i {
                if matches!(v, Value::Array(_)) {
                    if !seen.iter().any(|s| s == v) {
                        seen.push(v.clone());
                    }
                }
            }
        }
    }
    seen
}

/// Escape a Kin string into a double-quoted JS literal.
fn js_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A JS literal for a KIR value (scalars; arrays recursively).
fn js_value(v: &Value) -> String {
    match v {
        Value::Int(i) => format!("{}", i),
        Value::Float(f) => js_float(*f),
        Value::Bool(b) => format!("{}", b),
        Value::Str(s) => js_str(s),
        Value::Array(a) => {
            let items: Vec<String> = a.borrow().iter().map(js_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Unit => "undefined".to_string(),
    }
}

fn js_float(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() }
    } else if f != 0.0 && (f.abs() >= 1e21 || f.abs() < 1e-6) {
        // Shortest round-trip in exponent form (parses back exactly).
        format!("{:e}", f)
    } else {
        format!("{}", f)
    }
}

/// The kind tags the codegen needs: top-of-stack operands.
fn top(st: &TypeState, back: usize) -> ValKind {
    let n = st.stack.len();
    if n > back {
        st.stack[n - 1 - back].clone()
    } else {
        ValKind::Any
    }
}

/// Generate one function registration (deterministic).
fn gen_func(module: &Module, idx: usize, file: &str) -> String {
    let f = &module.funcs[idx];
    let states = analyze(f, module);
    let consts = array_consts(module);
    let mut js = String::new();
    let params: Vec<String> = (0..f.nparams).map(|i| format!("p{}", i)).collect();
    js.push_str(&format!(
        "K.reg({}, {}, {}, function ({}) {{\n",
        idx,
        js_str(&f.name),
        js_str(file),
        params.join(", ")
    ));
    if f.nparams > 0 {
        js.push_str(&format!("var L = [{}]; L.length = {};\n", params.join(", "), f.nlocals));
    } else if f.nlocals > 0 {
        js.push_str(&format!("var L = []; L.length = {};\n", f.nlocals));
    } else {
        js.push_str("var L = [];\n");
    }
    js.push_str("var S = [];\nvar ip = 0;\nfor (;;) {\nswitch (ip) {\n");

    for (ip, instr) in f.code.iter().enumerate() {
        let st = &states[ip];
        let line = f.lines.get(ip).copied().unwrap_or(0);
        let mut body = String::new();
        let mut next_ip = (ip + 1) as u32;
        match instr {
            Instr::Const(v) => {
                if matches!(v, Value::Array(_)) {
                    let ci = consts.iter().position(|c| c == v).expect("hoisted");
                    body.push_str(&format!("S.push(C{});", ci));
                } else {
                    body.push_str(&format!("S.push({});", js_value(v)));
                }
            }
            Instr::Load(n) => body.push_str(&format!("S.push(L[{}]);", n)),
            Instr::Store(n) => body.push_str(&format!("L[{}] = S.pop();", n)),
            Instr::Add => {
                let (a, b) = (top(st, 1), top(st, 0));
                match (a, b) {
                    (ValKind::Int, ValKind::Int) => binop(&mut body, ip, "K.h[0]", Some(line), true),
                    (ValKind::Float, ValKind::Float) => {
                        binop_raw(&mut body, ip, " + ", false)
                    }
                    (ValKind::Str, ValKind::Str) => binop_raw(&mut body, ip, " + ", false),
                    (ValKind::Array(_), ValKind::Array(_)) => {
                        binop(&mut body, ip, "K.h[12]", None, false)
                    }
                    _ => panic!(
                        "webkir: ambiguous operand kinds for Add at `{}:{}` ip {} — \
                         the static kinds are missing (provide param_kinds)",
                        f.name, file, ip
                    ),
                }
            }
            Instr::Sub => {
                let (a, b) = (top(st, 1), top(st, 0));
                match (a, b) {
                    (ValKind::Int, ValKind::Int) => binop(&mut body, ip, "K.h[1]", Some(line), false),
                    (ValKind::Float, ValKind::Float) => {
                        binop_raw(&mut body, ip, " - ", false)
                    }
                    _ => panic!("webkir: ambiguous kinds for Sub at `{}:{}` ip {}", f.name, file, ip),
                }
            }
            Instr::Mul => {
                let (a, b) = (top(st, 1), top(st, 0));
                match (a, b) {
                    (ValKind::Int, ValKind::Int) => binop(&mut body, ip, "K.h[2]", Some(line), true),
                    (ValKind::Float, ValKind::Float) => {
                        binop_raw(&mut body, ip, " * ", false)
                    }
                    _ => panic!("webkir: ambiguous kinds for Mul at `{}:{}` ip {}", f.name, file, ip),
                }
            }
            Instr::Div => {
                let (a, b) = (top(st, 1), top(st, 0));
                match (a, b) {
                    (ValKind::Int, ValKind::Int) => binop(&mut body, ip, "K.h[3]", Some(line), false),
                    (ValKind::Float, ValKind::Float) => {
                        binop(&mut body, ip, "K.h[9]", Some(line), false)
                    }
                    _ => panic!("webkir: ambiguous kinds for Div at `{}:{}` ip {}", f.name, file, ip),
                }
            }
            Instr::Mod => {
                binop(&mut body, ip, "K.h[4]", Some(line), false);
            }
            Instr::Lt => cmp_js(&mut body, ip, "<"),
            Instr::Gt => cmp_js(&mut body, ip, ">"),
            Instr::Le => cmp_js(&mut body, ip, "<="),
            Instr::Ge => cmp_js(&mut body, ip, ">="),
            Instr::Eq => binop(&mut body, ip, "K.h[11]", None, false),
            Instr::Ne => {
                body.push_str(&format!(
                    "var b{i} = S.pop(), a{i} = S.pop(); S.push(!K.h[11](a{i}, b{i}));",
                    i = ip
                ));
            }
            Instr::Neg => {
                let k = top(st, 0);
                if k == ValKind::Int {
                    body.push_str(&format!("S.push(K.h[5](S.pop(), {}));", line));
                } else if k == ValKind::Float {
                    body.push_str("S.push(-S.pop());");
                } else {
                    panic!("webkir: ambiguous kind for Neg at `{}:{}` ip {}", f.name, file, ip);
                }
            }
            Instr::Not => body.push_str("S.push(!S.pop());"),
            Instr::Jump(t) => {
                next_ip = *t;
            }
            Instr::JumpIfFalse(t) => {
                js.push_str(&format!(
                    "case {}: /*@kinn:{}:{}*/ ip = {}; if (!S.pop()) {{ ip = {}; }} break;\n",
                    ip, file, line, next_ip, t
                ));
                continue;
            }
            Instr::Call(c) => {
                let callee = &module.funcs[*c as usize];
                let m = callee.nparams as usize;
                if m == 0 {
                    body.push_str(&format!("S.push(K.k_call(F[{}], [], {}));", c, line));
                } else {
                    body.push_str(&format!(
                        "var t{i} = S.splice(S.length - {m}, {m}); S.push(K.k_call(F[{c}], t{i}, {l}));",
                        i = ip, m = m, c = c, l = line
                    ));
                }
            }
            Instr::Return => {
                if st.stack.is_empty() {
                    js.push_str(&format!(
                        "case {}: /*@kinn:{}:{}*/ return;\n",
                        ip, file, line
                    ));
                } else {
                    js.push_str(&format!(
                        "case {}: /*@kinn:{}:{}*/ return S.pop();\n",
                        ip, file, line
                    ));
                }
                continue;
            }
            Instr::Pop => body.push_str("S.pop();"),
            Instr::Print => {
                let tag = top(st, 0);
                body.push_str(&format!(
                    "K.out(K.h[10](S.pop(), {}));",
                    js_str(&tag.tag())
                ));
            }
            Instr::ArrNew(n) => {
                if *n == 0 {
                    body.push_str("S.push([]);");
                } else {
                    body.push_str(&format!(
                        "S.push(S.splice(S.length - {n}, {n}));",
                        n = n
                    ));
                }
            }
            Instr::ArrGet => {
                body.push_str(&format!(
                    "var i{i} = S.pop(); S.push(K.h[13](S.pop(), i{i}, {l}));",
                    i = ip, l = line
                ));
            }
            Instr::ArrSet => {
                body.push_str(&format!(
                    "var v{i} = S.pop(), i{i} = S.pop(); K.h[14](S.pop(), i{i}, v{i}, {l});",
                    i = ip, l = line
                ));
            }
            Instr::Native(id) => {
                gen_native(&mut body, ip, *id, st, line, &f.name, file);
            }
            Instr::Cast(target) => {
                let k = top(st, 0);
                match (target, &k) {
                    (CastTarget::Float, ValKind::Int) => {
                        // Int → Float is the identity on JS numbers.
                        body.push_str("/* int→float: identity */");
                    }
                    (CastTarget::Int, ValKind::Float) => {
                        body.push_str("S.push(K.h[22](S.pop()));");
                    }
                    (CastTarget::Str, _) => {
                        body.push_str(&format!(
                            "S.push(K.h[10](S.pop(), {}));",
                            js_str(&k.tag())
                        ));
                    }
                    (CastTarget::Float, ValKind::Float) => body.push_str("/* identity */"),
                    (CastTarget::Int, ValKind::Int) => body.push_str("/* identity */"),
                    (CastTarget::Int, ValKind::Str) => {
                        // Str→Int: never emitted by the checker; keep the
                        // honest error path visible.
                        panic!("webkir: cast string→int is not a KIR combination");
                    }
                    _ => panic!(
                        "webkir: ambiguous kind for Cast at `{}:{}` ip {}",
                        f.name, file, ip
                    ),
                }
            }
        }
        js.push_str(&format!(
            "case {}: /*@kinn:{}:{}*/ {} ip = {}; break;\n",
            ip, file, line, body, next_ip
        ));
    }
    let last_line = f.lines.last().copied().unwrap_or(0);
    js.push_str(&format!(
        "default: throw new K.KinRTError(\"E4999\", \"internal error: fell off the end of `{}`\", {});\n",
        f.name, last_line
    ));
    js.push_str("}\n}\n});\n");
    js
}

fn binop(body: &mut String, ip: usize, helper: &str, line: Option<u32>, commutative: bool) {
    if commutative {
        let l = line.map(|l| format!(", {}", l)).unwrap_or_default();
        body.push_str(&format!("S.push({h}(S.pop(), S.pop(){l}));", h = helper, l = l));
    } else {
        let l = line.map(|l| format!(", {}", l)).unwrap_or_default();
        body.push_str(&format!(
            "var b{i} = S.pop(), a{i} = S.pop(); S.push({h}(a{i}, b{i}{l}));",
            i = ip, h = helper, l = l
        ));
    }
}

fn binop_raw(body: &mut String, ip: usize, op: &str, _commutative: bool) {
    body.push_str(&format!(
        "var b{i} = S.pop(), a{i} = S.pop(); S.push(a{i} {op} b{i});",
        i = ip, op = op
    ));
}

fn cmp_js(body: &mut String, ip: usize, op: &str) {
    body.push_str(&format!(
        "var b{i} = S.pop(), a{i} = S.pop(); S.push(a{i} {op} b{i});",
        i = ip, op = op
    ));
}

fn gen_native(
    body: &mut String,
    ip: usize,
    id: NativeId,
    st: &TypeState,
    line: u32,
    fname: &str,
    file: &str,
) {
    let k0 = top(st, 0);
    let k1 = top(st, 1);
    match id {
        NativeId::Len => {
            if matches!(k0, ValKind::Array(_)) {
                body.push_str("S.push(S.pop().length);");
            } else if k0 == ValKind::Str {
                body.push_str("S.push(K.h[15](S.pop()));");
            } else {
                panic!("webkir: ambiguous kind for len at `{}:{}` ip {}", fname, file, ip);
            }
        }
        NativeId::Abs => {
            if k0 == ValKind::Int {
                body.push_str(&format!("S.push(K.h[6](S.pop(), {}));", line));
            } else if k0 == ValKind::Float {
                body.push_str("S.push(Math.abs(S.pop()));");
            } else {
                panic!("webkir: ambiguous kind for abs at `{}:{}` ip {}", fname, file, ip);
            }
        }
        NativeId::Min => {
            if k1 == ValKind::Int {
                body.push_str(&format!(
                    "var b{i} = S.pop(), a{i} = S.pop(); S.push(Math.min(a{i}, b{i}));",
                    i = ip
                ));
            } else if k1 == ValKind::Float {
                binop(body, ip, "K.h[7]", None, true);
            } else {
                panic!("webkir: ambiguous kind for min at `{}:{}` ip {}", fname, file, ip);
            }
        }
        NativeId::Max => {
            if k1 == ValKind::Int {
                body.push_str(&format!(
                    "var b{i} = S.pop(), a{i} = S.pop(); S.push(Math.max(a{i}, b{i}));",
                    i = ip
                ));
            } else if k1 == ValKind::Float {
                binop(body, ip, "K.h[8]", None, true);
            } else {
                panic!("webkir: ambiguous kind for max at `{}:{}` ip {}", fname, file, ip);
            }
        }
        NativeId::Str => {
            body.push_str(&format!(
                "S.push(K.h[10](S.pop(), {}));",
                js_str(&k0.tag())
            ));
        }
        NativeId::Upper => body.push_str("S.push(K.h[16](S.pop()));"),
        NativeId::Lower => body.push_str("S.push(K.h[17](S.pop()));"),
        NativeId::Trim => body.push_str("S.push(K.h[18](S.pop()));"),
        NativeId::Contains => binop(body, ip, "K.h[19]", None, false),
        NativeId::Split => binop(body, ip, "K.h[20]", None, false),
        NativeId::Join => binop(body, ip, "K.h[21]", None, false),
        // S7 route-parameter read: k_param(name) consults the router's
        // current params (kinnrt.js). Never errors — an unknown name is
        // the empty string (the checker's E3058 already caught literal
        // misspellings; dynamic names are runtime data).
        NativeId::Param => body.push_str("S.push(K.h[23](S.pop()));"),
    }
}

// ===========================================================================
// App lowering: AppModel → app-side FuncDefs (S4 boundary → S5 ABI)
// ===========================================================================

/// Event builtins the handler ABI can carry, in the fixed manifest order.
/// `form_valid`/`field_errors` are S9 scope and rejected today (E3071).
fn event_builtins() -> [(&'static str, ValKind, &'static str); 6] {
    [
    ("event_value", ValKind::Str, "the value of the input the event came from"),
    ("event_key", ValKind::Str, "the key name of a key event"),
    ("event_x", ValKind::Int, "the pointer x coordinate"),
    ("event_y", ValKind::Int, "the pointer y coordinate"),
    ("form_valid", ValKind::Bool, "form validity (S9)"),
    ("field_errors", ValKind::Array(Box::new(ValKind::Str)), "form field errors (S9)"),
    ]
}

struct AppCtx<'a> {
    app: &'a AppModel,
    module: &'a Module,
    /// name → user function index.
    fidx: HashMap<String, usize>,
    /// Const lookup: bare entry name and `module::name` both work.
    consts: HashMap<String, &'a Value>,
    /// computed name → ret kind (memoized, deps are acyclic per E3044).
    computed_kinds: std::cell::RefCell<HashMap<String, ValKind>>,
}

impl<'a> AppCtx<'a> {
    fn new(app: &'a AppModel, module: &'a Module) -> AppCtx<'a> {
        let fidx: HashMap<String, usize> = module
            .funcs
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect();
        let mut consts: HashMap<String, &Value> = HashMap::new();
        for (n, v) in &module.consts {
            consts.insert(n.clone(), v);
        }
        AppCtx {
            app,
            module,
            fidx,
            consts,
            computed_kinds: std::cell::RefCell::new(HashMap::new()),
        }
    }

    fn state_kind(&self, name: &str) -> Option<ValKind> {
        self.app.state.iter().find(|s| s.name == name).map(|s| kind_of_type(&s.ty))
    }

    fn is_state(&self, name: &str) -> bool {
        self.app.state.iter().any(|s| s.name == name)
    }

    fn is_computed(&self, name: &str) -> bool {
        self.app.computed.iter().any(|c| c.name == name)
    }

    fn computed_kind(&self, name: &str) -> ValKind {
        if let Some(k) = self.computed_kinds.borrow().get(name) {
            return k.clone();
        }
        let c = self.app.computed.iter().find(|c| c.name == name).expect("checked");
        let mut lo = FnAppLower::new(self, &c.kir, 0);
        let kind = lo.expr_kind(&c.expr);
        self.computed_kinds.borrow_mut().insert(name.to_string(), kind.clone());
        kind
    }

    /// `app:init` — compute every state initializer, return them as an
    /// array (the boot state).
    fn init_func(&self) -> Result<FuncDef, Diagnostic> {
        let mut lo = FnAppLower::new(self, "app:init", 0);
        // The state values live in slots 0..n-1 (no params).
        lo.max_slots = self.app.state.len() as u16;
        for (i, s) in self.app.state.iter().enumerate() {
            lo.lower_expr(&s.init);
            lo.store_slot(i as u16, s.init.line);
        }
        // return [v0, …, vn-1]
        for i in 0..self.app.state.len() {
            lo.load_slot(i as u16, 0);
        }
        lo.emit(Instr::ArrNew(self.app.state.len() as u16), 0);
        lo.emit(Instr::Return, 0);
        let elem = self.app.state.iter().map(|s| kind_of_type(&s.ty)).reduce(|a, b| join(&a, &b)).unwrap_or(ValKind::Any);
        Ok(lo.finish(Vec::new(), ValKind::Array(Box::new(elem))))
    }

    /// `app:computed:<name>` — params are the free names (state fields and
    /// other computeds), sorted; the body returns the expression value.
    fn computed_func(&self, c: &ComputedModel) -> Result<FuncDef, Diagnostic> {
        reject_host_builtins(&c.expr)?;
        let free = c.expr.free_names();
        let (state_names, computed_names) = self.split_free(&free);
        let mut params: Vec<(String, ValKind)> = Vec::new();
        for n in state_names {
            params.push((n.clone(), self.state_kind(&n).expect("checked")));
        }
        for n in &computed_names {
            params.push((n.clone(), self.computed_kind(n)));
        }
        // params sorted overall (state and computed names together).
        params.sort_by(|a, b| a.0.cmp(&b.0));
        let mut lo = FnAppLower::new(self, &c.kir, params.len() as u16);
        lo.bind_params(&params);
        let kind = lo.expr_kind(&c.expr);
        lo.lower_expr(&c.expr);
        lo.emit(Instr::Return, c.expr.line);
        Ok(lo.finish(
            params.iter().map(|(_, k)| k.clone()).collect(),
            kind,
        ))
    }

    /// `app:<owner>:<event>:<n>` — the S6 handler ABI: params are the
    /// written-or-read state fields (sorted) followed by the event
    /// builtins used (fixed order); the return value is the array of
    /// current state-param values (write-back).
    fn handler_func(&self, h: &HandlerModel) -> Result<FuncDef, Diagnostic> {
        for st in &h.body {
            reject_host_builtins_stmt(st)?;
        }
        // Owner scope: when the handler lives inside a component view its
        // scope names (props + component state) join the app state in the
        // ABI param list.
        let shadow: Vec<(String, ValKind)> = self
            .app
            .components
            .iter()
            .find(|c| c.name == h.owner)
            .map(|c| {
                c.props
                    .iter()
                    .map(|p| (p.name.clone(), kind_of_type(&p.ty)))
                    .chain(c.state.iter().map(|s| (s.name.clone(), kind_of_type(&s.ty))))
                    .collect()
            })
            .unwrap_or_default();
        let free = free_names_stmts(&h.body);
        let state_names: Vec<String> = free
            .iter()
            .filter(|n| shadow.iter().any(|(s, _)| s == *n) || self.is_state(n))
            .cloned()
            .collect();
        let _ = self.split_free(&free);
        let mut builtins: Vec<(String, ValKind)> = Vec::new();
        for (n, k, _) in event_builtins() {
            if free.contains(n) {
                if n == "form_valid" || n == "field_errors" {
                    return Err(e3071(
                        &format!("the web runtime does not implement the `{}` handler builtin yet", n),
                        h.body.first().map(|s| s.line).unwrap_or(1),
                        "form validation wiring arrives with S9 (0.4.0)",
                    ));
                }
                builtins.push((n.to_string(), k.clone()));
            }
        }
        let mut params: Vec<(String, ValKind)> = state_names
            .iter()
            .map(|n| {
                let kind = shadow
                    .iter()
                    .find(|(s, _)| s == n)
                    .map(|(_, k)| k.clone())
                    .unwrap_or_else(|| self.state_kind(n).expect("checked"));
                (n.clone(), kind)
            })
            .collect();
        params.sort_by(|a, b| a.0.cmp(&b.0));
        params.dedup_by(|a, b| a.0 == b.0);
        let n_state = params.len();
        params.extend(builtins);
        let mut lo = FnAppLower::new(self, &h.kir, params.len() as u16);
        lo.bind_params(&params);
        lo.lower_stmts(&h.body)?;
        // Return the state write-back array.
        for i in 0..n_state {
            lo.load_slot(i as u16, 0);
        }
        lo.emit(Instr::ArrNew(n_state as u16), 0);
        lo.emit(Instr::Return, 0);
        let elem = params[..n_state]
            .iter()
            .map(|(_, k)| k.clone())
            .reduce(|a, b| join(&a, &b))
            .unwrap_or(ValKind::Any);
        Ok(lo.finish(
            params.iter().map(|(_, k)| k.clone()).collect(),
            ValKind::Array(Box::new(elem)),
        ))
    }

    /// `app:init:<Component>` — component state initializers; props read
    /// by the initializers become params (sorted). The return value is
    /// the component's initial state array.
    fn component_init_func(
        &self,
        c: &'a crate::appmodel::ComponentModel,
        _file: &str,
    ) -> Result<FuncDef, Diagnostic> {
        let mut free = BTreeSet::new();
        for s in &c.state {
            free_names_expr(&s.init, &mut free);
        }
        let prop_names: Vec<String> = free
            .into_iter()
            .filter(|n| c.props.iter().any(|p| &p.name == n))
            .collect();
        let mut params: Vec<(String, ValKind)> = prop_names
            .iter()
            .map(|n| (n.clone(), kind_of_type(&c.props.iter().find(|p| &p.name == n).unwrap().ty)))
            .collect();
        params.sort_by(|a, b| a.0.cmp(&b.0));
        let name = format!("app:init:{}", c.name);
        let mut lo = FnAppLower::new(self, &name, params.len() as u16);
        lo.bind_params(&params);
        // Props in slots 0..p-1, component state in p..p+n-1.
        lo.max_slots = (params.len() + c.state.len()) as u16;
        for (i, s) in c.state.iter().enumerate() {
            lo.lower_expr(&s.init);
            lo.store_slot(params.len() as u16 + i as u16, s.init.line);
        }
        for i in 0..c.state.len() {
            lo.load_slot(params.len() as u16 + i as u16, 0);
        }
        lo.emit(Instr::ArrNew(c.state.len() as u16), 0);
        lo.emit(Instr::Return, 0);
        let elem = c.state.iter().map(|s| kind_of_type(&s.ty)).reduce(|a, b| join(&a, &b)).unwrap_or(ValKind::Any);
        Ok(lo.finish(
            params.iter().map(|(_, k)| k.clone()).collect(),
            ValKind::Array(Box::new(elem)),
        ))
    }

    fn split_free(&self, free: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
        let state: Vec<String> =
            free.iter().filter(|n| self.is_state(n)).cloned().collect();
        let computed: Vec<String> =
            free.iter().filter(|n| self.is_computed(n)).cloned().collect();
        (state, computed)
    }
}

fn e3071(message: &str, line: u32, hint: &str) -> Diagnostic {
    let mut d = Diagnostic::error("E3071", message.to_string(), line.max(1), 1, 0);
    d.hint = Some(hint.to_string());
    d
}

/// One function under construction.
struct FnAppLower<'a, 'b> {
    ctx: &'b AppCtx<'a>,
    kir_name: String,
    nparams: u16,
    code: Vec<Instr>,
    lines: Vec<u32>,
    scopes: Vec<HashMap<String, u16>>,
    scope_slots: Vec<Vec<u16>>,
    free: Vec<u16>,
    max_slots: u16,
    loops: Vec<LoopCtx>,
    hidden: u32,
    cur_line: u32,
    /// Name → kind for locals and params (feeds expr_kind).
    local_kinds: HashMap<String, ValKind>,
}

struct LoopCtx {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl<'a, 'b> FnAppLower<'a, 'b> {
    fn new(ctx: &'b AppCtx<'a>, kir_name: &str, nparams: u16) -> FnAppLower<'a, 'b> {
        FnAppLower {
            ctx: ctx,
            kir_name: kir_name.to_string(),
            nparams,
            code: Vec::new(),
            lines: Vec::new(),
            scopes: vec![HashMap::new()],
            scope_slots: vec![Vec::new()],
            free: Vec::new(),
            max_slots: nparams,
            loops: Vec::new(),
            hidden: 0,
            cur_line: 0,
            local_kinds: HashMap::new(),
        }
    }

    fn bind_params(&mut self, params: &[(String, ValKind)]) {
        for (i, (n, k)) in params.iter().enumerate() {
            self.scopes[0].insert(n.clone(), i as u16);
            self.scope_slots[0].push(i as u16);
            self.local_kinds.insert(n.clone(), k.clone());
        }
    }

    fn emit(&mut self, i: Instr, line: u32) {
        self.code.push(i);
        self.lines.push(line);
    }

    fn at(&mut self, line: u32) {
        self.cur_line = line;
    }

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

    fn store_slot(&mut self, slot: u16, line: u32) {
        self.at(line);
        self.emit(Instr::Store(slot), line);
    }

    fn load_slot(&mut self, slot: u16, line: u32) {
        self.at(line);
        self.emit(Instr::Load(slot), line);
    }

    fn finish(mut self, param_kinds: Vec<ValKind>, ret_kind: ValKind) -> FuncDef {
        self.emit(Instr::Const(Value::Unit), self.cur_line);
        self.emit(Instr::Return, self.cur_line);
        FuncDef {
            name: self.kir_name,
            nparams: self.nparams,
            nlocals: self.max_slots,
            code: self.code,
            lines: self.lines,
            param_kinds,
            ret_kind,
        }
    }

    fn try_slot(&self, name: &str) -> Option<u16> {
        for scope in self.scopes.iter().rev() {
            if let Some(s) = scope.get(name) {
                return Some(*s);
            }
        }
        None
    }

    // -- statements ------------------------------------------------------

    fn lower_stmts(&mut self, stmts: &[AppStmt]) -> Result<(), Diagnostic> {
        for s in stmts {
            self.lower_stmt(s)?;
        }
        Ok(())
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.scope_slots.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        let slots = self.scope_slots.pop().unwrap_or_default();
        self.scopes.pop();
        self.free.extend(slots);
    }

    fn lower_stmt(&mut self, s: &AppStmt) -> Result<(), Diagnostic> {
        match &s.kind {
            AppStmtKind::VarDecl { name, init, .. } => {
                let kind = self.expr_kind(init);
                self.lower_expr(init);
                self.at(s.line);
                let slot = self.alloc(name);
                self.local_kinds.insert(name.clone(), kind);
                self.emit(Instr::Store(slot), s.line);
            }
            AppStmtKind::Assign { name, op, value } => {
                self.lower_expr(value);
                self.at(s.line);
                let slot = match self.try_slot(name) {
                    Some(s) => s,
                    None => {
                        return Err(e3071(
                            &format!(
                                "assignment to `{}` does not resolve to a local or a state field the handler received",
                                name
                            ),
                            s.line,
                            "the handler ABI passes the state fields it reads or writes; this is a webkir bug if the name is a state field",
                        ));
                    }
                };
                match op {
                    AssignOp::Set => self.emit(Instr::Store(slot), s.line),
                    AssignOp::Add => {
                        self.load_slot(slot, s.line);
                        self.emit(Instr::Add, s.line);
                        self.emit(Instr::Store(slot), s.line);
                    }
                    AssignOp::Sub => {
                        self.load_slot(slot, s.line);
                        self.emit(Instr::Sub, s.line);
                        self.emit(Instr::Store(slot), s.line);
                    }
                }
            }
            AppStmtKind::IndexAssign { base, index, value } => {
                self.lower_expr(base);
                self.lower_expr(index);
                self.lower_expr(value);
                self.at(s.line);
                self.emit(Instr::ArrSet, s.line);
            }
            AppStmtKind::If { cond, then_branch, else_branch } => {
                self.lower_expr(cond);
                self.at(s.line);
                let jf = self.code.len();
                self.emit(Instr::JumpIfFalse(0), s.line);
                self.lower_stmts(then_branch)?;
                self.at(s.line);
                let jend = self.code.len();
                self.emit(Instr::Jump(0), s.line);
                let else_start = self.code.len();
                self.patch(jf, else_start as u32);
                if let Some(e) = else_branch {
                    self.lower_stmt(e)?;
                }
                let end = self.code.len();
                self.patch(jend, end as u32);
                self.patch(jf, else_start as u32);
            }
            AppStmtKind::While { cond, body } => {
                let top = self.code.len() as u32;
                self.lower_expr(cond);
                self.at(s.line);
                let jf = self.code.len();
                self.emit(Instr::JumpIfFalse(0), s.line);
                self.loops.push(LoopCtx { breaks: Vec::new(), continues: Vec::new() });
                self.lower_stmts(body)?;
                let ctx = self.loops.pop().unwrap();
                self.at(s.line);
                for c in ctx.continues {
                    self.patch(c, top);
                }
                self.emit(Instr::Jump(top), s.line);
                let end = self.code.len();
                self.patch(jf, end as u32);
                for b in ctx.breaks {
                    self.patch(b, end as u32);
                }
            }
            AppStmtKind::For { var, iter, body } => {
                self.lower_for(var, iter, body, s.line)?;
            }
            AppStmtKind::Return { .. } => {
                return Err(e3071(
                    "`return` is not allowed inside an app handler (E3045)",
                    s.line,
                    "handlers communicate state changes by assigning state fields",
                ));
            }
            AppStmtKind::Break => {
                self.at(s.line);
                let j = self.code.len();
                self.emit(Instr::Jump(0), s.line);
                self.loops.last_mut().expect("break inside loop").breaks.push(j);
            }
            AppStmtKind::Continue => {
                self.at(s.line);
                let j = self.code.len();
                self.emit(Instr::Jump(0), s.line);
                self.loops.last_mut().expect("continue inside loop").continues.push(j);
            }
            AppStmtKind::Expr(e) => {
                self.lower_expr(e);
                self.at(s.line);
                if !matches!(self.expr_kind(e), ValKind::Unit) {
                    self.emit(Instr::Pop, s.line);
                }
            }
            AppStmtKind::Block(b) => {
                self.push_scope();
                let r = self.lower_stmts(b);
                self.pop_scope();
                r?;
            }
        }
        Ok(())
    }

    fn patch(&mut self, at: usize, target: u32) {
        if let Some(Instr::Jump(t)) = self.code.get_mut(at) {
            *t = target;
        } else if let Some(Instr::JumpIfFalse(t)) = self.code.get_mut(at) {
            *t = target;
        }
    }

    fn lower_for(
        &mut self,
        var: &str,
        iter: &ForIterModel,
        body: &[AppStmt],
        line: u32,
    ) -> Result<(), Diagnostic> {
        match iter {
            ForIterModel::Range { start, end, inclusive } => {
                // $it, $end hidden slots (impossible user names).
                self.lower_expr(start);
                let it = self.hidden_slot();
                self.emit(Instr::Store(it), line);
                self.lower_expr(end);
                let lim = self.hidden_slot();
                self.emit(Instr::Store(lim), line);
                let top = self.code.len() as u32;
                self.load_slot(it, line);
                self.load_slot(lim, line);
                self.emit(if *inclusive { Instr::Le } else { Instr::Lt }, line);
                let jf = self.code.len();
                self.emit(Instr::JumpIfFalse(0), line);
                self.loops.push(LoopCtx { breaks: Vec::new(), continues: Vec::new() });
                self.push_scope();
                self.load_slot(it, line);
                let vslot = self.alloc(var);
                self.emit(Instr::Store(vslot), line);
                self.lower_stmts(body)?;
                self.pop_scope();
                let ctx = self.loops.pop().unwrap();
                // continue target: the increment
                let inc = self.code.len() as u32;
                self.load_slot(it, line);
                self.emit(Instr::Const(Value::Int(1)), line);
                self.emit(Instr::Add, line);
                self.emit(Instr::Store(it), line);
                self.emit(Instr::Jump(top), line);
                let end = self.code.len();
                self.patch(jf, end as u32);
                for c in ctx.continues {
                    self.patch(c, inc);
                }
                for b in ctx.breaks {
                    self.patch(b, end as u32);
                }
            }
            ForIterModel::Expr(iter_expr) => {
                // $arr, $idx hidden slots.
                self.lower_expr(iter_expr);
                let arr = self.hidden_slot();
                self.emit(Instr::Store(arr), line);
                self.emit(Instr::Const(Value::Int(0)), line);
                let idx = self.hidden_slot();
                self.emit(Instr::Store(idx), line);
                let top = self.code.len() as u32;
                self.load_slot(idx, line);
                self.load_slot(arr, line);
                self.emit(Instr::Native(NativeId::Len), line);
                self.emit(Instr::Lt, line);
                let jf = self.code.len();
                self.emit(Instr::JumpIfFalse(0), line);
                self.loops.push(LoopCtx { breaks: Vec::new(), continues: Vec::new() });
                self.push_scope();
                self.load_slot(arr, line);
                self.load_slot(idx, line);
                self.emit(Instr::ArrGet, line);
                let vslot = self.alloc(var);
                self.emit(Instr::Store(vslot), line);
                self.lower_stmts(body)?;
                self.pop_scope();
                let ctx = self.loops.pop().unwrap();
                let inc = self.code.len() as u32;
                self.load_slot(idx, line);
                self.emit(Instr::Const(Value::Int(1)), line);
                self.emit(Instr::Add, line);
                self.emit(Instr::Store(idx), line);
                self.emit(Instr::Jump(top), line);
                let end = self.code.len();
                self.patch(jf, end as u32);
                for c in ctx.continues {
                    self.patch(c, inc);
                }
                for b in ctx.breaks {
                    self.patch(b, end as u32);
                }
            }
        }
        Ok(())
    }

    fn hidden_slot(&mut self) -> u16 {
        self.hidden += 1;
        let slot = self.max_slots;
        self.max_slots += 1;
        slot
    }

    // -- expressions -----------------------------------------------------

    /// Lower an expression (emits instructions).
    fn lower_expr(&mut self, e: &AppExpr) {
        self.at(e.line);
        match &e.kind {
            AppExprKind::Int(v) => self.emit(Instr::Const(Value::Int(*v)), e.line),
            AppExprKind::Float(v) => self.emit(Instr::Const(Value::Float(*v)), e.line),
            AppExprKind::Bool(v) => self.emit(Instr::Const(Value::Bool(*v)), e.line),
            AppExprKind::Str(v) => self.emit(Instr::Const(Value::Str(v.clone())), e.line),
            AppExprKind::Array(items) => {
                for i in items {
                    self.lower_expr(i);
                }
                self.at(e.line);
                self.emit(Instr::ArrNew(items.len() as u16), e.line);
            }
            AppExprKind::Index(base, index) => {
                self.lower_expr(base);
                self.lower_expr(index);
                self.at(e.line);
                self.emit(Instr::ArrGet, e.line);
            }
            AppExprKind::Cast { expr, ty } => {
                self.lower_expr(expr);
                self.at(e.line);
                let target = match ty {
                    Type::Int => CastTarget::Int,
                    Type::Float => CastTarget::Float,
                    Type::Str => CastTarget::Str,
                    _ => unreachable!("checker-rejected cast"),
                };
                self.emit(Instr::Cast(target), e.line);
            }
            AppExprKind::Neg(inner) => {
                self.lower_expr(inner);
                self.at(e.line);
                self.emit(Instr::Neg, e.line);
            }
            AppExprKind::Not(inner) => {
                self.lower_expr(inner);
                self.at(e.line);
                self.emit(Instr::Not, e.line);
            }
            AppExprKind::Binary(op, lhs, rhs) => match op {
                BinOp::And => {
                    self.lower_expr(lhs);
                    let jf = self.code.len();
                    self.at(e.line);
                    self.emit(Instr::JumpIfFalse(0), e.line);
                    self.lower_expr(rhs);
                    let jend = self.code.len();
                    self.at(e.line);
                    self.emit(Instr::Jump(0), e.line);
                    let f_at = self.code.len();
                    self.patch(jf, f_at as u32);
                    self.emit(Instr::Const(Value::Bool(false)), e.line);
                    let end = self.code.len();
                    self.patch(jend, end as u32);
                }
                BinOp::Or => {
                    self.lower_expr(lhs);
                    let jf = self.code.len();
                    self.at(e.line);
                    self.emit(Instr::JumpIfFalse(0), e.line);
                    self.emit(Instr::Const(Value::Bool(true)), e.line);
                    let jend = self.code.len();
                    self.at(e.line);
                    self.emit(Instr::Jump(0), e.line);
                    let rhs_at = self.code.len();
                    self.patch(jf, rhs_at as u32);
                    self.lower_expr(rhs);
                    let end = self.code.len();
                    self.patch(jend, end as u32);
                }
                _ => {
                    self.lower_expr(lhs);
                    self.lower_expr(rhs);
                    self.at(e.line);
                    let instr = match op {
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
                    };
                    self.emit(instr, e.line);
                }
            },
            AppExprKind::Call { name, args } => {
                self.lower_call(name, args, e.line);
            }
            AppExprKind::ModCall { module, name, args } => {
                let qualified = self.resolve_qualified(module, name, e.line);
                for a in args {
                    self.lower_expr(a);
                }
                self.at(e.line);
                self.emit(Instr::Call(qualified as u16), e.line);
            }
            AppExprKind::ModConst { module, name } => {
                let q = format!("{}::{}", module, name);
                let v: Option<&Value> = self
                    .ctx
                    .consts
                    .get(&q)
                    .copied()
                    .or_else(|| {
                        // Suffix match: the import alias may differ from the
                        // path-derived prefix.
                        self.ctx
                            .consts
                            .iter()
                            .find(|(n, _)| n.ends_with(&format!("::{}", q)))
                            .map(|(_, v)| *v)
                    });
                if let Some(v) = v {
                    self.emit(Instr::Const(v.clone()), e.line);
                } else {
                    panic!("webkir: unresolved module const `{}`", q);
                }
            }
            AppExprKind::Local(name) => {
                if let Some(slot) = self.try_slot(name) {
                    self.load_slot(slot, e.line);
                } else if self.ctx.is_state(name) {
                    // Handler/computed params bind state fields as slots.
                    panic!(
                        "webkir: state field `{}` is not a parameter of `{}` — the free-name walk missed it",
                        name, self.kir_name
                    );
                } else if self.ctx.is_computed(name) {
                    // Recompute the computed value through its own function.
                    let c = self
                        .ctx
                        .app
                        .computed
                        .iter()
                        .find(|c| &c.name == name)
                        .expect("checked")
                        .clone();
                    let idx = self.ctx.app_func_index(&c.kir);
                    let free = c.expr.free_names();
                    let (state_names, computed_names) = self.ctx.split_free(&free);
                    let mut params: Vec<String> =
                        state_names.into_iter().chain(computed_names).collect();
                    params.sort();
                    for p in params {
                        self.lower_expr(&AppExpr {
                            kind: AppExprKind::Local(p),
                            line: e.line,
                        });
                    }
                    self.at(e.line);
                    self.emit(Instr::Call(idx as u16), e.line);
                } else if let Some(v) = self.ctx.consts.get(name.as_str()).cloned() {
                    self.emit(Instr::Const(v.clone()), e.line);
                } else if event_builtins().iter().any(|(n, _, _)| n == name) {
                    // Handler builtin params are bound like locals; if it
                    // is not a slot here, this is a computed/init reading a
                    // builtin — impossible per the checker.
                    panic!("webkir: event builtin `{}` outside a handler", name);
                } else {
                    panic!("webkir: unresolved name `{}` in `{}`", name, self.kir_name);
                }
            }
        }
    }

    fn lower_call(&mut self, name: &str, args: &[AppExpr], line: u32) {
        // Resolution mirrors the checker exactly: print, then the console
        // natives, then user functions, then the app-scope builtins.
        if name == "print" {
            if let Some(a) = args.first() {
                self.lower_expr(a);
            }
            self.at(line);
            self.emit(Instr::Print, line);
            return;
        }
        let native = match name {
            "len" => Some(NativeId::Len),
            "abs" => Some(NativeId::Abs),
            "min" => Some(NativeId::Min),
            "max" => Some(NativeId::Max),
            _ => None,
        };
        if let Some(id) = native {
            for a in args {
                self.lower_expr(a);
            }
            self.at(line);
            self.emit(Instr::Native(id), line);
            return;
        }
        if let Some(&idx) = self.ctx.fidx.get(name) {
            for a in args {
                self.lower_expr(a);
            }
            self.at(line);
            self.emit(Instr::Call(idx as u16), line);
            return;
        }
        let app_native = match name {
            "str" => Some(NativeId::Str),
            "upper" => Some(NativeId::Upper),
            "lower" => Some(NativeId::Lower),
            "trim" => Some(NativeId::Trim),
            "contains" => Some(NativeId::Contains),
            "split" => Some(NativeId::Split),
            "join" => Some(NativeId::Join),
            _ => None,
        };
        if let Some(id) = app_native {
            for a in args {
                self.lower_expr(a);
            }
            self.at(line);
            self.emit(Instr::Native(id), line);
            return;
        }
        match name {
            "param" => {
                // S7: a route-parameter read. Lowered as a native so the
                // KIR stays app-scoped; the JS side reads the router's
                // current params through the K.h[23] helper (k_param in
                // kinnrt.js) — see gen_native's NativeId::Param arm.
                for a in args {
                    self.lower_expr(a);
                }
                self.at(line);
                self.emit(Instr::Native(NativeId::Param), line);
                return;
            }
            "store_save" | "store_load" | "store_clear" | "json_of" | "save_json" => {
                unreachable!("host builtins are rejected by the E3071 scope gate");
            }
            _ => panic!("webkir: unresolved function `{}`", name),
        }
    }

    fn resolve_qualified(&self, module: &str, name: &str, _line: u32) -> usize {
        let q = format!("{}::{}", module, name);
        if let Some(&i) = self.ctx.fidx.get(&q) {
            return i;
        }
        // Suffix resolution for alias/prefix mismatches.
        let hits: Vec<usize> = self
            .ctx
            .module
            .funcs
            .iter()
            .enumerate()
            .filter(|(_, f)| f.name.ends_with(&format!("::{}", q)))
            .map(|(i, _)| i)
            .collect();
        if hits.len() == 1 {
            return hits[0];
        }
        panic!("webkir: unresolved module function `{}`", q);
    }

    /// The static kind of an expression (mirror of the lowering; used for
    /// param/ret kinds so the JS generator stays typed).
    fn expr_kind(&mut self, e: &AppExpr) -> ValKind {
        match &e.kind {
            AppExprKind::Int(_) => ValKind::Int,
            AppExprKind::Float(_) => ValKind::Float,
            AppExprKind::Bool(_) => ValKind::Bool,
            AppExprKind::Str(_) => ValKind::Str,
            AppExprKind::Array(items) => {
                let mut elem: Option<ValKind> = None;
                for i in items {
                    let k = self.expr_kind(i);
                    elem = Some(match elem {
                        None => k,
                        Some(p) if p == k => p,
                        Some(_) => ValKind::Any,
                    });
                }
                ValKind::Array(Box::new(elem.unwrap_or(ValKind::Any)))
            }
            AppExprKind::Index(base, _) => elem_kind(&self.expr_kind(base)),
            AppExprKind::Cast { ty, .. } => match ty {
                Type::Int => ValKind::Int,
                Type::Float => ValKind::Float,
                Type::Str => ValKind::Str,
                _ => ValKind::Any,
            },
            AppExprKind::Neg(inner) => self.expr_kind(inner),
            AppExprKind::Not(_) => ValKind::Bool,
            AppExprKind::Binary(op, lhs, rhs) => match op {
                BinOp::And | BinOp::Or | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
                | BinOp::Eq | BinOp::Ne => ValKind::Bool,
                _ => join_add(&self.expr_kind(lhs), &self.expr_kind(rhs)),
            },
            AppExprKind::Call { name, .. } => {
                if name == "print" {
                    return ValKind::Unit;
                }
                match name.as_str() {
                    "len" => return ValKind::Int,
                    "abs" => {
                        return ValKind::Any;
                    }
                    "min" | "max" => return ValKind::Any,
                    _ => {}
                }
                if let Some(&idx) = self.ctx.fidx.get(name) {
                    return self.ctx.module.funcs[idx].ret_kind.clone();
                }
                match name.as_str() {
                    "str" | "upper" | "lower" | "trim" | "join" => ValKind::Str,
                    "contains" => ValKind::Bool,
                    "split" => ValKind::Array(Box::new(ValKind::Str)),
                    _ => ValKind::Any,
                }
            }
            AppExprKind::ModCall { module, name, .. } => {
                let idx = self.resolve_qualified(module, name, e.line);
                self.ctx.module.funcs[idx].ret_kind.clone()
            }
            AppExprKind::ModConst { .. } => ValKind::Any,
            AppExprKind::Local(name) => {
                if self.try_slot(name).is_some() {
                    // Locals: recompute by finding the decl — kinds come
                    // from the initializer; params carry theirs.
                    self.local_param_kind(name)
                } else if self.ctx.is_state(name) {
                    self.ctx.state_kind(name).expect("checked")
                } else if self.ctx.is_computed(name) {
                    self.ctx.computed_kind(name)
                } else if let Some(v) = self.ctx.consts.get(name.as_str()) {
                    kind_of_value(v)
                } else {
                    ValKind::Any
                }
            }
        }
    }

    fn local_param_kind(&self, name: &str) -> ValKind {
        self.local_kinds
            .get(name)
            .cloned()
            .unwrap_or(ValKind::Any)
    }
}

// Free-name collection over app IR (scope-aware for handlers).
pub fn free_names_expr(e: &AppExpr, out: &mut BTreeSet<String>) {
    match &e.kind {
        AppExprKind::Int(_) | AppExprKind::Float(_) | AppExprKind::Bool(_)
        | AppExprKind::Str(_) => {}
        AppExprKind::Local(n) => {
            out.insert(n.clone());
        }
        AppExprKind::Array(items) => {
            for i in items {
                free_names_expr(i, out);
            }
        }
        AppExprKind::Index(base, index) => {
            free_names_expr(base, out);
            free_names_expr(index, out);
        }
        AppExprKind::Cast { expr, .. } => free_names_expr(expr, out),
        AppExprKind::Neg(i) | AppExprKind::Not(i) => free_names_expr(i, out),
        AppExprKind::Binary(_, l, r) => {
            free_names_expr(l, out);
            free_names_expr(r, out);
        }
        AppExprKind::Call { args, .. } | AppExprKind::ModCall { args, .. } => {
            for a in args {
                free_names_expr(a, out);
            }
        }
        AppExprKind::ModConst { .. } => {}
    }
}

impl AppExpr {
    /// Free value names of an expression (unresolved by local scope —
    /// scope-free contexts like computeds have no locals).
    pub fn free_names(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        free_names_expr(self, &mut out);
        out
    }
}

/// Free names of a statement list, shadowing declared names (var decls,
/// for vars) — for the handler ABI walk.
pub fn free_names_stmts(stmts: &[AppStmt]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut shadow = BTreeSet::new();
    for s in stmts {
        free_names_stmt(s, &mut out, &mut shadow);
    }
    out
}

fn free_names_stmt(s: &AppStmt, out: &mut BTreeSet<String>, shadow: &mut BTreeSet<String>) {
    match &s.kind {
        AppStmtKind::VarDecl { name, init, .. } => {
            let mut e = BTreeSet::new();
            free_names_expr(init, &mut e);
            for n in e {
                if !shadow.contains(&n) {
                    out.insert(n);
                }
            }
            shadow.insert(name.clone());
        }
        AppStmtKind::Assign { name, value, .. } => {
            if !shadow.contains(name) {
                out.insert(name.clone());
            }
            let mut e = BTreeSet::new();
            free_names_expr(value, &mut e);
            for n in e {
                if !shadow.contains(&n) {
                    out.insert(n);
                }
            }
        }
        AppStmtKind::IndexAssign { base, index, value } => {
            for x in [base, index, value] {
                let mut e = BTreeSet::new();
                free_names_expr(x, &mut e);
                for n in e {
                    if !shadow.contains(&n) {
                        out.insert(n);
                    }
                }
            }
        }
        AppStmtKind::If { cond, then_branch, else_branch } => {
            collect_cond(cond, out, shadow);
            let mut scoped = shadow.clone();
            for t in then_branch {
                free_names_stmt(t, out, &mut scoped);
            }
            if let Some(e) = else_branch {
                let mut scoped2 = shadow.clone();
                free_names_stmt(e, out, &mut scoped2);
            }
        }
        AppStmtKind::While { cond, body } => {
            collect_cond(cond, out, shadow);
            let mut scoped = shadow.clone();
            for b in body {
                free_names_stmt(b, out, &mut scoped);
            }
        }
        AppStmtKind::For { var, iter, body } => {
            match iter {
                ForIterModel::Range { start, end, .. } => {
                    collect_cond(start, out, shadow);
                    collect_cond(end, out, shadow);
                }
                ForIterModel::Expr(e) => collect_cond(e, out, shadow),
            }
            shadow.insert(var.clone());
            let mut scoped = shadow.clone();
            for b in body {
                free_names_stmt(b, out, &mut scoped);
            }
        }
        AppStmtKind::Return { value } => {
            if let Some(v) = value {
                collect_cond(v, out, shadow);
            }
        }
        AppStmtKind::Break | AppStmtKind::Continue => {}
        AppStmtKind::Expr(e) => collect_cond(e, out, shadow),
        AppStmtKind::Block(b) => {
            let mut scoped = shadow.clone();
            for s in b {
                free_names_stmt(s, out, &mut scoped);
            }
        }
    }
}

fn collect_cond(e: &AppExpr, out: &mut BTreeSet<String>, shadow: &BTreeSet<String>) {
    let mut names = BTreeSet::new();
    free_names_expr(e, &mut names);
    for n in names {
        if !shadow.contains(&n) {
            out.insert(n);
        }
    }
}

impl<'a> AppCtx<'a> {
    /// Index (into the app function list produced by app_funcs) of the
    /// function with this KIR name. Computed functions follow `app:init`.
    fn app_func_index(&self, kir: &str) -> usize {
        let mut i = 1; // app:init
        for c in &self.app.computed {
            if c.kir == kir {
                return i;
            }
            i += 1;
        }
        panic!("webkir: unknown computed function `{}`", kir);
    }
}


#[cfg(test)]
mod dist_tests {
    /// The shipped artifact itself must fit the hard budget: this is the
    /// exact blob the GitHub API reports for `kinnrt.js`.
    #[test]
    fn kinnrt_dist_size_budget() {
        assert!(
            super::KINNRT_JS.len() <= super::KINNRT_BUDGET,
            "checked-in kinnrt.js is {} bytes, budget is {}",
            super::KINNRT_JS.len(),
            super::KINNRT_BUDGET
        );
    }

    /// `kinnrt.js` must be exactly `js_minify(kinnrt.src.js)` — the
    /// checked-in artifact may never drift from the readable source.
    /// Regenerate with `KINNRT_WRITE=1 cargo test -p kinc kinnrt`.
    #[test]
    fn kinnrt_artifact_byte_identity() {
        let generated = super::js_minify(super::KINNRT_SRC_JS);
        if generated != super::KINNRT_JS {
            if std::env::var_os("KINNRT_WRITE").is_some() {
                super::kinnrt_write_artifact(&generated);
                eprintln!(
                    "KINNRT_WRITE: rewrote kinnrt.js ({} bytes)",
                    generated.len()
                );
                return;
            }
            panic!(
                "checked-in kinnrt.js ({:.1} KB) is NOT the deterministic \
                 minify(kinnrt.src.js) output ({:.1} KB): regenerate with \
                 KINNRT_WRITE=1 cargo test -p kinc kinnrt",
                super::KINNRT_JS.len() as f64 / 1024.0,
                generated.len() as f64 / 1024.0
            );
        }
    }

    /// The minifier is deterministic: same source in, same bytes out.
    #[test]
    fn kinnrt_minify_deterministic() {
        let a = super::js_minify(super::KINNRT_SRC_JS);
        let b = super::js_minify(super::KINNRT_SRC_JS);
        assert_eq!(a, b);
    }
}

