# Changelog

All notable changes to KINN are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow semantic versioning once there is a stable language surface.

## [Unreleased] — 0.4.0 (Phase 5 implementation started)

Dynamic web applications (owner directive). Design phases 1–4 landed
as `docs/DYNAMIC_WEB_ARCHITECTURE.md` + ADR-0023…0027; **the owner
approved all Part E decisions (D1–D6) on 2026-11-22 and Phase 5
implementation began with S1.**

### Added — S2: the `app` declaration — grammar, AST and honest guards

- Language: the full dynamic-app grammar of
  DYNAMIC_WEB_ARCHITECTURE B.2–B.9 parses: `app "Name" { ... }` with
  `navigation mpa|spa`, `not_found`, `theme` (ADR-0027 token
  whitelist), `state` blocks, `computed`, `animation` (keyframes with
  `from`/`to`/`step`, named transitions, duration/delay/easing incl.
  `cubic(...)`), `store`, `route "path/:id" -> Page`, app-level
  `transition`, `page` views and `component` declarations
  (`prop`/`state`/`view`). Views accept binding **expressions**
  wherever 0.3 elements take strings, plus `when`/`otherwise`,
  keyed `each`, `form`/`field` (6 kinds, 8 validation attributes),
  `input bind`, `on <event>` handler blocks (10 events + `on key "..."`
  sugar) and component instantiation with named props
  (`TaskRow(label: x)`, qualified `ui.Card(...)`).
- Parser: new codes **E2031–E2039** (unknown event, malformed
  each/when/instantiation, non-expression view argument, theme/field
  whitelists, animation malformations, misplaced handler blocks) — all
  positioned, all frozen by tests. **`each` vs casts**: a bare `as`
  followed by an identifier is always the loop-variable marker (cast
  types are keywords), so `each items as item` parses and casts in
  iterables need parentheses. Brace-aware recovery: a broken item
  inside a section/view/flat block never truncates its parent
  (regression-tested). Handler bodies are **official Kin statements**
  (semicolons, `if` not `when` — B.5/B.6 normative; the B.7 example's
  `when form_valid` was a doc slip, fixed).
- Checker: presence rules **E3051** (app outside the entry file) and
  **E3052** (website + app, or two apps) — pulled forward from S3 so
  an app half is never silently ignored by a website build; app
  programs do not need `main` (B.2). App internals are **not yet
  checked** (S3).
- CLI: `kinn ast` dumps the whole app tree; `kinn check` passes
  grammar + presence; `kinn run`/`kinn build` on app programs exit 1
  with honest guards ("building dynamic apps is not implemented yet")
  instead of pretending. `Compiled` carries the parsed `AppDecl`
  (lowering to an AppModel arrives with S4).
- Example: `examples/web/tasks_app.kin` — the target-shape app
  exercising every grammar construct (parses; documents its own
  intermediate status).
- Docs: LANGUAGE_SPEC §C3 (normative grammar + rules), ROADMAP S2
  checked, DYNAMIC_WEB_ARCHITECTURE Part E approval annotated.
- Tests: +54 (workspace 281 → 335, all green): 44 parser (incl. the
  70-word stays-identifier regression gate), 5 checker presence, 5
  CLI integration. 0.3.0 web dist rebuilt byte-identical; Studio
  66/66 against a freshly built wasm.

### Added — S1: cross-module value exports (`const`)

- Language: `const NAME: Type = expr;` top-level declarations with a
  mandatory type annotation (ADR-0028; normative rules in
  LANGUAGE_SPEC §C1). `const` is a contextual keyword (ADR-0019
  recipe) with the word-stays-identifier regression gate; imports must
  still precede all functions and consts (E2012 wording updated).
- Semantics: initializers are compile-time constant expressions
  (literals, array literals, operators, earlier same-file consts);
  calls, casts, indexing and qualified reads are rejected (new code
  **E3039**). Constant arithmetic follows VM rules — overflow and
  division/modulo by zero are compile-time errors. Assigning to a
  const is E3004; locals may shadow consts; void consts are E3015;
  builtin-name collisions are E3016; duplicate definitions are E3002.
