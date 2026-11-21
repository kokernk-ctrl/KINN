# KINN 0.3.0 — Audit Report

Full-repository audit for release 0.3.0 (2026-11-21). Every claim
names **how** it was verified. Categories: **verified** (built/tested
with evidence), **partial** (works with stated limits), **planned**
(honestly not built), **broken** (defect, tracked). The 0.2.0 baseline
was independently re-verified before any 0.3 work started.

---

## 1. Test suites (executed 2026-11-21)

| Suite | Where run | Result |
| --- | --- | --- |
| Rust workspace (`cargo test --workspace`) | local Linux (Rust 1.100.0) | **250 passed, 0 failed** |
| same | CI, ubuntu/windows/macos (`ci` run 44912150523 + platform matrix) | **passed** |
| Studio web (`npm test`) with the real wasm backend | local + CI | **66 passed, 0 failed** (15 execute the real compiler / target backends in WebAssembly) |
| Studio production build (`tsc && vite build`) | local + CI | **passed** |
| Android app build (`assembleDebug` + `assembleRelease`) | CI (`android` run 44912150502) | **BUILD SUCCESSFUL**; artifacts + signature report |
| **Backend APK** (exported Kin project → Gradle → APK) | CI (`android` runs 44911581668, 44912150502) | **BUILD SUCCESSFUL**; `apksigner verify` + zero-permission check passed |
| Windows packaging (zip + NSIS) | CI (`windows` run 44912150542) | **passed** |
| Linux packaging (DEB + AppImage + tarball) | CI (`linux` run 44912150499) | **passed** |

Rust breakdown: 153 kinc unit (lexer/parser/checker/project/
diagnostics/site/backends), 26 kinrt (VM/value), 22 kinc pipeline
(end-to-end incl. all `// expect:` examples), 28 kinn CLI (real
binary), 16 kinn-ffi, 5 manifest unit — plus kinn-android (JNI glue)
type-checked in the workspace on all three desktop platforms.

Baseline re-verification before 0.3 work: the untouched 0.2.0 tree
passed 190 Rust + 51 Studio tests locally at `6845496`.

## 2. Compiler — verified (0.2 surface unchanged)

All 0.2.0 findings remain valid: lexer with ranges, multi-error
parser recovery (E2099 cap), control-flow analysis (`while true`
divergence), slot reuse, modules (E3017–E3021), arrays/for/casts/
natives (E3022–E3028), multi-file diagnostics rendering. Every 0.2
test still passes untouched.

### Website module — verified (new in 0.3)

- **Parser**: contextual keywords (`website`, `page`, element words)
  parsed in their positions only; `let website = 1;` etc. still
  compile (dedicated test). E2016 (non-string site name), E2030
  (unknown element), missing-`open` rejection with hint, image-alt
  parse requirement, element-level error recovery (one error, later
  elements still parse).
- **Checker**: E3030 (two websites), E3031 (duplicate page names,
  case-insensitive), E3032 (unknown nav target with pages hint),
  E3033 (empty site name), E3034 (empty alt), E3035 (empty label),
  E3036 (empty src), E3037 (website in an imported module), W3035
  (unreachable page), W3036 (duplicate title), W3037 (empty text).
  Site-only programs do **not** trigger E3008.
- **Lowering**: `SiteModel` built from the declaration (first title
  wins, later titles demote to headings); `driver::Compiled.site`.

## 3. Runtime (kinrt) — verified, unchanged

26 tests; arrays, casts, natives, bounds/overflow/div-zero/depth
checks all retained. Zero external dependencies maintained.

## 4. CLI (`kinn` 0.3.0) — verified

28 integration tests run the real binary, including the new:

- `kinn build --target web` generates a real dist (files exist,
  title/aria-current asserted in the output HTML).
- `kinn build --target android` exports the full Gradle project.
- Default target (`web`), unknown-target rejection (lists supported),
  building a console program explains the missing website, `run` of a
  site-only program gives build guidance (exit 1, no fake execution).
- `kinn targets`; manifest-driven build from `kinn.toml` (entry +
  target + project name, output verified on disk).
