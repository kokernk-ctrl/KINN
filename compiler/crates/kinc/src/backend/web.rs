//! The web backend: `SiteModel` → real distribution directory
//! (ADR-0020).
//!
//! Output (all at the root, so the site works from any static host or
//! `file://`):
//!
//! - `index.html`  — the first declared page (home)
//! - `<slug>.html` — one semantic HTML5 page per additional page
//! - `styles.css`  — one stylesheet for the whole site
//! - `app.js`      — focus management after navigation only
//!
//! Mapping table (complete for the 0.3.0 element set — the grammar
//! cannot express anything else):
//!
//! | Kin                    | HTML                                    |
//! | ---------------------- | --------------------------------------- |
//! | `title "..."`          | `<title>` + `<h1 class="kinn-title">`   |
//! | `heading "..."`        | `<h2>`                                  |
//! | `text "..."`           | `<p>`                                   |
//! | `button "L" open P`    | `<a class="kinn-button" href=…>`        |
//! | `link "L" open P`      | `<a class="kinn-link" href=…>`          |
//! | `image "src" alt "..."`| `<img src=… alt=… loading="lazy">`      |
//! | `divider`              | `<hr>`                                  |
//!
//! Accessibility is generated in: landmarks, skip link,
//! `aria-current="page"`, alt text (enforced by the checker), heading
//! order (one h1 per page, h2 sections), visible focus. Buttons that
//! navigate render as `<a>` styled as buttons — real links work without
//! JavaScript.

use crate::site::{Element, PageModel, SiteModel};
use std::collections::HashMap;

/// Generate the complete web distribution for a validated site.
pub fn generate(site: &SiteModel) -> Vec<crate::backend::OutputFile> {
    assert!(
        !site.pages.is_empty(),
        "checker guarantees at least one page"
    );

    // Page name -> href, resolved once (the checker guarantees every
    // navigation target resolves, E3032).
    let mut hrefs: HashMap<&str, String> = HashMap::new();
    for (i, page) in site.pages.iter().enumerate() {
        let href = if i == 0 { "index.html".to_string() } else { format!("{}.html", slug(&page.name)) };
        hrefs.insert(page.name.as_str(), href);
    }

    let mut files = Vec::new();
    for (i, page) in site.pages.iter().enumerate() {
        let path = if i == 0 { "index.html".to_string() } else { format!("{}.html", slug(&page.name)) };
        files.push(crate::backend::OutputFile::text(&path, &page_html(site, page, &hrefs)));
    }
    files.push(crate::backend::OutputFile::text("styles.css", &styles_css(site)));
    files.push(crate::backend::OutputFile::text("app.js", &app_js()));
    files
}

/// URL slug for a page name: identifiers become lowercase, `_` becomes
/// `-`. Page-name uniqueness is case-insensitive (E3031), so slugs
/// cannot collide.
pub fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c == '_' {
            out.push('-');
        } else if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('-');
        }
    }
    out
}