- Modules export consts implicitly: declaring files read them bare,
  importers read them qualified (`m.NAME`; KIR name `m::NAME`).
  Unknown members E3020/E3021 now distinguish functions and consts
  with hints both ways; undefined variables hint at module consts.
- Compiler: new `kinc::consteval` evaluator (checker validation +
  lowering folding share one pure function); `Module` gained
  `consts: Vec<(String, Value)>` (qualified names, deterministic
  order) — **no new KIR instructions**; every const read compiles to
  `Const(value)`; `kinn kir` prints the folded constants
  (`const math::MAX = 100`).
- Examples: `examples/modules/` now demonstrates shared configuration
  values across files (`math.MAX`, entry-local `BASE`).
- Tests: +31 (workspace 250 → 281, all green): parser (contextual
  word gate, annotation error, recovery, ModConst), checker (15 tests:
  duplicates, collisions, non-constant initializers, type mismatches,
  void, arithmetic errors, assignment, shadowing, qualified reads and
  hints), pipeline (folding, qualified reads through calls,
  shared-singleton const arrays per ADR-0014, determinism, website
  compatibility), CLI (multi-file run + `kir` shows folded consts),
  kinrt listing, consteval unit tests.
- Compatibility: 0.3.0 website programs are unaffected —
  `examples/web/site.kin` dist rebuilt byte-identical (determinism
  re-verified); all 250 pre-existing tests pass unchanged (one parser
  test updated for the new `module.NAME` grammar, see S1 notes).

### Added — documentation & decisions

- `docs/DYNAMIC_WEB_ARCHITECTURE.md`: Phase 1 technical audit
  (re-verified on `c84a8f0`: 250 Rust tests green, real dist inspected),
  Phase 2 official `app` language module design (components, reactive
  state, binding expressions, events in official grammar, forms,
  animations, MPA/SPA routing, local persistence with normative honesty
  clause), Phase 3 KINN Web Runtime architecture (dependency-free
  ES2020, statically compiled binding graph, KIR→JS with differential
  testing, ≤ 16 KB budget), Phase 4 roadmap S1–S11 with tests and
  acceptance criteria, Phase 5 approval gate (decision points D1–D6).
- ADR-0023 (KINN Web Runtime; supersedes ADR-0020's no-runtime clause
  for `app` programs only), ADR-0024 (`app` module: components, state,
  events, forms), ADR-0025 (MPA default / SPA opt-in, honest `file://`
  behavior), ADR-0026 (local persistence + JSON, local-only), ADR-0027
  (declarative animation whitelist + theme tokens).
- ROADMAP re-scoped: 0.4.0 = Dynamic web applications (S1–S11);
  "breadth and developer comfort" items moved to 0.6.0 (D1).
- PROBLEM_REGISTRY: P-018–P-023 table formatting fixed; P-024
  (packaging CI still emits 0.2.0 names), P-025 (doc drift),
  P-026 (flat Studio VFS blocks 0.4 folders), P-027 (no runtime error/
  console forwarding in preview; E3038 CLI/FFI asymmetry) added.

## [0.3.0] — 2026-11-21

Multi-target delivery: the first complete backend vertical slice (web),
the Android application backend, Studio build integration on both
platforms, and the package/module foundation. The compiler core still
has **zero external dependencies**. 0.2.0 functionality is preserved
(backwards-compatibility is tested, including contextual-keyword
behavior).

### Added — targets & backends (ADR-0018)

- **Target/backend architecture**: `kinc::backend` with a stable
  contract — target identification (`kinn targets`), pure artifact
  generation (`OutputFile`), capability tables, and diagnostics before
  generation. Backends consume the validated `SiteModel`, never the
  AST.
