//! The Kin parser: tokens to AST (recursive descent).
//!
//! Since 0.2.0 the parser recovers from syntax errors: when a statement
//! fails to parse, it records the diagnostic, skips tokens up to the next
//! synchronization point (`;`, `}`, a statement keyword or a top-level
//! `func`/`import`) and continues, so one file can yield many precise
//! diagnostics instead of stopping at the first (P-001).
//!
//! The parse still fails as a whole when any error was recorded: the
//! checker and lowering only run on fully valid syntax.

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::token::{describe, Token, TokKind};

/// Stop collecting after this many syntax errors and skip the rest of the
/// input (E2099). This keeps error cascades from flooding the user.
const MAX_PARSE_ERRORS: usize = 25;

/// ADR-0027 theme token whitelist (E2034 outside this set).
const THEME_TOKENS: &[&str] =
    &["accent", "bg", "surface", "text", "muted", "font", "radius", "max_width"];

/// Field attribute words (E2036 outside this set).
const FIELD_ATTRS: &[&str] = &[
    "label",
    "placeholder",
    "required",
    "min_length",
    "max_length",
    "min_value",
    "max_value",
    "pattern",
];

/// Can a cast type start at this token? (`as` + identifier is the
/// `each ... as <name>` loop marker, never a cast — types are keywords).
fn type_can_follow_as(k: &TokKind) -> bool {
    matches!(
        k,
        TokKind::IntTy
            | TokKind::FloatTy
            | TokKind::BoolTy
            | TokKind::StrTy
            | TokKind::VoidTy
            | TokKind::LBracket
    )
}

/// Can this token begin an expression? (unary `not`/`-`, literals,
/// identifiers, `(`-groups and array literals). Used by the app view
/// argument pre-check so a missing binding expression gets the precise
/// E2033 message instead of a generic expression error.
fn expr_can_start(k: &TokKind) -> bool {
    matches!(
        k,
        TokKind::Int(_)
            | TokKind::Float(_)
            | TokKind::Str(_)
            | TokKind::Ident(_)
            | TokKind::True
            | TokKind::False
            | TokKind::Not
            | TokKind::Minus
            | TokKind::LBracket
            | TokKind::LParen
    )
}

/// Parse a token stream (as produced by [`crate::lexer::tokenize`]).
///
/// Returns the program, or all diagnostics collected (possibly several
/// per file thanks to recovery) when any syntax error was found.
pub fn parse(tokens: Vec<Token>) -> Result<Program, Vec<Diagnostic>> {
    let mut p = Parser { toks: tokens, pos: 0, diags: Vec::new(), had_error: false, gave_up: false };
    let prog = p.program();
    if p.had_error {
        Err(p.diags)
    } else {
        Ok(prog)
    }
}

type PResult<T> = Result<T, ()>;

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
    had_error: bool,
    /// Set when the error cap is hit; all loops stop and skip to EOF.
    gave_up: bool,
}

impl Parser {
    fn peek(&self) -> &TokKind {
        &self.toks[self.pos].kind
    }

    fn peek_token(&self) -> &Token {
        &self.toks[self.pos]
    }

    fn peek2_kind(&self) -> &TokKind {
        static EOF: TokKind = TokKind::Eof;
        match self.toks.get(self.pos + 1) {
            Some(t) => &t.kind,
            None => &EOF,
        }
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if !matches!(t.kind, TokKind::Eof) {
            self.pos += 1;
        }
        t
    }

    fn at(&self, k: TokKind) -> bool {
        *self.peek() == k
    }

    /// Central diagnostic sink: enforces the error cap.
    fn fail(&mut self, code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) {
        if self.gave_up {
            return;
        }
        if self.diags.len() >= MAX_PARSE_ERRORS {
            self.diags.push(Diagnostic::error(
                "E2099",
                format!("too many syntax errors ({}); stopping after the first {}", MAX_PARSE_ERRORS, MAX_PARSE_ERRORS),
                line,
                col,
                1,
            ));
            self.gave_up = true;
            return;
        }
        self.diags.push(Diagnostic::error(code, message, line, col, len));
        self.had_error = true;
    }

    fn error_here(&mut self, message: String) {
        let t = self.peek_token().clone();
        self.fail("E2001", message, t.line, t.col, t.len().max(1));
    }

    fn expect(&mut self, k: TokKind, what: &str) -> PResult<Token> {
        if self.at(k) {
            Ok(self.bump())
        } else {
            let t = self.peek_token().clone();
            self.fail(
                "E2002",
                format!("expected {} but found {}", what, describe(&t.kind)),
                t.line,
                t.col,
                t.len().max(1),
            );
            // An unexpected `as` is almost always a malformed cast: the
            // `each ... as <name>` marker is consumed by the `each`
            // parser itself, so a stray `as` here deserves a hint that
            // names both readings (S2 review item 5).
            if !self.gave_up && matches!(t.kind, TokKind::As) {
                let hint = if matches!(self.peek2_kind(), TokKind::Ident(_)) {
                    "`as <name>` is only the loop variable of `each <list> as <name>`; outside `each`, `as` is the cast operator and needs a type keyword (like `x as int` or `x as string`)"
                } else {
                    "a cast needs a type keyword after `as` (`int`, `float`, `bool`, `string` or `[T]`)"
                };
                if let Some(d) = self.diags.last_mut() {
                    d.hint = Some(hint.into());
                }
            }
            Err(())
        }
    }

    fn expect_ident(&mut self) -> PResult<Token> {
        if matches!(self.peek(), TokKind::Ident(_)) {
            Ok(self.bump())
        } else {
            let t = self.peek_token().clone();
            self.fail(
                "E2002",
                format!("expected an identifier but found {}", describe(&t.kind)),
                t.line,
                t.col,
                t.len().max(1),
            );
            Err(())
        }
    }