/// Escape a string for HTML text content and attribute values.
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn page_html(site: &SiteModel, page: &PageModel, hrefs: &HashMap<&str, String>) -> String {
    let title = if page.title.trim().is_empty() { &page.name } else { &page.title };
    let mut nav = String::new();
    for p in &site.pages {
        let href = &hrefs[p.name.as_str()];
        let current = if p.name == page.name { " aria-current=\"page\"" } else { "" };
        nav.push_str(&format!(
            "        <a href=\"{}\"{}>{}</a>\n",
            href,
            current,
            html_escape(&p.name)
        ));
    }

    let mut body = String::new();
    for e in &page.elements {
        match e {
            Element::Heading(v) => {
                body.push_str(&format!("      <h2>{}</h2>\n", html_escape(v)));
            }
            Element::Text(v) => {
                body.push_str(&format!("      <p>{}</p>\n", html_escape(v)));
            }
            Element::Button { label, target } => {
                body.push_str(&format!(
                    "      <p class=\"kinn-actions\"><a class=\"kinn-button\" href=\"{}\">{}</a></p>\n",
                    hrefs[target.as_str()],
                    html_escape(label)
                ));
            }
            Element::Link { label, target } => {
                body.push_str(&format!(
                    "      <p><a class=\"kinn-link\" href=\"{}\">{}</a></p>\n",
                    hrefs[target.as_str()],
                    html_escape(label)
                ));
            }
            Element::Image { src, alt } => {
                body.push_str(&format!(
                    "      <img src=\"{}\" alt=\"{}\" loading=\"lazy\">\n",
                    html_escape(src),
                    html_escape(alt)
                ));
            }
            Element::Divider => body.push_str("      <hr>\n"),
        }
    }

    format!(
        "<!doctype html>\n\
<html lang=\"en\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"generator\" content=\"KINN 0.3.0 web backend\">\n\
<title>{} · {}</title>\n\
<link rel=\"stylesheet\" href=\"styles.css\">\n\
</head>\n\
<body>\n\
<a class=\"kinn-skip\" href=\"#main\">Skip to content</a>\n\
<header class=\"kinn-header\">\n\
      <a class=\"kinn-site\" href=\"index.html\">{}</a>\n\
    <nav aria-label=\"Pages\">\n\
{}\
    </nav>\n\
</header>\n\
<main class=\"kinn-main\" id=\"main\">\n\
      <h1 class=\"kinn-title\">{}</h1>\n\
{}\
</main>\n\
<footer class=\"kinn-footer\">\n\
    <p>Built with <a href=\"https://github.com/kokernk-ctrl/KINN\">KINN</a></p>\n\
</footer>\n\
<script src=\"app.js\" defer></script>\n\
</body>\n\
</html>\n",
        html_escape(title),
        html_escape(&site.name),
        html_escape(&site.name),
        nav,
        html_escape(title),
        body,
    )
}

/// The site stylesheet: one clean, accessible, responsive design for
/// every generated page. Custom properties at the top are the only
/// knobs; no frameworks, no resets beyond the needed minimum.
fn styles_css(site: &SiteModel) -> String {
    let _ = site; // per-site tokens arrive with the 0.4 state module
    String::from(
        r#"/* Generated by KINN 0.3.0 web backend — one stylesheet for the site. */
:root {
  --kinn-bg: #fdfdfc;
  --kinn-ink: #191c1f;
  --kinn-muted: #5c646b;
  --kinn-accent: #d95400;
  --kinn-accent-ink: #ffffff;
  --kinn-line: #e4e2de;
  --kinn-card: #f6f5f3;
  --kinn-radius: 10px;
  --kinn-max: 46rem;
}

* { box-sizing: border-box; }

html { scroll-behavior: smooth; }
@media (prefers-reduced-motion: reduce) {
  html { scroll-behavior: auto; }
}

body {
  margin: 0;
  font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  line-height: 1.6;
  color: var(--kinn-ink);
  background: var(--kinn-bg);
}

.kinn-skip {
  position: absolute;
  left: -9999px;
  top: 0;
  background: var(--kinn-accent);
  color: var(--kinn-accent-ink);
  padding: 0.5rem 1rem;
  border-radius: 0 0 var(--kinn-radius) 0;
}
.kinn-skip:focus { left: 0; z-index: 10; }

.kinn-header {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem 1.5rem;
  align-items: baseline;
  justify-content: space-between;
  padding: 1rem 1.25rem;
  border-bottom: 1px solid var(--kinn-line);
}
.kinn-site {
  font-weight: 700;
  font-size: 1.15rem;
  text-decoration: none;
  color: var(--kinn-ink);
}
.kinn-header nav {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem 1rem;
}
.kinn-header nav a {
  color: var(--kinn-muted);
  text-decoration: none;
  padding: 0.15rem 0.35rem;
  border-radius: 6px;
}
.kinn-header nav a:hover { color: var(--kinn-ink); background: var(--kinn-card); }
.kinn-header nav a[aria-current="page"] {
  color: var(--kinn-accent);
  font-weight: 600;
}

.kinn-main {
  max-width: var(--kinn-max);
  margin: 0 auto;
  padding: 2rem 1.25rem 3rem;
}
.kinn-main h1 { font-size: 2rem; line-height: 1.2; margin: 0 0 1.25rem; }
.kinn-main h2 { font-size: 1.35rem; margin: 2rem 0 0.75rem; }
.kinn-main p { margin: 0.75rem 0; }
.kinn-main img {
  max-width: 100%;
  height: auto;
  border-radius: var(--kinn-radius);
  margin: 1rem 0;
}
.kinn-main hr { border: 0; border-top: 1px solid var(--kinn-line); margin: 2rem 0; }

.kinn-actions { margin: 1.5rem 0; }
.kinn-button {
  display: inline-block;
  background: var(--kinn-accent);
  color: var(--kinn-accent-ink);
  text-decoration: none;
  font-weight: 600;
  padding: 0.65rem 1.3rem;
  border-radius: var(--kinn-radius);
}
.kinn-button:hover { filter: brightness(1.08); }
.kinn-link { color: var(--kinn-accent); }

.kinn-footer {
  border-top: 1px solid var(--kinn-line);
  color: var(--kinn-muted);
  padding: 1rem 1.25rem 2rem;
  text-align: center;
}
.kinn-footer a { color: inherit; }

/* Visible focus for keyboard users, everywhere. */
a:focus-visible,
.kinn-skip:focus-visible {
  outline: 3px solid var(--kinn-accent);
  outline-offset: 2px;
}

/* Focus management (app.js) moves focus to the page heading. */
.kinn-js .kinn-title:focus {
  outline: none;
}
"#,
    )
}

