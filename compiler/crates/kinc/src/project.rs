//! Multi-file project loading: entry file plus transitive imports.
//!
//! A project is loaded from an entry source plus a loader callback that
//! resolves a project-relative path to source text (a file read on the
//! CLI, a virtual file system in KINN Studio and the Android app). All
//! front ends therefore share identical import semantics:
//!
//! - `import "path.kin" as alias;` — paths are relative to the importing
//!   file, must end in `.kin`, and may not escape the project root
//!   (no `..`, no absolute paths).
//! - Cycles and over-deep chains are rejected (E3018).
//! - A file imported twice (diamond) is parsed once and shared.
//!
//! Modules export their top-level functions; see docs/LANGUAGE_SPEC for
//! the 0.2 module rules.

use crate::ast::{ImportDecl, Program};
use crate::diagnostics::{Diagnostic, Severity};
use crate::{lexer, parser};
use std::collections::HashMap;

/// Maximum import chain depth (entry counts as depth 0).
const MAX_IMPORT_DEPTH: usize = 32;

/// A parsed file plus its resolved imports.
#[derive(Debug, Clone)]
pub struct FileUnit {
    /// Project-relative path (the entry keeps the name given by the
    /// caller; imports are resolved against the importing file).
    pub path: String,
    pub program: Program,
    /// Resolved imports, parallel to `program.imports`.
    pub imports: Vec<ResolvedImport>,
}

/// One `import ... as alias;` resolved to a file index in the project.
#[derive(Debug, Clone)]
pub struct ResolvedImport {
    pub alias: String,
    /// Index into `Project::files`.
    pub file: usize,
    pub line: u32,
    pub col: u32,
}

/// A loaded project: entry file first, then every transitively imported
/// file (each exactly once), plus the source text of every file (for
/// rendering diagnostics).
#[derive(Debug, Clone)]
pub struct Project {
    pub files: Vec<FileUnit>,
    /// Project-relative path → source text.
    pub sources: HashMap<String, String>,
}

/// Validate and join an import path. Returns the resolved path, in the
/// same (absolute or relative) form as the importer's path.
fn resolve_path(importer_path: &str, import_path: &str) -> Result<String, String> {
    if !import_path.ends_with(".kin") {
        return Err(format!(
            "import path `{}` must end in `.kin`",
            import_path
        ));
    }
    if import_path.starts_with('/') || import_path.contains('\\') || import_path.contains("..") {
        return Err(format!(
            "import path `{}` must stay inside the project (no `..`, no absolute paths)",
            import_path
        ));
    }
    let absolute = importer_path.starts_with('/');
    let dir = match importer_path.rfind('/') {
        Some(i) => &importer_path[..i],
        None => "",
    };
    let mut parts: Vec<&str> = Vec::new();
    for comp in dir.split('/').filter(|c| !c.is_empty() && *c != ".") {
        parts.push(comp);
    }
    for comp in import_path.split('/') {
        match comp {
            "" | "." => continue,
            ".." => return Err(format!(
                "import path `{}` must stay inside the project (no `..`)",
                import_path
            )),
            _ => parts.push(comp),
        }
    }
    let joined = parts.join("/");
    Ok(if absolute { format!("/{}", joined) } else { joined })
}

fn parse_file(path: &str, src: &str, diags: &mut Vec<Diagnostic>) -> Option<Program> {
    let (tokens, lex_diags) = lexer::tokenize(src);
    for d in lex_diags {
        diags.push(d.in_file(path));
    }
    if diags.iter().any(|d| d.is_error()) {
        return None;
    }
    match parser::parse(tokens) {
        Ok(p) => Some(p),
        Err(parse_diags) => {
            for d in parse_diags {
                diags.push(d.in_file(path));
            }
            None
        }
    }
}

