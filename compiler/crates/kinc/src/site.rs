//! The site model: the target-independent representation produced by
//! the compiler front end for `website` declarations.
//!
//! Every build backend (web, android, …) consumes a `SiteModel`, never
//! the AST — so all targets see identical, already-validated semantics
//! (ADR-0018). The model contains no positions: diagnostics were fully
//! reported by the checker before lowering runs.

/// A complete declarative site.
#[derive(Debug, Clone)]
pub struct SiteModel {
    /// Non-empty site name (checker rule E3033).
    pub name: String,
    /// Pages in declaration order; the first page is the home page.
    pub pages: Vec<PageModel>,
}

/// One page of a site.
#[derive(Debug, Clone)]
pub struct PageModel {
    /// Page identifier (unique, case-insensitive — checker rule E3031).
    /// Doubles as the navigation label and the URL slug source.
    pub name: String,
    /// Page title from the first `title` element (empty when the page
    /// has none; consumers fall back to the page name).
    pub title: String,
    pub elements: Vec<Element>,
}

/// One content element of a page.
///
/// This is the complete 0.3.0 element set; the grammar does not accept
/// anything else, so a backend can rely on total coverage (ADR-0020).
#[derive(Debug, Clone)]
pub enum Element {
    /// Section heading (`heading "..."`).
    Heading(String),
    /// Paragraph (`text "..."`).
    Text(String),
    /// Prominent navigation (`button "Label" open Page`).
    Button { label: String, target: String },
    /// Inline navigation (`link "Label" open Page`).
    Link { label: String, target: String },
    /// Image with mandatory alt text (`image "src" alt "..."`).
    Image { src: String, alt: String },
    /// Thematic break (`divider`).
    Divider,
}

impl Element {
    /// Stable machine name (tests and debug output).
    pub fn kind_name(&self) -> &'static str {
        match self {
            Element::Heading(_) => "heading",
            Element::Text(_) => "text",
            Element::Button { .. } => "button",
            Element::Link { .. } => "link",
            Element::Image { .. } => "image",
            Element::Divider => "divider",
        }
    }
}
