# Web target backend (Kin 0.3)

The first complete vertical slice of the multi-target architecture
(see docs/MULTI_TARGET_ARCHITECTURE.md, ADR-0018–0020): a Kin
developer writes a `website` declaration and gets a real, hostable
website — no HTML, CSS or JavaScript written by hand.

Status: **implemented and tested** (parser/checker/codegen unit tests,
CLI end-to-end tests, and a real-browser verification of the generated
dist).

---

## 1. The declarative web module

```kin
*mode = B;

website "MyWebsite" {
    page Home {
        title "Welcome to KINN"
        heading "My first website"
        text "Written in Kin, built by the KINN web backend."
        button "Get Started" open About
        divider
        link "Learn more" open About
        image "assets/banner.png" alt "Product banner"
    }
    page About {
        title "About"
        text "This page is generated from the declaration above."
        link "Back to Home" open Home
    }
}
```

Evaluated against the existing grammar (decision ADR-0019):

- `website`, `page`, and the element words are **contextual keywords**
  — they are recognized only in their grammatical positions. Existing
  0.2 programs that use them as identifiers keep compiling (tested).
- The mode directive keeps its semicolon (`*mode = B;`), consistent
  with the rest of the grammar; website elements are one-per-line
  without semicolons, like the declarative block they form.
- A website declaration is a language module, available in **all three
  modes** — modes never fork the language (B/S/A semantics unchanged).
- Websites live in the **entry file** only (E3037 in imported
  modules). A program may declare at most one website (E3030).
- Console programs keep `func main`; website programs do not need one.
  `kinn run` of a site-only program reports guidance toward
  `kinn build --target web` instead of a fake execution.

## 2. Element reference (complete 0.3.0 set)

| Element | Grammar | Generated HTML | Notes |
| --- | --- | --- | --- |
| `title "…"` | string | `<title>` + `<h1 class="kinn-title">` | one per page is normal; the first wins, later ones demote to `h2` with W3036 |
| `heading "…"` | string | `<h2>` | section heading |
| `text "…"` | string | `<p>` | paragraph |
| `button "…" open Page` | string + page name | `<a class="kinn-button" href="page.html">` | navigation; a real link styled as a button — works without JavaScript. Buttons without a target are rejected at parse time (interactive controls arrive with the 0.4 state module; nothing is faked) |
| `link "…" open Page` | string + page name | `<a class="kinn-link" href="page.html">` | inline navigation |
| `image "src" alt "…"` | two strings | `<img src="…" alt="…" loading="lazy">` | alt text is **required and non-empty** (E3034, E3036) — accessibility enforced at compile time |
| `divider` | — | `<hr>` | thematic break |

Anything else is rejected with **E2030** ("unknown page element …")
naming the supported set — the grammar cannot express what the backend
cannot generate.

## 3. Semantic rules (target-independent, checked before any backend)

| Code | Kind | Rule |
| --- | --- | --- |
| E3030 | error | more than one website declaration |
| E3031 | error | duplicate page name (case-insensitive — slugs would collide) |
| E3032 | error | `open` target is not a declared page (hint lists declared pages) |
| E3033 | error | website name empty |
| E3034 | error | image alt text empty (accessibility) |
| E3035 | error | button/link label empty |
| E3036 | error | image source empty |
| E3037 | error | website declared in a non-entry (imported) file |
| E3038 | error | program declares no website but a build target needs one (FFI/build) |
| W3035 | warning | page never linked from any other page (unreachable via navigation) |
| W3036 | warning | more than one `title` element |
| W3037 | warning | empty `heading`/`text` content |
| E2016 | parser | website name is not a string |
| E2030 | parser | unknown page element |

All strings are HTML-escaped at generation time (`& < > " '`), so user
content can never break the markup or inject scripts (tested,
including a `<script>` payload).

## 4. Generated distribution

`kinn build site.kin --target web` (default output:
`<entry-dir>/build/web`, override with `--out DIR`):

```
build/web/
  index.html     ← the FIRST declared page (home)
  about.html     ← one <slug>.html per additional page (slug = lowercased name, _ → -)
  styles.css     ← one stylesheet for the whole site
  app.js         ← focus management after navigation ONLY
  assets/…       ← copied verbatim from the project's assets/ directory, when present
```

- Navigation between pages uses plain `<a href>` — the site works with
  JavaScript disabled, from `file://`, or from any static host.
- `index.html` is the first page; every page links to every other page
  in the header nav with `aria-current="page"` on the current one.
- Deterministic output: same input → byte-identical files (tested).

## 5. Accessibility behavior (generated in, enforced at compile time)

- `lang="en"`, `<meta charset>`, viewport meta, generator meta.
- Landmarks: `header` (banner), `nav aria-label="Pages"`, `main
  id="main"`, `footer`.
- Skip link (`Skip to content`) as the first focusable element.
- Heading order: exactly one `h1` per page (the title), `h2` sections.
- Images cannot exist without alt text (compile-time rule).
- Visible keyboard focus everywhere (`:focus-visible` outlines in the
  stylesheet); `prefers-reduced-motion` respected.
- After navigation, `app.js` moves keyboard focus to the new page
  heading (standard multi-page-site practice). This is the *only*
  JavaScript in the output.

## 6. Styling, state, events — what is real and what is planned

- **Styling**: one generated stylesheet with CSS custom properties
  (single accent token), responsive header, card-free simple layout.
  Per-site theming tokens arrive with the 0.4 state module; the
  generated CSS is a clean, editable default — no framework.
- **State / events**: the 0.3 grammar has no state, forms, or event
  handlers, so the backend generates none — there is no generic
  JavaScript runtime pretending to support features the language
  cannot express (ADR-0020). Interactive elements (buttons with
  behavior, fields, forms) are the 0.4 milestone.

## 7. CLI

```
kinn build <file.kin> --target web [--out DIR]   # default out: build/web
kinn build                                        # reads kinn.toml (name/version/entry/target)
kinn targets                                      # lists web + android
```

Exit codes follow the CLI convention (0 ok, 1 compile diagnostics,
2 usage error). Building a console program for web explains the
missing `website` declaration (E3038 semantics). Assets copy from
`<entry-dir>/assets/` when that directory exists.

## 8. Verification performed

- 12 codegen unit tests (mapping, escaping, slugs, aria-current,
  determinism, unicode).
- Parser tests (13) and checker tests (12) for the rules above.
- 9 CLI integration tests run the real binary.
- **Real browser**: the generated dist was opened in a headless
  Chromium (`file://…/dist/index.html`), the accessibility tree
  verified (skip link, nav landmark, h1/h2 order, links), and clicking
  the generated `Get Started` button navigated to `about.html` with
  the correct document title.
- Studio (web + Android) builds the same backend through the wasm/JNI
  bridge and previews the result (see docs/KINN_STUDIO.md).

## 9. Host integration

| Host | Entry point | Result |
| --- | --- | --- |
| CLI | `kinc::backend::web::generate` | files written to disk |
| Studio web | `kinn_build_web` via kinn.wasm | preview panel + zip download |
| Studio Android | `KinnBridge.nBuildWeb` (JNI) | offline WebView preview |