    /// Skip to a statement boundary after a statement-level error:
    /// past `;`, or up to `}`, a statement keyword, a top-level `func` /
    /// `import`, or end of file.
    fn recover_stmt(&mut self) {
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return;
            }
            match self.peek() {
                TokKind::Eof => return,
                TokKind::Semi => {
                    self.bump();
                    return;
                }
                TokKind::RBrace
                | TokKind::Func
                | TokKind::Import
                | TokKind::Let
                | TokKind::Var
                | TokKind::If
                | TokKind::While
                | TokKind::For
                | TokKind::Return
                | TokKind::Break
                | TokKind::Continue
                | TokKind::LBrace => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Skip to the next top-level item (`func`, `import`, `const`) after
    /// a broken declaration header. A following `const` declaration is a
    /// fresh top-level item, so recovery stops before consuming it.
    fn recover_top(&mut self) {
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return;
            }
            match self.peek() {
                TokKind::Eof | TokKind::Func | TokKind::Import => return,
                TokKind::Ident(w) if w == "const" => return,
                TokKind::RBrace | TokKind::Semi => {
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn skip_to_eof(&mut self) {
        while !self.at(TokKind::Eof) {
            self.bump();
        }
    }

    // -- program structure ---------------------------------------------------

    fn program(&mut self) -> Program {
        let mut mode = Mode::default();
        let mut mode_declared = false;
        let mut imports = Vec::new();
        let mut consts = Vec::new();
        let mut funcs = Vec::new();
        let mut websites = Vec::new();
        let mut apps = Vec::new();

        if self.gave_up {
            return Program { mode, mode_declared, imports, consts, funcs, websites, apps };
        }

        // Optional mode directive — must be the very first thing.
        if self.at(TokKind::Star) {
            match self.mode_directive() {
                Ok(m) => {
                    mode = m;
                    mode_declared = true;
                }
                Err(()) => {
                    self.recover_top();
                }
            }
        }

        // Imports — before all functions.
        while !self.gave_up && self.at(TokKind::Import) {
            match self.import_decl() {
                Ok(d) => imports.push(d),
                Err(()) => self.recover_top(),
            }
        }

        // Top-level items: website declarations (0.3.0, contextual
        // keyword — ADR-0019) and functions, in any order after the
        // imports. A top-level `website` identifier is always a website
        // declaration attempt (the only other valid top-level item is
        // `func`), so a non-string name gets a precise E2016.
        while !self.gave_up && !self.at(TokKind::Eof) {
            if self.at(TokKind::Star) {
                self.error_here(
                    "the `*mode` directive must be the first statement of the file".to_string(),
                );
                self.recover_top();
                continue;
            }
            if self.at(TokKind::Import) {
                self.fail(
                    "E2012",
                    "`import` declarations must appear before all functions and consts",
                    self.peek_token().line,
                    self.peek_token().col,
                    6,
                );
                self.recover_top();
                continue;
            }
            if matches!(self.peek(), TokKind::Ident(w) if w == "website") {
                match self.website_decl() {
                    Ok(w) => websites.push(w),
                    Err(()) => self.recover_top(),
                }
                continue;
            }
            // `app` declarations (0.4.0 S2) follow the same ADR-0019
            // contextual recipe: at top level an `app` identifier is
            // always an app-declaration attempt.
            if matches!(self.peek(), TokKind::Ident(w) if w == "app") {
                match self.app_decl() {
                    Ok(a) => apps.push(a),
                    Err(()) => self.recover_top(),
                }
                continue;
            }
            // `const` declarations (0.4.0 S1) use the ADR-0019 contextual
            // recipe: at top level a `const` identifier is always a const
            // declaration attempt (the only other valid items are `func`,
            // `import` and `website`), so a malformed header gets a
            // precise diagnostic instead of a generic function error.
            if matches!(self.peek(), TokKind::Ident(w) if w == "const") {
                match self.const_decl() {
                    Ok(d) => consts.push(d),
                    Err(()) => self.recover_top(),
                }
                continue;
            }
            match self.func() {
                Ok(f) => funcs.push(f),
                Err(()) => self.recover_top(),
            }
        }

        Program { mode, mode_declared, imports, consts, funcs, websites, apps }
    }

    /// `const NAME: Type = expr;` — entered when a top-level `const`
    /// identifier is seen (ADR-0019 contextual recipe, 0.4.0 S1). The
    /// type annotation is part of the grammar: a const always declares
    /// its type explicitly.
    fn const_decl(&mut self) -> PResult<ConstDecl> {
        let kw = self.bump(); // Ident("const")
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect_colon_for_const(&name)?;
        let ty = self.parse_type()?;
        self.expect(TokKind::Assign, "`=`")?;
        let init = self.expr()?;
        self.expect(TokKind::Semi, "';'")?;
        Ok(ConstDecl { name, ty, init, line: kw.line, col: kw.col })
    }

    /// `const` declarations require a type annotation. Without this the
    /// generic `expected ':'` message would not mention the rule, so the
    /// parser says exactly what is missing.
    fn expect_colon_for_const(&mut self, name: &str) -> PResult<()> {
        if self.at(TokKind::Colon) {
            self.bump();
            return Ok(());
        }
        let t = self.peek_token().clone();
        self.fail(
            "E2003",
            format!(
                "a `const` must declare its type (like `const {}: int = 10;`) but found {}",
                name,
                describe(&t.kind)
            ),
            t.line,
            t.col,
            t.len().max(1),
        );
        Err(())
    }

    /// `website "Name" { page P { ... } ... }` — entered when a
    /// top-level `website` identifier is seen (ADR-0019).
    fn website_decl(&mut self) -> PResult<WebsiteDecl> {
        let kw = self.bump(); // Ident("website")
        let name_tok = self.peek_token().clone();
        let name = match name_tok.kind.clone() {
            TokKind::Str(s) => {
                self.bump();
                s
            }
            other => {
                self.fail(
                    "E2016",
                    format!(
                        "expected a string name after `website` (like `website \"My Site\" {{ ... }}`) but found {}",
                        describe(&other)
                    ),
                    name_tok.line,
                    name_tok.col,
                    name_tok.len().max(1),
                );
                return Err(());
            }
        };
        self.expect(TokKind::LBrace, "'{'")?;
        let mut pages = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            match self.page_decl() {
                Ok(p) => pages.push(p),
                Err(()) => return Err(()),
            }
        }
        Ok(WebsiteDecl { name, pages, line: kw.line, col: kw.col })
    }

    /// `page Home { ... }` inside a website declaration.
    fn page_decl(&mut self) -> PResult<PageDecl> {
        let kw = self.bump(); // Ident("page")
        let name = self.expect_ident()?;
        let name = match name.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect(TokKind::LBrace, "'{'")?;
        let mut elements = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            match self.element() {
                Ok(e) => elements.push(e),
                Err(()) => self.recover_page_element(),
            }
        }
        Ok(PageDecl { name, elements, line: kw.line, col: kw.col })
    }

    /// One declarative page element (one line, no semicolon).
    fn element(&mut self) -> PResult<ElementDecl> {
        let t = self.peek_token().clone();
        let word = match t.kind.clone() {
            TokKind::Ident(w) => w,
            other => {
                self.fail(
                    "E2001",
                    format!(
                        "expected a page element (`title`, `heading`, `text`, `button`, `link`, `image`, `divider`) but found {}",
                        describe(&other)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
        };
        self.bump();
        let kind = match word.as_str() {
            "title" => ElementKind::Title(self.string_after("title")?),
            "heading" => ElementKind::Heading(self.string_after("heading")?),
            "text" => ElementKind::Text(self.string_after("text")?),
            "divider" => ElementKind::Divider,
            "button" | "link" => {
                let label = self.string_after(&word)?;
                let mut target = None;
                let mut target_line = 0;
                let mut target_col = 0;
                if matches!(self.peek(), TokKind::Ident(w) if w == "open") {
                    self.bump();
                    let tgt = self.expect_ident()?;
                    if let TokKind::Ident(n) = tgt.kind {
                        target = Some(n);
                        target_line = tgt.line;
                        target_col = tgt.col;
                    }
                }
                match target {
                    Some(target) if word == "button" => ElementKind::Button {
                        label,
                        target,
                        target_line,
                        target_col,
                    },
                    Some(target) => ElementKind::Link { label, target, target_line, target_col },
                    None => {
                        let nt = self.peek_token().clone();
                        self.fail(
                            "E2002",
                            format!(
                                "a `{}` must navigate: expected `open <PageName>` but found {}",
                                word,
                                describe(&nt.kind)
                            ),
                            nt.line,
                            nt.col,
                            nt.len().max(1),
                        );
                        self.diags.last_mut().unwrap().hint = Some(
                            "example: button \"Get Started\" open Home — interactive buttons (state, forms) arrive in a later release".into(),
                        );
                        return Err(());
                    }
                }
            }
            "image" => {
                let src = self.string_after("image")?;
                let alt_tok = self.peek_token().clone();
                match alt_tok.kind.clone() {
                    TokKind::Ident(w) if w == "alt" => {
                        self.bump();
                        let alt = self.string_after("alt")?;
                        ElementKind::Image { src, alt, alt_line: alt_tok.line, alt_col: alt_tok.col }
                    }
                    other => {
                        self.fail(
                            "E2002",
                            format!(
                                "images need alt text for accessibility: expected `alt \"description\"` but found {}",
                                describe(&other)
                            ),
                            alt_tok.line,
                            alt_tok.col,
                            alt_tok.len().max(1),
                        );
                        return Err(());
                    }
                }
            }
            _ => {
                self.fail(
                    "E2030",
                    format!(
                        "unknown page element `{}` (supported: title, heading, text, button, link, image, divider)",
                        word
                    ),
                    t.line,
                    t.col,
                    t.len(),
                );
                return Err(());
            }
        };
        Ok(ElementDecl { kind, line: t.line, col: t.col })
    }

    /// Expect a string literal after a one-word element keyword.
    fn string_after(&mut self, word: &str) -> PResult<String> {
        let t = self.peek_token().clone();
        match t.kind.clone() {
            TokKind::Str(s) => {
                self.bump();
                Ok(s)
            }
            other => {
                self.fail(
                    "E2002",
                    format!(
                        "expected a string after `{}` but found {}",
                        word,
                        describe(&other)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                Err(())
            }
        }
    }

    /// Skip to the next page element, `}` or end of file after a broken
    /// element (keeps the remaining elements of the page parseable).
    fn recover_page_element(&mut self) {
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return;
            }
            match self.peek() {
                TokKind::Eof | TokKind::RBrace => return,
                TokKind::Ident(w) => {
                    if matches!(
                        w.as_str(),
                        "title" | "heading" | "text" | "button" | "link" | "image" | "divider"
                    ) {
                        return;
                    }
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    // -- app module (0.4.0 S2) ------------------------------------------------
    //
    // The dynamic sibling of `website` (docs/DYNAMIC_WEB_ARCHITECTURE.md
    // B.2–B.9). Every word below is a contextual keyword (ADR-0019): the
    // grammar is entered only by position, so the words stay ordinary
    // identifiers everywhere else (regression-tested).

    /// Every word that can start an app section inside `app { ... }`.
    const APP_SECTION_WORDS: &'static [&'static str] = &[
        "navigation", "not_found", "theme", "state", "computed", "animation", "store", "route",
        "transition", "page", "component",
    ];

    /// Every word that can start a view element (used by dispatch and
    /// recovery sync).
    const VIEW_ELEMENT_WORDS: &'static [&'static str] = &[
        "title", "heading", "text", "button", "link", "image", "divider", "input", "when",
        "each", "form", "field", "on",
    ];

    /// `app "Name" { sections }` — top level, entry file (checker E3051).
    fn app_decl(&mut self) -> PResult<AppDecl> {
        let kw = self.bump(); // Ident("app")
        let name_tok = self.peek_token().clone();
        let name = match name_tok.kind.clone() {
            TokKind::Str(s) => {
                self.bump();
                s
            }
            other => {
                self.fail(
                    "E2016",
                    format!(
                        "expected a string name after `app` (like `app \"TaskTrack\" {{ ... }}`) but found {}",
                        describe(&other)
                    ),
                    name_tok.line,
                    name_tok.col,
                    name_tok.len().max(1),
                );
                return Err(());
            }
        };
        self.expect(TokKind::LBrace, "'{'")?;
        let mut app = AppDecl {
            name,
            navigation: None,
            not_found: None,
            theme: Vec::new(),
            state: Vec::new(),
            computed: Vec::new(),
            animation: None,
            stores: Vec::new(),
            routes: Vec::new(),
            transition: None,
            pages: Vec::new(),
            components: Vec::new(),
            line: kw.line,
            col: kw.col,
        };
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            let t = self.peek_token().clone();
            let word = match &t.kind {
                TokKind::Ident(w) => w.clone(),
                other => {
                    self.fail(
                        "E2001",
                        format!(
                            "expected an app section ({}) but found {}",
                            Self::APP_SECTION_WORDS.join(", "),
                            describe(other)
                        ),
                        t.line,
                        t.col,
                        t.len().max(1),
                    );
                    return Err(());
                }
            };
            let r = match word.as_str() {
                "navigation" => self.app_navigation(&mut app),
                "not_found" => self.app_not_found(&mut app),
                "theme" => self.app_theme(&mut app),
                "state" => self.app_state(&mut app),
                "computed" => self.app_computed(&mut app),
                "animation" => self.app_animation(&mut app),
                "store" => self.app_store(&mut app),
                "route" => self.app_route(&mut app),
                "transition" => self.app_transition(&mut app),
                "page" => self.app_page(&mut app),
                "component" => self.app_component(&mut app),
                _ => {
                    self.fail(
                        "E2001",
                        format!(
                            "unknown app section `{}` (supported: {})",
                            word,
                            Self::APP_SECTION_WORDS.join(", ")
                        ),
                        t.line,
                        t.col,
                        t.len(),
                    );
                    Err(())
                }
            };
            if r.is_err() {
                // Recover to the next section word / `}` / EOF so one
                // broken section keeps the rest parseable.
                self.recover_app_section();
            }
        }
        Ok(app)
    }

    /// Skip to the next app section, `}` or EOF after a broken section.
    /// Brace-aware: when a section fails inside its own `{ ... }` block,
    /// the block's closing `}` is consumed (returning to section level)
    /// instead of being mistaken for the app's `}` — the `}` that would
    /// close the app itself is left for the caller.
    fn recover_app_section(&mut self) {
        let mut depth = 0usize;
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return;
            }
            match self.peek() {
                TokKind::Eof => return,
                TokKind::LBrace => {
                    depth += 1;
                    self.bump();
                }
                TokKind::RBrace => {
                    if depth == 0 {
                        // Closes the app (or an enclosing block): leave it.
                        return;
                    }
                    depth -= 1;
                    self.bump();
                }
                TokKind::Ident(w) => {
                    if depth == 0 && Self::APP_SECTION_WORDS.contains(&w.as_str()) {
                        return;
                    }
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// `navigation mpa | spa`.
    fn app_navigation(&mut self, app: &mut AppDecl) -> PResult<()> {
        self.bump(); // "navigation"
        let t = self.peek_token().clone();
        let mode = match &t.kind {
            TokKind::Ident(w) if w == "mpa" => NavigationMode::Mpa,
            TokKind::Ident(w) if w == "spa" => NavigationMode::Spa,
            other => {
                self.fail(
                    "E2001",
                    format!(
                        "expected `navigation mpa` or `navigation spa` but found {}",
                        describe(other)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
        };
        self.bump();
        app.navigation = Some(mode);
        Ok(())
    }

    /// `not_found <Page>`.
    fn app_not_found(&mut self, app: &mut AppDecl) -> PResult<()> {
        self.bump(); // "not_found"
        let t = self.expect_ident()?;
        if let TokKind::Ident(n) = t.kind {
            app.not_found = Some(OpenTarget { page: n, line: t.line, col: t.col });
        }
        Ok(())
    }

    /// `transition "name"` — app-level page/route transition.
    fn app_transition(&mut self, app: &mut AppDecl) -> PResult<()> {
        self.bump(); // "transition"
        let r = self.anim_ref_after("transition")?;
        app.transition = Some(r);
        Ok(())
    }

    /// `theme { token "value" ... }` — ADR-0027 whitelist (E2034).
    fn app_theme(&mut self, app: &mut AppDecl) -> PResult<()> {
        self.bump(); // "theme"
        self.expect(TokKind::LBrace, "'{'")?;
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            let t = self.peek_token().clone();
            let token = match &t.kind {
                TokKind::Ident(w) => w.clone(),
                other => {
                    self.fail(
                        "E2034",
                        format!(
                            "expected a theme token ({}) but found {}",
                            THEME_TOKENS.join(", "),
                            describe(other)
                        ),
                        t.line,
                        t.col,
                        t.len().max(1),
                    );
                    return Err(());
                }
            };
            if !THEME_TOKENS.contains(&token.as_str()) {
                self.fail(
                    "E2034",
                    format!(
                        "unknown theme token `{}` (supported: {})",
                        token,
                        THEME_TOKENS.join(", ")
                    ),
                    t.line,
                    t.col,
                    t.len(),
                );
                return Err(());
            }
            self.bump();
            match self.string_after("theme token") {
                Ok(value) => {
                    app.theme.push(ThemeToken { token, value, line: t.line, col: t.col })
                }
                Err(()) => {
                    if !self.recover_flat_item() {
                        return Err(());
                    }
                }
            }
        }
        Ok(())
    }

    /// `state { name: Type = expr ... }` — fields merge across blocks.
    fn app_state(&mut self, app: &mut AppDecl) -> PResult<()> {
        let fields = self.state_block()?;
        app.state.extend(fields);
        Ok(())
    }

    /// The body of a `state { ... }` block (shared app state and
    /// component state use the same field grammar).
    fn state_block(&mut self) -> PResult<Vec<StateField>> {
        self.bump(); // "state"
        self.expect(TokKind::LBrace, "'{'")?;
        let mut fields = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                return Ok(fields);
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            // Optional `,`/`;` separators between fields are accepted.
            if self.at(TokKind::Comma) || self.at(TokKind::Semi) {
                self.bump();
                continue;
            }
            match self.state_field("state") {
                Ok(f) => fields.push(f),
                Err(()) => {
                    // Recover INSIDE the block (field initializers are
                    // expressions and contain no braces, so any `}` found
                    // while skipping is this block's own) and keep the
                    // following fields parseable.
                    if !self.recover_flat_item() {
                        return Err(());
                    }
                }
            }
        }
    }

    /// Recovery for one broken item inside a flat `{ ... }` section
    /// (state fields, theme tokens, store keys, field attrs, animation
    /// options): skip to the next `,`/`;`/`}`. Returns `false` at EOF
    /// (the caller gives up). The closing `}` is left for the caller's
    /// loop to consume, so the enclosing section always keeps its own
    /// braces balanced.
    fn recover_flat_item(&mut self) -> bool {
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return false;
            }
            match self.peek() {
                TokKind::Eof => return false,
                TokKind::RBrace | TokKind::Comma | TokKind::Semi => return true,
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// `name: Type = expr` — one state field (shared or component).
    fn state_field(&mut self, owner: &str) -> PResult<StateField> {
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        if !self.at(TokKind::Colon) {
            let t = self.peek_token().clone();
            self.fail(
                "E2003",
                format!(
                    "a {} field must declare its type (like `{}: int = 0`) but found {}",
                    owner,
                    name,
                    describe(&t.kind)
                ),
                t.line,
                t.col,
                t.len().max(1),
            );
            return Err(());
        }
        self.bump();
        let ty = self.parse_type()?;
        self.expect(TokKind::Assign, "`=` (state fields need an initializer)")?;
        let init = self.view_expr("a state field initializer")?;
        Ok(StateField { name, ty, init, line: name_tok.line, col: name_tok.col })
    }

    /// `computed name = expr`.
    fn app_computed(&mut self, app: &mut AppDecl) -> PResult<()> {
        let kw = self.bump(); // "computed"
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect(TokKind::Assign, "`=`")?;
        let expr = self.view_expr("a computed value")?;
        app.computed.push(ComputedDecl { name, expr, line: kw.line, col: kw.col });
        Ok(())
    }

    /// `animation { keyframes ... transition ... }`.
    fn app_animation(&mut self, app: &mut AppDecl) -> PResult<()> {
        let kw = self.bump(); // "animation"
        self.expect(TokKind::LBrace, "'{'")?;
        let mut decl = AnimationDecl { keyframes: Vec::new(), transitions: Vec::new(), line: kw.line, col: kw.col };
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            let t = self.peek_token().clone();
            let word = match &t.kind {
                TokKind::Ident(w) if w == "keyframes" => "keyframes",
                TokKind::Ident(w) if w == "transition" => "transition",
                other => {
                    self.fail(
                        "E2037",
                        format!(
                            "expected `keyframes` or `transition` inside animation but found {}",
                            describe(other)
                        ),
                        t.line,
                        t.col,
                        t.len().max(1),
                    );
                    return Err(());
                }
            };
            let r = match word {
                "keyframes" => self.keyframes_decl().map(|k| decl.keyframes.push(k)),
                _ => self.transition_decl().map(|tr| decl.transitions.push(tr)),
            };
            if r.is_err() {
                // Recover inside the animation block: skip to the next
                // keyframes/transition/}/EOF.
                loop {
                    if self.gave_up {
                        self.skip_to_eof();
                        return Err(());
                    }
                    match self.peek() {
                        TokKind::Eof | TokKind::RBrace => break,
                        TokKind::Ident(w) if w == "keyframes" || w == "transition" => break,
                        _ => {
                            self.bump();
                        }
                    }
                }
            }
        }
        app.animation = Some(decl);
        Ok(())
    }

    /// `keyframes "name" { from {...} to {...} step N {...} duration <e>
    /// delay <e> easing <e> }`.
    fn keyframes_decl(&mut self) -> PResult<KeyframesDecl> {
        let kw = self.bump(); // "keyframes"
        let name = self.string_after("keyframes")?;
        self.expect(TokKind::LBrace, "'{'")?;
        let mut steps = Vec::new();
        let mut duration = None;
        let mut delay = None;
        let mut easing = None;
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            let t = self.peek_token().clone();
            match &t.kind {
                TokKind::Ident(w) if w == "from" => {
                    self.bump();
                    let props = self.anim_props()?;
                    steps.push(KeyframeStep { at: StepAt::From, props, line: t.line, col: t.col });
                }
                TokKind::Ident(w) if w == "to" => {
                    self.bump();
                    let props = self.anim_props()?;
                    steps.push(KeyframeStep { at: StepAt::To, props, line: t.line, col: t.col });
                }
                TokKind::Ident(w) if w == "step" => {
                    self.bump();
                    let at = self.view_expr("a step percentage")?;
                    let props = self.anim_props()?;
                    steps.push(KeyframeStep { at: StepAt::Step(at), props, line: t.line, col: t.col });
                }
                // NOTE: `from`/`to`/`step` bodies self-recover inside
                // anim_props; a failing view_expr above still propagates
                // (the step header itself is broken), which the E2037
                // recovery below then contains.
                TokKind::Ident(w) if w == "duration" => {
                    self.bump();
                    duration = Some(self.view_expr("a duration in milliseconds")?);
                }
                TokKind::Ident(w) if w == "delay" => {
                    self.bump();
                    delay = Some(self.view_expr("a delay in milliseconds")?);
                }
                TokKind::Ident(w) if w == "easing" => {
                    self.bump();
                    easing = Some(self.view_expr("an easing (linear, ease, ease_in, ease_out, ease_in_out, cubic(...))")?);
                }
                other => {
                    self.fail(
                        "E2037",
                        format!(
                            "expected a keyframe step (`from`, `to`, `step N`) or an option (`duration`, `delay`, `easing`) but found {}",
                            describe(other)
                        ),
                        t.line,
                        t.col,
                        t.len().max(1),
                    );
                    // Recover inside the keyframes block: skip to the
                    // next step/option word or the block's own `}` (a
                    // brace entered here belongs to the malformed step).
                    let mut depth = 0usize;
                    loop {
                        if self.gave_up {
                            self.skip_to_eof();
                            return Err(());
                        }
                        match self.peek() {
                            TokKind::Eof => return Err(()),
                            TokKind::LBrace => {
                                depth += 1;
                                self.bump();
                            }
                            TokKind::RBrace => {
                                if depth == 0 {
                                    break;
                                }
                                depth -= 1;
                                self.bump();
                            }
                            TokKind::Ident(w)
                                if depth == 0
                                    && matches!(
                                        w.as_str(),
                                        "from" | "to" | "step" | "duration" | "delay" | "easing"
                                    ) =>
                            {
                                break;
                            }
                            _ => {
                                self.bump();
                            }
                        }
                    }
                }
            }
        }
        Ok(KeyframesDecl { name, steps, duration, delay, easing, line: kw.line, col: kw.col })
    }

    /// `{ opacity 0 scale 0.96 }` — keyframe step body.
    fn anim_props(&mut self) -> PResult<Vec<AnimProp>> {
        self.expect(TokKind::LBrace, "'{'")?;
        let mut props = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                return Ok(props);
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            let t = match self.expect_ident() {
                Ok(t) => t,
                Err(()) => {
                    if !self.recover_flat_item() {
                        return Err(());
                    }
                    continue;
                }
            };
            let name = match t.kind {
                TokKind::Ident(n) => n,
                _ => unreachable!("expect_ident guarantees an identifier"),
            };
            match self.view_expr("an animation property value") {
                Ok(value) => props.push(AnimProp { name, value, line: t.line, col: t.col }),
                Err(()) => {
                    if !self.recover_flat_item() {
                        return Err(());
                    }
                }
            }
        }
    }

    /// `transition "name" { duration <e> delay <e> easing <e> }`.
    fn transition_decl(&mut self) -> PResult<TransitionDecl> {
        let kw = self.bump(); // "transition"
        let name = self.string_after("transition")?;
        self.expect(TokKind::LBrace, "'{'")?;
        let mut duration = None;
        let mut delay = None;
        let mut easing = None;
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            let t = self.peek_token().clone();
            match &t.kind {
                TokKind::Ident(w) if w == "duration" => {
                    self.bump();
                    duration = Some(self.view_expr("a duration in milliseconds")?);
                }
                TokKind::Ident(w) if w == "delay" => {
                    self.bump();
                    delay = Some(self.view_expr("a delay in milliseconds")?);
                }
                TokKind::Ident(w) if w == "easing" => {
                    self.bump();
                    easing = Some(self.view_expr("an easing (linear, ease, ease_in, ease_out, ease_in_out, cubic(...))")?);
                }
                other => {
                    self.fail(
                        "E2037",
                        format!(
                            "expected a transition option (`duration`, `delay`, `easing`) but found {}",
                            describe(other)
                        ),
                        t.line,
                        t.col,
                        t.len().max(1),
                    );
                    if !self.recover_flat_item() {
                        return Err(());
                    }
                }
            }
        }
        Ok(TransitionDecl { name, duration, delay, easing, line: kw.line, col: kw.col })
    }

    /// A quoted animation/transition name after `enter`/`exit`/`hover`/
    /// `press`/`scroll`/`transition` (E2037 when not a string).
    fn anim_ref_after(&mut self, word: &str) -> PResult<AnimRef> {
        let t = self.peek_token().clone();
        match t.kind.clone() {
            TokKind::Str(name) => {
                self.bump();
                Ok(AnimRef { name, line: t.line, col: t.col })
            }
            other => {
                self.fail(
                    "E2037",
                    format!(
                        "expected a quoted animation name after `{}` (like {} \"fade\") but found {}",
                        word,
                        word,
                        describe(&other)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                Err(())
            }
        }
    }

    /// `store "name" { key, key }` — persisted state keys.
    fn app_store(&mut self, app: &mut AppDecl) -> PResult<()> {
        let kw = self.bump(); // "store"
        let name = self.string_after("store")?;
        self.expect(TokKind::LBrace, "'{'")?;
        let mut keys = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            // Commas optional between keys (a trailing comma is fine).
            if self.at(TokKind::Comma) {
                self.bump();
                continue;
            }
            match self.expect_ident() {
                Ok(t) => {
                    if let TokKind::Ident(k) = t.kind {
                        keys.push(k);
                    }
                }
                Err(()) => {
                    if !self.recover_flat_item() {
                        return Err(());
                    }
                }
            }
        }
        app.stores.push(StoreDecl { name, keys, line: kw.line, col: kw.col });
        Ok(())
    }

    /// `route "path/:param" -> Page`.
    fn app_route(&mut self, app: &mut AppDecl) -> PResult<()> {
        let kw = self.bump(); // "route"
        let path = self.string_after("route")?;
        self.expect(TokKind::Arrow, "`->`")?;
        let t = self.expect_ident()?;
        let target = match t.kind {
            TokKind::Ident(n) => OpenTarget { page: n, line: t.line, col: t.col },
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        app.routes.push(RouteDecl { path, target, line: kw.line, col: kw.col });
        Ok(())
    }

    /// `page Name { view elements }`.
    fn app_page(&mut self, app: &mut AppDecl) -> PResult<()> {
        let kw = self.bump(); // "page"
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect(TokKind::LBrace, "'{'")?;
        let elements = self.view_elements()?;
        app.pages.push(AppPageDecl { name, elements, line: kw.line, col: kw.col });
        Ok(())
    }

    /// `component Name { prop... state... view {...} }`.
    fn app_component(&mut self, app: &mut AppDecl) -> PResult<()> {
        let kw = self.bump(); // "component"
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect(TokKind::LBrace, "'{'")?;
        let mut props = Vec::new();
        let mut state = Vec::new();
        let mut view: Option<Vec<ViewElement>> = None;
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                break;
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            let t = self.peek_token().clone();
            let word = match &t.kind {
                TokKind::Ident(w) => w.clone(),
                other => {
                    self.fail(
                        "E2001",
                        format!(
                            "expected a component section (`prop`, `state`, `view`) but found {}",
                            describe(other)
                        ),
                        t.line,
                        t.col,
                        t.len().max(1),
                    );
                    return Err(());
                }
            };
            let r = match word.as_str() {
                "prop" => self.component_prop(&mut props),
                // Component state is one declaration per line (B.3),
                // exactly like `prop` — only the app-level `state` is a
                // `{ ... }` block.
                "state" => self.component_state_field(&mut state),
                "view" => {
                    self.bump();
                    self.expect(TokKind::LBrace, "'{'")?;
                    let elems = self.view_elements()?;
                    view = Some(elems);
                    Ok(())
                }
                _ => {
                    self.fail(
                        "E2001",
                        format!(
                            "unknown component section `{}` (supported: prop, state, view)",
                            word
                        ),
                        t.line,
                        t.col,
                        t.len(),
                    );
                    Err(())
                }
            };
            if r.is_err() {
                // Recover to the next component section / `}` / EOF.
                // Prop and state field initializers are expressions (no
                // braces), and `view` bodies self-recover, so any `}` at
                // this level closes the component itself.
                loop {
                    if self.gave_up {
                        self.skip_to_eof();
                        return Err(());
                    }
                    match self.peek() {
                        TokKind::Eof | TokKind::RBrace => break,
                        TokKind::Ident(w) if w == "prop" || w == "state" || w == "view" => break,
                        _ => {
                            self.bump();
                        }
                    }
                }
            }
        }
        app.components.push(ComponentDecl {
            name,
            props,
            state,
            view: view.unwrap_or_default(),
            line: kw.line,
            col: kw.col,
        });
        Ok(())
    }

    /// `state name: Type = expr` — one component state field (B.3:
    /// component state is written like `prop`, not as a block).
    fn component_state_field(&mut self, state: &mut Vec<StateField>) -> PResult<()> {
        self.bump(); // "state"
        let f = self.state_field("state")?;
        state.push(f);
        Ok(())
    }

    /// `prop name: Type [= expr]`.
    fn component_prop(&mut self, props: &mut Vec<PropDecl>) -> PResult<()> {
        let kw = self.bump(); // "prop"
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        if !self.at(TokKind::Colon) {
            let t = self.peek_token().clone();
            self.fail(
                "E2003",
                format!(
                    "a prop must declare its type (like `prop {}: int`) but found {}",
                    name,
                    describe(&t.kind)
                ),
                t.line,
                t.col,
                t.len().max(1),
            );
            return Err(());
        }
        self.bump();
        let ty = self.parse_type()?;
        let default = if self.at(TokKind::Assign) {
            self.bump();
            Some(self.view_expr("a prop default value")?)
        } else {
            None
        };
        props.push(PropDecl { name, ty, default, line: kw.line, col: kw.col });
        Ok(())
    }

    /// Parse view elements until the closing `}` (which is consumed).
    /// Statement keywords get a view-specific message: logic belongs in
    /// `on` handlers, not in views.
    fn view_elements(&mut self) -> PResult<Vec<ViewElement>> {
        let mut elems = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                return Ok(elems);
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            match self.view_element() {
                Ok(e) => elems.push(e),
                Err(()) => self.recover_view_element(),
            }
        }
    }

    /// Skip to the next view element, `}` or EOF after a broken element.
    /// Brace-aware (same pattern as [`recover_app_section`]): a nested
    /// block's `}` closes that block instead of being mistaken for the
    /// enclosing view's `}`, so one broken element inside a `when`/
    /// `each`/`form` does not truncate the whole view.
    fn recover_view_element(&mut self) {
        let mut depth = 0usize;
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return;
            }
            match self.peek() {
                TokKind::Eof => return,
                TokKind::LBrace => {
                    depth += 1;
                    self.bump();
                }
                TokKind::RBrace => {
                    if depth == 0 {
                        // Closes the enclosing view: leave it.
                        return;
                    }
                    depth -= 1;
                    self.bump();
                }
                TokKind::Ident(w) => {
                    if depth == 0
                        && (Self::VIEW_ELEMENT_WORDS.contains(&w.as_str())
                            || Self::APP_SECTION_WORDS.contains(&w.as_str()))
                    {
                        return;
                    }
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// One view element (B.5–B.8). Dispatches on the leading word; a
    /// non-element identifier followed by `(` is a component
    /// instantiation.
    fn view_element(&mut self) -> PResult<ViewElement> {
        let t = self.peek_token().clone();
        let word = match t.kind.clone() {
            TokKind::Ident(w) => w,
            other => {
                self.fail(
                    "E2001",
                    format!(
                        "expected a view element ({}) but found {} — views contain elements, not statements; logic belongs in `on` handlers",
                        Self::VIEW_ELEMENT_WORDS.join(", "),
                        describe(&other)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
        };
        let line = t.line;
        let col = t.col;
        // The element word itself is consumed here, once for every case.
        self.bump();
        let kind = match word.as_str() {
            "title" => {
                let value = self.view_expr("the title text")?;
                let anim = self.element_anim()?;
                self.no_block_after(&word)?;
                ViewElementKind::Title { value, anim }
            }
            "heading" => {
                let value = self.view_expr("the heading text")?;
                let anim = self.element_anim()?;
                self.no_block_after(&word)?;
                ViewElementKind::Heading { value, anim }
            },
            "text" => {
                let value = self.view_expr("the text to show")?;
                let anim = self.element_anim()?;
                self.no_block_after(&word)?;
                ViewElementKind::Text { value, anim }
            }
            "divider" => {
                let anim = self.element_anim()?;
                self.no_block_after(&word)?;
                ViewElementKind::Divider { anim }
            }
            "button" | "link" => {
                let label = self.view_expr(&format!("the {} label", word))?;
                let open = self.open_target_after()?;
                let anim = self.element_anim()?;
                let handlers = self.handler_block(&["button", "link"])?;
                if word == "button" {
                    ViewElementKind::Button { label, open, anim, handlers }
                } else {
                    ViewElementKind::Link { label, open, anim, handlers }
                }
            }
            "image" => {
                let src = self.view_expr("the image source")?;
                let alt_tok = self.peek_token().clone();
                match alt_tok.kind.clone() {
                    TokKind::Ident(w) if w == "alt" => {
                        self.bump();
                        let alt = self.view_expr("the alt text")?;
                        let anim = self.element_anim()?;
                        let handlers = self.handler_block(&["image"])?;
                        ViewElementKind::Image { src, alt, anim, handlers }
                    }
                    other => {
                        self.fail(
                            "E2002",
                            format!(
                                "images need alt text for accessibility: expected `alt \"description\"` but found {}",
                                describe(&other)
                            ),
                            alt_tok.line,
                            alt_tok.col,
                            alt_tok.len().max(1),
                        );
                        return Err(());
                    }
                }
            }
            "input" => {
                let bind_tok = self.peek_token().clone();
                match bind_tok.kind.clone() {
                    TokKind::Ident(w) if w == "bind" => {
                        self.bump();
                        let key = self.expect_ident()?;
                        let bind = match key.kind {
                            TokKind::Ident(n) => OpenTarget { page: n, line: key.line, col: key.col },
                            _ => unreachable!("expect_ident guarantees an identifier"),
                        };
                        let anim = self.element_anim()?;
                        let handlers = self.handler_block(&["input"])?;
                        ViewElementKind::Input { bind, anim, handlers }
                    }
                    other => {
                        self.fail(
                            "E2039",
                            format!(
                                "an `input` binds to a state key: expected `input bind <key>` but found {}",
                                describe(&other)
                            ),
                            bind_tok.line,
                            bind_tok.col,
                            bind_tok.len().max(1),
                        );
                        return Err(());
                    }
                }
            }
            "when" => {
                let cond = self.view_expr("a `when` condition")?;
                self.expect(TokKind::LBrace, "'{'")?;
                let then_branch = self.view_elements()?;
                let mut else_branch = None;
                if matches!(self.peek(), TokKind::Ident(w) if w == "otherwise") {
                    self.bump();
                    self.expect(TokKind::LBrace, "'{'")?;
                    else_branch = Some(self.view_elements()?);
                }
                ViewElementKind::When { cond, then_branch, else_branch }
            }
            "each" => {
                let iter = self.view_expr("the list to iterate")?;
                if !self.at(TokKind::As) {
                    let as_tok = self.peek_token().clone();
                    self.fail(
                        "E2032",
                        format!(
                            "an `each` list needs a loop variable: expected `as <name>` but found {}",
                            describe(&as_tok.kind)
                        ),
                        as_tok.line,
                        as_tok.col,
                        as_tok.len().max(1),
                    );
                    return Err(());
                }
                self.bump(); // `as` keyword
                let var_tok = self.expect_ident()?;
                let var = match var_tok.kind {
                    TokKind::Ident(n) => n,
                    _ => unreachable!("expect_ident guarantees an identifier"),
                };
                let mut key = None;
                if matches!(self.peek(), TokKind::Ident(w) if w == "key") {
                    self.bump();
                    key = Some(self.view_expr("a key expression")?);
                }
                self.expect(TokKind::LBrace, "'{'")?;
                let body = self.view_elements()?;
                ViewElementKind::Each { iter, var, key, body }
            }
            "form" => {
                let legend = self.view_expr("the form legend")?;
                self.expect(TokKind::LBrace, "'{'")?;
                let mut body = Vec::new();
                let mut handlers = Vec::new();
                loop {
                    if self.gave_up {
                        self.skip_to_eof();
                        return Err(());
                    }
                    if self.at(TokKind::RBrace) {
                        self.bump();
                        break;
                    }
                    if self.at(TokKind::Eof) {
                        self.error_here("expected '}' but found end of file".to_string());
                        return Err(());
                    }
                    if matches!(self.peek(), TokKind::Ident(w) if w == "on") {
                        match self.on_event() {
                            Ok(h) => handlers.push(h),
                            Err(()) => {
                                // Recover inside the form: skip to the
                                // next element / `on` / `}` / EOF.
                                self.recover_view_element();
                            }
                        }
                        continue;
                    }
                    match self.view_element() {
                        Ok(e) => body.push(e),
                        Err(()) => self.recover_view_element(),
                    }
                }
                ViewElementKind::Form { legend, body, handlers }
            }
            "field" => {
                let key = self.expect_ident()?;
                let bind = match key.kind {
                    TokKind::Ident(n) => OpenTarget { page: n, line: key.line, col: key.col },
                    _ => unreachable!("expect_ident guarantees an identifier"),
                };
                let kind_tok = self.peek_token().clone();
                let kind = match &kind_tok.kind {
                    TokKind::Ident(w) if w == "kind" => {
                        self.bump();
                        let k = self.peek_token().clone();
                        match k.kind.clone() {
                            TokKind::Ident(word) => {
                                let fk = match word.as_str() {
                                    "text" => FieldKind::Text,
                                    "number" => FieldKind::Number,
                                    "date" => FieldKind::Date,
                                    "email" => FieldKind::Email,
                                    "password" => FieldKind::Password,
                                    "textarea" => FieldKind::Textarea,
                                    _ => {
                                        self.fail(
                                            "E2035",
                                            format!(
                                                "unknown field kind `{}` (supported: {})",
                                                word,
                                                FieldKind::ALL.join(", ")
                                            ),
                                            k.line,
                                            k.col,
                                            k.len(),
                                        );
                                        return Err(());
                                    }
                                };
                                self.bump();
                                fk
                            }
                            other => {
                                self.fail(
                                    "E2035",
                                    format!(
                                        "a field needs a kind (like `kind text`); expected one of {} but found {}",
                                        FieldKind::ALL.join(", "),
                                        describe(&other)
                                    ),
                                    k.line,
                                    k.col,
                                    k.len().max(1),
                                );
                                return Err(());
                            }
                        }
                    }
                    other => {
                        self.fail(
                            "E2039",
                            format!(
                                "a field needs a kind: expected `kind <{}>` but found {}",
                                FieldKind::ALL.join("|"),
                                describe(other)
                            ),
                            kind_tok.line,
                            kind_tok.col,
                            kind_tok.len().max(1),
                        );
                        return Err(());
                    }
                };
                let mut attrs = Vec::new();
                let mut handlers = Vec::new();
                if self.at(TokKind::LBrace) {
                    self.bump();
                    loop {
                        if self.gave_up {
                            self.skip_to_eof();
                            return Err(());
                        }
                        if self.at(TokKind::RBrace) {
                            self.bump();
                            break;
                        }
                        if self.at(TokKind::Eof) {
                            self.error_here("expected '}' but found end of file".to_string());
                            return Err(());
                        }
                        if matches!(self.peek(), TokKind::Ident(w) if w == "on") {
                            match self.on_event() {
                                Ok(h) => handlers.push(h),
                                Err(()) => self.recover_view_element(),
                            }
                            continue;
                        }
                        let at = self.peek_token().clone();
                        let attr_word = match &at.kind {
                            TokKind::Ident(w) => w.clone(),
                            other => {
                                self.fail(
                                    "E2036",
                                    format!(
                                        "expected a field attribute ({}) or an `on` handler but found {}",
                                        FIELD_ATTRS.join(", "),
                                        describe(other)
                                    ),
                                    at.line,
                                    at.col,
                                    at.len().max(1),
                                );
                                return Err(());
                            }
                        };
                        let attr = match attr_word.as_str() {
                            "label" => {
                                self.bump();
                                FieldAttr::Label(self.view_expr("a label")?)
                            }
                            "placeholder" => {
                                self.bump();
                                FieldAttr::Placeholder(self.view_expr("a placeholder")?)
                            }
                            "required" => {
                                self.bump();
                                FieldAttr::Required
                            }
                            "min_length" => {
                                self.bump();
                                FieldAttr::MinLength(self.view_expr("a minimum length")?)
                            }
                            "max_length" => {
                                self.bump();
                                FieldAttr::MaxLength(self.view_expr("a maximum length")?)
                            }
                            "min_value" => {
                                self.bump();
                                FieldAttr::MinValue(self.view_expr("a minimum value")?)
                            }
                            "max_value" => {
                                self.bump();
                                FieldAttr::MaxValue(self.view_expr("a maximum value")?)
                            }
                            "pattern" => {
                                self.bump();
                                FieldAttr::Pattern(self.view_expr("a pattern")?)
                            }
                            _ => {
                                self.fail(
                                    "E2036",
                                    format!(
                                        "unknown field attribute `{}` (supported: {})",
                                        attr_word,
                                        FIELD_ATTRS.join(", ")
                                    ),
                                    at.line,
                                    at.col,
                                    at.len(),
                                );
                                return Err(());
                            }
                        };
                        attrs.push(attr);
                    }
                }
                let anim = self.element_anim()?;
                ViewElementKind::Field { bind, kind, attrs, anim, handlers }
            }
            _ => {
                // Not a known element word — the word itself is already
                // consumed (central bump above). A `(` here (or
                // `.Name(`) makes it a component instantiation;
                // anything else is an unknown element.
                let is_qualified = self.at(TokKind::Dot)
                    && matches!(
                        self.toks.get(self.pos + 1).map(|t| &t.kind),
                        Some(TokKind::Ident(_))
                    )
                    && matches!(self.toks.get(self.pos + 2).map(|t| &t.kind), Some(TokKind::LParen));
                if self.at(TokKind::LParen) || is_qualified {
                    return self.component_instantiation(word, line, col);
                }
                self.fail(
                    "E2030",
                    format!(
                        "unknown view element `{}` (supported: {} — or a component instantiation like MyCard(title: \"Hi\"))",
                        word,
                        Self::VIEW_ELEMENT_WORDS.join(", ")
                    ),
                    t.line,
                    t.col,
                    t.len(),
                );
                if word == "otherwise" {
                    self.diags.last_mut().unwrap().hint =
                        Some("`otherwise` follows a `when` block".into());
                }
                return Err(());
            }
        };
        Ok(ViewElement { kind, line, col })
    }

    /// After `title`/`heading`/`text`/`divider`: a `{` here is an event
    /// block on an element that cannot take one (E2038) — a precise
    /// message beats the generic "expected a view element but found `{`".
    fn no_block_after(&mut self, word: &str) -> PResult<()> {
        if self.at(TokKind::LBrace) {
            let t = self.peek_token().clone();
            self.fail(
                "E2038",
                format!(
                    "a `{}` element does not take an event block (button, link, image, input, field, form and component instantiations do)",
                    word
                ),
                t.line,
                t.col,
                1,
            );
            self.diags.last_mut().unwrap().hint =
                Some("put logic in `on <event> { ... }` on an interactive element".into());
            return Err(());
        }
        Ok(())
    }

    /// `Name(prop: expr, ...)` or `alias.Name(prop: expr, ...)` — a
    /// component instantiation inside a view. The component name is
    /// already consumed by the caller (the central element bump); the
    /// parser sits on `(` or `.`.
    fn component_instantiation(&mut self, name: String, line: u32, col: u32) -> PResult<ViewElement> {
        // Unqualified `Name(...)`: `name` is the component. Qualified
        // `alias.Name(...)`: `name` is the module alias and the ident
        // after the dot is the component.
        let (module, comp_name) = if self.at(TokKind::Dot) {
            self.bump();
            let c = self.expect_ident()?;
            match c.kind {
                TokKind::Ident(n) => (Some(name.clone()), n),
                _ => unreachable!("expect_ident guarantees an identifier"),
            }
        } else {
            (None, name)
        };
        if !self.at(TokKind::LParen) {
            let t = self.peek_token().clone();
            self.fail(
                "E2032",
                format!(
                    "a component instantiation opens its props right after the name (like {}(title: \"Hi\")) but found {}",
                    comp_name,
                    describe(&t.kind)
                ),
                t.line,
                t.col,
                t.len().max(1),
            );
            return Err(());
        }
        self.bump();
        let mut args = Vec::new();
        if !self.at(TokKind::RParen) {
            loop {
                if self.gave_up {
                    self.skip_to_eof();
                    return Err(());
                }
                let a = self.peek_token().clone();
                let arg_name = match a.kind.clone() {
                    TokKind::Ident(n) => n,
                    other => {
                        self.fail(
                            "E2032",
                            format!(
                                "component props are passed by name (like `{}(label: value)`) but found {}",
                                comp_name,
                                describe(&other)
                            ),
                            a.line,
                            a.col,
                            a.len().max(1),
                        );
                        return Err(());
                    }
                };
                self.bump();
                self.expect(TokKind::Colon, "`:` (props are passed by name)")?;
                let value = self.view_expr("a prop value")?;
                args.push(PropArg { name: arg_name, value, line: a.line, col: a.col });
                if !self.at(TokKind::Comma) {
                    break;
                }
                self.bump();
                if self.at(TokKind::RParen) {
                    // Trailing comma.
                    break;
                }
            }
        }
        self.expect(TokKind::RParen, "')'")?;
        let anim = self.element_anim()?;
        let handlers = self.handler_block(&["component"])?;
        Ok(ViewElement {
            kind: ViewElementKind::Component { module, name: comp_name, args, anim, handlers },
            line,
            col,
        })
    }

    /// `open <Page>` on buttons/links (optional).
    fn open_target_after(&mut self) -> PResult<Option<OpenTarget>> {
        if matches!(self.peek(), TokKind::Ident(w) if w == "open") {
            self.bump();
            let t = self.expect_ident()?;
            if let TokKind::Ident(n) = t.kind {
                return Ok(Some(OpenTarget { page: n, line: t.line, col: t.col }));
            }
        }
        Ok(None)
    }

    /// Animation attributes after an element's arguments: `enter "..."`,
    /// `exit "..."`, `hover "..."`, `press "..."`, `scroll "..."`.
    fn element_anim(&mut self) -> PResult<ElementAnim> {
        let mut anim = ElementAnim::default();
        loop {
            let t = self.peek_token().clone();
            let word = match &t.kind {
                TokKind::Ident(w)
                    if matches!(w.as_str(), "enter" | "exit" | "hover" | "press" | "scroll") =>
                {
                    w.clone()
                }
                _ => return Ok(anim),
            };
            self.bump();
            let r = self.anim_ref_after(&word)?;
            match word.as_str() {
                "enter" => anim.enter = Some(r),
                "exit" => anim.exit = Some(r),
                "hover" => anim.hover = Some(r),
                "press" => anim.press = Some(r),
                _ => anim.scroll = Some(r),
            }
        }
    }

    /// `{ on <event> { ... } ... }` — the handler block of an interactive
    /// element. Only `on` clauses are allowed inside (E2038 otherwise);
    /// the block itself is optional.
    fn handler_block(&mut self, interactive: &[&str]) -> PResult<Vec<OnEvent>> {
        if !self.at(TokKind::LBrace) {
            return Ok(Vec::new());
        }
        let brace = self.peek_token().clone();
        self.bump();
        let mut handlers = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                return Ok(handlers);
            }
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            if matches!(self.peek(), TokKind::Ident(w) if w == "on") {
                match self.on_event() {
                    Ok(h) => handlers.push(h),
                    Err(()) => {
                        // Recover to the next `on`, `}` or EOF.
                        loop {
                            if self.gave_up {
                                self.skip_to_eof();
                                return Err(());
                            }
                            match self.peek() {
                                TokKind::Eof | TokKind::RBrace => break,
                                TokKind::Ident(w) if w == "on" => break,
                                _ => {
                                    self.bump();
                                }
                            }
                        }
                    }
                }
                continue;
            }
            let t = self.peek_token().clone();
            self.fail(
                "E2038",
                format!(
                    "only `on` event handlers may appear in this block ({} element{}), not {}",
                    interactive.join("/"),
                    if interactive.len() == 1 { "" } else { "s" },
                    describe(&t.kind)
                ),
                brace.line,
                brace.col,
                1,
            );
            self.diags.last_mut().unwrap().hint = Some(
                "put content elements in a `form` block; put logic in `on <event> { ... }`".into(),
            );
            return Err(());
        }
    }

    /// `on <event> [key "..."] { statements }` — one handler. The body
    /// reuses the ordinary statement parser (B.6).
    fn on_event(&mut self) -> PResult<OnEvent> {
        let kw = self.bump(); // "on"
        let t = self.peek_token().clone();
        let event = match &t.kind {
            TokKind::Ident(w) => {
                let ev = match w.as_str() {
                    "click" => EventKind::Click,
                    "input" => EventKind::Input,
                    "change" => EventKind::Change,
                    "submit" => EventKind::Submit,
                    "focus" => EventKind::Focus,
                    "blur" => EventKind::Blur,
                    "key" => EventKind::Key,
                    "keydown" => EventKind::Keydown,
                    "keyup" => EventKind::Keyup,
                    "pointerdown" => EventKind::Pointerdown,
                    "pointerup" => EventKind::Pointerup,
                    _ => {
                        self.fail(
                            "E2031",
                            format!(
                                "unknown event `{}` in `on` (supported: {})",
                                w,
                                EventKind::ALL.join(", ")
                            ),
                            t.line,
                            t.col,
                            t.len(),
                        );
                        return Err(());
                    }
                };
                self.bump();
                ev
            }
            other => {
                self.fail(
                    "E2031",
                    format!(
                        "expected an event name after `on` (one of {}) but found {}",
                        EventKind::ALL.join(", "),
                        describe(other)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
        };
        // `on key "Escape"` — required key filter for the sugar form,
        // optional for keydown/keyup, invalid elsewhere (E2039).
        let mut key_filter = None;
        if matches!(self.peek(), TokKind::Str(_)) {
            let needs_filter = matches!(event, EventKind::Key | EventKind::Keydown | EventKind::Keyup);
            if !needs_filter {
                let s = self.peek_token().clone();
                self.fail(
                    "E2039",
                    format!(
                        "only keyboard events take a key filter (write `on key \"Escape\" {{ ... }}`), not `on {}`",
                        event.as_str()
                    ),
                    s.line,
                    s.col,
                    s.len().max(1),
                );
                return Err(());
            }
            if let TokKind::Str(k) = self.peek_token().kind.clone() {
                self.bump();
                key_filter = Some(k);
            }
        } else if event == EventKind::Key {
            let s = self.peek_token().clone();
            self.fail(
                "E2039",
                format!(
                    "`on key` needs a quoted key filter (like `on key \"Escape\" {{ ... }}`) but found {}",
                    describe(&s.kind)
                ),
                s.line,
                s.col,
                s.len().max(1),
            );
            return Err(());
        }
        let body = self.block()?;
        Ok(OnEvent { event, key_filter, body, line: kw.line, col: kw.col })
    }

    /// Parse a dynamic view argument (a string literal is one kind of
    /// expression). Gives the positioned E2033 message when the next
    /// token cannot start an expression.
    fn view_expr(&mut self, what: &str) -> PResult<Expr> {
        if !expr_can_start(self.peek()) {
            let t = self.peek_token().clone();
            self.fail(
                "E2033",
                format!(
                    "expected {} as a string or an expression but found {}",
                    what,
                    describe(&t.kind)
                ),
                t.line,
                t.col,
                t.len().max(1),
            );
            return Err(());
        }
        self.expr()
    }

    fn mode_directive(&mut self) -> PResult<Mode> {
        let star = self.bump();
        let name = self.expect_ident()?;
        if !matches!(&name.kind, TokKind::Ident(n) if n == "mode") {
            self.fail(
                "E2010",
                "expected `mode` after `*` in the mode directive",
                star.line,
                star.col,
                1,
            );
            return Err(());
        }
        self.expect(TokKind::Assign, "`=`")?;
        let value = self.expect_ident()?;
        let mode = match &value.kind {
            TokKind::Ident(v) if v == "B" => Mode::B,
            TokKind::Ident(v) if v == "S" => Mode::S,
            TokKind::Ident(v) if v == "A" => Mode::A,
            TokKind::Ident(v) => {
                self.fail(
                    "E2011",
                    format!("invalid mode `{}` (expected `B`, `S` or `A`)", v),
                    value.line,
                    value.col,
                    value.len(),
                );
                return Err(());
            }
            _ => {
                self.fail(
                    "E2011",
                    format!(
                        "invalid mode (expected `B`, `S` or `A`) but found {}",
                        describe(&value.kind)
                    ),
                    value.line,
                    value.col,
                    value.len().max(1),
                );
                return Err(());
            }
        };
        self.expect(TokKind::Semi, "';'")?;
        Ok(mode)
    }

    /// `import "path.kin" as alias;`
    fn import_decl(&mut self) -> PResult<ImportDecl> {
        let kw = self.bump(); // Import
        let path_tok = self.peek_token().clone();
        let path = match path_tok.kind.clone() {
            TokKind::Str(s) => {
                self.bump();
                s
            }
            other => {
                self.fail(
                    "E2015",
                    format!(
                        "expected a string path after `import` (like `import \"lib.kin\" as lib;`) but found {}",
                        describe(&other)
                    ),
                    path_tok.line,
                    path_tok.col,
                    path_tok.len().max(1),
                );
                return Err(());
            }
        };
        self.expect(TokKind::As, "`as`")?;
        let alias = self.expect_ident()?;
        let alias = match alias.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect(TokKind::Semi, "';'")?;
        Ok(ImportDecl { path, alias, line: kw.line, col: kw.col })
    }

    fn func(&mut self) -> PResult<Func> {
        let func_tok = self.expect(TokKind::Func, "`func`")?;
        let name = self.expect_ident()?;
        let name = match name.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect(TokKind::LParen, "'('")?;

        let mut params = Vec::new();
        if !self.at(TokKind::RParen) {
            loop {
                let pt = self.expect_ident()?;
                let pname = match pt.kind {
                    TokKind::Ident(n) => n,
                    _ => unreachable!(),
                };
                self.expect(TokKind::Colon, "':'")?;
                let ty = self.parse_type()?;
                params.push(Param { name: pname, ty, line: pt.line, col: pt.col });
                if !self.at(TokKind::Comma) {
                    break;
                }
                self.bump();
            }
        }
        self.expect(TokKind::RParen, "')'")?;

        let ret = if self.at(TokKind::Arrow) {
            self.bump();
            self.parse_type()?
        } else {
            Type::Void
        };

        let body = self.block()?;
        Ok(Func { name, params, ret, body, line: func_tok.line, col: func_tok.col })
    }

    fn parse_type(&mut self) -> PResult<Type> {
        let t = self.peek_token().clone();
        let ty = match t.kind {
            TokKind::IntTy => {
                self.bump();
                Type::Int
            }
            TokKind::FloatTy => {
                self.bump();
                Type::Float
            }
            TokKind::BoolTy => {
                self.bump();
                Type::Bool
            }
            TokKind::StrTy => {
                self.bump();
                Type::Str
            }
            TokKind::VoidTy => {
                self.bump();
                Type::Void
            }
            TokKind::LBracket => {
                self.bump();
                let inner = self.parse_type()?;
                self.expect(TokKind::RBracket, "']'")?;
                Type::Array(Box::new(inner))
            }
            _ => {
                self.fail(
                    "E2003",
                    format!(
                        "expected a type (`int`, `float`, `bool`, `string`, `void` or `[T]`) but found {}",
                        describe(&t.kind)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
        };
        Ok(ty)
    }

    // -- statements ----------------------------------------------------------

    fn block(&mut self) -> PResult<Block> {
        self.expect(TokKind::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                return Ok(Block { stmts });
            }
            match self.peek() {
                TokKind::Eof => {
                    self.error_here("expected '}' but found end of file".to_string());
                    return Err(());
                }
                TokKind::Func | TokKind::Import => {
                    // A top-level item inside a block: the block was never
                    // closed. Point at it instead of mis-parsing it.
                    self.error_here(
                        "expected `}` before the next top-level declaration".to_string(),
                    );
                    return Err(());
                }
                _ => {}
            }
            match self.stmt() {
                Ok(s) => stmts.push(s),
                Err(()) => self.recover_stmt(),
            }
        }
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        let t = self.peek_token().clone();
        match &t.kind {
            TokKind::Let | TokKind::Var => self.var_decl(),
            TokKind::If => self.if_stmt(),
            TokKind::While => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                Ok(Stmt::While { cond, body, line: t.line, col: t.col })
            }
            TokKind::For => self.for_stmt(),
            TokKind::Return => {
                self.bump();
                let value = if self.at(TokKind::Semi) { None } else { Some(self.expr()?) };
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Return { value, line: t.line, col: t.col })
            }
            TokKind::Break => {
                self.bump();
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Break { line: t.line, col: t.col })
            }
            TokKind::Continue => {
                self.bump();
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Continue { line: t.line, col: t.col })
            }
            TokKind::LBrace => {
                let block = self.block()?;
                Ok(Stmt::Block { block, line: t.line, col: t.col })
            }
            TokKind::Ident(_)
                if matches!(
                    self.peek2_kind(),
                    TokKind::Assign | TokKind::PlusEq | TokKind::MinusEq
                ) =>
            {
                let name = match self.bump().kind {
                    TokKind::Ident(n) => n,
                    _ => unreachable!(),
                };
                let op_tok = self.bump();
                let op = match op_tok.kind {
                    TokKind::Assign => AssignOp::Set,
                    TokKind::PlusEq => AssignOp::Add,
                    TokKind::MinusEq => AssignOp::Sub,
                    _ => unreachable!(),
                };
                let value = self.expr()?;
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Assign { name, op, value, line: t.line, col: t.col })
            }
            _ => {
                // Expression statement, or an assignment whose target is
                // an indexing expression (`a[i] = v;`).
                let e = self.expr()?;
                if matches!(self.peek(), TokKind::Assign | TokKind::PlusEq | TokKind::MinusEq) {
                    let op_tok = self.bump();
                    let value = self.expr()?;
                    self.expect(TokKind::Semi, "';'")?;
                    let line = t.line;
                    let col = t.col;
                    return match (e.kind, op_tok.kind) {
                        (ExprKind::Ident(name), TokKind::Assign) => Ok(Stmt::Assign {
                            name,
                            op: AssignOp::Set,
                            value,
                            line,
                            col,
                        }),
                        (ExprKind::Ident(name), TokKind::PlusEq) => Ok(Stmt::Assign {
                            name,
                            op: AssignOp::Add,
                            value,
                            line,
                            col,
                        }),
                        (ExprKind::Ident(name), TokKind::MinusEq) => Ok(Stmt::Assign {
                            name,
                            op: AssignOp::Sub,
                            value,
                            line,
                            col,
                        }),
                        (ExprKind::Index { base, index }, TokKind::Assign) => {
                            Ok(Stmt::IndexAssign {
                                base: *base,
                                index: *index,
                                value,
                                line,
                                col,
                            })
                        }
                        (ExprKind::Index { .. }, _) => {
                            self.fail(
                                "E2014",
                                "compound assignment (`+=`, `-=`) to an array element is not supported; use `a[i] = a[i] + n;`",
                                line,
                                col,
                                1,
                            );
                            Err(())
                        }
                        _ => {
                            self.fail(
                                "E2013",
                                "the left side of an assignment must be a variable or an array element",
                                line,
                                col,
                                1,
                            );
                            Err(())
                        }
                    };
                }
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Expr { expr: e, line: t.line, col: t.col })
            }
        }
    }

    fn var_decl(&mut self) -> PResult<Stmt> {
        let kw = self.bump(); // Let or Var
        let mutable = kw.kind == TokKind::Var;
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!(),
        };
        let ty = if self.at(TokKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokKind::Assign, "`=`")?;
        let init = self.expr()?;
        self.expect(TokKind::Semi, "';'")?;
        Ok(Stmt::VarDecl {
            mutable,
            name,
            ty,
            init,
            line: kw.line,
            col: kw.col,
        })
    }

    fn if_stmt(&mut self) -> PResult<Stmt> {
        let if_tok = self.bump(); // If
        let cond = self.expr()?;
        let then_branch = self.block()?;
        let else_branch = if self.at(TokKind::Else) {
            self.bump();
            if self.at(TokKind::If) {
                Some(Box::new(self.if_stmt()?))
            } else {
                let lbrace = self.peek_token().clone();
                let block = self.block()?;
                Some(Box::new(Stmt::Block {
                    block,
                    line: lbrace.line,
                    col: lbrace.col,
                }))
            }
        } else {
            None
        };
        Ok(Stmt::If { cond, then_branch, else_branch, line: if_tok.line, col: if_tok.col })
    }

    /// `for name in iterable { ... }` where the iterable is a range
    /// (`a..b`, `a..=b`) or an array expression.
    fn for_stmt(&mut self) -> PResult<Stmt> {
        let for_tok = self.bump(); // For
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!(),
        };
        self.expect(TokKind::In, "`in`")?;
        let iter = self.for_iterable()?;
        let body = self.block()?;
        Ok(Stmt::For { var: name, iter, body, line: for_tok.line, col: for_tok.col })
    }

    fn for_iterable(&mut self) -> PResult<ForIter> {
        let start = self.expr()?;
        if self.at(TokKind::DotDot) {
            self.bump();
            let end = self.expr()?;
            Ok(ForIter::Range { start, end, inclusive: false })
        } else if self.at(TokKind::DotDotEq) {
            self.bump();
            let end = self.expr()?;
            Ok(ForIter::Range { start, end, inclusive: true })
        } else {
            Ok(ForIter::Expr(start))
        }
    }

    // -- expressions ---------------------------------------------------------

    fn expr(&mut self) -> PResult<Expr> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.and_expr()?;
        while self.at(TokKind::Or) {
            let op_tok = self.bump();
            let rhs = self.and_expr()?;
            lhs = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                line: op_tok.line,
                col: op_tok.col,
            };
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.cmp_expr()?;
        while self.at(TokKind::And) {
            let op_tok = self.bump();
            let rhs = self.cmp_expr()?;
            lhs = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::And,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                line: op_tok.line,
                col: op_tok.col,
            };
        }
        Ok(lhs)
    }

    fn cmp_expr(&mut self) -> PResult<Expr> {
        let lhs = self.add_expr()?;
        let op = match self.peek() {
            TokKind::Lt => Some(BinOp::Lt),
            TokKind::Gt => Some(BinOp::Gt),
            TokKind::Le => Some(BinOp::Le),
            TokKind::Ge => Some(BinOp::Ge),
            TokKind::Eq => Some(BinOp::Eq),
            TokKind::NotEq => Some(BinOp::Ne),
            _ => None,
        };
        if let Some(op) = op {
            let op_tok = self.bump();
            let rhs = self.add_expr()?;
            if matches!(
                self.peek(),
                TokKind::Lt | TokKind::Gt | TokKind::Le | TokKind::Ge | TokKind::Eq | TokKind::NotEq
            ) {
                let t = self.peek_token().clone();
                self.fail(
                    "E2004",
                    "chained comparisons are not allowed; use parentheses or boolean operators",
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
            return Ok(Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: op_tok.line,
                col: op_tok.col,
            });
        }
        Ok(lhs)
    }

    fn add_expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.mul_expr()?;
        loop {
            let op = match self.peek() {
                TokKind::Plus => BinOp::Add,
                TokKind::Minus => BinOp::Sub,
                _ => break,
            };
            let op_tok = self.bump();
            let rhs = self.mul_expr()?;
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: op_tok.line,
                col: op_tok.col,
            };
        }
        Ok(lhs)
    }

    fn mul_expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.unary_expr()?;
        loop {
            let op = match self.peek() {
                TokKind::Star => BinOp::Mul,
                TokKind::Slash => BinOp::Div,
                TokKind::Percent => BinOp::Mod,
                _ => break,
            };
            let op_tok = self.bump();
            let rhs = self.unary_expr()?;
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: op_tok.line,
                col: op_tok.col,
            };
        }
        Ok(lhs)
    }

