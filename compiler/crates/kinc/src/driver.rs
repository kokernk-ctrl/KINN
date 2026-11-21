//! The compile pipeline: source → tokens → AST → project → checked → KIR.
//!
//! Single files compile with [`compile_full`]; projects with imports use
//! [`compile_project`] with a loader callback that resolves module paths
//! to source text (a filesystem read in the CLI, a virtual filesystem in
//! KINN Studio and the Android app — import semantics stay identical).

use crate::ast::Mode;
use crate::checker;
use crate::diagnostics::{has_errors, Diagnostic};
use crate::lower;
use crate::project;
use crate::site::SiteModel;
use kinrt::Module;
use std::collections::HashMap;

/// A successfully compiled program: the linked KIR module, the entry
/// mode, the source of every file that took part (for rendering
/// diagnostics later), the site model when the program declares a
/// `website`, and the raw `app` declaration when it declares an `app`
/// (consumed by the build backends; `None` for console programs).
#[derive(Debug)]
pub struct Compiled {
    pub module: Module,
    pub mode: Mode,
    /// Project-relative path → source text.
    pub sources: HashMap<String, String>,
    /// Lowered `website` declaration, when the entry file has one.
    pub site: Option<SiteModel>,
    /// The parsed `app` declaration, when the entry file has one
    /// (0.4.0 S2: grammar only — checking is partial, lowering to an
    /// `AppModel` arrives with S4, so this is the raw AST for now).
    pub app: Option<crate::ast::AppDecl>,
}

/// Compile a project starting at `entry_path` / `entry_src`. The loader
/// resolves project-relative module paths (see [`project::load`]).
pub fn compile_project(
    entry_path: &str,
    entry_src: &str,
    loader: &mut dyn FnMut(&str) -> Result<String, String>,
) -> (Option<Compiled>, Vec<Diagnostic>) {
    let proj = match project::load(entry_path, entry_src, loader) {
        Ok(p) => p,
        Err(diags) => return (None, diags),
    };

    let diags = checker::check_project(&proj);
    if has_errors(&diags) {
        return (None, diags);
    }
    let mode = proj.files[0].program.mode;
    let module = lower::lower_project(&proj);
    let site = proj.files[0]
        .program
        .websites
        .first()
        .map(lower::site_model);
    let app = proj.files[0].program.apps.first().cloned();
    (Some(Compiled { module, mode, sources: proj.sources, site, app }), diags)
}

/// Compile a single file (imports cannot be resolved and are reported as
/// module-loading errors).
pub fn compile_full(src: &str) -> (Option<Compiled>, Vec<Diagnostic>) {
    compile_project("<main>", src, &mut |path| {
        Err(format!(
            "module loading is not available in single-file mode (`{}`)",
            path
        ))
    })
}

/// Convenience wrapper that discards warnings on success.
pub fn compile(src: &str) -> Result<Compiled, Vec<Diagnostic>> {
    let (compiled, diags) = compile_full(src);
    match compiled {
        Some(c) => Ok(c),
        None => Err(diags),
    }
}
