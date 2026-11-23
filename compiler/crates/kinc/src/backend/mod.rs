//! Target backends (ADR-0018): pure generators from validated compiler
//! output to artifact files.
//!
//! A backend never invents semantics: the front end (parser + checker +
//! lowering) has already validated everything a backend receives, and a
//! backend that cannot express something rejects it instead of emitting
//! output that misrepresents the source.

pub mod android;
pub mod web;

use crate::appmodel::AppModel;
use crate::diagnostics::Diagnostic;
use crate::site::SiteModel;

/// The capability gate (C.9: "No model is ever fed to a backend that
/// did not declare support for it"). The android backend consumes a
/// `SiteModel` exactly as today — a dynamic `app` program (whose
/// `AppModel` exists after S4) must never silently pass through it as
/// if dynamic application support existed: it is rejected with
/// **E3070** before any generation runs. Static `website` programs
/// build exactly as before.
pub fn android_gate(
    site: Option<&SiteModel>,
    app: Option<&AppModel>,
) -> Result<(), Diagnostic> {
    if app.is_some() {
        let mut d = Diagnostic::error(
            "E3070",
            "the android target does not support `app` programs — dynamic applications are web-only in 0.4.0",
            1,
            1,
            0,
        );
        d.hint = Some(
            "declare a `website` for the android target; the dynamic app dist arrives on the web target with the later 0.4.0 steps"
                .to_string(),
        );
        return Err(d);
    }
    debug_assert!(
        site.is_some() || app.is_none(),
        "the gate rejects apps before generation; website programs always carry a site"
    );
    Ok(())
}

/// One generated artifact: a path relative to the build output
/// directory plus its bytes (text files are UTF-8; binaries such as jar
/// files are possible).
#[derive(Debug, Clone, PartialEq)]
pub struct OutputFile {
    pub path: String,
    pub data: Vec<u8>,
}

impl OutputFile {
    pub fn text(path: &str, content: &str) -> OutputFile {
        OutputFile { path: path.to_string(), data: content.as_bytes().to_vec() }
    }

    /// Convenience for tests and hosts: the payload as UTF-8 text.
    pub fn as_text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.data)
    }
}

/// The build targets the toolchain ships, in UI order (CLI `kinn
/// targets`, Studio target pickers).
pub const TARGETS: [(&str, &str); 2] = [
    ("web", "a static website (HTML/CSS) from a `website` declaration"),
    (
        "android",
        "a native Android Gradle project (APK via a local SDK, CI, or export)",
    ),
];