    fn unary_expr(&mut self) -> PResult<Expr> {
        let t = self.peek_token().clone();
        match t.kind {
            TokKind::Not => {
                self.bump();
                let inner = self.unary_expr()?;
                Ok(Expr {
                    kind: ExprKind::Unary { op: UnOp::Not, expr: Box::new(inner) },
                    line: t.line,
                    col: t.col,
                })
            }
            TokKind::Minus => {
                self.bump();
                let inner = self.unary_expr()?;
                Ok(Expr {
                    kind: ExprKind::Unary { op: UnOp::Neg, expr: Box::new(inner) },
                    line: t.line,
                    col: t.col,
                })
            }
            _ => self.cast_expr(),
        }
    }

    /// `expr as type` — binds tighter than binary operators, looser than
    /// unary (`-x as int` parses as `-(x as int)`, like Rust).
    fn cast_expr(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        while self.at(TokKind::As) && type_can_follow_as(self.peek2_kind()) {
            // `as` is only a cast when a real type follows it. An `as`
            // followed by an identifier can never be a cast (types are
            // keywords) — it is the loop-variable marker of
            // `each <list> as <name>`, so the expression ends here and
            // the `each` parser consumes the `as` itself (0.4.0 S2).
            let as_tok = self.bump();
            let ty = self.parse_type()?;
            e = Expr {
                kind: ExprKind::Cast { expr: Box::new(e), ty },
                line: as_tok.line,
                col: as_tok.col,
            };
        }
        Ok(e)
    }