/// Focus management after navigation. The site works without this file;
/// JS only moves keyboard focus to the new page heading, which screen
/// readers and keyboard users expect on multi-page sites.
fn app_js() -> String {
    String::from(
        r#"/* Generated by KINN 0.3.0 web backend — focus management only.
   There is no application JavaScript in a KINN 0.3 site (ADR-0020);
   every feature works with JavaScript disabled. */
(function () {
  "use strict";
  document.documentElement.classList.add("kinn-js");
  window.addEventListener("pageshow", function () {
    var heading = document.querySelector(".kinn-title");
    if (!heading) return;
    if (!heading.hasAttribute("tabindex")) heading.setAttribute("tabindex", "-1");
    heading.focus({ preventScroll: true });
  });
})();
"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::site_model;
    use crate::parser;
    use crate::lexer;

    fn model(src: &str) -> SiteModel {
        let (toks, ld) = lexer::tokenize(src);
        assert!(ld.iter().all(|d| !d.is_error()), "lex diagnostics: {:?}", ld);
        let prog = parser::parse(toks).expect("parse");
        let w = prog.websites.first().expect("website");
        site_model(w)
    }

    const TWO_PAGES: &str = r#"*mode = B;
website "MyWebsite" {
    page Home {
        title "Welcome to KINN"
        text "My first website"
        button "Get Started" open About
    }
    page About {
        heading "About"
        link "Back home" open Home
    }
}"#;

    fn file<'a>(files: &'a [crate::backend::OutputFile], path: &str) -> &'a str {
        files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("missing file {}", path))
            .as_text()
            .into_owned()
            .leak()
    }

    #[test]
    fn generates_index_slugs_css_and_js() {
        let files = generate(&model(TWO_PAGES));
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["index.html", "about.html", "styles.css", "app.js"]
        );
    }

    #[test]
    fn home_page_maps_title_to_title_tag_and_h1() {
        let files = generate(&model(TWO_PAGES));
        let index = file(&files, "index.html");
        assert!(index.contains("<title>Welcome to KINN · MyWebsite</title>"), "{}", index);
        assert!(index.contains("<h1 class=\"kinn-title\">Welcome to KINN</h1>"));
        assert!(index.contains("<p>My first website</p>"));
        // Semantic, JS-free navigation button.
        assert!(index.contains("<a class=\"kinn-button\" href=\"about.html\">Get Started</a>"));
    }

    #[test]
    fn navigation_marks_current_page_and_labels_pages() {
        let files = generate(&model(TWO_PAGES));
        let index = file(&files, "index.html");
        assert!(index.contains("href=\"index.html\" aria-current=\"page\">Home</a>"));
        assert!(index.contains("href=\"about.html\">About</a>"));
        assert!(!index.contains("aria-current=\"page\">About"));
        let about = file(&files, "about.html");
        assert!(about.contains("href=\"about.html\" aria-current=\"page\">About</a>"));
        assert!(about.contains("<a class=\"kinn-link\" href=\"index.html\">Back home</a>"));
    }

    #[test]
    fn accessibility_structure_is_generated() {
        let files = generate(&model(TWO_PAGES));
        let index = file(&files, "index.html");
        assert!(index.starts_with("<!doctype html>"));
        assert!(index.contains("<html lang=\"en\">"));
        assert!(index.contains("class=\"kinn-skip\" href=\"#main\""));
        assert!(index.contains("aria-label=\"Pages\""));
        assert!(index.contains("id=\"main\""));
        assert!(index.contains("loading=\"lazy\"") == false); // no images here
    }

    #[test]
    fn images_render_with_mandatory_alt() {
        let src = r#"*mode = B;
website "S" {
    page Home {
        image "assets/logo.png" alt "Company logo"
        divider
    }
}"#;
        let files = generate(&model(src));
        let index = file(&files, "index.html");
        assert!(index.contains("<img src=\"assets/logo.png\" alt=\"Company logo\" loading=\"lazy\">"), "{}", index);
        assert!(index.contains("<hr>"));
    }

    #[test]
    fn html_escaping_never_breaks_the_markup() {
        let src = r#"*mode = B;
website "Weird & <Co>" {
    page Home {
        title "<script>alert(1)</script> & 'quotes'"
        text "a < b && c > d"
        button "<b>bold?</b>" open Home
    }
}"#;
        let files = generate(&model(src));
        let index = file(&files, "index.html");
        assert!(!index.contains("<script>alert"));
        assert!(index.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(index.contains("Weird &amp; &lt;Co&gt;"));
        assert!(index.contains("a &lt; b &amp;&amp; c &gt; d"));
        assert!(index.contains(">&lt;b&gt;bold?&lt;/b&gt;</a>"));
    }

    #[test]
    fn slugs_are_lowercase_and_underscore_free() {
        assert_eq!(slug("Home"), "home");
        assert_eq!(slug("About_Us"), "about-us");
        assert_eq!(slug("Mixed123"), "mixed123");
    }

    #[test]
    fn underscore_page_names_get_dash_slugs() {
        let src = r#"*mode = B;
website "S" {
    page Home { button "Go" open Contact_Us }
    page Contact_Us { link "Home" open Home }
}"#;
        let files = generate(&model(src));
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"contact-us.html"), "{:?}", paths);
        let index = file(&files, "index.html");
        assert!(index.contains("href=\"contact-us.html\""));
    }

    #[test]
    fn site_without_titles_falls_back_to_page_name() {
        let src = r#"*mode = B;
website "S" { page Home { text "hi" } }"#;
        let files = generate(&model(src));
        let index = file(&files, "index.html");
        assert!(index.contains("<title>Home · S</title>"));
        assert!(index.contains("<h1 class=\"kinn-title\">Home</h1>"));
    }

    #[test]
    fn styles_and_script_ship_real_content() {
        let files = generate(&model(TWO_PAGES));
        let css = file(&files, "styles.css");
        assert!(css.contains("aria-current"));
        assert!(css.contains("prefers-reduced-motion"));
        assert!(css.contains(":focus-visible"));
        let js = file(&files, "app.js");
        assert!(js.contains("focus"));
        assert!(js.contains("JavaScript disabled") || js.contains("no application JavaScript"));
    }

    #[test]
    fn generation_is_deterministic() {
        let a = generate(&model(TWO_PAGES));
        let b = generate(&model(TWO_PAGES));
        assert_eq!(a, b);
    }

    #[test]
    fn unicode_content_survives() {
        let src = r#"*mode = B;
website "Meu Site" {
    page Home { title "Bem-vindo" text "Olá → 世界" }
}"#;
        let files = generate(&model(src));
        let index = file(&files, "index.html");
        assert!(index.contains("Olá → 世界"));
        assert!(index.contains("<title>Bem-vindo · Meu Site</title>"));
    }
}
