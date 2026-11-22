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

#[derive(Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
    line: u32,
    col: u32,
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
                    self.error(
                        "E3004",
                        format!("cannot reassign immutable variable `{}` (declared with `fin`)", name),
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
}