- **Local build provider**: without a toolchain (PATH stripped,
  deterministic test) the status is exactly `project exported`; with a
  real toolchain (GitHub ubuntu runner, Gradle + SDK preinstalled) the
  same command assembled a **real debug APK** (first 0.3.0 `ci` run,
  job log: `local build provider: debug APK at …`).

## 5. Target backends — verified

### Web (complete vertical slice, ADR-0020)

- 12 codegen unit tests: element mapping table, HTML escaping
  (including a `<script>` payload), slug generation, `aria-current`,
  skip link/landmarks, alt/lazy images, deterministic byte-identical
  output, unicode content.
- **Real-browser verification**: the dist built from
  `examples/web/site.kin` was opened in headless Chromium
  (`file://…/dist/index.html`): document title correct, accessibility
  tree shows the skip link, `navigation "Pages"` landmark, h1/h2
  order, styled button; clicking the generated `Get Started` button
  navigated to `about.html` (`About · MyWebsite`).
- Assets copying verified end-to-end (project `assets/` → dist).

### Android (ADR-0021)

- 9 codegen unit tests: complete Gradle file set, manifest with
  launcher activity and **no permissions**, package sanitization
  (incl. Kotlin-keyword and digit-leading names), Kotlin string
  escaping, navigation indices, asset mapping with honest alt
  fallback, no AndroidX/no WebView, honest README ("project, not an
  APK").
- **CI end-to-end (remote provider)**: `android.yml` `backend-apk`
  job exports the project with the real CLI, completes the Gradle
  wrapper, runs `./gradlew assembleDebug` (BUILD SUCCESSFUL), verifies
  the signature (`Signer #1 certificate DN: C=US, O=Android, CN=Android
  Debug` — debug-signed, honestly named `…-debug.apk`), verifies **no
  requested permissions** via `aapt dump badging`, uploads the APK.
  Artifact `kinn-android-backend-apk` downloaded locally and
  re-verified: 798,982 bytes, `classes.dex` ×3, manifest present,
  SHA-256 `6e08a6aa…51673` matching the CI `SHA256SUMS`.
- Export determinism is tested (byte-identical generated project);
  APK bytes are **not** reproducible (Gradle embeds timestamps) —
  hashes are published per run instead of being faked.

## 6. FFI bridges — verified

- 16 tests (12 prior + 4 new): `kinn_build_web` returns the site JSON
  with generated files; `kinn_build_android` returns project files
  (never an APK claim); E3038 for site-less programs; compile
  diagnostics surface through the build calls.
- wasm rebuild (`npm run build:wasm`) green; 15 Studio tests execute
  the real compiler and target backends through the wasm bridge.
- JNI glue (`nBuildWeb`/`nBuildAndroid`) type-checked in the workspace
  on all three platforms; built for `aarch64-linux-android` in CI.

## 7. KINN Studio web — verified

- 66 vitest tests: all 0.2 suites plus the 0.3 additions — preview
  builder (CSS/JS inlining, navigation interceptor, honest error on
  unknown page), ZIP writer (CRC-32 canonical vector, header/EOCD
  structure), and wasm-level build tests.
- The store-only ZIP output was additionally validated with real
  `unzip` (`testing: index.html OK … No errors detected`) and content
  extracted byte-identical.
- Preview panel: sandboxed iframe, page selector, postMessage
  navigation; Download .zip exports the real dist; Export Android
  downloads the project (labeled a project).
- Honest degradation retained (missing wasm → badge + exact build
  instructions).

## 8. KINN Studio Android — verified (with stated limits)

- Built in CI (`assembleDebug` + `assembleRelease`, run 44912150502)
  with the 0.3 additions: Build Web button, `build web`/`build
  android`/`preview`/`export android` terminal commands,
  PreviewActivity (file:// WebView with in-app navigation, non-file
  URLs blocked), BuildSupport (private-storage writer + SAF zip
  export), 0.3 cheat sheet.
- Signature: debug APK verified by `apksigner` in CI; release APK
  remains **UNSIGNED-labeled** without `ANDROID_KEYSTORE_*` secrets
  (ADR-0017) — correct, honest behavior.
- Limits (partial, honest): no physical-device run yet (P-017/P-019),
  no automated UI tests (P-021), `tokens` command still absent (P-015).

## 9. Release pipeline — verified

- `ci.yml` — **green** (both branches): 3-platform matrix with
  target smoke (web dist + android export checks), the
  `backend-build` job uploading the generated web dist, Studio tests
  with the real wasm backend.
- `android.yml` — **green**: two jobs — the Studio app APK pipeline
  and the backend APK job (§5).
- `windows.yml` / `linux.yml` — **green**: packaging unchanged from
  0.2.0 (zip/NSIS; DEB/AppImage/tarball), rebuilt with the 0.3 CLI.
- One CI defect was found **by CI** and fixed (commit `066b102`): the
  new `build/` gitignore pattern had silently excluded
  `studio/src/build/` from the push; detected via the failing Studio
  production build, root-caused, fixed, re-run green.

## 10. Security and reproducibility — verified

- **No secrets in history**: full-history grep for token/key patterns
  remains clean; automation tokens live only in the transient sandbox
  credential helper, never in the repo or commits.
- **Generated app permissions**: none (verified twice — unit test on
  the generated manifest and `aapt dump badging` in CI).
- **Escaping**: all user strings HTML-escaped in web output (tested
  against script injection); Kotlin string literals escaped (tested).
- **Remote builds need no user credentials**: the CI provider runs
  with repository-scoped GitHub permissions; SECURITY.md documents
  the policy (no tokens in chat/source/config).
- **Determinism**: generated dists/projects are byte-identical across
  runs (tested); APKs embed build timestamps — hashes published per
  run, never claimed reproducible.

## 11. Specification alignment — verified

- LANGUAGE_SPEC §B1/§B2 added (normative website grammar + E2016,
  E2030, E3030–E3038, W3035–W3037); every rule maps to the tests in
  §2/§5.
- BACKEND_WEB.md / BACKEND_ANDROID.md publish the element table,
  distribution layout, build-provider statuses, and verification
  evidence.
- MODULE_SYSTEM.md states exactly what exists (modules + kinn.toml)
  and what does not (manager, lockfile, registry).
- PROBLEM_REGISTRY updated (P-018 verified-and-reworded, P-019…P-023
  added); ROADMAP 0.3.0 items checked off with the two honest
  carry-overs (string casts/interpolation → 0.4, native codegen → 0.4+).

## 12. Summary table

| Area | Status |
| --- | --- |
| Compiler 0.2 surface (lexer/parser/checker/KIR/VM) | **verified** (unchanged, all prior tests green) |
| Website module (parse/check/lower) | **verified** |
| Web backend + `kinn build --target web` | **verified** (incl. real browser) |
| Android backend export (Gradle project, native UI) | **verified** |
| Android APK via CI (remote provider) | **verified** (debug-signed, apksigner + aapt evidence) |
| Android APK via local provider | **verified on the CI toolchain**; other toolchain shapes untested (P-018) |
| APK installation on a physical device | **planned** (P-019) — never claimed |
| CLI build/targets/manifest | **verified** |
| FFI build exports (wasm + JNI) | **verified** |
| Studio web build/preview/export | **verified** (ZIP validated with unzip) |
| Studio Android build/preview/export | **verified in CI build**; on-device flows untested (P-021) |
| Package manager / registry / lockfiles | **planned** (spec only, no fake scaffolding) |
| Site state, forms, events | **planned 0.4** (grammar rejects them today) |
| Broken functionality | **one CI regression found by CI and fixed** (gitignore); none remaining |

*Audit performed against `main` @ the 0.3.0 merge and `develop`
@ `066b102`+audit, 2026-11-21. Evidence: local runs (Rust 1.100.0,
Node 24, headless Chromium) and GitHub Actions runs 44912150523 (ci),
44912150502 (android), 44912150499 (linux), 44912150542 (windows),
44911581668 (first backend-APK run with local-provider evidence).*
