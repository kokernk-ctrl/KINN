//! Minimal `kinn.toml` manifest reader (ADR-0022) — the first step of
//! the package foundation.
//!
//! Supported subset (deliberately small; the full package spec lives in
//! docs/MODULE_SYSTEM.md):
//!
//! ```toml
//! # KINN project manifest
//! name = "my-site"
//! version = "0.1.0"
//! entry = "site.kin"
//! target = "web"        # default build target for `kinn build`
//! ```
//!
//! Rules: `key = "value"` string pairs at the top level (before any
//! `[section]`, which ends parsing of top-level keys for forward
//! compatibility), `#` comments, UTF-8. Unknown keys are ignored so
//! future manifests stay readable by older CLIs.

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub version: Option<String>,
    pub entry: Option<String>,
    pub target: Option<String>,
}

/// Read and parse a manifest file.
pub fn load(path: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read `{}`: {}", path.display(), e))?;
    parse(&text)
}

/// Parse manifest text (unit-tested; `load` is a thin file wrapper).
pub fn parse(text: &str) -> Result<Manifest, String> {
    let mut m = Manifest { name: String::new(), version: None, entry: None, target: None };
    let mut top_level = true;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            // First section: everything below is table data this
            // minimal reader does not interpret yet.
            top_level = false;
            continue;
        }
        if !top_level {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some(kv) => kv,
            None => {
                return Err(format!("{}: line {}: expected `key = \"value\"`, found: {}", "<kinn.toml>", n + 1, line));
            }
        };
        let key = key.trim();
        let value = parse_string(value)
            .map_err(|e| format!("<kinn.toml>: line {}: {}", n + 1, e))?;
        match key {
            "name" => m.name = value,
            "version" => m.version = Some(value),
            "entry" => m.entry = Some(value),
            "target" => m.target = Some(value),
            _ => {} // forward compatibility
        }
    }
    if m.name.is_empty() {
        return Err("<kinn.toml>: missing `name`".to_string());
    }
    Ok(m)
}

/// Values must be double-quoted strings (predictable subset; no escape
/// processing beyond the common `\"` and `\\`).
fn parse_string(v: &str) -> Result<String, String> {
    let v = v.trim();
    if !v.starts_with('"') {
        return Err(format!("expected a quoted string, found: {}", v));
    }
    let inner = &v[1..];
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(out),
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            c => out.push(c),
        }
    }
    Err("unterminated string (missing closing `\"`)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_manifest() {
        let m = parse("# comment\nname = \"my-site\"\nversion = \"0.1.0\"\nentry = \"site.kin\"\ntarget = \"web\"\n").unwrap();
        assert_eq!(m.name, "my-site");
        assert_eq!(m.version.as_deref(), Some("0.1.0"));
        assert_eq!(m.entry.as_deref(), Some("site.kin"));
        assert_eq!(m.target.as_deref(), Some("web"));
    }

    #[test]
    fn sections_and_unknown_keys_are_ignored() {
        let m = parse("name = \"s\"\nunknown = \"x\"\n[target.web]\nout = \"dist\"\n").unwrap();
        assert_eq!(m.name, "s");
        assert_eq!(m.entry, None);
    }

    #[test]
    fn escapes_are_supported() {
        let m = parse("name = \"a \\\"quoted\\\" site\"").unwrap();
        assert_eq!(m.name, "a \"quoted\" site");
    }

    #[test]
    fn unquoted_values_are_rejected_with_a_line_number() {
        let e = parse("name = s\n").unwrap_err();
        assert!(e.contains("line 1"), "{}", e);
        assert!(e.contains("quoted"));
    }

    #[test]
    fn broken_lines_and_missing_name_are_errors() {
        assert!(parse("just a line").is_err());
        assert!(parse("version = \"1\"").unwrap_err().contains("`name`"));
        assert!(parse("name = \"unterminated").is_err());
    }
}
