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
        consts: Vec::new(),
        imports: HashMap::new(),
        cur_file: 0,
        file: "<main>".to_string(),
        scopes: Vec::new(),
        ret: Type::Void,
        loop_depth: 0,
        assigned: HashSet::new(),
        var_decls: Vec::new(),
        app: None,
        handler_depth: 0,
    };

    // Pass 0: website declarations (0.3.0). Only the entry file may
    // declare one; every rule below is target-independent (ADR-0018) —
    // the web and android backends consume an already-valid site.
    let mut entry_site: Option<&WebsiteDecl> = None;
    for (i, unit) in project.files.iter().enumerate() {
        c.file = unit.path.clone();
        for w in &unit.program.websites {
            if i != 0 {
                c.error(
                    "E3037",
                    "website declarations belong in the entry file (imported modules cannot declare websites)",
                    w.line,
                    w.col,
                    7,
                );
                continue;
            }
            if entry_site.is_some() {
                c.error(
                    "E3030",
                    "a program can declare only one website",
                    w.line,
                    w.col,
                    7,
                );
                continue;
            }
            if w.name.trim().is_empty() {
                c.error(
                    "E3033",
                    "the website name must not be empty",
                    w.line,
                    w.col,
                    (w.name.chars().count() as u32) + 2,
                );
                continue;
            }
            entry_site = Some(w);
        }
    }
    if let Some(w) = entry_site {
        // Websites are entry-only (E3037), so their diagnostics belong
        // to the entry file — restore `c.file` after the loop above
        // left it at the last project file. Without this, every
        // `check_website` diagnostic (the E3032 family, W3035–W3037)
        // in a multi-file program rendered under the last imported
        // module (P-028); this mirrors the app path below.
        c.file = project.files[0].path.clone();
        check_website(&mut c, w);
    }

    // Pass 0b: app declarations (0.4.0 S2 presence + S3 slice 1
    // declaration validation). Presence rules first: E3051/E3052 are
    // implemented so a program never silently builds its `website`
    // while an `app` half is ignored, and so imported modules cannot
    // smuggle an app past the entry-file rule. E3062 (empty app name)
    // mirrors E3033's placement in pass 0: checked before registration,
    // and an unnamed app is not further checked.
    let mut entry_app: Option<&AppDecl> = None;
    for (i, unit) in project.files.iter().enumerate() {
        c.file = unit.path.clone();
        for a in &unit.program.apps {
            if i != 0 {
                c.error(
                    "E3051",
                    "app declarations belong in the entry file (imported modules cannot declare apps)",
                    a.line,
                    a.col,
                    3,
                );
                continue;
            }
            if entry_app.is_some() {
                c.error(
                    "E3052",
                    "a program can declare only one app",
                    a.line,
                    a.col,
                    3,
                );
                continue;
            }
            if entry_site.is_some() {
                c.error(
                    "E3052",
                    "a program declares a `website` or an `app`, never both — remove one of them",
                    a.line,
                    a.col,
                    3,
                );
                continue;
            }
            if a.name.trim().is_empty() {
                let mut d = Diagnostic::error(
                    "E3062",
                    "the app name must not be empty",
                    a.line,
                    a.col,
                    (a.name.chars().count() as u32) + 2,
                );
                d.hint = Some(
                    "name the app in its declaration, like `app \"Tasks\" { ... }` — \
                     whitespace-only names are rejected too"
                        .to_string(),
                );
                c.diags.push(d.in_file(c.file.clone()));
                continue;
            }
            entry_app = Some(a);
        }
    }
    // S3 slice 1: declaration validation for the registered app —
    // route rules (E3055–E3057) and duplicate state/computed/prop
    // names per scope (E3042). Everything else (scope resolution,
    // handler subset, prop checking, computed cycles, stores, fields,
    // animations) stays unchecked until later S3 slices — the honest
    // intermediate state is documented in LANGUAGE_SPEC §C3.
    if let Some(a) = entry_app {
        // The app is entry-only (E3051), so its diagnostics belong to
        // the entry file — restore `c.file` after the loop above left
        // it at the last project file.
        c.file = project.files[0].path.clone();
        check_app(&mut c, a);
    }

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

        // Top-level consts (0.4.0 S1): validated in source order, so a
        // const initializer sees exactly the consts declared earlier in
        // this file — no forward references, no cycles. `const_vals`
        // keeps the real folded values (the evaluator needs them when a
        // later const references an earlier one); `const_table` keeps the
        // type/position info the rest of the checker resolves against.
        let mut const_table: HashMap<String, ConstInfo> = HashMap::new();
        let mut const_vals: HashMap<String, (kinrt::Value, Type)> = HashMap::new();
        for cd in &unit.program.consts {
            if BUILTINS.contains(&cd.name.as_str()) {
                c.error(
                    "E3016",
                    format!(
                        "`{}` is a builtin function; a `const` cannot use that name",
                        cd.name
                    ),
                    cd.line,
                    cd.col,
                    cd.name.len() as u32,
                );
                continue;
            }
            if let Some(sig) = sigs.get(&cd.name) {
                c.error(
                    "E3002",
                    format!(
                        "duplicate definition `{}` (also declared as a function at line {})",
                        cd.name, sig.line
                    ),
                    cd.line,
                    cd.col,
                    cd.name.len() as u32,
                );
                continue;
            }
            if let Some(first) = const_table.get(&cd.name) {
                c.error(
                    "E3002",
                    format!(
                        "duplicate const `{}` (first defined at line {})",
                        cd.name, first.line
                    ),
                    cd.line,
                    cd.col,
                    cd.name.len() as u32,
                );
                continue;
            }
            if cd.ty == Type::Void {
                c.error(
                    "E3015",
                    format!("a `const` cannot have type void (`{}`)", cd.name),
                    cd.line,
                    cd.col,
                    cd.name.len() as u32,
                );
                continue;
            }
            match crate::consteval::eval_const_expr(&cd.init, &const_vals, Some(&cd.ty)) {
                Ok((value, ty)) => {
                    if ty != Type::Error && ty != cd.ty {
                        c.mismatch(&cd.ty, &ty, cd.line, cd.col, cd.name.len() as u32);
                    }
                    const_vals.insert(cd.name.clone(), (value, cd.ty.clone()));
                    const_table.insert(
                        cd.name.clone(),
                        ConstInfo { ty: cd.ty.clone(), line: cd.line },
                    );
                }
                Err(crate::consteval::ConstError::NotConstant(msg)) => {
                    c.error(
                        "E3039",
                        format!(
                            "invalid constant expression for `{}`: {} (allowed: literals, array literals, operators and earlier consts of this file)",
                            cd.name, msg
                        ),
                        cd.init.line,
                        cd.init.col,
                        1,
                    );
                }
                Err(crate::consteval::ConstError::Arithmetic(msg)) => {
                    c.error("E3039", msg, cd.init.line, cd.init.col, 1);
                }
            }
        }
        c.sigs.push(sigs);
        c.consts.push(const_table);
    }

    // Pass 1b: `main` requirements — entry file only. A program that
    // declares a website or an app is a web-target program and does not
    // need `main` (run/build report guidance for it instead of a
    // compile error).
    let main_info = c.sigs[0]
        .get("main")
        .map(|m| (m.params.len(), m.ret.clone(), m.line, m.col));
    match main_info {
        None if entry_site.is_none() && entry_app.is_none() => {
            c.error("E3008", "program must define `func main()`", 1, 1, 0);
        }
        None => {} // website/app-only program: see `kinn build --target web`
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

    // Pass 1c: app semantics (0.4.0 S3 slices 3–4) — scope resolution,
    // binding typing, the app builtin table and E3058/E3066/E3043/W3041
    // for the app registered in pass 0b, plus the instantiation-site and
    // handler-subset rules of slice 4 (E3040/E3041, E3045/E3046, W3040).
    // Runs after pass 1, because binding expressions and handler bodies
    // resolve against the entry file's functions, consts and imports
    // through the ordinary checker (B.6: handler bodies are official
    // statements), and before pass 2 so app diagnostics keep preceding
    // function-body diagnostics.
    if let Some(a) = entry_app {
        c.cur_file = 0;
        c.file = project.files[0].path.clone();
        c.mode = project.files[0].program.mode;
        c.imports = project.files[0]
            .imports
            .iter()
            .map(|r| (r.alias.clone(), r.file))
            .collect();
        check_app_semantics(&mut c, a);
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
                    VarInfo { ty: prm.ty.clone(), mutable: false, line: prm.line, col: prm.col, app_kind: None },
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
                                    "variable `{}` is declared `var` but never reassigned; consider `fin`",
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

/// Semantic rules for a checked-as-present website declaration
/// (codes E3030–E3037, W3035–W3037; documented in LANGUAGE_SPEC).
fn check_website(c: &mut Checker, w: &WebsiteDecl) {
    // Page names must be unique. Slugs are lowercased by the backends,
    // so the comparison is case-insensitive (`Home` and `home` would
    // otherwise produce two `home.html` files).
    let mut seen: HashMap<String, u32> = HashMap::new();
    for p in &w.pages {
        let key = p.name.to_lowercase();
        if let Some(first_line) = seen.get(&key) {
            c.error(
                "E3031",
                format!("duplicate page name `{}` (first defined at line {})", p.name, first_line),
                p.line,
                p.col,
                p.name.chars().count() as u32,
            );
            continue;
        }
        seen.insert(key, p.line);

        let mut title_count = 0u32;
        for e in &p.elements {
            match &e.kind {
                ElementKind::Title(_) => {
                    title_count += 1;
                    if title_count == 2 {
                        c.warn(
                            "W3036",
                            "page has more than one `title`; the first one is used and later ones render as section headings",
                            e.line,
                            e.col,
                            5,
                        );
                    }
                }
                ElementKind::Heading(v) | ElementKind::Text(v) => {
                    if v.trim().is_empty() {
                        c.warn(
                            "W3037",
                            format!("`{}` content is empty and renders nothing", e.kind.kind_name()),
                            e.line,
                            e.col,
                            e.kind.kind_name().len() as u32,
                        );
                    }
                }
                ElementKind::Button { label, .. } | ElementKind::Link { label, .. } => {
                    if label.trim().is_empty() {
                        c.error(
                            "E3035",
                            format!("`{}` label must not be empty", e.kind.kind_name()),
                            e.line,
                            e.col,
                            e.kind.kind_name().len() as u32,
                        );
                    }
                }
                ElementKind::Image { src, alt, alt_line, alt_col } => {
                    if alt.trim().is_empty() {
                        c.error(
                            "E3034",
                            "image alt text must not be empty (accessibility: screen readers need a description)",
                            *alt_line,
                            *alt_col,
                            3,
                        );
                    }
                    if src.trim().is_empty() {
                        c.error(
                            "E3036",
                            "image source must not be empty",
                            e.line,
                            e.col,
                            5,
                        );
                    }
                }
                ElementKind::Divider => {}
            }
        }
    }

    // Every navigation target must reference a declared page.
    for p in &w.pages {
        for e in &p.elements {
            let (label, target, tl, tc) = match &e.kind {
                ElementKind::Button { label, target, target_line, target_col }
                | ElementKind::Link { label, target, target_line, target_col } => {
                    (label, target, *target_line, *target_col)
                }
                _ => continue,
            };
            if !w.pages.iter().any(|p2| p2.name == *target) {
                let mut d = Diagnostic::error(
                    "E3032",
                    format!(
                        "`{}` opens page `{}` which is not declared in this website",
                        label, target
                    ),
                    tl,
                    tc,
                    target.chars().count() as u32,
                );
                d.hint = Some(format!(
                    "declared pages: {}",
                    w.pages.iter().map(|p2| p2.name.as_str()).collect::<Vec<_>>().join(", ")
                ));
                d.file = c.file.clone();
                c.diags.push(d);
            }
        }
    }

    // Pages beyond the first that nothing links to are unreachable in
    // the generated navigation.
    for p in w.pages.iter().skip(1) {
        let linked = w.pages.iter().any(|other| {
            other.elements.iter().any(|e| {
                matches!(
                    &e.kind,
                    ElementKind::Button { target, .. } | ElementKind::Link { target, .. }
                    if *target == p.name
                )
            })
        });
        if !linked {
            c.warn(
                "W3035",
                format!(
                    "page `{}` is never linked from another page (visitors can only reach it by URL)",
                    p.name
                ),
                p.line,
                p.col,
                p.name.chars().count() as u32,
            );
        }
    }
}

/// S3 slices 1–2: app declaration validation (B.11 registered
/// meanings). Covers E3042 (duplicate state/computed/prop name in one
/// scope), E3055 (malformed route path), E3056 (duplicate route),
/// E3057 (SPA without `not_found`, or `not_found` in MPA), E3063
/// (unknown page target — routes, `not_found`, and `open` on buttons
/// and links anywhere in the app's views), E3064 (duplicate page
/// name, case-insensitive) and E3065 (duplicate component name,
/// case-sensitive). Pages and components are separate namespaces: a
/// page and a component may share a name (use-site position
/// disambiguates). NOT covered yet, honestly: scope resolution,
/// handler subset enforcement, prop checking at instantiation sites,
/// computed cycles, and store/field/animation semantics (later S3
/// slices — see LANGUAGE_SPEC §C3 and ROADMAP).
fn check_app(c: &mut Checker, a: &AppDecl) {
    // -- E3042: duplicate names in one scope --------------------------------
    //
    // Two scopes exist at declaration level: the app scope (state
    // fields + computed values share one namespace — `state` blocks
    // merge) and each component's own scope (props + component state
    // fields). Cross-kind collisions count (`state x` + `computed x`
    // is a duplicate): a binding expression could not tell them apart.

    // App scope. State fields anchor on the name token; computed
    // declarations anchor on the `computed` keyword.
    let mut app_scope: HashMap<&str, (&'static str, u32)> = HashMap::new();
    for f in &a.state {
        if let Some((kind, first_line)) = app_scope.get(f.name.as_str()) {
            let mut d = Diagnostic::error(
                "E3042",
                format!(
                    "duplicate name `{}` in the app scope (first declared as a {} at line {})",
                    f.name, kind, first_line
                ),
                f.line,
                f.col,
                f.name.chars().count() as u32,
            );
            d.hint = Some(
                "app state fields and `computed` values share one namespace — rename one of them"
                    .to_string(),
            );
            c.diags.push(d.in_file(c.file.clone()));
            continue;
        }
        app_scope.insert(f.name.as_str(), ("state field", f.line));
    }
    for cd in &a.computed {
        if let Some((kind, first_line)) = app_scope.get(cd.name.as_str()) {
            let mut d = Diagnostic::error(
                "E3042",
                format!(
                    "duplicate name `{}` in the app scope (first declared as a {} at line {})",
                    cd.name, kind, first_line
                ),
                cd.line,
                cd.col,
                8, // the `computed` keyword
            );
            d.hint = Some(
                "app state fields and `computed` values share one namespace — rename one of them"
                    .to_string(),
            );
            c.diags.push(d.in_file(c.file.clone()));
            continue;
        }
        app_scope.insert(cd.name.as_str(), ("computed value", cd.line));
    }

    // Component scopes: props and component state fields.
    for comp in &a.components {
        let mut scope: HashMap<&str, (&'static str, u32)> = HashMap::new();
        for p in &comp.props {
            if let Some((kind, first_line)) = scope.get(p.name.as_str()) {
                let mut d = Diagnostic::error(
                    "E3042",
                    format!(
                        "duplicate name `{}` in component `{}` (first declared as a {} at line {})",
                        p.name, comp.name, kind, first_line
                    ),
                    p.line,
                    p.col,
                    4, // the `prop` keyword
                );
                d.hint = Some(format!(
                    "props and state fields of component `{}` share one namespace — rename one of them",
                    comp.name
                ));
                c.diags.push(d.in_file(c.file.clone()));
                continue;
            }
            scope.insert(p.name.as_str(), ("prop", p.line));
        }
        for f in &comp.state {
            if let Some((kind, first_line)) = scope.get(f.name.as_str()) {
                let mut d = Diagnostic::error(
                    "E3042",
                    format!(
                        "duplicate name `{}` in component `{}` (first declared as a {} at line {})",
                        f.name, comp.name, kind, first_line
                    ),
                    f.line,
                    f.col,
                    f.name.chars().count() as u32,
                );
                d.hint = Some(format!(
                    "props and state fields of component `{}` share one namespace — rename one of them",
                    comp.name
                ));
                c.diags.push(d.in_file(c.file.clone()));
                continue;
            }
            scope.insert(f.name.as_str(), ("state field", f.line));
        }
    }

    // -- E3064/E3065: duplicate page and component names ---------------------
    //
    // Pages and components are SEPARATE namespaces: a page and a
    // component may share a name, because use sites disambiguate —
    // `<Card>(...)` instantiates the component, `open Card` /
    // `route -> Card` targets the page. Within each namespace the name
    // must be unique. Page names compare case-insensitively (mirrors
    // E3031: page slugs are lowercased by the backends); component
    // names are identifiers and compare case-sensitively (matching
    // function names, E3002).
    let mut seen_app_pages: HashMap<String, u32> = HashMap::new();
    for p in &a.pages {
        let key = p.name.to_lowercase();
        if let Some(first_line) = seen_app_pages.get(&key) {
            let mut d = Diagnostic::error(
                "E3064",
                format!(
                    "duplicate page name `{}` (first defined at line {})",
                    p.name, first_line
                ),
                p.line,
                p.col,
                4, // the `page` keyword anchors the duplicate
            );
            d.hint = Some(
                "page names are case-insensitive (they become lowercase slugs in the generated output)"
                    .to_string(),
            );
            c.diags.push(d.in_file(c.file.clone()));
            continue;
        }
        seen_app_pages.insert(key, p.line);
    }
    let mut seen_components: HashMap<&str, u32> = HashMap::new();
    for comp in &a.components {
        if let Some(first_line) = seen_components.get(comp.name.as_str()) {
            let mut d = Diagnostic::error(
                "E3065",
                format!(
                    "duplicate component name `{}` (first defined at line {})",
                    comp.name, first_line
                ),
                comp.line,
                comp.col,
                9, // the `component` keyword anchors the duplicate
            );
            d.hint = Some(
                "component names are case-sensitive identifiers (like function names)".to_string(),
            );
            c.diags.push(d.in_file(c.file.clone()));
            continue;
        }
        seen_components.insert(comp.name.as_str(), comp.line);
    }

    // -- E3055/E3056: routes -------------------------------------------------
    //
    // A route path is relative (no leading/trailing `/`), has no empty
    // segments, and every segment is either a literal of
    // `[A-Za-z0-9._-]` (`.`/`..` excluded — they would escape the
    // generated output directory) or a `:name` parameter (identifier,
    // unique within the path). Two routes are duplicates (E3056) when
    // their match shapes are identical — literals equal and parameters
    // at the same positions; parameter NAMES may differ (`item/:id`
    // and `item/:name` match the same addresses and no router could
    // tell them apart).
    let mut shapes: HashMap<RouteShape, (&str, u32)> = HashMap::new();
    for r in &a.routes {
        if let Some(reason) = route_path_error(&r.path) {
            let mut d = Diagnostic::error(
                "E3055",
                format!("malformed route path `{}`: {}", r.path, reason),
                r.path_line,
                r.path_col,
                (r.path.chars().count() as u32) + 2,
            );
            d.hint = Some(
                "route paths are `/`-separated literal or `:param` segments, like \"items/:id\""
                    .to_string(),
            );
            c.diags.push(d.in_file(c.file.clone()));
            continue; // a malformed path cannot be shape-compared
        }
        let shape = RouteShape::of(&r.path);
        if let Some((first_path, first_line)) = shapes.get(&shape) {
            let mut d = Diagnostic::error(
                "E3056",
                format!(
                    "duplicate route `{}` — it matches the same addresses as `route \"{}\"` (line {})",
                    r.path, first_path, first_line
                ),
                r.path_line,
                r.path_col,
                (r.path.chars().count() as u32) + 2,
            );
            d.hint = Some(
                "two routes must be distinguishable — change one path or remove one route".to_string(),
            );
            c.diags.push(d.in_file(c.file.clone()));
            continue;
        }
        shapes.insert(shape, (r.path.as_str(), r.path_line));
    }

    // -- E3057: navigation mode vs not_found ---------------------------------
    let spa = a.navigation == Some(NavigationMode::Spa);
    if spa && a.not_found.is_none() {
        let mut d = Diagnostic::error(
            "E3057",
            "SPA navigation needs a `not_found` page — the router must render something for unknown paths",
            a.line,
            a.col,
            3, // the `app` keyword
        );
        d.hint = Some(
            "add `not_found <Page>` next to `navigation spa`, pointing at a declared page".to_string(),
        );
        c.diags.push(d.in_file(c.file.clone()));
    }
    if !spa {
        if let Some(nf) = &a.not_found {
            let mut d = Diagnostic::error(
                "E3057",
                "`not_found` is a SPA-only section — MPA serves one file per page and has no unknown-path fallback to render",
                nf.line,
                nf.col,
                nf.page.chars().count() as u32,
            );
            d.hint = Some(
                "remove the `not_found` section, or switch to `navigation spa` (MPA is the default)"
                    .to_string(),
            );
            c.diags.push(d.in_file(c.file.clone()));
        }
    }

    // -- E3063: page-target resolution ---------------------------------------
    //
    // Every page-target position must name a declared page: `route
    // "path" -> Page`, `not_found Page`, and `open <Page>` on buttons
    // and links anywhere in the app's views (pages and components;
    // `when`/`otherwise`, `each` and `form` bodies included). This is
    // the app-scoped sibling of E3032 — E3032 is website-scoped and
    // frozen, it is never reused here. A `not_found` rejected by E3057
    // (MPA) is not further resolved, mirroring the established "invalid
    // declarations are not further checked" pattern (E3062, E3055).
    let page_names: Vec<&str> = a.pages.iter().map(|p| p.name.as_str()).collect();
    let component_names: HashSet<&str> =
        a.components.iter().map(|comp| comp.name.as_str()).collect();
    for r in &a.routes {
        unknown_page_target(
            c,
            &r.target,
            &format!("route \"{}\"", r.path),
            "targets",
            &page_names,
            &component_names,
        );
    }
    if spa {
        if let Some(nf) = &a.not_found {
            unknown_page_target(
                c,
                nf,
                "`not_found`",
                "targets",
                &page_names,
                &component_names,
            );
        }
    }
    let mut opens: Vec<(&OpenTarget, String, &str)> = Vec::new();
    for p in &a.pages {
        collect_open_targets(&p.elements, &mut opens);
    }
    for comp in &a.components {
        collect_open_targets(&comp.view, &mut opens);
    }
    for (t, subject, verb) in opens {
        unknown_page_target(c, t, &subject, verb, &page_names, &component_names);
    }
}

/// One E3063 for an unknown page target. The hint lists the declared
/// pages; a target that names a **component** gets the namespace note
/// ('X' is a component; routes target pages) — pages and components
/// are separate namespaces, so only pages satisfy a target.
fn unknown_page_target(
    c: &mut Checker,
    target: &OpenTarget,
    subject: &str,
    verb: &str,
    page_names: &[&str],
    component_names: &HashSet<&str>,
) {
    if page_names.iter().any(|p| *p == target.page) {
        return;
    }
    let mut d = Diagnostic::error(
        "E3063",
        format!(
            "{} {} page `{}` which is not declared in this app",
            subject, verb, target.page
        ),
        target.line,
        target.col,
        target.page.chars().count() as u32,
    );
    let pages_list = if page_names.is_empty() {
        "this app declares no pages".to_string()
    } else {
        format!("declared pages: {}", page_names.join(", "))
    };
    d.hint = Some(if component_names.contains(target.page.as_str()) {
        format!(
            "'{}' is a component; routes target pages — {}",
            target.page, pages_list
        )
    } else {
        pages_list
    });
    c.diags.push(d.in_file(c.file.clone()));
}

/// Every `open <Page>` target inside a view list, in source order,
/// paired with its message subject and verb ("opens", mirroring
/// E3032). Literal labels name the button (`button "Go"`); non-literal
/// labels fall back to the element kind name (`button`). Recursive
/// over `when`/`otherwise`, `each` and `form` bodies. `input bind`/
/// `field bind` reuse the `OpenTarget` type for **state keys** — they
/// are not page targets, so they are not collected (scope resolution
/// arrives in slice 3).
fn collect_open_targets<'a>(
    elements: &'a [ViewElement],
    targets: &mut Vec<(&'a OpenTarget, String, &'static str)>,
) {
    for e in elements {
        match &e.kind {
            ViewElementKind::Button { label, open: Some(t), .. } => {
                let subject = match &label.kind {
                    ExprKind::StrLit(s) => format!("button \"{s}\""),
                    _ => "button".to_string(),
                };
                targets.push((t, subject, "opens"));
            }
            ViewElementKind::Link { label, open: Some(t), .. } => {
                let subject = match &label.kind {
                    ExprKind::StrLit(s) => format!("link \"{s}\""),
                    _ => "link".to_string(),
                };
                targets.push((t, subject, "opens"));
            }
            ViewElementKind::Button { open: None, .. } | ViewElementKind::Link { open: None, .. } => {}
            ViewElementKind::When { then_branch, else_branch, .. } => {
                collect_open_targets(then_branch, targets);
                if let Some(els) = else_branch {
                    collect_open_targets(els, targets);
                }
            }
            ViewElementKind::Each { body, .. } => collect_open_targets(body, targets),
            ViewElementKind::Form { body, .. } => collect_open_targets(body, targets),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// S3 slices 3–4: app scope resolution, binding typing, instantiation
// sites and the handler subset (0.4.0).
// ---------------------------------------------------------------------------

/// Scope resolution, binding typing, the app builtins and the
/// instantiation-site/handler-subset rules for a registered app (the
/// pass-0b rules and the slice-1/2 rules already ran: presence,
/// declaration validation, page targets).
///
/// Names resolve through the ordinary checker scopes: the app layer
/// (state fields + computed values) is the outermost scope, each
/// component pushes its props/state layer on top, `each` loop variables
/// are scoped to their body, and handler locals sit above the event
/// builtins. Reads that resolve nowhere report **E3066** (the
/// app-scoped sibling of E3001); assignment rules are B.4's —
/// **E3043** for props/computed values/event builtins, E3004 unchanged
/// for `fin` locals and consts; `param("…")` must name a declared route
/// parameter (**E3058**); a handler local shadowing a state field warns
/// **W3041**. Binding expressions type through the ordinary checker:
/// text/heading/label/placeholder expressions must be strings, `when`
/// conditions bool, `each` iterables arrays (E3003/E3011, no implicit
/// conversions — B.5). Since slice 4, component instantiations validate
/// against the app's component declarations — **E3040** for an unknown
/// component, **E3041** for unknown/missing/duplicate/mistyped props —
/// and handler bodies obey the handler subset: **E3045** (`return`
/// illegal), **E3046** (duplicate handler per event name on one
/// element), **W3040** (`while` warns but stays legal).
///
/// NOT checked here, honestly (P-030, narrowed by slice 4): computed
/// cycles (E3044), component recursion/depth caps (E3053/E3054),
/// store/field/animation semantics (E3047–E3061), W3042–W3044 and
/// N3040. E3067–E3069 stay reserved.
fn check_app_semantics(c: &mut Checker, a: &AppDecl) {
    // `:name` parameters of valid routes; malformed paths are not
    // further checked (the established E3062/E3055 pattern).
    let mut route_params: Vec<String> = Vec::new();
    for r in &a.routes {
        if route_path_error(&r.path).is_none() {
            for seg in r.path.split('/') {
                if let Some(name) = seg.strip_prefix(':') {
                    route_params.push(name.to_string());
                }
            }
        }
    }
    // Component signatures for the instantiation-site rules (slice 4).
    // E3065 already rejected duplicate component names in pass 0b; the
    // first declaration wins here. E3042 already rejected duplicate
    // prop names within one component — the signature keeps every
    // declaration and lookups take the first, mirroring how the app
    // layer resolves duplicates ("the name still resolves to the first").
    let mut components: HashMap<String, ComponentSig> = HashMap::new();
    let mut component_order: Vec<String> = Vec::new();
    for comp in &a.components {
        if components.contains_key(&comp.name) {
            continue; // E3065 already fired in pass 0b; the first wins
        }
        components.insert(
            comp.name.clone(),
            ComponentSig {
                props: comp
                    .props
                    .iter()
                    .map(|p| PropSig {
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                        has_default: p.default.is_some(),
                    })
                    .collect(),
            },
        );
        component_order.push(comp.name.clone());
    }
    let pages: Vec<String> = a.pages.iter().map(|p| p.name.clone()).collect();
    let hint = app_scope_hint(a, None);
    c.app = Some(AppScope { route_params, hint, components, component_order, pages });

    // App layer. E3042 already rejected duplicates in this scope; a
    // duplicate insert keeps the first declaration (its error is
    // reported and the name still resolves to the first).
    let mut layer: HashMap<String, VarInfo> = HashMap::new();
    for f in &a.state {
        layer.entry(f.name.clone()).or_insert(VarInfo {
            ty: f.ty.clone(),
            mutable: true,
            line: f.line,
            col: f.col,
            app_kind: Some(AppVarKind::State),
        });
    }
    for cd in &a.computed {
        layer.entry(cd.name.clone()).or_insert(VarInfo {
            ty: Type::Error, // published below, in source order
            mutable: false,
            line: cd.line,
            col: cd.col,
            app_kind: Some(AppVarKind::Computed),
        });
    }

    // State initializers are binding expressions over the full app
    // layer (consts, functions and imports resolve as anywhere else).
    // An empty array literal is checkable against the declared type,
    // exactly like `fin a: [int] = [];`.
    for f in &a.state {
        let init_ty = match (&f.ty, &f.init.kind) {
            (Type::Array(_), ExprKind::ArrayLit(items)) if items.is_empty() => f.ty.clone(),
            _ => c.check_expr(&f.init),
        };
        if init_ty != Type::Error && init_ty != f.ty {
            c.mismatch(&f.ty, &init_ty, f.init.line, f.init.col, 1);
        }
    }

    // Computed expressions. Each computed publishes its type in source
    // order; references to later computed values poison silently until
    // the E3044 (cycles) slice — names always resolve, cycles do not
    // cascade as unknowns.
    c.scopes.push(layer);
    for cd in &a.computed {
        let t = c.check_expr(&cd.expr);
        if let Some(v) = c.scopes.last_mut().unwrap().get_mut(&cd.name) {
            if v.app_kind == Some(AppVarKind::Computed) && v.ty == Type::Error {
                v.ty = t;
            }
        }
    }

    // Pages: views and handlers against the app layer.
    for p in &a.pages {
        c.app.as_mut().unwrap().hint = app_scope_hint(a, None);
        check_app_elements(c, &p.elements);
    }

    // Components: the props/state layer shadows the app layer (legal —
    // the E3042 ruling; a handler local shadowing it warns W3041).
    for comp in &a.components {
        c.app.as_mut().unwrap().hint = app_scope_hint(a, Some(comp));
        let mut cl: HashMap<String, VarInfo> = HashMap::new();
        for p in &comp.props {
            cl.entry(p.name.clone()).or_insert(VarInfo {
                ty: p.ty.clone(),
                mutable: false,
                line: p.line,
                col: p.col,
                app_kind: Some(AppVarKind::Prop),
            });
        }
        for f in &comp.state {
            cl.entry(f.name.clone()).or_insert(VarInfo {
                ty: f.ty.clone(),
                mutable: true,
                line: f.line,
                col: f.col,
                app_kind: Some(AppVarKind::ComponentState),
            });
        }
        c.scopes.push(cl);
        // Prop defaults and component state initializers, like the app
        // state initializers above.
        for p in &comp.props {
            if let Some(d) = &p.default {
                let t = c.check_expr(d);
                if t != Type::Error && t != p.ty {
                    c.mismatch(&p.ty, &t, d.line, d.col, 1);
                }
            }
        }
        for f in &comp.state {
            let init_ty = match (&f.ty, &f.init.kind) {
                (Type::Array(_), ExprKind::ArrayLit(items)) if items.is_empty() => f.ty.clone(),
                _ => c.check_expr(&f.init),
            };
            if init_ty != Type::Error && init_ty != f.ty {
                c.mismatch(&f.ty, &init_ty, f.init.line, f.init.col, 1);
            }
        }
        check_app_elements(c, &comp.view);
        c.scopes.pop();
    }

    c.scopes.pop();
    c.app = None;
}

/// The E3066 hint: what the scope being checked makes visible, in
/// source order, so the unknown-name diagnostic teaches the names that
/// exist. Component contexts list their props/state first.
fn app_scope_hint(a: &AppDecl, comp: Option<&ComponentDecl>) -> String {
    let names = |xs: &[String]| xs.join(", ");
    let mut parts: Vec<String> = Vec::new();
    if let Some(comp) = comp {
        if !comp.props.is_empty() {
            parts.push(format!("props: {}", names(&comp.props.iter().map(|p| p.name.clone()).collect::<Vec<_>>())));
        }
        if !comp.state.is_empty() {
            parts.push(format!(
                "component state: {}",
                names(&comp.state.iter().map(|f| f.name.clone()).collect::<Vec<_>>())
            ));
        }
    }
    if !a.state.is_empty() {
        parts.push(format!(
            "app state: {}",
            names(&a.state.iter().map(|f| f.name.clone()).collect::<Vec<_>>())
        ));
    }
    if !a.computed.is_empty() {
        parts.push(format!(
            "computed: {}",
            names(&a.computed.iter().map(|f| f.name.clone()).collect::<Vec<_>>())
        ));
    }
    if parts.is_empty() {
        "this app declares no state fields, computed values or props".to_string()
    } else {
        format!("declared here: {}", parts.join("; "))
    }
}

/// Views (B.5): binding expressions type through the ordinary checker
/// with the app/component layers open. `each` loop variables are
/// read-only and scoped to the body (mirroring `for`). Handlers are
/// official statement bodies (B.6) checked by the existing checker with
/// the event builtins in scope and the handler subset enforced (slice 4:
/// E3045/E3046, W3040). Component instantiations validate against the
/// app's component declarations since slice 4 (E3040/E3041).
fn check_app_elements(c: &mut Checker, elements: &[ViewElement]) {
    let expect = |c: &mut Checker, e: &Expr, want: &Type| {
        let t = c.check_expr(e);
        if t != Type::Error && t != *want {
            c.mismatch(want, &t, e.line, e.col, 1);
        }
    };
    for e in elements {
        match &e.kind {
            ViewElementKind::Title { value, .. }
            | ViewElementKind::Heading { value, .. }
            | ViewElementKind::Text { value, .. } => expect(c, value, &Type::Str),
            ViewElementKind::Button { label, handlers, .. }
            | ViewElementKind::Link { label, handlers, .. } => {
                expect(c, label, &Type::Str);
                check_app_handlers(c, handlers);
            }
            ViewElementKind::Image { src, alt, handlers, .. } => {
                expect(c, src, &Type::Str);
                expect(c, alt, &Type::Str);
                check_app_handlers(c, handlers);
            }
            ViewElementKind::Divider { .. } => {}
            ViewElementKind::Input { bind, handlers, .. } => {
                check_bind_target(c, bind, "input");
                check_app_handlers(c, handlers);
            }
            ViewElementKind::When { cond, then_branch, else_branch, .. } => {
                let t = c.check_expr(cond);
                if t != Type::Bool && t != Type::Error {
                    c.error(
                        "E3003",
                        format!("`when` condition must be bool, found {}", t.as_str()),
                        cond.line,
                        cond.col,
                        4,
                    );
                }
                check_app_elements(c, then_branch);
                if let Some(els) = else_branch {
                    check_app_elements(c, els);
                }
            }
            ViewElementKind::Each { iter, var, key, body } => {
                let t = c.check_expr(iter);
                let elem_ty = match t {
                    Type::Array(el) => *el,
                    Type::Error => Type::Error,
                    other => {
                        c.error(
                            "E3003",
                            format!("`each` iterable must be an array, found {}", other.as_str()),
                            iter.line,
                            iter.col,
                            4,
                        );
                        Type::Error
                    }
                };
                let mut scope = HashMap::new();
                scope.insert(
                    var.clone(),
                    VarInfo { ty: elem_ty, mutable: false, line: e.line, col: e.col, app_kind: None },
                );
                c.scopes.push(scope);
                // The key expression sits inside the `each` construct:
                // it sees the loop variable (`key item` is the
                // canonical stable key — B.5).
                if let Some(k) = key {
                    c.check_expr(k);
                }
                check_app_elements(c, body);
                c.scopes.pop();
            }
            ViewElementKind::Component { module, name, args, handlers, .. } => {
                // Slice 4: instantiation sites validate against the
                // app's component declarations (E3040/E3041). The
                // argument expressions still resolve as everywhere in
                // the app scope — even for an unknown component, so
                // names inside the arguments keep reporting E3066.
                check_component_instantiation(c, e.line, e.col, module.as_deref(), name, args);
                check_app_handlers(c, handlers);
            }
            ViewElementKind::Form { legend, body, handlers } => {
                expect(c, legend, &Type::Str);
                check_app_elements(c, body);
                check_app_handlers(c, handlers);
            }
            ViewElementKind::Field { bind, attrs, handlers, .. } => {
                check_bind_target(c, bind, "field");
                for attr in attrs {
                    match attr {
                        FieldAttr::Label(v)
                        | FieldAttr::Placeholder(v)
                        | FieldAttr::Pattern(v) => expect(c, v, &Type::Str),
                        FieldAttr::MinLength(v)
                        | FieldAttr::MaxLength(v)
                        | FieldAttr::MinValue(v)
                        | FieldAttr::MaxValue(v) => expect(c, v, &Type::Int),
                        FieldAttr::Required => {}
                    }
                }
                check_app_handlers(c, handlers);
            }
        }
    }
}

/// A component instantiation inside a view (B.3): `Name(prop: expr, …)`
/// or the qualified `alias.Name(...)`. Slice 4 validates it against the
/// app's component declarations:
///
/// - **E3040** — the component is not declared in this app. The hint
///   lists the declared components; a name that matches a declared
///   *page* gets the namespace note (pages and components are separate
///   namespaces — the mirror of E3063's component note); the qualified
///   form is always unknown today because module files cannot declare
///   components yet, so its hint says so honestly.
/// - **E3041** — prop mismatches, four shapes: an unknown prop name, a
///   prop passed twice, a type mismatch against the prop's declared
///   type (no implicit conversions — int and float do not mix), and a
///   required prop (no default) that was not passed. Omitting a prop
///   with a default is legal.
///
/// The argument expressions always resolve through the ordinary checker
/// (slice 3), even when the component is unknown — unknown names inside
/// the arguments keep reporting E3066, and a poisoned (`Error`) argument
/// suppresses only the E3041 type comparison.
fn check_component_instantiation(
    c: &mut Checker,
    line: u32,
    col: u32,
    module: Option<&str>,
    name: &str,
    args: &[PropArg],
) {
    // E3040 — the component table lives in the app scope; the walk only
    // runs inside the app pass, so it is always present here.
    let sig = c
        .app
        .as_ref()
        .and_then(|app| app.components.get(name))
        .map(|s| s.props.clone());
    if sig.is_none() {
        let display = match module {
            Some(m) => format!("{}.{}", m, name),
            None => name.to_string(),
        };
        let len = display.chars().count() as u32;
        let mut d = Diagnostic::error(
            "E3040",
            format!("unknown component `{}`", display),
            line,
            col,
            len,
        );
        d.hint = Some(match module {
            // Qualified instantiation: no module can declare components
            // in 0.4.0 (components are app sections) — say so instead
            // of pretending the import path exists.
            Some(_) => "components are declared inside `app { … }`; importing \
                        components from module files is not implemented in 0.4.0"
                .to_string(),
            None => {
                let app = c.app.as_ref().unwrap();
                if app.component_order.is_empty() {
                    "this app declares no components".to_string()
                } else if app.pages.iter().any(|p| p == name) {
                    format!(
                        "'{}' is a page; views instantiate components — pages and \
                         components are separate namespaces (declared components: {})",
                        name,
                        app.component_order.join(", ")
                    )
                } else {
                    format!("declared components: {}", app.component_order.join(", "))
                }
            }
        });
        c.diags.push(d.in_file(c.file.clone()));
    }

    // E3041 — prop-level checks. Arguments are checked in source order;
    // every value expression resolves through the ordinary checker even
    // when its prop name is unknown (the expression's own errors, like
    // E3066, still fire).
    let mut passed: Vec<&str> = Vec::new();
    for arg in args {
        let arg_ty = c.check_expr(&arg.value);
        let Some(props) = &sig else { continue };
        let Some(prop) = props.iter().find(|p| p.name == arg.name) else {
            let mut d = Diagnostic::error(
                "E3041",
                format!(
                    "component `{}` has no prop `{}`",
                    name, arg.name
                ),
                arg.line,
                arg.col,
                arg.name.chars().count() as u32,
            );
            d.hint = Some(if props.is_empty() {
                format!("component `{}` declares no props — instantiate it as `{}()`", name, name)
            } else {
                format!(
                    "props of `{}`: {}",
                    name,
                    props
                        .iter()
                        .map(|p| {
                            if p.has_default {
                                format!("{} (optional)", p.name)
                            } else {
                                format!("{} (required)", p.name)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
            c.diags.push(d.in_file(c.file.clone()));
            continue;
        };
        if passed.contains(&arg.name.as_str()) {
            let mut d = Diagnostic::error(
                "E3041",
                format!(
                    "prop `{}` is passed twice in this `{}` instantiation",
                    arg.name, name
                ),
                arg.line,
                arg.col,
                arg.name.chars().count() as u32,
            );
            d.hint =
                Some("a prop is passed at most once — remove one of the two arguments".to_string());
            c.diags.push(d.in_file(c.file.clone()));
        } else {
            passed.push(arg.name.as_str());
        }
        if arg_ty != Type::Error && arg_ty != prop.ty {
            let mut d = Diagnostic::error(
                "E3041",
                format!(
                    "prop `{}` of component `{}` expects {}, found {}",
                    prop.name,
                    name,
                    prop.ty.as_str(),
                    arg_ty.as_str()
                ),
                arg.value.line,
                arg.value.col,
                1,
            );
            d.hint = Some(
                "props have exact types — there is no implicit conversion (int and float do not mix)"
                    .to_string(),
            );
            c.diags.push(d.in_file(c.file.clone()));
        }
    }

    // E3041 — required props (no default) that were not passed. One
    // diagnostic per missing prop, anchored on the instantiation name.
    if let Some(props) = &sig {
        for prop in props {
            if !prop.has_default && !passed.contains(&prop.name.as_str()) {
                let mut d = Diagnostic::error(
                    "E3041",
                    format!(
                        "component `{}` is missing the required prop `{}` (it has no default value)",
                        name, prop.name
                    ),
                    line,
                    col,
                    name.chars().count() as u32,
                );
                d.hint = Some(format!(
                    "pass `{}: <value>` inside the parentheses, or give the prop a default: \
                     `prop {}: {} = …`",
                    prop.name, prop.name, prop.ty.as_str()
                ));
                c.diags.push(d.in_file(c.file.clone()));
            }
        }
    }
}

/// `input bind x` / `field x kind …` targets are **state keys** (the
/// slice-2 §C2 ruling): shared state in pages, component state in
/// components. Two-way binding writes, so a computed value or prop in
/// that position is E3043 (an invalid assignment target — B.4), and a
/// name that resolves nowhere is E3066.
fn check_bind_target(c: &mut Checker, t: &OpenTarget, element: &str) {
    if let Some(v) = c.lookup(&t.page) {
        match v.app_kind {
            Some(AppVarKind::State) | Some(AppVarKind::ComponentState) => {}
            Some(kind) => {
                let mut d = Diagnostic::error(
                    "E3043",
                    format!(
                        "the `{element}` binds `{}`, a {} — two-way binding writes, so only state fields are legal",
                        t.page,
                        kind.as_str()
                    ),
                    t.line,
                    t.col,
                    t.page.chars().count() as u32,
                );
                d.hint =
                    Some("bind a state field (declare it in `state { … }`)".to_string());
                c.diags.push(d.in_file(c.file.clone()));
            }
            None => {
                let mut d = Diagnostic::error(
                    "E3043",
                    format!(
                        "the `{element}` binds `{}`, which is not a state field — two-way binding writes, so only state fields are legal",
                        t.page
                    ),
                    t.line,
                    t.col,
                    t.page.chars().count() as u32,
                );
                d.hint =
                    Some("bind a state field (declare it in `state { … }`)".to_string());
                c.diags.push(d.in_file(c.file.clone()));
            }
        }
    } else if c.consts[c.cur_file].contains_key(&t.page) {
        let mut d = Diagnostic::error(
            "E3043",
            format!(
                "the `{element}` binds the const `{}` — consts are compile-time values; bind a state field",
                t.page
            ),
            t.line,
            t.col,
            t.page.chars().count() as u32,
        );
        d.hint = Some("bind a state field (declare it in `state { … }`)".to_string());
        c.diags.push(d.in_file(c.file.clone()));
    } else {
        let hint = c.app.as_ref().map(|a| a.hint.clone()).unwrap_or_default();
        let mut d = Diagnostic::error(
            "E3066",
            format!(
                "the `{element}` binds `{}`, which is not a declared state key",
                t.page
            ),
            t.line,
            t.col,
            t.page.chars().count() as u32,
        );
        d.hint = Some(hint);
        c.diags.push(d.in_file(c.file.clone()));
    }
}

/// Handler bodies are official Kin statements (B.6) checked by the
/// existing checker, with the event builtins in scope below the
/// locals. Names/types only for the builtins — which builtin belongs to
/// which event context is enforced at S6 (the approved A2 ruling).
/// Slice 4 adds the handler subset (B.6):
///
/// - **E3046** — one handler per event name on one element. Handlers
///   are compared by the word after `on` (`click`, `key`, …): two `on
///   key` handlers collide even with different key filters (the filter
///   is runtime data, B.6), while `on key "Escape"` and `on keydown`
///   are different event names and coexist. The duplicate's body is
///   still checked — errors inside it are not hidden by E3046.
/// - **E3045** — `return` is illegal inside a handler (they are
///   callbacks; control returns to the caller automatically). Enforced
///   in [`Checker::check_stmt`] via `handler_depth`.
/// - **W3040** — `while` stays legal but always warns (a long loop in
///   a handler freezes the page). Also via `handler_depth`, so a
///   `while` nested under `if`/`for`/blocks inside the handler warns
///   too. `for` never warns — it is the suggested alternative.
fn check_app_handlers(c: &mut Checker, handlers: &[OnEvent]) {
    let mut seen: Vec<EventKind> = Vec::new();
    for on in handlers {
        // E3046 — duplicate event name on one element, anchored on the
        // duplicate's `on` keyword. One diagnostic per extra handler.
        if seen.contains(&on.event) {
            let mut d = Diagnostic::error(
                "E3046",
                format!(
                    "duplicate `on {}` handler on this element — one handler per event name",
                    on.event.as_str()
                ),
                on.line,
                on.col,
                2, // the `on` keyword
            );
            d.hint = Some(if on.event == EventKind::Key {
                "`on key \"A\"` and `on key \"B\"` both count as `on key` — combine them \
                 into one handler that checks `event_key`"
                    .to_string()
            } else {
                format!(
                    "merge the two `on {}` bodies, or listen for a different event",
                    on.event.as_str()
                )
            });
            c.diags.push(d.in_file(c.file.clone()));
        } else {
            seen.push(on.event);
        }
        let mut scope: HashMap<String, VarInfo> = HashMap::new();
        for (name, ty) in [
            ("event_value", Type::Str),
            ("event_key", Type::Str),
            ("event_x", Type::Int),
            ("event_y", Type::Int),
            ("form_valid", Type::Bool),
            ("field_errors", Type::Array(Box::new(Type::Str))),
        ] {
            scope.insert(
                name.to_string(),
                VarInfo {
                    ty,
                    mutable: false,
                    line: on.line,
                    col: on.col,
                    app_kind: Some(AppVarKind::EventBuiltin),
                },
            );
        }
        c.scopes.push(scope);
        let saved_ret = c.ret.clone();
        c.ret = Type::Void;
        c.loop_depth = 0;
        c.assigned.clear();
        c.var_decls.clear();
        c.handler_depth += 1;
        c.check_block(&on.body);
        c.handler_depth -= 1;
        c.ret = saved_ret;
        c.scopes.pop();
    }
}

/// Why a route path is malformed (E3055); `None` = valid. See
/// [`check_app`] for the full rule.
fn route_path_error(path: &str) -> Option<String> {
    if path.trim().is_empty() {
        return Some("the path is empty".to_string());
    }
    if path.starts_with('/') {
        return Some(
            "paths are relative and must not start with `/` (write \"items/:id\", not \"/items/:id\")"
                .to_string(),
        );
    }
    if path.ends_with('/') {
        return Some("the path must not end with `/`".to_string());
    }
    let mut params: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        if seg.is_empty() {
            return Some("the path has an empty segment (`//`)".to_string());
        }
        if let Some(name) = seg.strip_prefix(':') {
            if name.is_empty() {
                return Some("a `:` segment needs a parameter name, like `:id`".to_string());
            }
            let mut chars = name.chars();
            let first_ok = chars.next().map_or(false, |ch| ch.is_ascii_alphabetic() || ch == '_');
            let rest_ok = chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
            if !first_ok || !rest_ok {
                return Some(format!(
                    "`:{name}` is not a valid parameter name (identifiers are letters, digits and `_`, starting with a letter)"
                ));
            }
            if params.contains(&name) {
                return Some(format!("the parameter `:{name}` appears twice in this path"));
            }
            params.push(name);
        } else {
            if seg.contains(':') {
                return Some(format!(
                    "segment `{seg}` mixes a literal with a `:` parameter marker — a segment is either a literal or `:name`"
                ));
            }
            let charset_ok = seg.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-');
            if !charset_ok {
                return Some(
                    "literal segments may contain letters, digits, `.`, `_` and `-` only".to_string(),
                );
            }
            if seg == "." || seg == ".." {
                return Some(format!(
                    "segment `{seg}` is not allowed (it would escape the generated output directory)"
                ));
            }
        }
    }
    None
}

/// The match shape of a valid route path for E3056: literals compared
/// exactly, parameters by position only (their names do not change
/// which addresses a route matches).
#[derive(PartialEq, Eq, Hash, Clone)]
enum RouteShape {
    Segments(Vec<ShapeSeg>),
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum ShapeSeg {
    Lit(String),
    Param,
}

impl RouteShape {
    fn of(path: &str) -> RouteShape {
        RouteShape::Segments(
            path.split('/')
                .map(|seg| match seg.strip_prefix(':') {
                    Some(_) => ShapeSeg::Param,
                    None => ShapeSeg::Lit(seg.to_string()),
                })
                .collect(),
        )
    }
}

struct FnSig {
    params: Vec<Type>,
    ret: Type,
    line: u32,
    col: u32,
}

/// What a name in an app scope layer is (0.4.0 S3 slice 3).
/// Assignment rules (B.4): state fields are the only app bindings
/// handlers may assign to; props, computed values and event builtins
/// are read-only (E3043). `fin` locals and `const`s keep E3004.
#[derive(Clone, Copy, PartialEq)]
enum AppVarKind {
    State,
    ComponentState,
    Computed,
    Prop,
    EventBuiltin,
}

impl AppVarKind {
    fn as_str(self) -> &'static str {
        match self {
            AppVarKind::State => "app state field",
            AppVarKind::ComponentState => "component state field",
            AppVarKind::Computed => "computed value",
            AppVarKind::Prop => "prop",
            AppVarKind::EventBuiltin => "event builtin",
        }
    }
}

/// State carried while the app semantic pass walks a registered app
/// (S3 slices 3–4). Present only inside that pass: it switches unknown-name
/// diagnostics to the app-scoped E3066 (the sibling of E3001, the way
/// E3063 siblings E3032 — frozen meanings are never reused,
/// ADR-0011), carries the `:param` names valid routes declare (E3058),
/// holds the hint naming what the scope being checked makes
/// visible, and keeps the component signatures for the
/// instantiation-site rules (E3040/E3041, slice 4).
struct AppScope {
    route_params: Vec<String>,
    hint: String,
    /// Component signatures by name (slice 4). Duplicate component names
    /// were rejected by E3065 in pass 0b; the first declaration wins
    /// here (the established "invalid declarations are not further
    /// checked" pattern).
    components: HashMap<String, ComponentSig>,
    /// Component names in declaration order — HashMap iteration is
    /// random, and E3040's hint teaches better when it lists components
    /// the way they were declared.
    component_order: Vec<String>,
    /// Declared page names, for E3040's pages-vs-components namespace
    /// note (the mirror of E3063's component note).
    pages: Vec<String>,
}

/// A component's checked signature (S3 slice 4): its props in
/// declaration order, for E3041 at instantiation sites.
struct ComponentSig {
    props: Vec<PropSig>,
}

/// One prop of a component signature: name, declared type, and whether
/// a default value makes it optional at instantiation sites (B.3 —
/// missing props without defaults are E3041).
#[derive(Clone)]
struct PropSig {
    name: String,
    ty: Type,
    has_default: bool,
}

#[derive(Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
    line: u32,
    col: u32,
    /// What this binding is inside an app scope (S3 slice 3); `None`
    /// for ordinary locals and params. Drives E3043 (read-only app
    /// bindings) and the W3041 shadow warning.
    app_kind: Option<AppVarKind>,
}

/// A checked top-level `const` declaration (0.4.0 S1): declared type and
/// definition line, per file, keyed by the const's (unqualified) name.
/// Consts are file-scoped: a file's own consts are read bare, other
/// files' consts are read qualified (`module.NAME`).
#[derive(Clone)]
struct ConstInfo {
    ty: Type,
    line: u32,
}

struct Checker {
    diags: Vec<Diagnostic>,
    mode: Mode,
    /// Signatures per project file, indexed like `project.files`.
    sigs: Vec<HashMap<String, FnSig>>,
    /// Const tables per project file, indexed like `project.files`.
    consts: Vec<HashMap<String, ConstInfo>>,
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
    /// App semantic pass state (0.4.0 S3 slices 3–4); `Some` only while a
    /// registered app is being walked (see [`check_app_semantics`]).
    app: Option<AppScope>,
    /// Depth of event-handler bodies on the check stack (S3 slice 4).
    /// Greater than zero only while a handler's statements are being
    /// checked (nested blocks included — a `while` under an `if` inside
    /// the handler still counts). Function bodies checked in pass 2
    /// always see zero: handlers cannot contain function declarations,
    /// and the app pass closes before pass 2. Drives E3045 (`return` is
    /// illegal in handlers) and W3040 (`while` in a handler warns).
    handler_depth: u32,
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
                // 0.4.0 S3 slice 3: a handler local shadowing a state
                // field warns (B.4) so beginners see the shadow
                // immediately. Shadowing anything else (computed
                // values, props, consts) stays silent, matching the
                // ordinary rule that locals shadow consts.
                if self.app.is_some() {
                    if let Some(existing) = self.lookup(name) {
                        if matches!(
                            existing.app_kind,
                            Some(AppVarKind::State) | Some(AppVarKind::ComponentState)
                        ) {
                            let mut d = Diagnostic::warning(
                                "W3041",
                                format!(
                                    "local `{}` shadows the {} `{}` declared at line {}",
                                    name,
                                    existing.app_kind.map(|k| k.as_str()).unwrap_or(""),
                                    name,
                                    existing.line
                                ),
                                *line,
                                *col,
                                name.len() as u32,
                            );
                            d.hint = Some(
                                "the shadow is legal — rename the local if you meant the state field"
                                    .to_string(),
                            );
                            self.diags.push(d.in_file(self.file.clone()));
                        }
                    }
                }
                self.declare(
                    name,
                    VarInfo { ty: declared, mutable: *mutable, line: *line, col: *col, app_kind: None },
                    *line,
                    *col,
                );
            }
            Stmt::Assign { name, op, value, line, col } => {
                let (vty, vmutable, vline, vcol, vkind) = match self.lookup(name) {
                    Some(v) => (v.ty, v.mutable, v.line, v.col, v.app_kind),
                    None => {
                        if self.consts[self.cur_file].contains_key(name) {
                            self.error(
                                "E3004",
                                format!(
                                    "cannot assign to `{}` (consts are immutable, declared with `const`)",
                                    name
                                ),
                                *line,
                                *col,
                                name.len() as u32,
                            );
                        } else if self.app.is_some() {
                            // S3 slice 3: the app-scoped sibling of
                            // E3001 (frozen for ordinary function
                            // bodies — ADR-0011).
                            let mut d = Diagnostic::error(
                                "E3066",
                                format!(
                                    "assignment target `{}` is not declared in the app scope",
                                    name
                                ),
                                *line,
                                *col,
                                name.len() as u32,
                            );
                            d.hint = Some(self.app.as_ref().unwrap().hint.clone());
                            self.diags.push(d.in_file(self.file.clone()));
                        } else {
                            self.error(
                                "E3001",
                                format!("undefined variable `{}`", name),
                                *line,
                                *col,
                                name.len() as u32,
                            );
                        }
                        self.check_expr(value);
                        return;
                    }
                };
                if !vmutable {
                    match vkind {
                        // S3 slice 3 (B.4): props, computed values and
                        // event builtins are read-only app bindings —
                        // E3043. `fin` locals and consts keep E3004
                        // (frozen meanings, ADR-0011).
                        Some(AppVarKind::Prop) => {
                            let mut d = Diagnostic::error(
                                "E3043",
                                format!(
                                    "cannot assign to the prop `{}` — props are component inputs and read-only in handlers",
                                    name
                                ),
                                *line,
                                *col,
                                name.len() as u32,
                            );
                            d.hint = Some(
                                "declare a `state` field in the component and assign to that instead"
                                    .to_string(),
                            );
                            self.diags.push(d.in_file(self.file.clone()));
                        }
                        Some(AppVarKind::Computed) => {
                            let mut d = Diagnostic::error(
                                "E3043",
                                format!(
                                    "cannot assign to the computed value `{}` — computed values are derived from state",
                                    name
                                ),
                                *line,
                                *col,
                                name.len() as u32,
                            );
                            d.hint = Some(
                                "assign to a state field the computed value reads — it recomputes automatically"
                                    .to_string(),
                            );
                            self.diags.push(d.in_file(self.file.clone()));
                        }
                        Some(AppVarKind::EventBuiltin) => {
                            let mut d = Diagnostic::error(
                                "E3043",
                                format!("cannot assign to the event builtin `{}`", name),
                                *line,
                                *col,
                                name.len() as u32,
                            );
                            d.hint = Some(
                                "event builtins (`event_value`, `form_valid`, …) are read-only in handlers"
                                    .to_string(),
                            );
                            self.diags.push(d.in_file(self.file.clone()));
                        }
                        _ => {
                            self.error(
                                "E3004",
                                format!(
                                    "cannot reassign immutable variable `{}` (declared with `fin`)",
                                    name
                                ),
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
                    }
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
                                // S3 slice 3 (B.4/B.7): `state_field +=
                                // [item]` appends when the field is a
                                // same-typed array — app expressions
                                // only; execution arrives with S5.
                                if *op == AssignOp::Add
                                    && self.app.is_some()
                                    && matches!(vty, Type::Array(_))
                                    && vt == vty
                                {
                                    // same-typed array append, app scope only
                                } else {
                                    self.error(
                                        "E3003",
                                        format!(
                                            "`{}` requires a numeric variable, found {}",
                                            symbol,
                                            vty.as_str()
                                        ),
                                        *line,
                                        *col,
                                        2,
                                    );
                                }
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
            Stmt::While { cond, body, line, col } => {
                // S3 slice 4 (B.6): `while` stays legal inside event
                // handlers but always warns — a long loop freezes the
                // page. `for` (bounded) is the suggested alternative and
                // never warns. Fires once per `while` statement, nested
                // control flow included (handler_depth survives blocks).
                if self.handler_depth > 0 {
                    let mut d = Diagnostic::warning(
                        "W3040",
                        "a long `while` loop in an event handler freezes the page; prefer `for` over a range",
                        *line,
                        *col,
                        5, // the `while` keyword
                    );
                    d.hint = Some(
                        "`while` remains allowed — this is guidance; a `for` over a range always ends"
                            .to_string(),
                    );
                    self.diags.push(d.in_file(self.file.clone()));
                }
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
                    VarInfo { ty: var_ty, mutable: false, line: *line, col: *col, app_kind: None },
                );
                self.loop_depth += 1;
                self.check_block(body);
                self.loop_depth -= 1;
                self.scopes.pop();
            }
            Stmt::Return { value, line, col } => {
                // S3 slice 4 (B.6): `return` is illegal inside an event
                // handler — handlers are callbacks that return to the
                // caller automatically when the body ends. E3045 fires
                // for both `return;` and `return expr;`; the ordinary
                // E3006/mismatch checks are skipped in handlers so one
                // statement reports one error, but a value expression is
                // still checked — unknown names inside it keep reporting
                // their own diagnostics.
                let in_handler = self.handler_depth > 0;
                if in_handler {
                    let mut d = Diagnostic::error(
                        "E3045",
                        "`return` is not allowed inside an event handler — handlers are callbacks that return to the caller automatically",
                        *line,
                        *col,
                        6, // the `return` keyword
                    );
                    d.hint = Some("remove the `return` statement".to_string());
                    self.diags.push(d.in_file(self.file.clone()));
                }
                match value {
                    None => {
                        if !in_handler && self.ret != Type::Void {
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
                        if !in_handler {
                            if self.ret == Type::Void {
                                self.error("E3006", "void function cannot return a value", *line, *col, 6);
                            } else if vt != Type::Error && vt != self.ret {
                                self.mismatch(&self.ret.clone(), &vt, v.line, v.col, 1);
                            }
                        }
                    }
                }
            }
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
                // Fall back to the file's own consts (0.4.0 S1). Locals
                // win: a local binding shadows a const of the same name.
                None => match self.consts[self.cur_file].get(name) {
                    Some(ci) => ci.ty.clone(),
                    None => {
                        // Honest hint when the name is a const of an
                        // imported module: it must be read qualified.
                        let mut hint = String::new();
                        for (alias, file) in &self.imports {
                            if self.consts[*file].contains_key(name) {
                                hint = format!(
                                    " (const `{}.{}` exists in module `{}`; read it qualified)",
                                    alias, name, alias
                                );
                                break;
                            }
                        }
                        // S3 slice 3: inside the app semantic pass the
                        // app-scoped sibling E3066 fires instead of E3001
                        // (frozen for ordinary function bodies,
                        // ADR-0011) — the same sibling pattern as
                        // E3063/E3032. A module-const hint, when one
                        // applies, is more specific than the scope
                        // list and wins.
                        if self.app.is_some() {
                            let scope_hint = self.app.as_ref().unwrap().hint.clone();
                            let mut d = Diagnostic::error(
                                "E3066",
                                format!("unknown name `{}` in the app scope", name),
                                e.line,
                                e.col,
                                name.len() as u32,
                            );
                            d.hint =
                                Some(if hint.is_empty() { scope_hint } else { hint });
                            self.diags.push(d.in_file(self.file.clone()));
                            return Type::Error;
                        }
                        self.error(
                            "E3001",
                            format!("undefined variable `{}`{}", name, hint),
                            e.line,
                            e.col,
                            name.len() as u32,
                        );
                        Type::Error
                    }
                },
            },
            ExprKind::ArrayLit(items) => {
                if items.is_empty() {
                    self.error(
                        "E3022",
                        "cannot infer the element type of an empty array literal; add a type annotation, e.g. `fin a: [int] = [];`",
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
                        // S3 slice 3 (B.7's normative example:
                        // `items = items + [draft]`): same-typed array
                        // concatenation is legal inside app expressions.
                        // Execution lands with S5 (KIR/JS); console
                        // programs keep the 0.2 rule, so this arm is
                        // app-only (`self.app` is `Some` only inside
                        // the app semantic pass).
                        (Type::Array(a), Type::Array(b))
                            if a == b && self.app.is_some() =>
                        {
                            Type::Array(a.clone())
                        }
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
            ExprKind::ModConst { module, name } => {
                self.check_mod_const(module, name, e.line, e.col)
            }
        }
    }

    /// `module.NAME` — a qualified const read (0.4.0 S1).
    fn check_mod_const(&mut self, module: &str, name: &str, line: u32, col: u32) -> Type {
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
                return Type::Error;
            }
        };
        if let Some(ci) = self.consts[file].get(name) {
            return ci.ty.clone();
        }
        if self.sigs[file].contains_key(name) {
            self.error(
                "E3021",
                format!(
                    "module `{}` member `{}` is a function; call it as `{}.{}(...)`",
                    module, name, module, name
                ),
                line,
                col,
                (module.len() + name.len() + 1) as u32,
            );
        } else {
            self.error(
                "E3021",
                format!("module `{}` has no function or const `{}`", module, name),
                line,
                col,
                (module.len() + name.len() + 1) as u32,
            );
        }
        Type::Error
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
                // S3 slice 3: app builtin functions resolve only inside
                // the app semantic pass, after user functions and the
                // global natives — user code shadows builtins and no
                // redefinition guard exists (the builtin is a
                // fallback, not a reserved word).
                if self.app.is_some() {
                    if let Some(t) = self.check_app_fn(name, args, line, col) {
                        return t;
                    }
                    let scope_hint = self.app.as_ref().unwrap().hint.clone();
                    let mut d = Diagnostic::error(
                        "E3066",
                        format!("unknown function `{}` in the app scope", name),
                        line,
                        col,
                        name.len() as u32,
                    );
                    d.hint = Some(scope_hint);
                    self.diags.push(d.in_file(self.file.clone()));
                    for a in args {
                        self.check_expr(a);
                    }
                    return Type::Error;
                }
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

    /// App builtin functions (0.4.0 S3 slice 3, A2 ruling): `param`,
    /// the store/JSON trio and the B.10 string helpers, registered by
    /// name and type so binding expressions type-check. Known limit,
    /// recorded in LANGUAGE_SPEC §C3 and P-030: **param/store names are
    /// recognized; context-restricted invocation (view-only `param`,
    /// handler-only stores) is enforced at S6**, and the helpers' VM/JS
    /// natives arrive with S5 — app programs cannot build or run
    /// before S4+ anyway, so nothing silently executes. Returns `None`
    /// when `name` is not an app builtin.
    fn check_app_fn(&mut self, name: &str, args: &[Expr], line: u32, col: u32) -> Option<Type> {
        let (params, ret): (Vec<Type>, Type) = match name {
            // `str(expr)` mirrors the `as string` cast surface: any
            // scalar renders; arrays do not.
            "str" => {
                if args.len() != 1 {
                    self.error(
                        "E3007",
                        format!("`str` expects 1 argument, found {}", args.len()),
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
                return Some(match t {
                    Type::Int | Type::Float | Type::Bool | Type::Str => Type::Str,
                    Type::Error => Type::Error,
                    other => {
                        self.error(
                            "E3003",
                            format!(
                                "`str` expects a number, bool or string, found {}",
                                other.as_str()
                            ),
                            args[0].line,
                            args[0].col,
                            3,
                        );
                        Type::Error
                    }
                });
            }
            // `param("name")` — E3058: a literal argument must name a
            // parameter some valid route declares (malformed paths are
            // not further checked — the established E3055 pattern).
            // Non-literal arguments pass; they are runtime data.
            "param" => {
                self.check_args(name, &[Type::Str], args, line, col);
                let (empty, list) = match &self.app {
                    Some(app) => (app.route_params.is_empty(), app.route_params.join(", ")),
                    None => (true, String::new()),
                };
                if let Some(ExprKind::StrLit(pname)) = args.first().map(|a| &a.kind) {
                    if let Some(app) = &self.app {
                        if !app.route_params.iter().any(|p| p == pname) {
                            let mut d = Diagnostic::error(
                                "E3058",
                                format!(
                                    "`param(\"{}\")` reads a route parameter this app does not declare",
                                    pname
                                ),
                                args[0].line,
                                args[0].col,
                                pname.chars().count() as u32 + 2,
                            );
                            d.hint = Some(if empty {
                                "no route declares a parameter yet — add a `:name` segment to a route path"
                                    .to_string()
                            } else {
                                format!("declared route parameters: {}", list)
                            });
                            self.diags.push(d.in_file(self.file.clone()));
                        }
                    }
                }
                return Some(Type::Str);
            }
            "store_save" | "store_load" | "store_clear" => (vec![Type::Str], Type::Void),
            "json_of" => (vec![Type::Str], Type::Str),
            "save_json" => (vec![Type::Str, Type::Str], Type::Bool),
            "upper" | "lower" | "trim" => (vec![Type::Str], Type::Str),
            "contains" => (vec![Type::Str, Type::Str], Type::Bool),
            "split" => (vec![Type::Str, Type::Str], Type::Array(Box::new(Type::Str))),
            "join" => (
                vec![Type::Array(Box::new(Type::Str)), Type::Str],
                Type::Str,
            ),
            _ => return None,
        };
        self.check_args(name, &params, args, line, col);
        Some(ret)
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
                // Distinguish a const of that name from a truly unknown
                // member so the message tells the user what to do.
                if self.consts[file].contains_key(name) {
                    self.error(
                        "E3021",
                        format!(
                            "`{}.{}` is a const, not a function; read it without parentheses: `{}.{}`",
                            module, name, module, name
                        ),
                        line,
                        col,
                        (module.len() + name.len() + 1) as u32,
                    );
                } else {
                    self.error(
                        "E3021",
                        format!("module `{}` has no function `{}`", module, name),
                        line,
                        col,
                        (module.len() + name.len() + 1) as u32,
                    );
                }
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

    const HELLO: &str = "*mode = B;\nfunc main() { print(\"hi\"); fin x = 1; }\n";

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
        let diags = check_src("func main() { fin x = 1; x = 2; }");
        assert!(diags.iter().any(|d| d.code == "E3004"));
        assert!(diags.iter().any(|d| d.code == "N3002" && d.message.contains("declared here")));
    }

    #[test]
    fn reassigning_fin_reports_the_fin_wording() {
        // ADR-0029: the immutable keyword is `fin`; E3004 still fires and
        // its message names the new keyword.
        let diags = check_src("func main() { fin x = 1; x = 2; }");
        let d = diags.iter().find(|d| d.code == "E3004").expect("E3004");
        assert!(d.message.contains("(declared with `fin`)"), "{}", d.message);
        // The old wording is gone.
        assert!(!d.message.contains("`let`"), "{}", d.message);
        // var keeps compiling when reassigned.
        clean("func main() { var x = 1; x = 2; print(x); }");
    }

    #[test]
    fn var_never_reassigned_note_suggests_fin() {
        // B-mode lint N3001 suggests the new keyword.
        let diags = check_src("*mode = B;\nfunc main() { var x = 1; print(x); }");
        assert!(diags.iter().any(|d| d.code == "N3001" && d.message.contains("consider `fin`")));
    }

    #[test]
    fn no_implicit_int_float_conversion() {
        assert!(has("E3011", "func main() { print(1 + 0.5); }"));
        assert!(has("E3011", "func main() { fin f: float = 1.5; f = 2; }"));
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
        assert!(has("E3015", "func main() { fin x = print(\"a\"); }"));
        assert!(has("E3010", "func main() { print(print(\"a\")); }"));
    }

    #[test]
    fn void_params_and_vars_are_rejected() {
        assert!(has("E3014", "func f(x: void) { } func main() { f(); }"));
        assert!(has("E3015", "func main() { fin x: void = 1; }"));
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
        assert!(has("E3002", "func main() { fin x = 1; fin x = 2; }"));
        assert!(has("E3002", "func f(a: int, a: int) { } func main() { f(1, 2); }"));
    }

    #[test]
    fn shadowing_in_inner_scope_is_allowed() {
        clean("func main() { fin x = 1; { fin x = 2.5; print(x); } print(x); }");
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
        clean("func main() { fin a = [1, 2, 3]; print(a[0]); }");
        clean("func main() { fin m = [[1], [2, 3]]; print(m[1][1]); }");
        clean("func main() { fin s = [\"a\", \"b\"]; print(s[1]); }");
        assert!(has("E3028", "func main() { fin a = [1, 2.5]; }"));
    }

    #[test]
    fn empty_array_needs_annotation() {
        clean("func main() { fin a: [int] = []; print(len(a)); }");
        assert!(has("E3022", "func main() { fin a = []; }"));
    }

    #[test]
    fn indexing_rules() {
        assert!(has("E3023", "func main() { fin a = [1]; print(a[true]); }"));
        assert!(has("E3024", "func main() { fin x = 5; print(x[0]); }"));
        assert!(has("E3024", "func main() { fin s = \"hi\"; print(s[0]); }"));
    }

    #[test]
    fn index_assignment_rules() {
        clean("func main() { fin a = [1, 2]; a[0] = 9; print(a[0]); }");
        // element type must match
        assert!(has("E3003", "func main() { fin a = [1]; a[0] = \"x\"; }"));
        assert!(has("E3023", "func main() { fin a = [1]; a[true] = 2; }"));
        assert!(has("E3024", "func main() { fin x = 1; x[0] = 2; }"));
    }

    #[test]
    fn arrays_compare_deeply_and_pass_to_functions() {
        clean("func sum(xs: [int]) -> int { return xs[0] + xs[1]; } func main() { print(sum([3, 4])); if [1] == [1] { print(1); } }");
        assert!(has("E3003", "func f(xs: [int]) { } func main() { f([1.5]); }"));
    }

    // -- 0.2.0: casts ---------------------------------------------------------

    #[test]
    fn supported_casts_type_check() {
        clean("func main() { fin f = 2 as float; print(f + 0.5); }");
        clean("func main() { fin i = 2.9 as int; print(i); }");
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
        clean("func main() { fin xs = [1, 2]; for x in xs { print(x); } }");
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
        clean("func main() { fin a = [1, 2, 3]; for i in 0..len(a) { print(a[i]); } }");
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

    // -- website rules (0.3.0, E3030-E3037 / W3035-W3037) -------------------

    const SITE: &str = "*mode = B;\nwebsite \"S\" {\n    page Home {\n        title \"Home\"\n        button \"Go\" open About\n    }\n    page About {\n        heading \"About\"\n        link \"Back\" open Home\n    }\n}\n";

    #[test]
    fn valid_site_passes_without_main() {
        clean(SITE); // website programs do not need `main`
    }

    #[test]
    fn website_with_main_still_passes() {
        clean(&format!("{} func main() {{ print(1); }}", SITE));
    }

    #[test]
    fn two_websites_are_rejected() {
        let src = format!("{} website \"T\" {{ page P1 {{ text \"x\" }} }}", SITE);
        assert!(has("E3030", &src));
    }

    #[test]
    fn empty_site_name_is_rejected() {
        assert!(has("E3033", "website \"  \" { page Home { text \"x\" } }"));
    }

    #[test]
    fn duplicate_page_names_are_rejected_case_insensitively() {
        assert!(has("E3031", "website \"S\" { page Home { text \"a\" } page home { text \"b\" } }"));
    }

    #[test]
    fn unknown_navigation_target_is_rejected_with_hint() {
        let diags = check_src("website \"S\" { page Home { button \"Go\" open Missing } }");
        let d = diags.iter().find(|d| d.code == "E3032").expect("E3032");
        assert!(d.hint.as_deref().unwrap().contains("Home"));
    }

    #[test]
    fn empty_image_alt_is_rejected() {
        assert!(has("E3034", "website \"S\" { page Home { image \"x.png\" alt \" \" } }"));
    }

    #[test]
    fn empty_button_label_is_rejected() {
        assert!(has("E3035", "website \"S\" { page Home { button \" \" open Home } }"));
    }

    #[test]
    fn empty_image_src_is_rejected() {
        assert!(has("E3036", "website \"S\" { page Home { image \" \" alt \"ok\" } }"));
    }

    #[test]
    fn website_in_imported_module_is_rejected() {
        let diags = check_two(
            "import \"lib.kin\" as l;\nfunc main() { print(l.f()); }",
            "website \"S\" { page Home { text \"x\" } }\nfunc f() -> int { return 1; }",
        );
        assert!(diags.iter().any(|d| d.code == "E3037"), "{:?}", diags);
    }

    #[test]
    fn website_diagnostics_are_attributed_to_the_entry_file() {
        // P-028 regression: the pass-0 loop leaves `file` at the last
        // project file, so an entry-file website diagnostic (E3032
        // family) used to render under the last imported module. The
        // checker restores the entry path before `check_website`
        // (mirroring the app path).
        let diags = check_two(
            concat!(
                "import \"lib.kin\" as l;\n",
                "website \"S\" {\n",
                "    page Home { button \"Go\" open Missing }\n",
                "    page Other { text \"y\" }\n",
                "}\n",
                "func main() { print(l.f()); }\n",
            ),
            "func f() -> int { return 1; }",
        );
        let d = diags.iter().find(|d| d.code == "E3032").expect("E3032");
        assert_eq!(d.file, "main.kin", "the entry file, not lib.kin");
        assert_eq!(d.line, 3, "the `open Missing` target");
        assert_eq!(d.col, 34, "the target identifier");
        assert_eq!(d.len, 7, "the width of `Missing`");
        // The same restore covers the rest of check_website: the
        // unreachable-page warning names the entry file too.
        let w = diags.iter().find(|d| d.code == "W3035").expect("W3035");
        assert_eq!(w.file, "main.kin");
        assert_eq!(w.line, 4, "the `page Other` declaration");
    }

    #[test]
    fn duplicate_title_warns_and_unreachable_page_warns() {
        let diags = check_src(
            "website \"S\" {\n    page Home {\n        title \"A\"\n        title \"B\"\n    }\n    page Orphan { text \"never linked\" }\n}",
        );
        assert!(diags.iter().any(|d| d.code == "W3036"));
        assert!(diags.iter().any(|d| d.code == "W3035"));
    }

    #[test]
    fn empty_text_warns() {
        assert!(has("W3037", "website \"S\" { page Home { text \"  \" } }"));
    }

    #[test]
    fn website_only_program_has_no_e3008() {
        // Missing main with a website present is NOT E3008 (the run
        // command reports guidance instead).
        assert!(!has("E3008", SITE));
    }

    // -- 0.4.0 S2: app presence rules (E3051/E3052; internals land in S3) ----

    const APP: &str = "app \"A\" { page Home { text \"hi\" } }\n";

    #[test]
    fn app_only_program_has_no_e3008() {
        // B.2: `main` remains optional for app programs (like websites).
        clean(APP);
    }

    #[test]
    fn app_with_main_still_passes() {
        clean("func main() { }\napp \"A\" { page Home { text \"hi\" } }\n");
    }

    #[test]
    fn app_outside_entry_file_is_e3051() {
        let diags = check_two(
            "import \"lib.kin\" as lib;\napp \"A\" { page Home { text \"hi\" } }\n",
            "app \"B\" { page Home { text \"x\" } }\n",
        );
        assert!(diags.iter().any(|d| d.code == "E3051"), "{:?}", diags);
    }

    #[test]
    fn website_and_app_together_is_e3052() {
        let src = format!("{}{}\n", SITE, APP);
        assert!(has("E3052", &src));
    }

    #[test]
    fn two_apps_are_e3052() {
        let src = format!("{}{}", APP, APP);
        assert!(has("E3052", &src));
        assert!(!has("E3030", &src)); // E3030 stays website-specific
    }

    // -- 0.4.0 S3 slice 1: app declaration validation (E3062, E3042, ------
    //    E3055-E3057) -------------------------------------------------------

    #[test]
    fn empty_app_name_is_rejected() {
        assert!(has("E3062", "app \"\" { page Home { text \"x\" } }"));
        assert!(has("E3062", "app \"   \" { page Home { text \"x\" } }"));
        // A name that trims to content is fine (mirrors E3033's trim rule).
        clean("app \"  Tasks  \" { page Home { text \"x\" } }");
    }

    #[test]
    fn empty_app_name_is_positioned_with_hint_and_skips_internals() {
        // Mirrors E3033 exactly: the diagnostic anchors on the app
        // declaration, carries a hint, and an unnamed app is not
        // registered — its internals are not further checked (the
        // broken route adds no E3055), and E3008 pairs with it just as
        // it does for websites.
        let src = "app \"\" { route \"bad path\" -> Home page Home { text \"x\" } }";
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3062").expect("E3062");
        assert_eq!((d.line, d.col), (1, 1));
        assert!(d.hint.as_deref().unwrap().contains("app \"Tasks\" {"));
        assert!(!diags.iter().any(|d| d.code == "E3055"), "{:?}", diags);
        assert!(diags.iter().any(|d| d.code == "E3008"));
    }

    #[test]
    fn duplicate_app_state_fields_are_rejected() {
        let src = "app \"A\" {\n    state { n: int = 0 }\n    state { n: int = 1 }\n    page Home { text \"x\" }\n}\n";
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3042").expect("E3042");
        assert_eq!(d.line, 3, "the duplicate, not the first");
        assert_eq!(d.len, 1, "the width of `n`");
        assert!(
            d.message.contains("first declared as a state field at line 2"),
            "{}",
            d.message
        );
        assert!(d.hint.is_some());
    }

    #[test]
    fn state_and_computed_names_collide_in_the_app_scope() {
        let diags = check_src(
            "app \"A\" { state { n: int = 0 } computed n = 1 page Home { text \"x\" } }",
        );
        let d = diags.iter().find(|d| d.code == "E3042").expect("E3042");
        assert!(d.message.contains("state field"), "{}", d.message);
        // The reverse order collides too.
        assert!(has(
            "E3042",
            "app \"A\" { computed n = 1 state { n: int = 0 } page Home { text \"x\" } }"
        ));
    }

    #[test]
    fn duplicate_computed_names_are_rejected() {
        let diags =
            check_src("app \"A\" { computed a = 1 computed a = 2 page Home { text \"x\" } }");
        let d = diags.iter().find(|d| d.code == "E3042").expect("E3042");
        assert_eq!(d.len, 8, "the `computed` keyword anchors the duplicate");
    }

    #[test]
    fn duplicate_component_names_in_one_scope_are_rejected() {
        // prop vs prop
        assert!(has(
            "E3042",
            "app \"A\" { component R { prop l: string prop l: string view { text l } } page Home { text \"x\" } }"
        ));
        // prop vs state
        assert!(has(
            "E3042",
            "app \"A\" { component R { prop l: string state l: string = \"x\" view { text l } } page Home { text \"x\" } }"
        ));
        // state vs state
        assert!(has(
            "E3042",
            "app \"A\" { component R { state n: int = 0 state n: int = 1 view { text \"x\" } } page Home { text \"x\" } }"
        ));
    }

    #[test]
    fn same_name_in_two_components_is_not_a_duplicate() {
        // Component scopes are independent.
        clean(
            "app \"A\" { component R1 { prop l: string view { text l } } component R2 { prop l: string view { text l } } page Home { text \"x\" } }",
        );
        // A component prop may share a name with app state — different
        // scopes; the shadowing rules arrive with scope resolution.
        clean(
            "app \"A\" { state { l: string = \"x\" } component R { prop l: string view { text l } } page Home { text \"x\" } }",
        );
    }

    #[test]
    fn malformed_route_paths_are_rejected() {
        for bad in [
            "", "/about", "about/", "a//b", "a/:", "x/:1id", "x/:id/:id", "a:b", "../secret",
            "./here", "item list",
        ] {
            let src = format!(
                "app \"A\" {{ route \"{bad}\" -> Home page Home {{ text \"x\" }} }}"
            );
            assert!(has("E3055", &src), "expected E3055 for path {bad:?}");
        }
    }

    #[test]
    fn valid_route_paths_pass() {
        clean(
            "app \"A\" { route \"about\" -> Home route \"items/:id\" -> Home route \"a/b/:c/d\" -> Home route \"user-1/_p\" -> Home route \"v1.2/api\" -> Home page Home { text \"x\" } }",
        );
    }

    #[test]
    fn malformed_route_path_is_positioned_on_the_string_with_hint() {
        let src = "app \"A\" {\n    route \"/about\" -> Home\n    page Home { text \"x\" }\n}\n";
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3055").expect("E3055");
        assert_eq!((d.line, d.col, d.len), (2, 11, 8), "the quoted \"/about\"");
        assert!(d.message.contains("must not start with `/`"), "{}", d.message);
        assert!(d.hint.as_deref().unwrap().contains(":param"));
    }

    #[test]
    fn duplicate_routes_are_rejected() {
        let src = "app \"A\" {\n    route \"items/:id\" -> Home\n    route \"items/:id\" -> Home\n    page Home { text \"x\" }\n}\n";
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3056").expect("E3056");
        assert_eq!(d.line, 3, "the later route");
        assert!(d.message.contains("line 2"), "{}", d.message);
        assert!(d.hint.is_some());
    }

    #[test]
    fn same_shape_with_different_param_names_is_a_duplicate_route() {
        // "item/:id" and "item/:name" match the same addresses; no
        // router could tell them apart (implementation ruling, C2).
        assert!(has(
            "E3056",
            "app \"A\" { route \"item/:id\" -> Home route \"item/:name\" -> Home page Home { text \"x\" } }"
        ));
    }

    #[test]
    fn different_shapes_are_not_duplicates() {
        clean(
            "app \"A\" { route \"item/:id\" -> Home route \"item/detail\" -> Home route \"about\" -> Home route \"about/us\" -> Home page Home { text \"x\" } }",
        );
    }

    #[test]
    fn spa_without_not_found_is_rejected() {
        let src = "app \"A\" {\n    navigation spa\n    page Home { text \"x\" }\n}\n";
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3057").expect("E3057");
        assert_eq!((d.line, d.col), (1, 1), "the app declaration");
        assert!(d.hint.as_deref().unwrap().contains("not_found"));
    }

    #[test]
    fn not_found_in_mpa_is_rejected() {
        // Default mode (no navigation section) is mpa.
        let src = "app \"A\" {\n    not_found Missing\n    page Home { text \"x\" }\n}\n";
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3057").expect("E3057");
        assert_eq!(d.line, 2, "positioned on the not_found target");
        // Explicit mpa behaves the same.
        assert!(has(
            "E3057",
            "app \"A\" { navigation mpa not_found Missing page Home { text \"x\" } }"
        ));
    }

    #[test]
    fn spa_with_not_found_and_mpa_without_both_pass() {
        clean("app \"A\" { navigation spa not_found Home page Home { text \"x\" } }");
        clean("app \"A\" { navigation mpa page Home { text \"x\" } }");
        clean("app \"A\" { page Home { text \"x\" } }"); // default mpa
    }

    #[test]
    fn app_diagnostics_accumulate() {
        let src = "app \"A\" {\n    navigation spa\n    route \"bad path\" -> Home\n    route \"/x\" -> Home\n    page Home { text \"x\" }\n}\n";
        let diags = check_src(src);
        assert_eq!(diags.iter().filter(|d| d.code == "E3055").count(), 2);
        assert!(diags.iter().any(|d| d.code == "E3057"));
    }

    #[test]
    fn app_diagnostics_are_attributed_to_the_entry_file() {
        // The Pass 0b loop leaves `file` at the last project file; the
        // checker restores the entry path before checking app internals
        // (the website path does the same since P-028 was fixed).
        let diags = check_two(
            "import \"lib.kin\" as l;\napp \"A\" { navigation spa page Home { text \"x\" } }\n",
            "func f() { }\n",
        );
        let d = diags.iter().find(|d| d.code == "E3057").expect("E3057");
        assert_eq!(d.file, "main.kin");
    }

    // -- 0.4.0 S3 slice 2: page-target resolution (E3063) and ----------------
    //    page/component name rules (E3064/E3065; P-029 code assignment) ------

    #[test]
    fn unknown_app_page_targets_are_rejected() {
        // Every page-target position the app grammar admits: route ->,
        // not_found, and open on buttons/links.
        let src = "app \"A\" { route \"x\" -> Missing page Home { text \"x\" } }";
        let d = check_src(src)
            .into_iter()
            .find(|d| d.code == "E3063")
            .expect("E3063 for route target");
        assert_eq!((d.line, d.col, d.len), (1, 24, 7), "the `Missing` identifier");
        assert!(
            d.message
                .contains("route \"x\" targets page `Missing` which is not declared in this app"),
            "{}",
            d.message
        );

        let src = "app \"A\" { navigation spa not_found Missing page Home { text \"x\" } }";
        let d = check_src(src)
            .into_iter()
            .find(|d| d.code == "E3063")
            .expect("E3063 for not_found target");
        assert_eq!((d.line, d.col, d.len), (1, 36, 7));
        assert!(
            d.message
                .contains("`not_found` targets page `Missing` which is not declared in this app"),
            "{}",
            d.message
        );

        let src = "app \"A\" { page Home { button \"Go\" open Missing } }";
        let d = check_src(src)
            .into_iter()
            .find(|d| d.code == "E3063")
            .expect("E3063 for button open target");
        assert_eq!((d.line, d.col, d.len), (1, 40, 7));
        assert!(
            d.message
                .contains("button \"Go\" opens page `Missing` which is not declared in this app"),
            "{}",
            d.message
        );

        let src = "app \"A\" { page Home { link \"More\" open Missing } }";
        let d = check_src(src)
            .into_iter()
            .find(|d| d.code == "E3063")
            .expect("E3063 for link open target");
        assert_eq!((d.line, d.col, d.len), (1, 40, 7));
        assert!(
            d.message
                .contains("link \"More\" opens page `Missing` which is not declared in this app"),
            "{}",
            d.message
        );

        // A non-literal label falls back to the element kind name.
        let diags = check_src("app \"A\" { page Home { button label open Missing } }");
        let d = diags.iter().find(|d| d.code == "E3063").expect("E3063");
        assert!(d.message.starts_with("button opens page"), "{}", d.message);
    }

    #[test]
    fn e3063_hint_lists_declared_pages() {
        let src = "app \"A\" {\n    route \"items\" -> Missing\n    page Home { text \"x\" }\n}\n";
        let d = check_src(src)
            .into_iter()
            .find(|d| d.code == "E3063")
            .expect("E3063");
        assert_eq!(d.hint.as_deref().unwrap(), "declared pages: Home");
        // An app with no pages at all still words the hint honestly.
        let diags = check_src("app \"A\" { route \"x\" -> Missing }");
        let d = diags.iter().find(|d| d.code == "E3063").expect("E3063");
        assert_eq!(d.hint.as_deref().unwrap(), "this app declares no pages");
    }

    #[test]
    fn e3063_hint_names_a_component_target() {
        // Pages and components are separate namespaces: a target that
        // names a component is unknown as a page, and the hint says so
        // (architect ruling — 'X' is a component; routes target pages).
        let src = "app \"A\" { route \"x\" -> Row component Row { view { text \"y\" } } page Home { text \"x\" } }";
        let d = check_src(src)
            .into_iter()
            .find(|d| d.code == "E3063")
            .expect("E3063");
        assert!(
            d.hint
                .as_deref()
                .unwrap()
                .starts_with("'Row' is a component; routes target pages"),
            "{}",
            d.hint.unwrap()
        );
        assert!(d.hint.as_deref().unwrap().contains("declared pages: Home"));
    }

    #[test]
    fn valid_app_page_targets_pass() {
        // All four positions, all resolved: routes, not_found (SPA),
        // and open on buttons/links in page and component views.
        clean(
            "app \"A\" {\n    navigation spa\n    not_found About\n    route \"items/:id\" -> Item\n    page About { text \"back\" }\n    page Item { button \"Go\" open About link \"More\" open About }\n    component Nav { view { button \"N\" open About } }\n}\n",
        );
    }

    #[test]
    fn e3063_fires_inside_nested_views() {
        // `when`/`otherwise`, `each` and `form` bodies and component
        // views hold page-target positions too.
        let src = concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        when true { button \"W\" open MissingW }\n",
            "        each [1, 2] as item { link \"E\" open MissingE }\n",
            "        form \"F\" { button \"F1\" open MissingF }\n",
            "    }\n",
            "    component Card { view { button \"C\" open MissingC } }\n",
            "}\n",
        );
        let diags = check_src(src);
        let codes: Vec<_> = diags.iter().filter(|d| d.code == "E3063").collect();
        assert_eq!(codes.len(), 4, "{:#?}", diags);
        for name in ["MissingW", "MissingE", "MissingF", "MissingC"] {
            assert!(
                codes.iter().any(|d| d.message.contains(name)),
                "no E3063 for {name}"
            );
        }
        // Lines: the four targets anchor on their identifiers.
        assert_eq!(codes[0].line, 3);
        assert_eq!(codes[1].line, 4);
        assert_eq!(codes[2].line, 5);
        assert_eq!(codes[3].line, 7);
    }

    #[test]
    fn app_page_targets_never_reuse_e3032() {
        // E3032 is website-scoped and frozen (ADR-0011); the app-side
        // sibling never borrows it.
        let src = "app \"A\" { route \"x\" -> Missing page Home { button \"Go\" open Missing } }";
        assert!(has("E3063", src));
        assert!(!has("E3032", src));
    }

    #[test]
    fn mpa_not_found_target_is_not_resolved() {
        // A `not_found` rejected by E3057 (MPA) is not further resolved
        // — the same "invalid declarations are not further checked"
        // pattern as E3062 and malformed E3055 routes.
        let diags = check_src("app \"A\" { not_found Missing page Home { text \"x\" } }");
        assert!(diags.iter().any(|d| d.code == "E3057"));
        assert!(!diags.iter().any(|d| d.code == "E3063"), "{:?}", diags);
    }

    #[test]
    fn app_pages_and_components_are_separate_namespaces() {
        // A page and a component may share a name: `<Card>(...)`
        // instantiates the component, `route -> Card` targets the page.
        clean(
            "app \"A\" { route \"card\" -> Card component Card { view { text \"c\" } } page Card { text \"p\" } }",
        );
        // The same with an open target: it resolves to the page.
        clean(
            "app \"A\" { component Card { view { text \"c\" } } page Home { button \"Go\" open Card } page Card { text \"p\" } }",
        );
    }

    #[test]
    fn duplicate_app_page_names_are_rejected_case_insensitively() {
        // Mirrors E3031 (website pages): slugs are lowercased, so
        // `Home` and `home` would collide in the output directory.
        let src = "app \"A\" {\n    page Home { text \"a\" }\n    page home { text \"b\" }\n}\n";
        let d = check_src(src)
            .into_iter()
            .find(|d| d.code == "E3064")
            .expect("E3064");
        assert_eq!(d.line, 3, "the later page");
        assert_eq!(d.col, 5, "the `page` keyword");
        assert_eq!(d.len, 4);
        assert!(
            d.message.contains("duplicate page name `home` (first defined at line 2)"),
            "{}",
            d.message
        );
        assert!(
            d.hint
                .as_deref()
                .unwrap()
                .contains("case-insensitive"),
            "{}",
            d.hint.unwrap()
        );
    }

    #[test]
    fn duplicate_app_component_names_are_rejected_case_sensitively() {
        let src = "app \"A\" {\n    component Card { view { text \"x\" } }\n    component Card { view { text \"y\" } }\n}\n";
        let d = check_src(src)
            .into_iter()
            .find(|d| d.code == "E3065")
            .expect("E3065");
        assert_eq!(d.line, 3, "the later component");
        assert_eq!(d.col, 5, "the `component` keyword");
        assert_eq!(d.len, 9);
        assert!(
            d.message
                .contains("duplicate component name `Card` (first defined at line 2)"),
            "{}",
            d.message
        );
        // Identifier semantics: `card` and `Card` are different names
        // (matching function names, E3002).
        clean(
            "app \"A\" { component Card { view { text \"x\" } } component card { view { text \"y\" } } }",
        );
    }

    // -- 0.4.0 S1: cross-module value exports (`const`) ----------------------

    const CONST_PROG: &str = "const MAX: int = 10;\nfunc main() { print(MAX); }\n";

    #[test]
    fn const_declaration_passes() {
        clean(CONST_PROG);
        clean(
            "const A: [int] = [1, 2, 3];\nconst B: [string] = [];\nconst C: bool = true;\nconst D: float = 1.5 + 2.5;\nfunc main() { }\n",
        );
    }

    #[test]
    fn const_read_types_like_its_declared_type() {
        clean("const GREETING: string = \"hi\";\nfunc main() { fin s: string = GREETING; print(s); }");
        // Type flows: passing a string const where int is expected fails
        // (E3003, the general type-mismatch code; E3011 is int/float only).
        assert!(has(
            "E3003",
            "const GREETING: string = \"hi\";\nfunc main() { fin x: int = GREETING; }"
        ));
    }

    #[test]
    fn duplicate_const_is_rejected() {
        assert!(has(
            "E3002",
            "const MAX: int = 10;\nconst MAX: int = 20;\nfunc main() { }"
        ));
    }

    #[test]
    fn const_and_function_name_collision_is_rejected() {
        // const declared after the function of the same name...
        assert!(has(
            "E3002",
            "func MAX() { }\nconst MAX: int = 10;\nfunc main() { }"
        ));
        // ...and before it.
        assert!(has(
            "E3002",
            "const MAX: int = 10;\nfunc MAX() { }\nfunc main() { }"
        ));
    }

    #[test]
    fn const_cannot_shadow_a_builtin() {
        assert!(has(
            "E3016",
            "const print: int = 1;\nfunc main() { }"
        ));
    }

    #[test]
    fn const_initializer_must_be_constant() {
        // A function call in the initializer.
        assert!(has(
            "E3039",
            "func f() -> int { return 1; }\nconst X: int = f();\nfunc main() { }"
        ));
        // A local variable can never be a constant.
        assert!(has(
            "E3039",
            "func main() { fin y = 1; }\nconst X: int = y;\n"
        ));
        // A forward reference (const declared later in the file).
        assert!(has(
            "E3039",
            "const A: int = B + 1;\nconst B: int = 2;\nfunc main() { }"
        ));
        // A qualified read is not allowed in an initializer.
        let diags = check_two(
            "import \"lib.kin\" as l;\nconst A: int = l.C;\nfunc main() { print(A); }",
            "const C: int = 1;\nfunc f() { }\n",
        );
        assert!(diags.iter().any(|d| d.code == "E3039"), "{:?}", diags);
    }

    #[test]
    fn const_type_mismatch_is_rejected() {
        // int/float mismatch (no implicit conversion, ADR-0004) → E3011.
        assert!(has("E3011", "const X: int = 1.5;\nfunc main() { }"));
        assert!(has("E3011", "const X: float = 1;\nfunc main() { }"));
        // string vs int → E3003 (general type mismatch).
        assert!(has("E3003", "const X: int = \"hi\";\nfunc main() { }"));
        // declared array type vs scalar → E3003.
        assert!(has("E3003", "const X: [int] = 5;\nfunc main() { }"));
    }

    #[test]
    fn const_cannot_be_void() {
        assert!(has("E3015", "const X: void = 1;\nfunc main() { }"));
    }

    #[test]
    fn const_arithmetic_errors_are_compile_time() {
        // Integer overflow in a constant expression.
        assert!(has(
            "E3039",
            "const X: int = 9223372036854775807 + 1;\nfunc main() { }"
        ));
        // Integer division by zero.
        assert!(has("E3039", "const X: int = 1 / 0;\nfunc main() { }"));
        // Float division by zero (ADR-0008).
        assert!(has("E3039", "const X: float = 1.0 / 0.0;\nfunc main() { }"));
        // Integer multiplication overflow.
        assert!(has(
            "E3039",
            "const X: int = 9223372036854775807 * 2;\nfunc main() { }"
        ));
    }

    #[test]
    fn assigning_to_a_const_is_rejected() {
        assert!(has(
            "E3004",
            "const MAX: int = 10;\nfunc main() { MAX = 5; }"
        ));
        assert!(has(
            "E3004",
            "const MAX: int = 10;\nfunc main() { MAX += 1; }"
        ));
    }

    #[test]
    fn local_shadows_const_without_diagnostic() {
        // A local binding wins over a const of the same name (documented
        // rule); the program is valid and no diagnostic fires.
        let diags = check_src(
            "const X: int = 1;\nfunc main() { fin X: int = 2; print(X); }\n",
        );
        assert!(
            !diags.iter().any(|d| d.is_error()),
            "{:?}",
            diags
        );
    }

    #[test]
    fn qualified_const_read_checks_types() {
        let diags = check_two(
            "import \"lib.kin\" as l;\nfunc main() { fin x: int = l.LIMIT; print(x); }",
            "const LIMIT: int = 100;\nfunc f() { }\n",
        );
        assert!(
            !diags.iter().any(|d| d.is_error()),
            "{:?}",
            diags
        );
        // Wrong type through the qualified read.
        let diags = check_two(
            "import \"lib.kin\" as l;\nfunc main() { fin x: int = l.NAME; }",
            "const NAME: string = \"kinn\";\nfunc f() { }\n",
        );
        assert!(diags.iter().any(|d| d.code == "E3003"), "{:?}", diags);
    }

    #[test]
    fn qualified_read_of_unknown_member_is_rejected() {
        // Unknown const name.
        let diags = check_two(
            "import \"lib.kin\" as l;\nfunc main() { fin x: int = l.NOPE; }",
            "const LIMIT: int = 1;\nfunc f() { }\n",
        );
        assert!(diags.iter().any(|d| d.code == "E3021"), "{:?}", diags);
        // Reading a function as a const tells the user to call it.
        let diags = check_two(
            "import \"lib.kin\" as l;\nfunc main() { fin x: int = l.f; }",
            "const LIMIT: int = 1;\nfunc f() -> int { return 1; }\n",
        );
        let d = diags.iter().find(|d| d.code == "E3021").expect("E3021");
        assert!(d.message.contains("is a function"), "{}", d.message);
        // Calling a const like a function tells the user to read it.
        let diags = check_two(
            "import \"lib.kin\" as l;\nfunc main() { print(l.LIMIT()); }",
            "const LIMIT: int = 1;\nfunc f() { }\n",
        );
        let d = diags.iter().find(|d| d.code == "E3021").expect("E3021");
        assert!(d.message.contains("not a function"), "{}", d.message);
    }

    #[test]
    fn qualified_read_of_unknown_module_is_rejected() {
        let diags = check_two(
            "func main() { fin x: int = nope.NOPE; }",
            "const LIMIT: int = 1;\nfunc f() { }\n",
        );
        assert!(diags.iter().any(|d| d.code == "E3020"), "{:?}", diags);
    }

    #[test]
    fn undefined_variable_hints_at_module_const() {
        let diags = check_two(
            "import \"lib.kin\" as l;\nfunc main() { print(LIMIT); }",
            "const LIMIT: int = 1;\nfunc f() { }\n",
        );
        let d = diags.iter().find(|d| d.code == "E3001").expect("E3001");
        assert!(d.message.contains("l.LIMIT"), "{}", d.message);
    }

    // -- S3 slice 3: app scope resolution and binding typing ---------------

    /// The slice-3 feature surface in one app: state, computed, props,
    /// component state, `each` with a stable key, `input bind` and
    /// `field` on declared state keys, `param` for a declared route
    /// parameter, store/event builtins, `str`, and B.7's array append.
    const MINI_APP: &str = concat!(
        "*mode = B;\n",
        "const CAP: int = 3;\n",
        "func double(x: int) -> int { return x + x; }\n",
        "app \"Mini\" {\n",
        "    navigation spa\n",
        "    not_found Home\n",
        "    route \"item/:id\" -> Detail\n",
        "    state {\n",
        "        items: [string] = []\n",
        "        n: int = 0\n",
        "        draft: string = \"\"\n",
        "    }\n",
        "    computed label = str(double(n)) + \" left\"\n",
        "    store \"s\" { items }\n",
        "    component Row {\n",
        "        prop label: string\n",
        "        state open: bool = false\n",
        "        view {\n",
        "            text label\n",
        "            button \"Toggle\" {\n",
        "                on click { open = not open; }\n",
        "            }\n",
        "        }\n",
        "    }\n",
        "    page Home {\n",
        "        heading label\n",
        "        each items as item key item {\n",
        "            Row(label: item)\n",
        "        }\n",
        "        input bind n\n",
        "        when n == 0 {\n",
        "            text \"empty\"\n",
        "        } otherwise {\n",
        "            text label\n",
        "        }\n",
        "        form \"Add\" {\n",
        "            field draft kind text {\n",
        "                on input { draft = event_value; }\n",
        "            }\n",
        "            button \"Add\" {\n",
        "                on submit {\n",
        "                    if form_valid {\n",
        "                        items = items + [draft];\n",
        "                        n += 1;\n",
        "                        store_save(\"s\");\n",
        "                    }\n",
        "                }\n",
        "            }\n",
        "        }\n",
        "        link \"Detail\" open Detail\n",
        "    }\n",
        "    page Detail {\n",
        "        text param(\"id\")\n",
        "        link \"Back\" open Home\n",
        "    }\n",
        "}\n",
    );

    #[test]
    fn slice3_feature_surface_checks_clean() {
        // Positive master: every slice-3 feature type-checks with zero
        // diagnostics (warnings included — none are expected here).
        clean(MINI_APP);
    }

    #[test]
    fn unknown_names_in_views_computed_and_handlers_report_e3066() {
        // View binding expression.
        let src = "app \"A\" {\n    state { n: int = 1 }\n    page Home {\n        text m\n    }\n}\n";
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3066").expect("E3066 in view");
        assert_eq!((d.line, d.col, d.len), (4, 14, 1), "anchored on the identifier");
        assert!(d.hint.as_deref().unwrap().contains("app state: n"), "{:?}", d.hint);
        // Computed expression.
        assert!(has(
            "E3066",
            "app \"A\" { state { n: int = 1 } computed c = m page Home { text \"x\" } }"
        ));
        // Handler statement (read) and call.
        assert!(has(
            "E3066",
            "app \"A\" { page Home { button \"B\" { on click { nope(1); } } } }"
        ));
        assert!(has(
            "E3066",
            "app \"A\" { page Home { button \"B\" { on click { fin x = m; } } } }"
        ));
        // Assignment target that names nothing.
        let diags = check_src("app \"A\" { page Home { button \"B\" { on click { ghost = 1; } } } }");
        let d = diags
            .iter()
            .find(|d| d.code == "E3066")
            .expect("E3066 for assignment target");
        assert!(
            d.message.contains("assignment target `ghost`"),
            "{}",
            d.message
        );
        // E3001 stays frozen for ordinary function bodies.
        assert!(has("E3001", "func main() { print(x); }"));
        assert!(!has("E3066", "func main() { print(x); }"));
    }

    #[test]
    fn unknown_bind_targets_report_e3066() {
        let src = "app \"A\" {\n    state { n: int = 1 }\n    page Home {\n        field due kind text { }\n        input bind nope\n    }\n}\n";
        let diags = check_src(src);
        let field_d = diags
            .iter()
            .find(|d| d.code == "E3066" && d.message.contains("`field` binds"))
            .expect("E3066 for the field target");
        assert!(field_d.message.contains("`due`"), "{}", field_d.message);
        assert!(field_d.hint.as_deref().unwrap().contains("app state: n"), "{:?}", field_d.hint);
        assert!(diags
            .iter()
            .any(|d| d.code == "E3066" && d.message.contains("`input` binds `nope`")));
        // Negative: binding a declared state key is clean (MINI_APP's
        // `input bind n` and `field draft` both resolve).
        clean("app \"A\" { state { n: int = 1 } page Home { input bind n field n kind number { } } }");
    }

    #[test]
    fn e3066_hint_lists_component_scope_first() {
        let src = concat!(
            "app \"A\" {\n",
            "    state { n: int = 1 }\n",
            "    component Row {\n",
            "        prop label: string\n",
            "        state open: bool = false\n",
            "        view {\n",
            "            text nope\n",
            "        }\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3066").expect("E3066");
        let hint = d.hint.as_deref().unwrap();
        assert!(hint.contains("props: label"), "{:?}", hint);
        assert!(hint.contains("component state: open"), "{:?}", hint);
        assert!(hint.contains("app state: n"), "{:?}", hint);
        assert!(hint.starts_with("declared here: props"), "{:?}", hint);
    }

    #[test]
    fn props_computed_and_event_builtins_are_e3043_in_handlers() {
        // Prop assignment.
        let src = concat!(
            "app \"A\" {\n",
            "    component Row {\n",
            "        prop label: string\n",
            "        view { button \"B\" { on click { label = \"x\"; } } }\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3043").expect("E3043 for prop");
        assert!(d.message.contains("prop `label`"), "{}", d.message);
        assert!(
            d.hint.as_deref().unwrap().contains("in the component and assign"),
            "{:?}",
            d.hint
        );
        // Computed assignment.
        let src = concat!(
            "app \"A\" {\n",
            "    state { n: int = 1 }\n",
            "    computed c = n\n",
            "    page Home { button \"B\" { on click { c = 2; } } }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3043").expect("E3043 for computed");
        assert!(d.message.contains("computed value `c`"), "{}", d.message);
        assert!(
            d.hint.as_deref().unwrap().contains("recomputes automatically"),
            "{:?}",
            d.hint
        );
        // Event builtin assignment.
        assert!(has(
            "E3043",
            "app \"A\" { page Home { button \"B\" { on click { event_value = \"x\"; } } } }"
        ));
    }

    #[test]
    fn state_assignment_is_legal_and_e3004_stays_for_fin_and_consts() {
        // Negative: assigning to state (shared or component) is legal.
        clean(concat!(
            "app \"A\" {\n",
            "    state { n: int = 1 }\n",
            "    page Home {\n",
            "        button \"B\" { on click { n = 2; } }\n",
            "        button \"C\" { on click { n += 1; } }\n",
            "    }\n",
            "}\n",
        ));
        clean(concat!(
            "app \"A\" {\n",
            "    component Row {\n",
            "        state open: bool = false\n",
            "        view { button \"B\" { on click { open = true; } } }\n",
            "    }\n",
            "    page Home { text \"x\" }\n",
            "}\n",
        ));
        // `fin` locals and consts keep E3004 (frozen meanings — the
        // B.4 erratum): assigning to them in a handler is the ordinary
        // immutable-assignment error, never E3043.
        let diags = check_src(concat!(
            "const K: int = 1;\n",
            "app \"A\" {\n",
            "    state { n: int = 1 }\n",
            "    page Home {\n",
            "        button \"B\" { on click { fin x = 1; x = 2; K = 3; } }\n",
            "    }\n",
            "}\n",
        ));
        let e3004s: Vec<_> = diags.iter().filter(|d| d.code == "E3004").collect();
        assert_eq!(e3004s.len(), 2, "{:#?}", diags);
        assert!(diags.iter().all(|d| d.code != "E3043"), "{:#?}", diags);
        // State array element assignment is legal (B.4).
        clean(concat!(
            "app \"A\" {\n",
            "    state { xs: [int] = [1] }\n",
            "    page Home { button \"B\" { on click { xs[0] = 9; } } }\n",
            "}\n",
        ));
    }

    #[test]
    fn bind_targets_naming_computed_props_or_consts_are_e3043() {
        let diags = check_src(concat!(
            "const K: int = 1;\n",
            "app \"A\" {\n",
            "    state { n: int = 1 }\n",
            "    computed c = n\n",
            "    component Row {\n",
            "        prop label: string\n",
            "        view { input bind label }\n",
            "    }\n",
            "    page Home {\n",
            "        field c kind text { }\n",
            "        input bind K\n",
            "    }\n",
            "}\n",
        ));
        let e3043s: Vec<_> = diags.iter().filter(|d| d.code == "E3043").collect();
        assert_eq!(e3043s.len(), 3, "{:#?}", diags);
        assert!(e3043s.iter().any(|d| d.message.contains("a prop")), "{:#?}", e3043s);
        assert!(e3043s.iter().any(|d| d.message.contains("a computed value")), "{:#?}", e3043s);
        assert!(e3043s.iter().any(|d| d.message.contains("the const `K`")), "{:#?}", e3043s);
    }

    #[test]
    fn param_of_undeclared_route_parameter_is_e3058() {
        let src = concat!(
            "app \"A\" {\n",
            "    route \"items/:id\" -> Home\n",
            "    page Home {\n",
            "        text param(\"nope\")\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3058").expect("E3058");
        assert_eq!((d.line, d.col), (4, 20), "anchored on the quoted argument");
        assert_eq!(d.len, 6, "the quoted \"nope\"");
        assert!(
            d.hint.as_deref().unwrap().contains("declared route parameters: id"),
            "{:?}",
            d.hint
        );
        // Negative: the declared parameter resolves (MINI_APP uses
        // param(\"id\") with route \"item/:id\"), and a non-literal
        // argument is runtime data, not checkable statically.
        clean("app \"A\" { state { s: string = \"id\" } route \"x/:id\" -> Home page Home { text param(s) } }");
        // No routes at all: the hint says how to declare one.
        let diags = check_src("app \"A\" { page Home { text param(\"id\") } }");
        let d = diags.iter().find(|d| d.code == "E3058").expect("E3058");
        assert!(
            d.hint.as_deref().unwrap().contains("no route declares a parameter yet"),
            "{:?}",
            d.hint
        );
    }

    #[test]
    fn handler_local_shadowing_state_warns_w3041() {
        let src = concat!(
            "app \"A\" {\n",
            "    state { n: int = 0 }\n",
            "    page Home {\n",
            "        button \"B\" {\n",
            "            on click { fin n = 1; }\n",
            "        }\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags
            .iter()
            .find(|d| d.code == "W3041")
            .expect("W3041 for the shadowing local");
        assert_eq!(d.line, 5, "positioned on the local declaration");
        assert!(
            d.message.contains("local `n` shadows the app state field `n` declared at line 2"),
            "{}",
            d.message
        );
        assert!(d.hint.is_some());
        // Component state shadows warn the same way.
        assert!(has(
            "W3041",
            "app \"A\" { component Row { state open: bool = false view { button \"B\" { on click { fin open = true; } } } } page Home { text \"x\" } }"
        ));
        // Negative: a distinct local name is clean; shadowing a
        // computed value or a const stays silent (B.4 names state).
        clean(concat!(
            "app \"A\" {\n",
            "    state { n: int = 0 }\n",
            "    computed c = n\n",
            "    page Home {\n",
            "        button \"B\" { on click { fin m = 1; fin c2 = 2; } }\n",
            "    }\n",
            "}\n",
        ));
        assert!(has(
            "W3041",
            "app \"A\" { state { n: int = 0 } computed c = n page Home { button \"B\" { on click { fin c = 1; } } } }"
        ) == false);
    }

    #[test]
    fn binding_typing_uses_the_ordinary_mismatch_codes() {
        // text/heading expressions must be strings.
        let diags = check_src("app \"A\" { state { n: int = 1 } page Home { text n } }");
        let d = diags.iter().find(|d| d.code == "E3003").expect("E3003 for text");
        assert!(d.message.contains("expected string, found int"), "{}", d.message);
        // `when` conditions must be bool (E3003 — `when` is not `if`
        // and never borrows E3005's frozen message).
        assert!(has("E3003", "app \"A\" { state { n: int = 1 } page Home { when n { text \"x\" } } }"));
        // `each` iterables must be arrays.
        let diags = check_src("app \"A\" { state { n: int = 1 } page Home { each n as x { text \"x\" } } }");
        let d = diags.iter().find(|d| d.code == "E3003").expect("E3003 for each");
        assert!(d.message.contains("`each` iterable must be an array"), "{}", d.message);
        // No implicit int/float conversion in bindings (E3011 — the
        // int↔float pair keeps its dedicated message in app scope too).
        assert!(has(
            "E3011",
            "app \"A\" { state { n: int = 1 } page Home { button \"B\" { on click { n = 1.5; } } } }"
        ));
        // A float bound as text is the ordinary E3003 mismatch.
        assert!(has(
            "E3003",
            "app \"A\" { state { f: float = 1.5 } page Home { text f } }"
        ));
        // Field attributes type-check.
        assert!(has(
            "E3003",
            "app \"A\" { state { n: int = 1 } page Home { field n kind number { max_length \"x\" } } }"
        ));
        // Int/float mixing in a field attribute keeps the E3011
        // dedicated message (no implicit conversions anywhere).
        assert!(has(
            "E3011",
            "app \"A\" { state { n: int = 1 } page Home { field n kind number { max_value 1.5 } } }"
        ));
        // `when` conditions in components, and labels, use the same rule.
        assert!(has(
            "E3003",
            "app \"A\" { component Row { view { button 5 { } } } page Home { text \"x\" } }"
        ));
    }

    #[test]
    fn state_and_prop_initializers_type_check() {
        let diags = check_src("app \"A\" { state { n: int = \"x\" } page Home { text \"x\" } }");
        assert!(diags.iter().any(|d| d.code == "E3003"), "{:#?}", diags);
        let diags = check_src(concat!(
            "app \"A\" {\n",
            "    component Row {\n",
            "        prop label: string = 5\n",
            "        view { text label }\n",
            "    }\n",
            "    page Home { text \"x\" }\n",
            "}\n",
        ));
        assert!(diags.iter().any(|d| d.code == "E3003"), "{:#?}", diags);
        // Negative: typed initializers and empty array literals against
        // a declared array type are clean (exactly like `fin a: [int] = [];`).
        clean(concat!(
            "app \"A\" {\n",
            "    state { xs: [int] = [] n: int = 1 }\n",
            "    component Row {\n",
            "        prop label: string = \"x\"\n",
            "        state open: bool = false\n",
            "        view { text label }\n",
            "    }\n",
            "    page Home { text \"x\" }\n",
            "}\n",
        ));
    }

    #[test]
    fn app_builtin_signatures_are_enforced() {
        // `str` renders scalars, not arrays.
        let diags = check_src(
            "app \"A\" { state { xs: [int] = [1] } page Home { text str(xs) } }",
        );
        let d = diags.iter().find(|d| d.code == "E3003").expect("E3003 from str");
        assert!(d.message.contains("`str` expects a number, bool or string"), "{}", d.message);
        // Arity is the ordinary E3007.
        assert!(has(
            "E3007",
            "app \"A\" { state { s: string = \"x\" } page Home { button \"B\" { on click { store_save(); } } } }"
        ));
        // Types are the ordinary mismatches.
        assert!(has(
            "E3003",
            "app \"A\" { state { s: string = \"x\" } page Home { text upper(5) } }"
        ));
        // split returns [string]: using it as a text value is a mismatch.
        assert!(has(
            "E3003",
            "app \"A\" { state { s: string = \"x\" } page Home { text split(s, \",\") } }"
        ));
        // Negative: the helpers type-check where used correctly
        // (contains(...) is a bool condition; join(...) is a string).
        clean(concat!(
            "app \"A\" {\n",
            "    state { s: string = \"a,b\" }\n",
            "    page Home {\n",
            "        when contains(s, \"a\") {\n",
            "            text join(split(s, \",\"), \"-\")\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));
        // The param/store names are recognized in views AND handlers
        // (context-restricted invocation is enforced at S6, not here).
        clean(concat!(
            "app \"A\" {\n",
            "    route \"x/:id\" -> Home\n",
            "    state { s: string = \"\" }\n",
            "    page Home {\n",
            "        button \"B\" {\n",
            "            on click {\n",
            "                s = param(\"id\");\n",
            "                store_save(\"s\");\n",
            "                store_load(\"s\");\n",
            "                store_clear(\"s\");\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));
    }

    #[test]
    fn user_definitions_shadow_app_builtins() {
        // A user function with a builtin's name wins over the fallback
        // table (no redefinition guard exists for app builtins).
        clean(concat!(
            "func upper(s: string) -> string { return s; }\n",
            "app \"A\" {\n",
            "    state { name: string = \"kinn\" }\n",
            "    page Home { text upper(name) }\n",
            "}\n",
        ));
    }

    #[test]
    fn array_concat_types_only_inside_app_scope() {
        // In apps: same-typed array concat is legal in handlers and in
        // binding expressions (B.7's normative example) — MINI_APP's
        // `items = items + [draft];` covers the handler case.
        clean(concat!(
            "app \"A\" {\n",
            "    state { xs: [int] = [1] ys: [int] = [2] }\n",
            "    computed zs = xs + ys\n",
            "    page Home { button \"B\" { on click { xs += [3]; } } }\n",
            "}\n",
        ));
        // Different element types never concatenate.
        assert!(has(
            "E3003",
            "app \"A\" { state { xs: [int] = [1] ss: [string] = [\"x\"] } page Home { button \"B\" { on click { fin u = xs + ss; } } } }"
        ));
        // Console programs keep the 0.2 rule (E3003), so the extension
        // is honestly app-only until the runtime lands with S5.
        assert!(has(
            "E3003",
            "func main() { fin a: [int] = [1]; fin b: [int] = a + [2]; print(len(b)); }"
        ));
    }

    #[test]
    fn component_scope_shadows_the_app_layer_for_reads() {
        // A prop or component state field named like app state shadows
        // it for reads (E3042 only rejects duplicates *within* one
        // scope — the slice-1 ruling).
        clean(concat!(
            "app \"A\" {\n",
            "    state { label: string = \"app\" n: int = 1 }\n",
            "    component Row {\n",
            "        prop label: string\n",
            "        view { text label }\n",
            "    }\n",
            "    page Home { text label Row(label: \"row\") }\n",
            "}\n",
        ));
    }

    // -- S3 slice 4: instantiation sites and the handler subset --------

    /// The task's normative example: valid instantiation, default
    /// omission, multiple instantiations, nested component usage and
    /// distinct handlers on one element — zero diagnostics.
    const SLICE4_APP: &str = concat!(
        "*mode = B;\n",
        "app \"Tasks\" {\n",
        "    component TaskRow {\n",
        "        prop title: string\n",
        "        prop completed: bool = false\n",
        "        view {\n",
        "            text title\n",
        "            button \"Toggle\" {\n",
        "                on click { print(\"t\"); }\n",
        "            }\n",
        "        }\n",
        "    }\n",
        "    component Card {\n",
        "        prop heading: string\n",
        "        view {\n",
        "            TaskRow(title: heading, completed: true)\n",
        "        }\n",
        "    }\n",
        "    state { n: int = 0 }\n",
        "    page Home {\n",
        "        TaskRow(title: \"Study\")\n",
        "        TaskRow(title: \"Again\", completed: true)\n",
        "        Card(heading: \"Group\")\n",
        "        button \"Save\" {\n",
        "            on click { n += 1; }\n",
        "            on focus { n = 0; }\n",
        "        }\n",
        "    }\n",
        "}\n",
    );

    #[test]
    fn slice4_feature_surface_checks_clean() {
        // Positive master: every slice-4 feature checks with zero
        // diagnostics — warnings included (no `while` here).
        let diags = check_src(SLICE4_APP);
        assert!(diags.is_empty(), "expected a clean app, got: {:#?}", diags);
    }

    #[test]
    fn unknown_components_report_e3040() {
        let src = concat!(
            "app \"A\" {\n",
            "    component Row {\n",
            "        prop label: string\n",
            "        view { text label }\n",
            "    }\n",
            "    page Home {\n",
            "        Card(label: \"x\")\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3040").expect("E3040");
        assert_eq!((d.line, d.col, d.len), (7, 9, 4), "anchored on the component name");
        assert!(d.message.contains("unknown component `Card`"), "{}", d.message);
        assert!(
            d.hint.as_deref().unwrap().contains("declared components: Row"),
            "{:?}",
            d.hint
        );
        // A name that matches a declared page gets the namespace note
        // (the mirror of E3063's component note).
        let diags = check_src(
            "app \"A\" { page Home { text \"x\" } component Row { view { text \"r\" } } page Other { Home() } }",
        );
        let d = diags.iter().find(|d| d.code == "E3040").expect("E3040 for the page name");
        assert!(
            d.hint.as_deref().unwrap().contains("'Home' is a page; views instantiate components"),
            "{:?}",
            d.hint
        );
        // No components at all.
        let diags = check_src("app \"A\" { page Home { Ghost() } }");
        let d = diags.iter().find(|d| d.code == "E3040").expect("E3040 for Ghost");
        assert!(
            d.hint.as_deref().unwrap().contains("this app declares no components"),
            "{:?}",
            d.hint
        );
        // Qualified instantiation: no module can declare components in
        // 0.4.0, so the honest hint says the import path is not
        // implemented — instead of pretending the name could resolve.
        let diags = check_src("app \"A\" { page Home { ui.Card(title: \"Hi\") } }");
        let d = diags.iter().find(|d| d.code == "E3040").expect("E3040 for ui.Card");
        assert!(d.message.contains("unknown component `ui.Card`"), "{}", d.message);
        assert_eq!(d.len, 7, "the anchor spans `ui.Card`");
        assert!(
            d.hint
                .as_deref()
                .unwrap()
                .contains("importing components from module files is not implemented in 0.4.0"),
            "{:?}",
            d.hint
        );
        // Argument expressions still resolve when the component is
        // unknown (slice-3 behavior preserved): `nope` reports E3066.
        let diags = check_src("app \"A\" { page Home { Ghost(x: nope) } }");
        assert!(diags.iter().any(|d| d.code == "E3040"));
        assert!(diags.iter().any(|d| d.code == "E3066"), "{:#?}", diags);
    }

    #[test]
    fn unknown_component_props_report_e3041() {
        let src = concat!(
            "app \"A\" {\n",
            "    component TaskRow {\n",
            "        prop title: string\n",
            "        prop completed: bool = false\n",
            "        view { text title }\n",
            "    }\n",
            "    page Home {\n",
            "        TaskRow(title: \"Study\", name: \"X\")\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags
            .iter()
            .find(|d| d.code == "E3041" && d.message.contains("no prop"))
            .expect("E3041 for the unknown prop");
        assert_eq!((d.line, d.col, d.len), (8, 33, 4), "anchored on the prop name");
        assert!(
            d.message.contains("component `TaskRow` has no prop `name`"),
            "{}",
            d.message
        );
        let hint = d.hint.as_deref().unwrap();
        assert!(hint.contains("title (required)"), "{:?}", hint);
        assert!(hint.contains("completed (optional)"), "{:?}", hint);
        // A component without props teaches the empty instantiation.
        let diags = check_src(
            "app \"A\" { component Row { view { text \"x\" } } page Home { Row(a: 1) } }",
        );
        let d = diags
            .iter()
            .find(|d| d.code == "E3041" && d.message.contains("no prop"))
            .expect("E3041 for the prop of a propless component");
        assert!(
            d.hint.as_deref().unwrap().contains("declares no props"),
            "{:?}",
            d.hint
        );
        // Omitting the unknown prop but keeping the required one was the
        // task's second negative example — it reports the unknown prop
        // (and the still-missing nothing else).
        assert!(has(
            "E3041",
            "app \"A\" { component TaskRow { prop title: string view { text title } } page Home { TaskRow(title: \"Study\", name: \"Study\") } }"
        ));
    }

    #[test]
    fn missing_required_props_report_e3041() {
        let src = concat!(
            "app \"A\" {\n",
            "    component TaskRow {\n",
            "        prop title: string\n",
            "        prop completed: bool = false\n",
            "        view { text title }\n",
            "    }\n",
            "    page Home {\n",
            "        TaskRow()\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags
            .iter()
            .find(|d| d.code == "E3041" && d.message.contains("missing the required prop"))
            .expect("E3041 for the missing prop");
        assert_eq!((d.line, d.col, d.len), (8, 9, 7), "anchored on the instantiation name");
        assert!(
            d.message.contains("component `TaskRow` is missing the required prop `title`"),
            "{}",
            d.message
        );
        let hint = d.hint.as_deref().unwrap();
        assert!(hint.contains("pass `title: <value>`"), "{:?}", hint);
        assert!(hint.contains("`prop title: string = …`"), "{:?}", hint);
        // Passing only the optional prop leaves the required one missing.
        assert!(has(
            "E3041",
            "app \"A\" { component TaskRow { prop title: string prop completed: bool = false view { text title } } page Home { TaskRow(completed: true) } }"
        ));
        // Two required props missing → two diagnostics, declaration order.
        let diags = check_src(concat!(
            "app \"A\" {\n",
            "    component Pair {\n",
            "        prop a: int\n",
            "        prop b: string\n",
            "        view { text b }\n",
            "    }\n",
            "    page Home { Pair() }\n",
            "}\n",
        ));
        let missing: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E3041" && d.message.contains("missing the required prop"))
            .collect();
        assert_eq!(missing.len(), 2, "{:#?}", diags);
        assert!(missing[0].message.contains("`a`"), "{}", missing[0].message);
        assert!(missing[1].message.contains("`b`"), "{}", missing[1].message);
        // Negative: omitting a prop WITH a default is legal — the
        // master SLICE4_APP covers it (`TaskRow(title: \"Study\")`).
    }

    #[test]
    fn prop_type_mismatches_report_e3041() {
        let src = concat!(
            "app \"A\" {\n",
            "    component TaskRow {\n",
            "        prop title: string\n",
            "        prop completed: bool = false\n",
            "        view { text title }\n",
            "    }\n",
            "    page Home {\n",
            "        TaskRow(title: 123)\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags
            .iter()
            .find(|d| d.code == "E3041" && d.message.contains("expects"))
            .expect("E3041 for the type mismatch");
        assert_eq!((d.line, d.col, d.len), (8, 24, 1), "anchored on the value expression");
        assert!(
            d.message.contains("prop `title` of component `TaskRow` expects string, found int"),
            "{}",
            d.message
        );
        assert!(
            d.hint.as_deref().unwrap().contains("no implicit conversion"),
            "{:?}",
            d.hint
        );
        // int↔float keeps the no-conversion rule but reports E3041 (the
        // instantiation-site code), never the frozen E3011 wording.
        let diags = check_src(
            "app \"A\" { component F { prop n: float view { text \"x\" } } page Home { F(n: 1) } }",
        );
        assert!(diags.iter().any(|d| d.code == "E3041" && d.message.contains("expects float, found int")));
        assert!(!diags.iter().any(|d| d.code == "E3011"), "E3011 stays frozen: {:#?}", diags);
        // A void value is a real mismatch.
        assert!(has(
            "E3041",
            "app \"A\" { component T { prop s: string view { text s } } page Home { T(s: print(\"x\")) } }"
        ));
        // A poisoned argument suppresses only the E3041 comparison —
        // the unknown name still reports E3066 (no cascade).
        let diags = check_src(
            "app \"A\" { component T { prop s: string view { text s } } page Home { T(s: ghost) } }",
        );
        assert!(diags.iter().any(|d| d.code == "E3066"), "{:#?}", diags);
        assert!(!diags.iter().any(|d| d.code == "E3041"), "{:#?}", diags);
    }

    #[test]
    fn duplicate_prop_args_report_e3041() {
        let src = concat!(
            "app \"A\" {\n",
            "    component Row {\n",
            "        prop label: string\n",
            "        view { text label }\n",
            "    }\n",
            "    page Home {\n",
            "        Row(label: \"a\", label: \"b\")\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags
            .iter()
            .find(|d| d.code == "E3041" && d.message.contains("passed twice"))
            .expect("E3041 for the duplicate prop");
        assert_eq!((d.line, d.col, d.len), (7, 25, 5), "anchored on the second occurrence");
        assert!(
            d.message.contains("prop `label` is passed twice in this `Row` instantiation"),
            "{}",
            d.message
        );
    }

    #[test]
    fn instantiations_validate_in_every_view_position() {
        // Pages, component views, `when`/`otherwise`, `each` and `form`
        // bodies all validate: one bad instantiation inside an `each`
        // reports E3041 once, nested component views report too, and a
        // good instantiation beside a bad one reports nothing extra.
        let src = concat!(
            "app \"A\" {\n",
            "    state { items: [string] = [\"a\"] }\n",
            "    component Row {\n",
            "        prop label: string\n",
            "        view { text label }\n",
            "    }\n",
            "    component Group {\n",
            "        view {\n",
            "            Row(label: 1)\n",
            "        }\n",
            "    }\n",
            "    page Home {\n",
            "        each items as item {\n",
            "            Row(label: item)\n",
            "            Row(label: 2)\n",
            "        }\n",
            "        when len(items) == 0 {\n",
            "            text \"empty\"\n",
            "        } otherwise {\n",
            "            Row(label: 3)\n",
            "        }\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let mismatches: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E3041" && d.message.contains("expects string"))
            .collect();
        assert_eq!(mismatches.len(), 3, "component view, each body, otherwise body: {:#?}", diags);
        // Raw diagnostics arrive in walk order (pages before components);
        // `render` sorts by position for display, so assert the set.
        let mut lines: Vec<u32> = mismatches.iter().map(|d| d.line).collect();
        lines.sort();
        assert_eq!(lines, [9, 15, 20], "all three positions: {:#?}", mismatches);
    }

    #[test]
    fn instantiation_uses_the_first_declaration_after_e3042_or_e3065() {
        // E3042 already rejected the duplicate prop; the signature keeps
        // the first declaration, so a matching instantiation must not
        // cascade into E3041 ("invalid declarations are not further
        // checked" — the established pattern).
        let diags = check_src(concat!(
            "app \"A\" {\n",
            "    component Row {\n",
            "        prop label: string\n",
            "        prop label: int\n",
            "        view { text label }\n",
            "    }\n",
            "    page Home { Row(label: \"x\") }\n",
            "}\n",
        ));
        assert!(diags.iter().any(|d| d.code == "E3042"), "{:#?}", diags);
        assert!(!diags.iter().any(|d| d.code == "E3041"), "{:#?}", diags);
        // E3065-duplicate components: the first declaration resolves, so
        // the instantiation reports no E3040.
        let diags = check_src(concat!(
            "app \"A\" {\n",
            "    component Card { view { text \"first\" } }\n",
            "    component Card { view { text \"second\" } }\n",
            "    page Home { Card() }\n",
            "}\n",
        ));
        assert!(diags.iter().any(|d| d.code == "E3065"), "{:#?}", diags);
        assert!(!diags.iter().any(|d| d.code == "E3040"), "{:#?}", diags);
    }

    #[test]
    fn return_inside_handlers_is_e3045() {
        let src = concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"Save\" {\n",
            "            on click {\n",
            "                return;\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3045").expect("E3045");
        assert_eq!((d.line, d.col, d.len), (5, 17, 6), "anchored on the `return` keyword");
        assert!(
            d.message.contains("`return` is not allowed inside an event handler"),
            "{}",
            d.message
        );
        assert_eq!(d.hint.as_deref(), Some("remove the `return` statement"));
        // `return expr;` is E3045, and the ordinary E3006 does not
        // double-report the same statement.
        let diags = check_src(
            "app \"A\" { page Home { button \"B\" { on click { return 1; } } } }",
        );
        assert!(diags.iter().any(|d| d.code == "E3045"), "{:#?}", diags);
        assert!(!diags.iter().any(|d| d.code == "E3006"), "{:#?}", diags);
        // Nested control flow still counts — the handler's statements
        // are on the stack under `if` and `while` alike.
        assert!(has(
            "E3045",
            "app \"A\" { page Home { button \"B\" { on click { if true { return; } } } } }"
        ));
        let diags = check_src(
            "app \"A\" { page Home { button \"B\" { on click { while true { return; } } } } }",
        );
        assert!(diags.iter().any(|d| d.code == "E3045"), "{:#?}", diags);
        assert!(diags.iter().any(|d| d.code == "W3040"), "{:#?}", diags);
        // The value expression of a rejected `return` still resolves:
        // `ghost` reports E3066 next to the E3045.
        let diags = check_src(
            "app \"A\" { page Home { button \"B\" { on click { return ghost; } } } }",
        );
        assert!(diags.iter().any(|d| d.code == "E3045"), "{:#?}", diags);
        assert!(diags.iter().any(|d| d.code == "E3066"), "{:#?}", diags);
        // Negative: `return` in ordinary functions is untouched —
        // including a function called from a handler.
        clean(concat!(
            "func f() { return; }\n",
            "func g() -> int { while true { return 1; } }\n",
            "app \"A\" {\n",
            "    page Home { button \"B\" { on click { f(); print(g()); } } }\n",
            "}\n",
        ));
    }

    #[test]
    fn duplicate_handlers_are_e3046() {
        let src = concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"Save\" {\n",
            "            on click { print(\"First\"); }\n",
            "            on click { print(\"Second\"); }\n",
            "        }\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        let d = diags.iter().find(|d| d.code == "E3046").expect("E3046");
        assert_eq!((d.line, d.col, d.len), (5, 13, 2), "anchored on the duplicate's `on` keyword");
        assert!(
            d.message.contains("duplicate `on click` handler on this element"),
            "{}",
            d.message
        );
        assert!(
            d.hint.as_deref().unwrap().contains("merge the two `on click` bodies"),
            "{:?}",
            d.hint
        );
        // Slice-4 ruling (B.6: "one handler per event name"): the event
        // NAME is the word after `on`. Two `on key` handlers collide
        // even with different filters — the filter is runtime data — and
        // the hint teaches the `event_key` merge; `on key` and `on
        // keydown` are different names and coexist.
        let diags = check_src(concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"K\" {\n",
            "            on key \"A\" { print(\"a\"); }\n",
            "            on key \"B\" { print(\"b\"); }\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));
        let d = diags.iter().find(|d| d.code == "E3046").expect("E3046 for the two `on key`");
        assert!(
            d.hint.as_deref().unwrap().contains("checks `event_key`"),
            "{:?}",
            d.hint
        );
        clean(concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"K\" {\n",
            "            on key \"Escape\" { print(\"esc\"); }\n",
            "            on keydown { print(\"down\"); }\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));
        // Distinct events on one element are fine (the master app pairs
        // click + focus); three same-name handlers report two E3046s.
        let diags = check_src(concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"B\" {\n",
            "            on click { print(1); }\n",
            "            on click { print(2); }\n",
            "            on click { print(3); }\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));
        assert_eq!(diags.iter().filter(|d| d.code == "E3046").count(), 2, "{:#?}", diags);
        // Handler blocks on component instantiations are elements too.
        assert!(has(
            "E3046",
            "app \"A\" { component Row { view { text \"x\" } } page Home { Row() { on click { print(1); } on click { print(2); } } } }"
        ));
        // The duplicate's body is still checked — `ghost;` inside it
        // reports E3066 next to the E3046.
        let diags = check_src(concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"B\" {\n",
            "            on click { print(1); }\n",
            "            on click { ghost; }\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));
        assert!(diags.iter().any(|d| d.code == "E3046"), "{:#?}", diags);
        assert!(diags.iter().any(|d| d.code == "E3066"), "{:#?}", diags);
    }

    #[test]
    fn while_inside_handlers_warns_w3040() {
        let src = concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"Process\" {\n",
            "            on click {\n",
            "                while true {\n",
            "                    print(\"Processing\");\n",
            "                }\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        );
        let diags = check_src(src);
        // A warning, never a fatal error: exactly one diagnostic and it
        // is the W3040 warning — the app still checks.
        assert_eq!(diags.len(), 1, "{:#?}", diags);
        let d = &diags[0];
        assert_eq!(d.code, "W3040");
        assert!(
            d.severity == crate::diagnostics::Severity::Warning,
            "W3040 must be a warning, not an error"
        );
        assert_eq!((d.line, d.col, d.len), (5, 17, 5), "anchored on the `while` keyword");
        assert!(
            d.message.contains(
                "a long `while` loop in an event handler freezes the page; prefer `for` over a range"
            ),
            "{}",
            d.message
        );
        // Nested control flow inside the handler warns too.
        assert!(has(
            "W3040",
            "app \"A\" { page Home { button \"B\" { on click { if true { while false { print(1); } } } } } }"
        ));
        // Every `while` statement warns once — two loops, two warnings.
        let diags = check_src(concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"B\" {\n",
            "            on click {\n",
            "                while false { print(1); }\n",
            "                while false { print(2); }\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));
        assert_eq!(diags.iter().filter(|d| d.code == "W3040").count(), 2, "{:#?}", diags);
        // `for` never warns — it is the suggested alternative.
        clean(concat!(
            "app \"A\" {\n",
            "    page Home {\n",
            "        button \"B\" { on click { for i in 0..3 { print(i); } } }\n",
            "    }\n",
            "}\n",
        ));
        // `while` outside handlers stays silent: ordinary functions —
        // including one called from a handler — never see W3040.
        let diags = check_src(concat!(
            "func spin() { while true { break; } }\n",
            "app \"A\" {\n",
            "    page Home { button \"B\" { on click { spin(); } } }\n",
            "}\n",
        ));
        assert!(!diags.iter().any(|d| d.code == "W3040"), "{:#?}", diags);
    }
}