- **Web backend (complete vertical slice)**: `kinn build --target web`
  generates a real distribution — semantic HTML5 (one file per page,
  `index.html` for the first page), one shared stylesheet, focus
  management script, copied `assets/`. Accessibility enforced at
  compile time and generated in (skip link, landmarks, aria-current,
  alt text, focus-visible, reduced-motion). JS-free by design
  (ADR-0020): state/forms/events do not exist in 0.3 and are not
  faked. Verified in a real browser (accessibility tree + navigation).
- **Android backend (project export + remote APK)**:
  `kinn build --target android` exports a complete Gradle project
  (AGP 8.5.2, Kotlin 1.9.24) whose `MainActivity.kt` renders the site
  with native Android views — no WebView, no AndroidX, no permissions.
  Build providers: local Gradle/SDK (honest degradation), remote CI
  (real APK built + `apksigner verify` + artifact), export (always).
  ADR-0021.

### Added — language (Kin 0.3, ADR-0019)

- **Declarative `website` module**: `website "Name" { page Home { … } }`
  with `title`, `heading`, `text`, `button "…" open Page`, `link "…"
  open Page`, `image "src" alt "…"`, `divider`. Contextual keywords —
  no existing identifier breaks (tested). Websites live in the entry
  file; site-only programs need no `main` (run reports guidance).
- **New diagnostics** (normative in LANGUAGE_SPEC §B2): E2016, E2030,
  E3030–E3038, W3035–W3037.

### Added — CLI

- `kinn build [file] [--target web|android] [--out DIR]` with per-file
  build report and honest statuses (`project exported` vs `debug APK`).
- `kinn targets`; `kinn.toml` manifest support (name/version/entry/
  target) with a strict minimal parser (ADR-0022).

### Added — Studio (web + Android)

- Web Studio: **Build Web** button, `build [target]` and `preview`
  terminal commands, preview panel (page selector, iframe preview with
  inlined CSS/JS and intercepted navigation), **Download .zip** of the
  real dist, **Export Android** project zip. Store-only ZIP writer
  (crc32 vector-tested; validated with `unzip`).
- Android Studio: `build web` / `build android` / `preview` /
  `export android`; **PreviewActivity** renders the generated site
  offline via file:// with in-app page navigation; Build Web button.

### Added — ecosystem foundation

- `docs/MODULE_SYSTEM.md`: module/package/registry/SDK layering; what
  exists vs what is deliberately deferred. `kinn.toml` is the first
  working piece; no registry, no lockfile, no downloads (none faked).

### Changed

- Versions across compiler, CLI, FFI, Studio and the Android app to
  0.3.0 (FFI `kinn_version` = 0x00_03_00_00).
- CI: target smoke tests (web dist + android export) on all three
  platforms; a new `backend-apk` job builds a real APK from the
  exported Kin project on a GitHub runner with signature verification
  and zero-permission check; Studio test count 51 → 66.

### Tests

- Rust: 190 → **250** tests (+12 web backend, +9 android backend,
  +13 parser, +12 checker, +4 FFI build, +9 CLI build, +5 manifest,
  +1 provider status; version test updated).
- Studio: 51 → **66** vitest tests (preview builder, zip writer, wasm
  build-web/build-android against the real backend).
- Browser verification of the generated dist (rendering, a11y tree,
  navigation) performed with headless Chromium.

## [0.2.0] — 2026-11-21

Language breadth, Studio integration, the Android app and the release
pipeline. The compiler core still has **zero external dependencies**.

### Added — language (Kin 0.2)

- **Modules / multi-file projects**: `import "lib.kin" as m;` with
  qualified calls `m.add(1, 2)`. Paths resolve relative to the importing
  file, cannot escape the entry directory (E3017), cycles are rejected
  with the full chain (E3018), duplicate aliases rejected (E3019), and a
  file imported twice is parsed once. Imported `main` is inert; only the
  entry's `main` runs.
- **Arrays**: `[1, 2, 3]` literals, `[T]` types, indexing `a[i]`, element
  assignment `a[i] = v`, nesting, `len(a)`. Reference semantics
  (assignment shares the array) with deep structural equality; bounds
  checks at runtime (E4005). Empty literals need a type annotation
  (`let a: [int] = [];`, E3022).
