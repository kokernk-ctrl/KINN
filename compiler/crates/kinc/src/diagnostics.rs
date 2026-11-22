//! Diagnostics: structured errors, warnings and notes, plus rendering.

use crate::ast::Mode;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub fn label(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

/// A positioned diagnostic with a stable code.
///
/// Code ranges:
/// - `E1xxx` — lexical errors
/// - `E2xxx` — syntax errors
/// - `E3xxx` — semantic / type errors
/// - `E4xxx` — runtime errors (produced by `kinrt`)
/// - `W3xxx` — warnings, `N3xxx` — notes
///
/// `file` is the project-relative path of the file the diagnostic came
/// from (empty for single-file compilation, where the caller supplies the
/// display name to [`render`]).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub hint: Option<String>,
    pub file: String,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) -> Self {
        Diagnostic {
            severity: Severity::Error,
            code,
            message: message.into(),
            line,
            col,
            len,
            hint: None,
            file: String::new(),
        }
    }

    pub fn warning(code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            code,
            message: message.into(),
            line,
            col,
            len,
            hint: None,
            file: String::new(),
        }
    }

    pub fn note(code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) -> Self {
        Diagnostic {
            severity: Severity::Note,
            code,
            message: message.into(),
            line,
            col,
            len,
            hint: None,
            file: String::new(),
        }
    }

    /// Attach the project-relative file this diagnostic belongs to.
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = file.into();
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.is_error())
}

/// Render diagnostics for a single-file compilation. The `filename`
/// argument supplies the display name for every diagnostic.
///
/// Mode `A` (Advanced) suppresses `note`-severity entries for terse
/// output; modes `B` and `S` show everything.
pub fn render(diags: &[Diagnostic], src: &str, filename: &str, mode: Mode) -> String {
    let mut sorted: Vec<&Diagnostic> = diags.iter().collect();
    sorted.sort_by_key(|d| (d.line, d.col));

    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::new();
    for d in sorted {
        if mode == Mode::A && d.severity == Severity::Note {
            continue;
        }
        out.push_str(&format!("{}[{}]: {}\n", d.severity.label(), d.code, d.message));
        out.push_str(&format!("  --> {}:{}:{}\n", filename, d.line, d.col));
        let li = d.line as usize - 1;
        if li < lines.len() {
            let text = lines[li];
            let num = d.line.to_string();
            let pad = " ".repeat(num.len() + 1);
            out.push_str(&format!("{} |\n", pad));
            out.push_str(&format!("{} | {}\n", num, text));
            let caret_pad = " ".repeat((d.col as usize).saturating_sub(1));
            out.push_str(&format!(
                "{} | {}{}\n",
                pad,
                caret_pad,
                "^".repeat(d.len.max(1) as usize)
            ));
        }
        if let Some(h) = &d.hint {
            out.push_str(&format!("hint: {}\n", h));
        }
    }
    out
}

/// Render diagnostics for a multi-file project. Each diagnostic carries
/// the path of its file in `d.file`; `sources` maps those paths to the
/// source text (a missing entry renders without a snippet).
pub fn render_project(
    diags: &[Diagnostic],
    sources: &HashMap<String, String>,
    mode: Mode,
) -> String {
    let mut sorted: Vec<&Diagnostic> = diags.iter().collect();
    sorted.sort_by(|a, b| (&a.file, a.line, a.col).cmp(&(&b.file, b.line, b.col)));

    let mut out = String::new();
    for d in sorted {
        if mode == Mode::A && d.severity == Severity::Note {
            continue;
        }
        out.push_str(&format!("{}[{}]: {}\n", d.severity.label(), d.code, d.message));
        out.push_str(&format!("  --> {}:{}:{}\n", d.file, d.line, d.col));
        if let Some(src) = sources.get(&d.file) {
            let lines: Vec<&str> = src.lines().collect();
            let li = d.line as usize - 1;
            if li < lines.len() {
                let text = lines[li];
                let num = d.line.to_string();
                let pad = " ".repeat(num.len() + 1);
                out.push_str(&format!("{} |\n", pad));
                out.push_str(&format!("{} | {}\n", num, text));
                let caret_pad = " ".repeat((d.col as usize).saturating_sub(1));
                out.push_str(&format!(
                    "{} | {}{}\n",
                    pad,
                    caret_pad,
                    "^".repeat(d.len.max(1) as usize)
                ));
            }
        }
        if let Some(h) = &d.hint {
            out.push_str(&format!("hint: {}\n", h));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_error_with_caret_and_location() {
        let src = "func main() {\n    fin x = 1;\n}\n";
        let d = Diagnostic::error("E3004", "cannot reassign `x`", 2, 5, 1);
        let out = render(&[d], src, "t.kin", Mode::S);
        assert!(out.contains("error[E3004]: cannot reassign `x`"));
        assert!(out.contains("--> t.kin:2:5"));
        assert!(out.contains(" |     fin x = 1;"));
        assert!(out.contains("^"));
    }

    #[test]
    fn advanced_mode_hides_notes() {
        let src = "func main() {\n    fin x = 1;\n}\n";
        let n = Diagnostic::note("N3002", "`x` declared here", 2, 9, 1);
        assert!(render(&[n.clone()], src, "t.kin", Mode::A).is_empty());
        assert!(!render(&[n], src, "t.kin", Mode::S).is_empty());
    }

    #[test]
    fn hints_are_rendered_in_b_and_s_modes() {
        let src = "func main() {\n    fin x = 1;\n}\n";
        let d = Diagnostic::error("E3004", "boom", 2, 5, 1);
        let d = Diagnostic { hint: Some("try `var`".into()), ..d };
        let out = render(&[d], src, "t.kin", Mode::B);
        assert!(out.contains("hint: try `var`"));
    }

    #[test]
    fn project_rendering_groups_by_file() {
        let mut sources = HashMap::new();
        sources.insert("main.kin".to_string(), "func main() { }\n".to_string());
        sources.insert("lib.kin".to_string(), "func f() { fin x = ; }\n".to_string());

        let d1 = Diagnostic::error("E3001", "in main", 1, 1, 1).in_file("main.kin");
        let d2 = Diagnostic::error("E3001", "in lib", 1, 13, 1).in_file("lib.kin");
        let out = render_project(&[d2, d1], &sources, Mode::S);
        assert!(out.contains("--> main.kin:1:1"));
        assert!(out.contains("--> lib.kin:1:13"));
        // lib renders its own source line
        assert!(out.contains("func f() { fin x = ; }"));
        // files sort alphabetically: lib.kin before main.kin
        let lib_pos = out.find("--> lib.kin").unwrap();
        let main_pos = out.find("--> main.kin").unwrap();
        assert!(lib_pos < main_pos);
    }
}