    fn primary(&mut self) -> PResult<Expr> {
        let t = self.peek_token().clone();
        let mut e = match t.kind {
            TokKind::Int(v) => {
                self.bump();
                Expr { kind: ExprKind::IntLit(v), line: t.line, col: t.col }
            }
            TokKind::Float(v) => {
                self.bump();
                Expr { kind: ExprKind::FloatLit(v), line: t.line, col: t.col }
            }
            TokKind::Str(v) => {
                self.bump();
                Expr { kind: ExprKind::StrLit(v), line: t.line, col: t.col }
            }
            TokKind::True => {
                self.bump();
                Expr { kind: ExprKind::BoolLit(true), line: t.line, col: t.col }
            }
            TokKind::False => {
                self.bump();
                Expr { kind: ExprKind::BoolLit(false), line: t.line, col: t.col }
            }
            TokKind::LBracket => {
                self.bump();
                let mut items = Vec::new();
                if !self.at(TokKind::RBracket) {
                    loop {
                        items.push(self.expr()?);
                        if !self.at(TokKind::Comma) {
                            break;
                        }
                        self.bump();
                    }
                }
                self.expect(TokKind::RBracket, "']'")?;
                Expr { kind: ExprKind::ArrayLit(items), line: t.line, col: t.col }
            }
            TokKind::Ident(name) => {
                self.bump();
                if self.at(TokKind::LParen) {
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at(TokKind::RParen) {
                        loop {
                            args.push(self.expr()?);
                            if !self.at(TokKind::Comma) {
                                break;
                            }
                            self.bump();
                        }
                    }
                    self.expect(TokKind::RParen, "')'")?;
                    Expr { kind: ExprKind::Call { name, args }, line: t.line, col: t.col }
                } else if self.at(TokKind::Dot) {
                    // Qualified access: `module.function(...)` for calls,
                    // `module.NAME` for a const read (0.4.0 S1).
                    self.bump();
                    let f = self.expect_ident()?;
                    let fname = match f.kind {
                        TokKind::Ident(n) => n,
                        _ => unreachable!("expect_ident guarantees an identifier"),
                    };
                    if !self.at(TokKind::LParen) {
                        return Ok(Expr {
                            kind: ExprKind::ModConst { module: name, name: fname },
                            line: t.line,
                            col: t.col,
                        });
                    }
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at(TokKind::RParen) {
                        loop {
                            args.push(self.expr()?);
                            if !self.at(TokKind::Comma) {
                                break;
                            }
                            self.bump();
                        }
                    }
                    self.expect(TokKind::RParen, "')'")?;
                    Expr {
                        kind: ExprKind::ModCall { module: name, name: fname, args },
                        line: t.line,
                        col: t.col,
                    }
                } else {
                    Expr { kind: ExprKind::Ident(name), line: t.line, col: t.col }
                }
            }
            TokKind::LParen => {
                self.bump();
                let e = self.expr()?;
                self.expect(TokKind::RParen, "')'")?;
                e
            }
            _ => {
                self.fail(
                    "E2001",
                    format!("expected an expression but found {}", describe(&t.kind)),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
        };

        // Postfix indexing: `a[0]`, `m.f(x)[i][j]`, ...
        while self.at(TokKind::LBracket) {
            self.bump();
            let idx = self.expr()?;
            self.expect(TokKind::RBracket, "']'")?;
            e = Expr {
                kind: ExprKind::Index { base: Box::new(e), index: Box::new(idx) },
                line: t.line,
                col: t.col,
            };
        }

        Ok(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Program {
        let (toks, diags) = crate::lexer::tokenize(src);
        assert!(diags.is_empty(), "lexer errors: {:?}", diags);
        parse(toks).expect("parse failed")
    }

    fn parse_err(src: &str) -> Vec<Diagnostic> {
        let (toks, _) = crate::lexer::tokenize(src);
        parse(toks).expect_err("expected a parse error")
    }

    #[test]
    fn parses_mode_directive() {
        let p = parse_ok("*mode = B;\nfunc main() { }\n");
        assert_eq!(p.mode, Mode::B);
        assert!(p.mode_declared);
        assert_eq!(p.funcs.len(), 1);
    }

    #[test]
    fn mode_defaults_to_s() {
        let p = parse_ok("func main() { }");
        assert_eq!(p.mode, Mode::S);
        assert!(!p.mode_declared);
    }

    #[test]
    fn invalid_mode_is_rejected() {
        let diags = parse_err("*mode = Z;\nfunc main() { }");
        assert!(diags.iter().any(|d| d.code == "E2011"));
    }

    #[test]
    fn mode_directive_only_allowed_first() {
        let diags = parse_err("func main() { }\n*mode = B;");
        assert!(diags.iter().any(|d| d.code == "E2001"));
    }

    #[test]
    fn parses_function_signature() {
        let p = parse_ok("func add(a: int, b: int) -> int { return a + b; }");
        let f = &p.funcs[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "a");
        assert_eq!(f.params[0].ty, Type::Int);
        assert_eq!(f.ret, Type::Int);
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let p = parse_ok("func main() { print(1 + 2 * 3); }");
        let stmt = &p.funcs[0].body.stmts[0];
        match stmt {
            Stmt::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { args, .. } => match &args[0].kind {
                    ExprKind::Binary { op, lhs, rhs } => {
                        assert_eq!(*op, BinOp::Add);
                        assert!(matches!(&lhs.kind, ExprKind::IntLit(1)));
                        assert!(matches!(
                            &rhs.kind,
                            ExprKind::Binary { op: BinOp::Mul, .. }
                        ));
                    }
                    k => panic!("expected binary, got {:?}", k),
                },
                k => panic!("expected call, got {:?}", k),
            },
            s => panic!("expected expr stmt, got {:?}", s),
        }
    }

    #[test]
    fn parses_else_if_chain() {
        let p = parse_ok("func f(n: int) -> int { if n < 0 { return 0; } else if n > 9 { return 9; } else { return n; } }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::If { else_branch, .. } => {
                // else must be an If (the `else if`), which itself has a Block else.
                match else_branch.as_deref() {
                    Some(Stmt::If { else_branch: inner_else, .. }) => {
                        assert!(matches!(inner_else.as_deref(), Some(Stmt::Block { .. })));
                    }
                    other => panic!("expected nested if, got {:?}", other),
                }
            }
            s => panic!("expected if, got {:?}", s),
        }
    }

    #[test]
    fn parses_assignment_and_compound_assignment() {
        let p = parse_ok("func main() { var x = 1; x = 2; x += 3; x -= 1; }");
        let stmts = &p.funcs[0].body.stmts;
        assert!(matches!(&stmts[1], Stmt::Assign { op: AssignOp::Set, name, .. } if name == "x"));
        assert!(matches!(&stmts[2], Stmt::Assign { op: AssignOp::Add, .. }));
        assert!(matches!(&stmts[3], Stmt::Assign { op: AssignOp::Sub, .. }));
    }

    #[test]
    fn parses_unary_operators() {
        let p = parse_ok("func main() { print(not true); print(-5); }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::Expr { expr, .. } => {
                assert!(matches!(&expr.kind, ExprKind::Call { name, .. } if name == "print"));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
        // -5 is unary minus applied to 5
        match &p.funcs[0].body.stmts[1] {
            Stmt::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { args, .. } => {
                    assert!(matches!(&args[0].kind, ExprKind::Unary { op: UnOp::Neg, .. }));
                }
                k => panic!("expected call, got {:?}", k),
            },
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn missing_semicolon_is_an_error() {
        let diags = parse_err("func main() { let x = 1 print(x); }");
        assert!(diags.iter().any(|d| d.code == "E2002" && d.message.contains(';')));
    }

    #[test]
    fn missing_brace_is_an_error() {
        let diags = parse_err("func main() { let x = 1;");
        assert!(!diags.is_empty());
    }

    #[test]
    fn chained_comparison_is_rejected() {
        let diags = parse_err("func main() { print(1 < 2 < 3); }");
        assert!(diags.iter().any(|d| d.code == "E2004"));
    }

    #[test]
    fn unknown_type_is_rejected() {
        let diags = parse_err("func main() { let x: number = 1; }");
        assert!(diags.iter().any(|d| d.code == "E2003"));
    }

    #[test]
    fn bare_blocks_are_statements() {
        let p = parse_ok("func main() { let x = 1; { let y = 2; print(y); } print(x); }");
        assert_eq!(p.funcs[0].body.stmts.len(), 3);
        assert!(matches!(&p.funcs[0].body.stmts[1], Stmt::Block { .. }));
    }

    // -- 0.2.0 syntax --------------------------------------------------------

    #[test]
    fn parses_array_types() {
        let p = parse_ok("func f(xs: [int]) -> [[int]] { return [[1]]; }");
        assert_eq!(p.funcs[0].params[0].ty, Type::Array(Box::new(Type::Int)));
        assert_eq!(
            p.funcs[0].ret,
            Type::Array(Box::new(Type::Array(Box::new(Type::Int))))
        );
    }

    #[test]
    fn parses_array_literals_and_indexing() {
        let p = parse_ok("func main() { let a = [1, 2, 3]; print(a[0] + a[2]); }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::VarDecl { init, .. } => {
                assert!(matches!(&init.kind, ExprKind::ArrayLit(items) if items.len() == 3));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
        match &p.funcs[0].body.stmts[1] {
            Stmt::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { args, .. } => {
                    assert!(matches!(&args[0].kind, ExprKind::Binary { op: BinOp::Add, .. }));
                }
                k => panic!("expected call, got {:?}", k),
            },
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_empty_array_literal() {
        let p = parse_ok("func main() { let a: [int] = []; }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::VarDecl { init, .. } => {
                assert!(matches!(&init.kind, ExprKind::ArrayLit(items) if items.is_empty()));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_nested_indexing_and_index_assignment() {
        let p = parse_ok("func main() { let m = [[1]]; print(m[0][0]); m[0][0] = 5; }");
        let stmts = &p.funcs[0].body.stmts;
        match &stmts[2] {
            Stmt::IndexAssign { .. } => {}
            s => panic!("expected IndexAssign, got {:?}", s),
        }
    }

    #[test]
    fn compound_index_assignment_is_rejected() {
        let diags = parse_err("func main() { let a = [1]; a[0] += 1; }");
        assert!(diags.iter().any(|d| d.code == "E2014"));
    }

    #[test]
    fn invalid_assignment_target_is_rejected() {
        let diags = parse_err("func main() { f(1) = 2; }");
        assert!(diags.iter().any(|d| d.code == "E2013"));
    }

    #[test]
    fn parses_for_over_exclusive_and_inclusive_ranges() {
        let p = parse_ok("func main() { for i in 0..5 { print(i); } for i in 0..=5 { print(i); } }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::For { var, iter, .. } => {
                assert_eq!(var, "i");
                assert!(matches!(iter, ForIter::Range { inclusive: false, .. }));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
        match &p.funcs[0].body.stmts[1] {
            Stmt::For { iter, .. } => {
                assert!(matches!(iter, ForIter::Range { inclusive: true, .. }));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_for_over_array() {
        let p = parse_ok("func main() { let xs = [1, 2]; for x in xs { print(x); } }");
        match &p.funcs[0].body.stmts[1] {
            Stmt::For { var, iter, .. } => {
                assert_eq!(var, "x");
                assert!(matches!(iter, ForIter::Expr(_)));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_casts() {
        let p = parse_ok("func main() { let f = 2 as float; let s = 3 as string; }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::VarDecl { init, .. } => {
                assert!(matches!(&init.kind, ExprKind::Cast { ty: Type::Float, .. }));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_imports() {
        let p = parse_ok("import \"math.kin\" as math;\nfunc main() { }");
        assert_eq!(p.imports.len(), 1);
        assert_eq!(p.imports[0].path, "math.kin");
        assert_eq!(p.imports[0].alias, "math");
    }

    #[test]
    fn import_after_functions_is_rejected() {
        let diags = parse_err("func main() { }\nimport \"math.kin\" as math;");
        assert!(diags.iter().any(|d| d.code == "E2012"));
    }

    #[test]
    fn import_without_alias_is_rejected() {
        let diags = parse_err("import \"math.kin\";\nfunc main() { }");
        assert!(diags.iter().any(|d| d.code == "E2002" && d.message.contains("`as`")));
    }

    #[test]
    fn import_with_non_string_path_is_rejected() {
        let diags = parse_err("import math.kin as math;\nfunc main() { }");
        assert!(diags.iter().any(|d| d.code == "E2015"));
    }

    #[test]
    fn parses_qualified_calls() {
        let p = parse_ok("import \"math.kin\" as math;\nfunc main() { print(math.add(1, 2)); }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { args, .. } => match &args[0].kind {
                    ExprKind::ModCall { module, name, args } => {
                        assert_eq!(module, "math");
                        assert_eq!(name, "add");
                        assert_eq!(args.len(), 2);
                    }
                    k => panic!("expected ModCall, got {:?}", k),
                },
                k => panic!("expected call, got {:?}", k),
            },
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn qualified_const_read_parses_without_parens() {
        // 0.4.0 S1: `module.NAME` is a qualified const read; the parser
        // accepts it and the checker decides whether NAME is a const
        // (E3039-free) or a function that must be called (E3021 there).
        let p = parse_ok("func main() { math.add; }");
        // The dump path proves it parsed as one expression statement.
        let dump = crate::ast::dump(&p);
        assert!(dump.contains("math.add"), "{}", dump);
        // A qualified access whose member is missing still errors.
        let diags = parse_err("func main() { math.; }");
        assert!(diags.iter().any(|d| d.is_error()));
    }

    // -- recovery (P-001) ----------------------------------------------------

    #[test]
    fn multiple_syntax_errors_are_all_reported() {
        let diags = parse_err(
            "func main() {\n    let x = ;\n    let y = ;\n    let z = ;\n}\n",
        );
        let errs = diags.iter().filter(|d| d.is_error()).count();
        assert_eq!(errs, 3, "expected 3 separate diagnostics, got: {:#?}", diags);
        // every one points at its own line
        let lines: Vec<u32> = diags.iter().map(|d| d.line).collect();
        assert!(lines.contains(&2) && lines.contains(&3) && lines.contains(&4));
    }

    #[test]
    fn recovery_does_not_cascade_into_following_statements() {
        // One broken statement; the rest must parse fine and produce
        // exactly one diagnostic.
        let diags = parse_err(
            "func main() {\n    let x = ;\n    let y = 2;\n    print(y);\n}\n",
        );
        assert_eq!(diags.len(), 1, "expected exactly one diagnostic, got: {:#?}", diags);
    }

    #[test]
    fn recovery_reports_errors_in_multiple_functions() {
        let diags = parse_err("func a( { }\nfunc main( { }");
        assert!(diags.iter().filter(|d| d.is_error()).count() >= 2);
    }

    #[test]
    fn unclosed_block_before_next_func_is_reported_cleanly() {
        let diags = parse_err("func a() {\n    let x = 1;\nfunc main() { }");
        assert!(diags.iter().any(|d| d.message.contains("}")));
        // The second function header itself is not the error.
        assert!(!diags.iter().any(|d| d.message.contains("expected `func`")));
    }

    #[test]
    fn error_cap_kicks_in() {
        let mut src = String::from("func main() {\n");
        for _ in 0..40 {
            src.push_str("    let x = ;\n");
        }
        src.push_str("}\n");
        let diags = parse_err(&src);
        assert!(diags.iter().any(|d| d.code == "E2099"));
        assert!(diags.len() <= 26);
    }

    #[test]
    fn semicolon_sync_recovers_within_one_line() {
        // Two errors on different statements; second is still reported.
        let diags = parse_err("func main() { let 1 = 2; let 3 = 4; }");
        assert!(diags.iter().filter(|d| d.is_error()).count() >= 2);
    }

    // -- website declarations (0.3.0) ---------------------------------------

    fn parse_website(src: &str) -> Program {
        let (toks, diags) = crate::lexer::tokenize(src);
        assert!(diags.iter().all(|d| !d.is_error()), "lex diags: {:?}", diags);
        match parse(toks) {
            Ok(p) => p,
            Err(e) => panic!("unexpected parse errors: {:?}", e.iter().map(|d| (d.code, &*d.message)).collect::<Vec<_>>()),
        }
    }

    #[test]
    fn website_parses_all_elements() {
        let p = parse_website(
            "*mode = B;\nwebsite \"My Site\" {\n    page Home {\n        title \"Welcome\"\n        heading \"Intro\"\n        text \"Hello world\"\n        button \"Go\" open About\n        link \"More\" open About\n        image \"a.png\" alt \"A\"\n        divider\n    }\n    page About { text \"about\" }\n}\nfunc main() { }\n",
        );
        assert_eq!(p.websites.len(), 1);
        let w = &p.websites[0];
        assert_eq!(w.name, "My Site");
        assert_eq!(w.pages.len(), 2);
        assert_eq!(w.pages[0].name, "Home");
        assert_eq!(w.pages[0].elements.len(), 7);
        assert!(matches!(&w.pages[0].elements[3].kind, ElementKind::Button { target, .. } if target == "About"));
        assert!(matches!(&w.pages[0].elements[5].kind, ElementKind::Image { alt, .. } if alt == "A"));
    }

    #[test]
    fn website_and_functions_coexist() {
        let p = parse_website("func helper() -> int { return 1; }\nfunc main() { print(helper()); }\nwebsite \"S\" { page Home { text \"t\" } }\n");
        assert_eq!(p.funcs.len(), 2);
        assert_eq!(p.websites.len(), 1);
    }

    #[test]
    fn website_words_stay_identifiers_outside_the_declaration() {
        // Backwards compatibility: `website`/`page`/`button` as ordinary
        // names keep compiling (ADR-0019).
        let p = parse_website(
            "func main() {\n    let website = 1;\n    let page = website + 1;\n    print(page);\n}\n",
        );
        assert!(p.websites.is_empty());
        assert_eq!(p.funcs.len(), 1);
    }

    #[test]
    fn website_name_must_be_a_string() {
        let diags = parse_err("website Home { }");
        assert!(diags.iter().any(|d| d.code == "E2016"), "{:?}", diags);
    }

    // -- 0.4.0 S1: `const` declarations (contextual keyword, ADR-0019) -------

    #[test]
    fn const_word_stays_identifier_outside_the_declaration() {
        // Backwards compatibility gate: `const` as an ordinary name keeps
        // compiling everywhere except the top-level item position.
        let p = parse_ok(
            "func main() {\n    let const = 1;\n    let x = const + 1;\n    print(x);\n}\n",
        );
        assert!(p.consts.is_empty());
        assert_eq!(p.funcs.len(), 1);
        // Even as a parameter name and in assignment position.
        parse_ok("func f(const: int) -> int { return const; }\nfunc main() { print(f(2)); }\n");
        parse_ok("func main() { let c = 0; const = c + 1; print(const); }\n");
    }

    #[test]
    fn const_declarations_parse() {
        let p = parse_ok(
            "import \"m.kin\" as m;\nconst MAX: int = 10;\nconst NAME: string = \"kinn\";\nconst LIMITS: [int] = [1, 2, MAX];\nfunc main() { }\n",
        );
        assert_eq!(p.consts.len(), 3);
        assert_eq!(p.consts[0].name, "MAX");
        assert_eq!(p.consts[0].ty, Type::Int);
        assert_eq!(p.consts[2].name, "LIMITS");
        assert_eq!(p.consts[2].ty, Type::Array(Box::new(Type::Int)));
        // Position points at `const`.
        assert_eq!(p.consts[0].line, 2);
        assert_eq!(p.consts[0].col, 1);
        // The `kinn ast` dump shows the declaration.
        let dump = crate::ast::dump(&p);
        assert!(dump.contains("const MAX: int = 10"), "{}", dump);
        assert!(dump.contains("const LIMITS: [int]"), "{}", dump);
    }

    #[test]
    fn const_requires_a_type_annotation() {
        let diags = parse_err("const MAX = 10;\nfunc main() { }\n");
        let d = diags
            .iter()
            .find(|d| d.code == "E2003")
            .expect("the missing-annotation error is E2003");
        assert!(d.message.contains("must declare its type"), "{}", d.message);
    }

    #[test]
    fn const_parse_errors_recover_to_the_next_declaration() {
        // A broken const header produces exactly one precise diagnostic
        // (E2003 from the missing `:`) and recovery stops before the next
        // `const` item instead of consuming it, so nothing else cascades.
        let diags = parse_err(
            "const A int = 1;\nconst B: int = 2;\nfunc main() { print(1); }\n",
        );
        let errs: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert_eq!(errs.len(), 1, "one precise error, got: {:#?}", errs);
        assert_eq!(errs[0].line, 1, "points at the broken header");
        assert!(errs[0].message.contains("must declare its type"));
    }

    #[test]
    fn module_const_read_parses_as_mod_const() {
        let p = parse_ok(
            "import \"m.kin\" as m;\nfunc main() { print(m.LIMIT); }\n",
        );
        // The call-free qualified read is one expression.
        let dump = crate::ast::dump(&p);
        assert!(dump.contains("m.LIMIT"), "{}", dump);
        // Calls keep their parentheses-required shape.
        let p = parse_ok(
            "import \"m.kin\" as m;\nfunc main() { print(m.f(1)); }\n",
        );
        let dump = crate::ast::dump(&p);
        assert!(dump.contains("m.f(1)"), "{}", dump);
    }

    #[test]
    fn unknown_element_is_E2030() {
        let diags = parse_err("website \"S\" { page Home { carousel \"x\" } }");
        assert!(diags.iter().any(|d| d.code == "E2030"), "{:?}", diags);
    }

    #[test]
    fn button_without_open_is_rejected_with_hint() {
        let diags = parse_err("website \"S\" { page Home { button \"Go\" } }");
        let d = diags.iter().find(|d| d.message.contains("`button`")).expect("button diagnostic");
        assert!(d.hint.is_some());
    }

    #[test]
    fn image_without_alt_is_rejected_at_parse_time() {
        let diags = parse_err("website \"S\" { page Home { image \"x.png\" } }");
        assert!(diags.iter().any(|d| d.message.contains("alt")), "{:?}", diags);
    }

    #[test]
    fn broken_element_recovers_and_later_elements_parse() {
        let diags = parse_err(
            "website \"S\" {\n    page Home {\n        text\n        text \"after\"\n    }\n}",
        );
        // one error for the missing string, then parsing continues
        assert_eq!(diags.iter().filter(|d| d.is_error()).count(), 1, "{:?}", diags);
        assert!(diags[0].message.contains("string after `text`"), "{}", diags[0].message);
    }

    // -- 0.4.0 S2: the `app` declaration (parser + AST) ---------------------

    fn parse_app(src: &str) -> AppDecl {
        let p = parse_website(src);
        assert_eq!(p.apps.len(), 1, "expected exactly one app");
        p.apps[0].clone()
    }

    #[test]
    fn app_minimal_parses() {
        let a = parse_app("app \"Demo\" { page Home { text \"hi\" } }\n");
        assert_eq!(a.name, "Demo");
        assert_eq!(a.pages.len(), 1);
        assert_eq!(a.pages[0].name, "Home");
        assert_eq!(a.pages[0].elements.len(), 1);
        assert!(matches!(&a.pages[0].elements[0].kind, ViewElementKind::Text { .. }));
    }

    #[test]
    fn app_name_must_be_a_string() {
        let diags = parse_err("app Demo { page Home { text \"x\" } }");
        assert!(diags.iter().any(|d| d.code == "E2016"), "{:?}", diags);
    }

    #[test]
    fn app_navigation_modes_and_default() {
        let a = parse_app("app \"D\" { navigation spa page Home { text \"x\" } }");
        assert_eq!(a.navigation, Some(NavigationMode::Spa));
        let a = parse_app("app \"D\" { navigation mpa page Home { text \"x\" } }");
        assert_eq!(a.navigation, Some(NavigationMode::Mpa));
        let a = parse_app("app \"D\" { page Home { text \"x\" } }");
        assert_eq!(a.navigation, None); // default = mpa
        let diags = parse_err("app \"D\" { navigation single }");
        assert!(diags.iter().any(|d| d.code == "E2001" && d.message.contains("mpa")), "{:?}", diags);
    }

    #[test]
    fn app_not_found_parses() {
        let a = parse_app("app \"D\" { not_found Missing page Home { text \"x\" } }");
        assert_eq!(a.not_found.as_ref().unwrap().page, "Missing");
    }

    #[test]
    fn app_theme_tokens_parse_and_unknown_is_e2034() {
        let a = parse_app(
            "app \"D\" { theme { accent \"#111\" bg \"#222\" surface \"#333\" text \"#444\" muted \"#555\" font \"sans\" radius \"8px\" max_width \"720px\" } page Home { text \"x\" } }",
        );
        assert_eq!(a.theme.len(), 8);
        assert_eq!(a.theme[0].token, "accent");
        let diags = parse_err("app \"D\" { theme { sparkle \"#111\" } }");
        assert!(diags.iter().any(|d| d.code == "E2034"), "{:?}", diags);
    }

    #[test]
    fn app_state_fields_parse_with_optional_separators() {
        let a = parse_app(
            "app \"D\" { state { items: [string] = [] filter: string = \"all\", count: int = 0; } page Home { text \"x\" } }",
        );
        assert_eq!(a.state.len(), 3);
        assert_eq!(a.state[0].name, "items");
        assert_eq!(a.state[0].ty, Type::Array(Box::new(Type::Str)));
        assert_eq!(a.state[2].name, "count");
    }

    #[test]
    fn app_state_field_requires_type_and_initializer() {
        let diags = parse_err("app \"D\" { state { count = 0 } }");
        assert!(diags.iter().any(|d| d.code == "E2003"), "{:?}", diags);
        let diags = parse_err("app \"D\" { state { count: int } }");
        assert!(diags.iter().any(|d| d.message.contains("initializer")), "{:?}", diags);
    }

    #[test]
    fn app_computed_parses() {
        let a = parse_app(
            "app \"D\" { computed remaining = len(items) - 1 computed label = \"open: \" + str(n) page Home { text \"x\" } }",
        );
        assert_eq!(a.computed.len(), 2);
        assert_eq!(a.computed[0].name, "remaining");
    }

    #[test]
    fn app_animation_keyframes_full_form() {
        let a = parse_app(
            "app \"D\" { animation { keyframes \"pop\" { from { opacity 0 scale 0.96 } to { opacity 1 scale 1 } step 50 { rotate 180 } duration 200 delay 10 easing ease_out } } page Home { text \"x\" } }",
        );
        let an = a.animation.as_ref().unwrap();
        assert_eq!(an.keyframes.len(), 1);
        let k = &an.keyframes[0];
        assert_eq!(k.name, "pop");
        assert_eq!(k.steps.len(), 3);
        assert!(matches!(k.steps[2].at, StepAt::Step(_)));
        assert_eq!(k.steps[0].props.len(), 2);
        assert!(k.duration.is_some() && k.delay.is_some() && k.easing.is_some());
    }

    #[test]
    fn app_animation_transition_full_form() {
        let a = parse_app(
            "app \"D\" { animation { transition \"fade\" { duration 150 delay 0 easing cubic(0.1, 0.2, 0.3, 0.4) } } page Home { text \"x\" } }",
        );
        let an = a.animation.as_ref().unwrap();
        assert_eq!(an.transitions.len(), 1);
        assert_eq!(an.transitions[0].name, "fade");
        assert!(an.transitions[0].duration.is_some());
    }

    #[test]
    fn app_animation_malformed_steps_and_options_are_e2037() {
        let diags = parse_err("app \"D\" { animation { keyframes \"k\" { midway { opacity 1 } } } }");
        assert!(diags.iter().any(|d| d.code == "E2037"), "{:?}", diags);
        let diags = parse_err("app \"D\" { animation { transition \"t\" { speed 100 } } }");
        assert!(diags.iter().any(|d| d.code == "E2037"), "{:?}", diags);
        let diags = parse_err("app \"D\" { animation { spin \"k\" { } } }");
        assert!(diags.iter().any(|d| d.code == "E2037"), "{:?}", diags);
    }

    #[test]
    fn animation_attributes_require_quoted_names_e2037() {
        let diags = parse_err("app \"D\" { page Home { text \"x\" enter pop } }");
        assert!(diags.iter().any(|d| d.code == "E2037"), "{:?}", diags);
    }

    #[test]
    fn app_transition_selects_page_transition() {
        let a = parse_app("app \"D\" { transition \"fade\" page Home { text \"x\" } }");
        assert_eq!(a.transition.as_ref().unwrap().name, "fade");
    }

    #[test]
    fn app_store_keys_parse_with_or_without_commas() {
        let a = parse_app(
            "app \"D\" { store \"tasks\" { items, done, filter } store \"ui\" { open } page Home { text \"x\" } }",
        );
        assert_eq!(a.stores.len(), 2);
        assert_eq!(a.stores[0].keys, vec!["items", "done", "filter"]);
        assert_eq!(a.stores[1].keys, vec!["open"]);
    }

    #[test]
    fn app_route_parses() {
        let a = parse_app(
            "app \"D\" { route \"item/:id\" -> ItemDetail route \"about\" -> About page Home { text \"x\" } }",
        );
        assert_eq!(a.routes.len(), 2);
        assert_eq!(a.routes[0].path, "item/:id");
        assert_eq!(a.routes[0].target.page, "ItemDetail");
        let diags = parse_err("app \"D\" { route \"item\" ItemDetail }");
        assert!(diags.iter().any(|d| d.message.contains("->")), "{:?}", diags);
    }

    #[test]
    fn app_page_elements_accept_strings_and_expressions() {
        let a = parse_app(
            "app \"D\" { page Home { title \"T\" heading \"H\" text \"Open: \" + str(count) text filter divider } }",
        );
        let els = &a.pages[0].elements;
        assert_eq!(els.len(), 5);
        assert!(matches!(&els[0].kind, ViewElementKind::Title { .. }));
        // element 2 is a binary expression binding
        assert!(matches!(&els[2].kind, ViewElementKind::Text { value, .. } if matches!(value.kind, ExprKind::Binary { .. })));
        // element 3 is a bare identifier binding
        assert!(matches!(&els[3].kind, ViewElementKind::Text { value, .. } if matches!(value.kind, ExprKind::Ident(_))));
    }

    #[test]
    fn view_argument_that_cannot_start_an_expression_is_e2033() {
        let diags = parse_err("app \"D\" { page Home { text } }");
        assert!(diags.iter().any(|d| d.code == "E2033"), "{:?}", diags);
    }

    #[test]
    fn app_button_open_and_handler_block() {
        let a = parse_app(
            "app \"D\" { page Home { button \"Go\" open About button \"Save\" { on click { saved = true; } } } page About { text \"x\" } }",
        );
        let els = &a.pages[0].elements;
        assert!(matches!(&els[0].kind, ViewElementKind::Button { open: Some(t), handlers, .. } if t.page == "About" && handlers.is_empty()));
        assert!(matches!(&els[1].kind, ViewElementKind::Button { open: None, handlers, .. } if handlers.len() == 1));
    }

    #[test]
    fn app_all_event_kinds_parse() {
        let src = "app \"D\" { page Home { button \"B\" { on click { a = 1; } on input { b = 2; } on change { c = 3; } on submit { d = 4; } on focus { e = 5; } on blur { f = 6; } on keydown \"Enter\" { g = 7; } on keyup \"Escape\" { h = 8; } on pointerdown { i = 9; } on pointerup { j = 10; } on key \"Tab\" { k = 11; } } } }";
        let a = parse_app(src);
        let els = &a.pages[0].elements;
        assert!(matches!(&els[0].kind, ViewElementKind::Button { handlers, .. } if handlers.len() == 11));
    }

    #[test]
    fn app_unknown_event_is_e2031() {
        let diags = parse_err("app \"D\" { page Home { button \"B\" { on tap { x = 1; } } } }");
        assert!(diags.iter().any(|d| d.code == "E2031"), "{:?}", diags);
    }

    #[test]
    fn app_key_filter_rules_are_e2039() {
        // `on key` without a filter
        let diags = parse_err("app \"D\" { page Home { button \"B\" { on key { x = 1; } } } }");
        assert!(diags.iter().any(|d| d.code == "E2039"), "{:?}", diags);
        // a filter on a non-keyboard event
        let diags = parse_err("app \"D\" { page Home { button \"B\" { on click \"Enter\" { x = 1; } } } }");
        assert!(diags.iter().any(|d| d.code == "E2039"), "{:?}", diags);
    }

    #[test]
    fn when_otherwise_parses() {
        let a = parse_app(
            "app \"D\" { page Home { when len(items) == 0 { text \"empty\" } otherwise { text \"full\" } when flag { text \"on\" } } }",
        );
        let els = &a.pages[0].elements;
        assert_eq!(els.len(), 2);
        assert!(matches!(&els[0].kind, ViewElementKind::When { else_branch: Some(_), .. }));
        assert!(matches!(&els[1].kind, ViewElementKind::When { else_branch: None, .. }));
    }

    #[test]
    fn stray_otherwise_is_an_unknown_element_with_hint() {
        let diags = parse_err("app \"D\" { page Home { otherwise { text \"x\" } } }");
        assert!(diags.iter().any(|d| d.code == "E2030"), "{:?}", diags);
        let d = diags.iter().find(|d| d.code == "E2030").unwrap();
        assert!(d.hint.as_deref().unwrap_or("").contains("when"), "{:?}", d.hint);
    }

    #[test]
    fn each_parses_with_and_without_key() {
        let a = parse_app(
            "app \"D\" { page Home { each items as item key item { text item } each rows as r { text r } } }",
        );
        let els = &a.pages[0].elements;
        assert!(matches!(&els[0].kind, ViewElementKind::Each { var, key: Some(_), .. } if var == "item"));
        assert!(matches!(&els[1].kind, ViewElementKind::Each { var, key: None, .. } if var == "r"));
    }

    #[test]
    fn each_requires_as_and_tolerates_casts_in_parens() {
        let diags = parse_err("app \"D\" { page Home { each items { text \"x\" } } }");
        assert!(diags.iter().any(|d| d.code == "E2032"), "{:?}", diags);
        // a cast in the iterable needs parentheses (as binds tighter)
        let a = parse_app(
            "app \"D\" { page Home { each (vals as [string]) as v { text v } } }",
        );
        assert!(matches!(&a.pages[0].elements[0].kind, ViewElementKind::Each { .. }));
        // plain casts elsewhere still work (regression gate)
        parse_ok("func main() { let x = 1; print(x as float); }");
    }

    #[test]
    fn component_declares_prop_state_view() {
        let a = parse_app(
            "app \"D\" { component TaskRow { prop label: string prop done: bool = false state expanded: bool = false view { text label } } page Home { text \"x\" } }",
        );
        assert_eq!(a.components.len(), 1);
        let c = &a.components[0];
        assert_eq!(c.name, "TaskRow");
        assert_eq!(c.props.len(), 2);
        assert!(c.props[0].default.is_none());
        assert!(c.props[1].default.is_some());
        assert_eq!(c.state.len(), 1);
        assert_eq!(c.state[0].name, "expanded");
        assert_eq!(c.view.len(), 1);
    }

    #[test]
    fn component_prop_requires_type() {
        let diags = parse_err("app \"D\" { component C { prop label = \"x\" view { text \"v\" } } }");
        assert!(diags.iter().any(|d| d.code == "E2003"), "{:?}", diags);
    }

    #[test]
    fn component_instantiation_named_args() {
        let a = parse_app(
            "app \"D\" { component C { prop label: string view { text label } } page Home { C(label: \"hi\") C() C(label: upper(name), done: true,) } }",
        );
        let els = &a.pages[0].elements;
        assert!(matches!(&els[1].kind, ViewElementKind::Component { name, args, .. } if name == "C" && args.is_empty()));
        assert!(matches!(&els[2].kind, ViewElementKind::Component { args, .. } if args.len() == 2)); // trailing comma ok
    }

    #[test]
    fn component_instantiation_module_qualified() {
        // `alias.Name(...)` — same dot convention as calls and const reads.
        let a = parse_app(
            "app \"D\" { page Home { ui.Card(title: \"Hi\") } }",
        );
        assert!(matches!(
            &a.pages[0].elements[0].kind,
            ViewElementKind::Component { module: Some(m), name, .. } if m == "ui" && name == "Card"
        ));
    }

    #[test]
    fn component_instantiation_positional_args_are_e2032() {
        let diags = parse_err("app \"D\" { page Home { C(\"hi\") } }");
        assert!(diags.iter().any(|d| d.code == "E2032"), "{:?}", diags);
    }

    #[test]
    fn unknown_word_without_paren_is_unknown_element_e2030() {
        let diags = parse_err("app \"D\" { page Home { sparkles \"x\" } }");
        assert!(diags.iter().any(|d| d.code == "E2030"), "{:?}", diags);
    }

    #[test]
    fn form_with_fields_and_submit_parses() {
        let a = parse_app(
            "app \"D\" { page Home { form \"Add task\" { field draft kind text { label \"Title\" placeholder \"What?\" required min_length 1 max_length 80 on input { draft = event_value; } } field qty kind number { min_value 1 max_value 99 } field due kind date { label \"Due\" } field email kind email { pattern \"^[^@]+@[^@]+$\" } field pw kind password { } field notes kind textarea { } button \"Add\" { on submit { if form_valid { items = items + [draft]; } } } } } }",
        );
        let els = &a.pages[0].elements;
        assert!(matches!(&els[0].kind, ViewElementKind::Form { body, handlers, .. } if body.len() == 7 && handlers.is_empty()));
        let f = match &els[0].kind {
            ViewElementKind::Form { body, .. } => match &body[0].kind {
                ViewElementKind::Field { attrs, handlers, kind, .. } => {
                    assert_eq!(*kind, FieldKind::Text);
                    assert_eq!(attrs.len(), 5);
                    assert_eq!(handlers.len(), 1);
                    true
                }
                _ => false,
            },
            _ => false,
        };
        assert!(f);
    }

    #[test]
    fn field_unknown_kind_is_e2035() {
        let diags = parse_err("app \"D\" { page Home { form \"F\" { field x kind color { } } } }");
        assert!(diags.iter().any(|d| d.code == "E2035"), "{:?}", diags);
    }

    #[test]
    fn field_missing_kind_is_e2039_and_unknown_attr_is_e2036() {
        let diags = parse_err("app \"D\" { page Home { form \"F\" { field x { } } } }");
        assert!(diags.iter().any(|d| d.code == "E2039"), "{:?}", diags);
        let diags = parse_err("app \"D\" { page Home { form \"F\" { field x kind text { sparkle 1 } } } }");
        assert!(diags.iter().any(|d| d.code == "E2036"), "{:?}", diags);
    }

    #[test]
    fn input_bind_parses_and_missing_bind_is_e2039() {
        let a = parse_app(
            "app \"D\" { page Home { input bind draft { on input { draft = event_value; } } } }",
        );
        assert!(matches!(&a.pages[0].elements[0].kind, ViewElementKind::Input { bind, handlers, .. } if bind.page == "draft" && handlers.len() == 1));
        let diags = parse_err("app \"D\" { page Home { input draft } }");
        assert!(diags.iter().any(|d| d.code == "E2039"), "{:?}", diags);
    }

    #[test]
    fn element_animation_attributes_parse() {
        let a = parse_app(
            "app \"D\" { page Home { heading \"H\" enter \"pop\" exit \"fade\" button \"B\" hover \"raise\" press \"squish\" { on click { x = 1; } } image \"i.png\" alt \"A\" scroll \"reveal\" divider enter \"pop\" } }",
        );
        let els = &a.pages[0].elements;
        assert!(matches!(&els[0].kind, ViewElementKind::Heading { anim, .. } if anim.enter.is_some() && anim.exit.is_some()));
        assert!(matches!(&els[1].kind, ViewElementKind::Button { anim, .. } if anim.hover.is_some() && anim.press.is_some()));
        assert!(matches!(&els[2].kind, ViewElementKind::Image { anim, .. } if anim.scroll.is_some()));
        assert!(matches!(&els[3].kind, ViewElementKind::Divider { anim } if anim.enter.is_some()));
    }

    #[test]
    fn element_without_handlers_rejects_blocks_e2038() {
        let diags = parse_err("app \"D\" { page Home { text \"x\" { on click { y = 1; } } } }");
        assert!(diags.iter().any(|d| d.code == "E2038"), "{:?}", diags);
    }

    #[test]
    fn views_reject_statements_with_guidance() {
        let diags = parse_err("app \"D\" { page Home { let x = 1; text \"ok\" } }");
        assert!(
            diags.iter().any(|d| d.message.contains("logic belongs in `on` handlers")),
            "{:?}",
            diags
        );
    }

    #[test]
    fn broken_state_block_does_not_close_the_app() {
        // Regression (brace-aware recovery): a type error inside `state`
        // must not consume the state block's `}` as the app's `}` — the
        // sections after it stay part of the app.
        let diags = parse_err(
            "app \"D\" {\n    state { items: [Str] = [] }\n    computed n = 1\n    page Home { text \"x\" }\n}",
        );
        assert!(diags.iter().any(|d| d.code == "E2003"), "{:?}", diags);
        assert_eq!(diags.iter().filter(|d| d.is_error()).count(), 1, "{:?}", diags);
    }

    #[test]
    fn broken_element_inside_when_does_not_truncate_the_view() {
        let diags = parse_err(
            "app \"D\" { page Home { when flag { text } text \"after\" } }",
        );
        assert!(diags.iter().any(|d| d.code == "E2033"), "{:?}", diags);
        // exactly one error: the `when` body recovers, the page keeps parsing
        assert_eq!(diags.iter().filter(|d| d.is_error()).count(), 1, "{:?}", diags);
    }

    #[test]
    fn multi_error_view_recovers_per_element() {
        // Note: `text\nheading` on its own is a VALID binding
        // (`text <identifier>`), so real argument errors need a token
        // that cannot start an expression.
        let diags = parse_err(
            "app \"D\" {\n    page Home {\n        text ,\n        heading )\n        text \"fine\"\n    }\n}",
        );
        let errs: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert_eq!(errs.len(), 2, "{:?}", diags);
        assert!(errs.iter().all(|d| d.code == "E2033"));
    }

    #[test]
    fn handler_bodies_are_official_statements() {
        let a = parse_app(
            "app \"D\" { page Home { button \"B\" { on click { var n = 0; for i in 0..10 { n = n + i; } if n > 5 { n = 0; } else { n = 1; } while n < 3 { n = n + 1; } saved = n; store_save(\"k\"); } } } }",
        );
        match &a.pages[0].elements[0].kind {
            ViewElementKind::Button { handlers, .. } => {
                assert_eq!(handlers[0].body.stmts.len(), 6);
            }
            _ => panic!("expected a button"),
        }
    }

    #[test]
    fn app_words_stay_identifiers_outside_the_declaration() {
        // THE contextual-keyword regression gate (ADR-0019, B.0): every
        // 0.4.0 word must remain a legal identifier outside grammar
        // positions. Mirrors website_words_stay_identifiers_...
        let words = [
            "app", "component", "state", "computed", "view", "prop", "each", "when",
            "otherwise", "on", "route", "store", "animation", "keyframes", "from",
            "to", "step", "duration", "delay", "easing", "enter", "exit", "hover",
            "press", "scroll", "transition", "navigation", "mpa", "spa", "not_found",
            "form", "field", "kind", "validate", "bind", "theme", "min_length",
            "max_length", "min_value", "max_value", "required", "placeholder",
            "label", "pattern", "save", "load", "clear", "key", "input", "submit",
            "click", "change", "focus", "blur", "keydown", "keyup", "pointerdown",
            "pointerup", "accent", "bg", "surface", "muted", "font", "radius",
            "max_width", "open", "alt",
            // S2 review: the field `kind` values (E2035 whitelist) —
            // `text` doubles as a theme token (E2034 whitelist)
            "text", "number", "date", "email", "password", "textarea",
            // S2 review: the 0.3 view-element words the app grammar
            // shares — heritage closure (never gate-tested before)
            "title", "heading", "divider", "button", "link", "image",
        ];
        for w in words {
            // as a variable name and assignment target
            let src = format!(
                "func main() {{\n    let {} = 1;\n    let y = {} + 1;\n    print(y);\n}}\n",
                w, w
            );
            let p = parse_ok(&src);
            assert!(p.apps.is_empty(), "`{}` accidentally entered app grammar", w);
            // as a function name and parameter name
            let src = format!("func {}({}: int) -> int {{ return {}; }}\nfunc main() {{ print({}(1)); }}\n", w, w, w, w);
            parse_ok(&src);
        }
    }

    #[test]
    fn as_identifier_outside_each_is_a_positioned_helpful_cast_error() {
        // S2 review item 5: `x as some_ident` outside `each`. Cast
        // types are keywords, so `as` + identifier is never a cast —
        // the expression ends and the stray `as` is reported. The
        // diagnostic must sit ON the `as` and explain both readings.
        let diags = parse_err(
            "func main() {\n    var y = x as some_ident;\n    print(y);\n}\n",
        );
        let d = diags
            .iter()
            .find(|d| d.code == "E2002")
            .expect("E2002 for the stray `as`");
        assert_eq!(d.line, 2, "positioned: {:?}", diags);
        assert_eq!(d.col, 15, "points at the `as` keyword: {:?}", diags);
        assert!(d.message.contains("expected ';'"), "{}", d.message);
        let hint = d.hint.as_deref().expect("the cast hint");
        assert!(hint.contains("each"), "names the each marker: {}", hint);
        assert!(hint.contains("cast"), "names the cast operator: {}", hint);
        assert!(hint.contains("type keyword"), "says what a cast needs: {}", hint);
        // Contrast: the legal cast spelling right next to it.
        parse_ok("func main() {\n    let x = 1;\n    let y = x as float;\n    print(y);\n}\n");
        // And a non-identifier follower gets the short cast hint.
        let diags = parse_err("func main() {\n    let x = 1;\n    let y = x as;\n}\n");
        let d = diags.iter().find(|d| d.code == "E2002").expect("E2002");
        let hint = d.hint.as_deref().expect("short cast hint");
        assert!(hint.contains("type keyword"), "{}", hint);
    }

    #[test]
    fn ast_dump_shows_the_app_tree() {
        let p = parse_ok(
            "app \"Demo\" {\n    navigation spa\n    state { n: int = 0 }\n    page Home {\n        heading \"Hi\" enter \"pop\"\n        each items as item key item { text item }\n    }\n}",
        );
        let d = crate::ast::dump(&p);
        assert!(d.contains("app \"Demo\""), "{}", d);
        assert!(d.contains("navigation spa"), "{}", d);
        assert!(d.contains("n: int = 0"), "{}", d);
        assert!(d.contains("heading \"Hi\" enter \"pop\""), "{}", d);
        assert!(d.contains("each items as item key item"), "{}", d);
    }
}

