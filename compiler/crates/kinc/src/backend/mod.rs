//! Target backends (ADR-0018): pure generators from validated compiler
//! output to artifact files.
//!
//! A backend never invents semantics: the front end (parser + checker +
//! lowering) has already validated everything a backend receives, and a
//! backend that cannot express something rejects it instead of emitting
//! output that misrepresents the source.

pub mod android;
pub mod web;

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