- **`for` loops**: `for i in 0..10` (exclusive), `for i in 0..=9`
  (inclusive), `for x in array`. `break`/`continue` work; `continue`
  jumps to the increment; the loop variable is a fresh immutable binding
  each iteration.
- **Explicit casts**: `x as float`, `x as int` (truncating toward zero,
  saturating), `x as string` for int/float/bool. Unsupported casts
  rejected (E3025).
- **Native stdlib foundations**: `len` (arrays and strings — string
  length counts characters), `abs`, `min`, `max`; reserved against
  redefinition (E3016).
- **Parser error recovery**: multiple syntax errors per file with
  statement-level synchronization and a cap (E2099); recovery tested to
  not cascade.

### Changed — compiler

- Control-flow analysis: `while true` without a reachable `break` is
  recognized as diverging — no more false E3012 for
  `while true { return x; }`; code after it is unreachable (W3001).
- KIR local slots are reused after scope exit (free-list allocator):
  smaller frames, same semantics.
- `Type` gained `Array(Box<Type>)`; diagnostics carry the source file
  (`render_project` renders multi-file diagnostics grouped by file).
- `kinc::project` (new module) implements loading, path validation and
  cycle detection; `driver::compile_project` takes a source loader used
  identically by the CLI, Studio (wasm) and Android (JNI).
- CLI 0.2.0: `run`/`check`/`kir` load multi-file projects; `kir` shows
  qualified names (`math::add`).

### Added — kinn-ffi / kinn-android (Rust)

- `kinn-ffi`: C-ABI bridge compiling the same pipeline to
  `wasm32-unknown-unknown` and native `.so`. Exports `kinn_check` /
  `kinn_run` / `kinn_check_text` (JSON results with diagnostics,
  stdout, runtime errors), `kinn_set_module`/`kinn_clear_modules` for
  multi-file projects, `kinn_alloc`/`kinn_free` memory protocol.
  Panic-safe (E5000). 12 tests.
- `kinn-android`: JNI glue for `dev.kinn.studio.KinnBridge`
  (arm64-v8a). The only crate allowed an external dependency (`jni`
  0.21); the compiler core stays zero-dep (ADR-0014).

### Added — KINN Studio (web) 0.2.0

- **Real in-Studio check and run**: the Rust compiler+VM compiled to
  `public/kinn.wasm` (via `kinn-ffi`), running multi-file projects in
  the browser, fully offline. Graceful, honest fallback with build
  instructions when the wasm is not built.
- **Persistence**: projects survive reloads via IndexedDB (debounced
  autosave); explicit `save` command.
- **Import/export**: `.kinnproj` JSON project format (versioned,
  validated) for download/upload; single `.kin` file import.
- Terminal: real `check`/`run`, `rm`, `mv`, `save`, `export`, `import`;
  Kin 0.2 cheat sheet; backend status badge in the top bar.
- Editor lexer mirrors the 0.2 token set (ranges `..`/`..=`, brackets,
  `for`/`in`/`as`/`import`).
- Tests: 22 → 51, ten of them executing the real compiler in
  WebAssembly (skipped loudly, not silently, when `kinn.wasm` is
  missing).

### Added — KINN Studio for Android

- First real Android app (Kotlin, `android/`): multi-project explorer
  with persistent local storage, editor (line numbers, Kin syntax
  highlighting), problems panel (click to jump), terminal with 19 real
  commands, offline bundled documentation, SAF import/export.
- **On-device compilation on ARM64**: check/run call the real compiler
  through JNI (`libkinn_android.so`); when the library is missing the
  app degrades honestly and says so. No permissions — no INTERNET.
- Built and verified locally: debug APK 6.6 MB bundling the 933 KB
  native compiler and the docs; signature verified with apksigner.

### Added — CI and release pipeline

