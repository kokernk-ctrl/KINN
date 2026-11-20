//! Diagnostics: structured errors, warnings and notes, plus rendering.

use crate::ast::Mode;

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
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) -> Self {
        Diagnostic { severity: Severity::Error, code, message: message.into(), line, col, len, hint: None }
    }

    pub fn warning(code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) -> Self {
        Diagnostic { severity: Severity::Warning, code, message: message.into(), line, col, len, hint: None }
    }

    pub fn note(code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) -> Self {
        Diagnostic { severity: Severity::Note, code, message: message.into(), line, col, len, hint: None }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.is_error())
}

/// Render diagnostics for humans, in the style of:
///
/// ```text
/// error[E3004]: cannot reassign immutable variable `x` (declared with `let`)
///   --> file.kin:7:5
///    |
///  7 |     x = 2;
///    |     ^
/// note[N3002]: `x` declared here
///   --> file.kin:6:9
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_error_with_caret_and_location() {
        let src = "func main() {\n    let x = 1;\n}\n";
        let d = Diagnostic::error("E3004", "cannot reassign `x`", 2, 5, 1);
        let out = render(&[d], src, "t.kin", Mode::S);
        assert!(out.contains("error[E3004]: cannot reassign `x`"));
        assert!(out.contains("--> t.kin:2:5"));
        assert!(out.contains(" |     let x = 1;"));
        assert!(out.contains("^"));
    }

    #[test]
    fn advanced_mode_hides_notes() {
        let src = "func main() {\n    let x = 1;\n}\n";
        let n = Diagnostic::note("N3002", "`x` declared here", 2, 9, 1);
        assert!(render(&[n.clone()], src, "t.kin", Mode::A).is_empty());
        assert!(!render(&[n], src, "t.kin", Mode::S).is_empty());
    }

    #[test]
    fn hints_are_rendered_in_b_and_s_modes() {
        let src = "func main() {\n    let x = 1;\n}\n";
        let d = Diagnostic::error("E3004", "boom", 2, 5, 1);
        let d = Diagnostic { hint: Some("try `var`".into()), ..d };
        let out = render(&[d], src, "t.kin", Mode::B);
        assert!(out.contains("hint: try `var`"));
    }
}
