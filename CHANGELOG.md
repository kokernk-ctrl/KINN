# Changelog

All notable changes to KINN are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow semantic versioning once there is a stable language surface.

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