- `ci.yml`: Rust tests on Ubuntu/Windows/macOS + release CLI smoke test
  on each platform; Studio tests with the wasm backend; Studio site
  artifact.
- `android.yml`: NDK + cargo-ndk build of `libkinn_android.so`,
  assembleDebug/assembleRelease; **release signing only from encrypted
  `ANDROID_KEYSTORE_*` secrets** (unsigned + clearly labeled otherwise —
  never a fake identity); apksigner verification and SHA-256 checksums
  in the `signature-report.txt` artifact.
- `windows.yml`: tests + release build on a Windows runner; portable
  zip and NSIS installer.
- `linux.yml`: tests + release build on Linux; DEB (installed and
  exercised on the runner), AppImage via linuxdeploy (experimental),
  tarball.

### Added — documentation

- `AUDIT_REPORT.md` (verified / partial / planned / broken per area),
  `INSTALL.md` (per-platform installation).

### Known limitations

See [PROBLEM_REGISTRY.md](docs/PROBLEM_REGISTRY.md): P-001/P-002/P-003
and P-005–P-008 are resolved in 0.2.0; string interpolation was decided
against for 0.2 (ADR-0013, revisit in 0.3); the Android app is a v1
shell (no `tokens` command, single-color terminal lines before colors
landed — see the registry for the current list).

## [0.1.0] — 2026-11-21

First verified milestone: the complete Kin 0.1 pipeline from source text
to execution, plus the first KINN Studio shell.

### Added — language and compiler (`compiler/`)

- Kin 0.1 language subset: `func`, `let`/`var` (with inference),
  assignment and compound assignment (`=` `+=` `-=`), `if` / `else if` /
  `else`, `while`, `break`, `continue`, `return`, expression statements,
  bare blocks; types `int`, `float`, `bool`, `string`, `void`; the
  `print` builtin; `and` / `or` / `not` with short-circuit evaluation.
- Programming mode directive `*mode = B | S | A;` (parsed, validated,
  carried through the pipeline; see PROGRAMMING_MODES for 0.1 behavior).
- Lexer with full position tracking, line and block comments, string
  escapes, precise E1xxx diagnostics.
- Recursive-descent parser producing a typed AST; explicit braces and
  required semicolons; chained comparisons rejected (E2004).
- Semantic analysis: block scoping with shadowing, function signatures,
  `main()` requirements, void-value protection, conservative
  missing-return analysis (E3012), unreachable-code warning (W3001),
  no implicit int/float conversion (E3011), Beginner-mode `var` lint
  (N3001).
- KIR: flat stack-based intermediate representation with per-instruction
  source lines.
- `kinrt` runtime: stack VM with call frames, checked integer overflow
  (E4004), division/modulo by zero (E4001), call depth limit (E4002),
  float formatting that keeps `2.0` visible as `2.0`.
- `kinn` CLI: `run`, `check`, `tokens`, `ast`, `kir`, `version`; exit
  codes 0/1/2/3; caret-style diagnostics with source snippets.
- Example programs with machine-verified expected output
  (`// expect:` blocks) and error programs with expected diagnostic
  codes (`// error:` / `// runtime-error:` markers).
- 90 Rust tests (unit + integration), zero external dependencies.

### Added — KINN Studio (`studio/`)

- Terminal-inspired web shell: project explorer, code editor with Kin
  syntax highlighting, integrated terminal (13 real commands), problems
  panel with clickable editor checks, responsive desktop/mobile layout.
- Editor-side TypeScript lexer mirroring the Rust lexer; 22 vitest
  tests; production build with zero runtime dependencies.

### Added — documentation

- Full document set (see README index): vision, language design, spec,
  modes, type system, compiler/runtime/graphics architecture, Studio,
  CLI, roadmap, decisions, problem registry, security, AI context.

### Known limitations

See [PROBLEM_REGISTRY.md](docs/PROBLEM_REGISTRY.md): single-syntax-error
parsing, conservative return analysis, no slot reuse, Studio files are
in-memory only, no in-Studio execution, no CI yet.