/// Load a project starting at `entry_path` / `entry_src`. The `loader`
/// receives project-relative paths and returns file contents.
pub fn load(
    entry_path: &str,
    entry_src: &str,
    loader: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<Project, Vec<Diagnostic>> {
    let mut st = Loader {
        files: Vec::new(),
        by_path: std::collections::HashMap::new(),
        stack: Vec::new(),
        diags: Vec::new(),
        sources: std::collections::HashMap::new(),
    };
    let ok = st.visit(entry_path, entry_src, loader);
    if ok && !st.diags.iter().any(|d| d.is_error()) {
        Ok(Project { files: st.files, sources: st.sources })
    } else if st.diags.is_empty() {
        // Defensive: a failure must always produce a diagnostic.
        st.diags.push(
            Diagnostic::error("E3017", "failed to load the project", 1, 1, 1)
                .in_file(entry_path),
        );
        Err(st.diags)
    } else {
        Err(st.diags)
    }
}

struct Loader {
    files: Vec<FileUnit>,
    by_path: std::collections::HashMap<String, usize>,
    /// DFS stack of paths, for cycle reporting.
    stack: Vec<String>,
    diags: Vec<Diagnostic>,
    sources: std::collections::HashMap<String, String>,
}

impl Loader {
    fn visit(
        &mut self,
        path: &str,
        src: &str,
        loader: &mut dyn FnMut(&str) -> Result<String, String>,
    ) -> bool {
        self.stack.push(path.to_string());
        let program = match parse_file(path, src, &mut self.diags) {
            Some(p) => p,
            None => {
                self.stack.pop();
                return false;
            }
        };

        let idx = self.files.len();
        self.sources.insert(path.to_string(), src.to_string());
        self.files.push(FileUnit {
            path: path.to_string(),
            program,
            imports: Vec::new(),
        });
        self.by_path.insert(path.to_string(), idx);

        // Import declarations for this file. Borrow dance: the decls live
        // in the program we just stored, and we mutate `imports` below.
        let decls: Vec<ImportDecl> = self.files[idx].program.imports.clone();

        let mut ok = true;
        let mut seen_aliases: Vec<String> = Vec::new();
        for decl in &decls {
            if seen_aliases.contains(&decl.alias) {
                self.diags.push(
                    Diagnostic::error(
                        "E3019",
                        format!("duplicate import alias `{}` in this file", decl.alias),
                        decl.line,
                        decl.col,
                        decl.alias.len() as u32,
                    )
                    .in_file(path),
                );
                ok = false;
                continue;
            }
            seen_aliases.push(decl.alias.clone());

            let resolved = match resolve_path(path, &decl.path) {
                Ok(r) => r,
                Err(msg) => {
                    self.diags.push(
                        Diagnostic::error("E3017", msg, decl.line, decl.col, decl.path.len() as u32)
                            .in_file(path),
                    );
                    ok = false;
                    continue;
                }
            };

            if self.stack.contains(&resolved) {
                let mut chain: Vec<String> =
                    self.stack.iter().map(|s| s.clone()).collect();
                chain.push(resolved.clone());
                self.diags.push(
                    Diagnostic::error(
                        "E3018",
                        format!("circular import: {}", chain.join(" -> ")),
                        decl.line,
                        decl.col,
                        decl.path.len() as u32,
                    )
                    .in_file(path),
                );
                ok = false;
                continue;
            }
            if self.stack.len() > MAX_IMPORT_DEPTH {
                self.diags.push(
                    Diagnostic::error(
                        "E3018",
                        format!("import chain too deep (limit is {} files)", MAX_IMPORT_DEPTH),
                        decl.line,
                        decl.col,
                        decl.path.len() as u32,
                    )
                    .in_file(path),
                );
                ok = false;
                continue;
            }

            let child_idx = match self.by_path.get(&resolved) {
                Some(i) => *i,
                None => {
                    let child_src = match loader(&resolved) {
                        Ok(s) => s,
                        Err(msg) => {
                            self.diags.push(
                                Diagnostic::error(
                                    "E3017",
                                    format!("cannot load module `{}` ({})", resolved, msg),
                                    decl.line,
                                    decl.col,
                                    decl.path.len() as u32,
                                )
                                .in_file(path),
                            );
                            ok = false;
                            continue;
                        }
                    };
                    match self.visit(&resolved, &child_src, loader) {
                        true => *self.by_path.get(&resolved).expect("just inserted"),
                        false => {
                            ok = false;
                            continue;
                        }
                    }
                }
            };

            self.files[idx].imports.push(ResolvedImport {
                alias: decl.alias.clone(),
                file: child_idx,
                line: decl.line,
                col: decl.col,
            });
        }

        self.stack.pop();
        ok
    }
}

/// The qualified KIR-level name of a function: entry functions keep their
/// name; imported functions are prefixed with their path relative to the
/// entry file's directory (without `.kin`) so identical names in
/// different files cannot collide: `math::add`, `lib/util::f`.
pub fn qualified_name(file_path: &str, entry_dir: &str, is_entry: bool, func_name: &str) -> String {
    if is_entry {
        return func_name.to_string();
    }
    let rel = if entry_dir.is_empty() {
        file_path
    } else {
        file_path
            .strip_prefix(&format!("{}/", entry_dir))
            .unwrap_or(file_path)
    };
    let stem = rel.strip_suffix(".kin").unwrap_or(rel);
    format!("{}::{}", stem, func_name)
}

/// Directory part of a slash-separated path ("" when there is none).
pub fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_ok(
        entry: &str,
        files: &[(&str, &str)],
    ) -> Project {
        let map: std::collections::HashMap<String, String> = files
            .iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect();
        let mut loader = move |path: &str| {
            map.get(path).cloned().ok_or_else(|| "not found".to_string())
        };
        load("main.kin", entry, &mut loader).expect("project must load")
    }

    fn load_err(entry: &str, files: &[(&str, &str)]) -> Vec<Diagnostic> {
        let map: std::collections::HashMap<String, String> = files
            .iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect();
        let mut loader = move |path: &str| {
            map.get(path).cloned().ok_or_else(|| "not found".to_string())
        };
        load("main.kin", entry, &mut loader).expect_err("must fail")
    }

    #[test]
    fn loads_transitive_imports() {
        let p = load_ok(
            "import \"lib/a.kin\" as a;\nfunc main() { }",
            &[
                ("lib/a.kin", "import \"b.kin\" as b;\nfunc fa() { }"),
                ("lib/b.kin", "func fb() { }"),
            ],
        );
        assert_eq!(p.files.len(), 3);
        assert_eq!(p.files[0].path, "main.kin");
        assert_eq!(p.files[1].path, "lib/a.kin");
        assert_eq!(p.files[2].path, "lib/b.kin");
        assert_eq!(p.files[0].imports.len(), 1);
        assert_eq!(p.files[0].imports[0].alias, "a");
        assert_eq!(p.files[0].imports[0].file, 1);
        assert_eq!(p.files[1].imports[0].file, 2);
    }

    #[test]
    fn diamond_import_parses_each_file_once() {
        let p = load_ok(
            "import \"a.kin\" as a;\nimport \"b.kin\" as b;\nfunc main() { }",
            &[
                ("a.kin", "import \"shared.kin\" as s;\nfunc fa() { }"),
                ("b.kin", "import \"shared.kin\" as s2;\nfunc fb() { }"),
                ("shared.kin", "func fs() { }"),
            ],
        );
        assert_eq!(p.files.len(), 4);
        let shared_count = p.files.iter().filter(|f| f.path == "shared.kin").count();
        assert_eq!(shared_count, 1);
    }

    #[test]
    fn cycles_are_rejected_with_the_chain() {
        let diags = load_err(
            "import \"a.kin\" as a;\nfunc main() { }",
            &[
                ("a.kin", "import \"main.kin\" as m;\nfunc fa() { }"),
            ],
        );
        let d = diags.iter().find(|d| d.code == "E3018").expect("cycle diagnostic");
        assert!(d.message.contains("main.kin -> a.kin -> main.kin"), "{}", d.message);
        assert_eq!(d.file, "a.kin");
    }

    #[test]
    fn escaping_paths_are_rejected() {
        let diags = load_err(
            "import \"../secret.kin\" as s;\nfunc main() { }",
            &[],
        );
        assert!(diags.iter().any(|d| d.code == "E3017" && d.message.contains("..")));
        let diags = load_err(
            "import \"/etc/passwd.kin\" as s;\nfunc main() { }",
            &[],
        );
        assert!(diags.iter().any(|d| d.code == "E3017"));
        let diags = load_err("import \"lib.txt\" as s;\nfunc main() { }", &[]);
        assert!(diags.iter().any(|d| d.code == "E3017" && d.message.contains(".kin")));
    }

    #[test]
    fn missing_modules_report_the_loader_error() {
        let diags = load_err("import \"nope.kin\" as n;\nfunc main() { }", &[]);
        let d = diags.iter().find(|d| d.code == "E3017").unwrap();
        assert!(d.message.contains("nope.kin"));
    }

    #[test]
    fn duplicate_aliases_are_rejected() {
        let diags = load_err(
            "import \"a.kin\" as x;\nimport \"b.kin\" as x;\nfunc main() { }",
            &[("a.kin", "func f() { }"), ("b.kin", "func g() { }")],
        );
        assert!(diags.iter().any(|d| d.code == "E3019"));
    }

    #[test]
    fn same_file_two_aliases_is_allowed() {
        let p = load_ok(
            "import \"a.kin\" as x;\nimport \"a.kin\" as y;\nfunc main() { }",
            &[("a.kin", "func f() { }")],
        );
        assert_eq!(p.files.len(), 2);
        assert_eq!(p.files[0].imports.len(), 2);
        assert_eq!(p.files[0].imports[0].file, p.files[0].imports[1].file);
    }

    #[test]
    fn parse_errors_in_imports_carry_the_file() {
        let diags = load_err(
            "import \"a.kin\" as a;\nfunc main() { }",
            &[("a.kin", "func broken( { }")],
        );
        let d = diags
            .iter()
            .find(|d| d.severity == Severity::Error)
            .expect("parse error surfaces");
        assert_eq!(d.file, "a.kin");
    }

    #[test]
    fn nested_relative_paths_resolve_against_the_importer() {
        let p = load_ok(
            "import \"lib/a.kin\" as a;\nfunc main() { }",
            &[
                ("lib/a.kin", "import \"sub/b.kin\" as b;\nfunc fa() { }"),
                ("lib/sub/b.kin", "func fb() { }"),
            ],
        );
        assert_eq!(p.files[1].imports[0].file, 2);
        assert_eq!(p.files[2].path, "lib/sub/b.kin");
    }

    #[test]
    fn qualified_names() {
        assert_eq!(qualified_name("main.kin", "", true, "main"), "main");
        assert_eq!(qualified_name("math.kin", "", false, "add"), "math::add");
        assert_eq!(
            qualified_name("examples/modules/math.kin", "examples/modules", false, "add"),
            "math::add"
        );
        assert_eq!(
            qualified_name("examples/modules/lib/util.kin", "examples/modules", false, "f"),
            "lib/util::f"
        );
    }
}
