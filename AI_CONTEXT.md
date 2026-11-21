# AI Context

Status snapshot for AI assistants (and humans) working on KINN.
**Last updated: 2026-11-22, release 0.3.0 + 0.4.0 S1.** Update this
file after significant milestones. Keep it between 500 and 900 words.

## What KINN is

An offline-first programming language (Kin) and development environment
(KINN Studio). No accounts, no cloud, no online-only compilation.
Android ARM64 ships real on-device compilation. The project rule that
matters most: never claim untested functionality works. Distinguish
implemented / experimental / planned / unsupported everywhere.

## Actual implementation status (0.3.0)

Everything from 0.2.0 is preserved (the full 0.2 suite keeps passing;
backwards compatibility is tested, including contextual keywords).

- **Kin 0.2 language + compiler pipeline** (unchanged): modules,
  arrays, `for`, casts, natives; multi-error parser recovery;
  control-flow analysis; KIR with slot reuse; stack VM.
- **Multi-target architecture (ADR-0018)**: `kinc::backend` — backends
  are pure generators over the target-independent `SiteModel`
  (`kinc::site`); capability tables; diagnostics before generation.
  KIR + `kinrt` remain the execution path for console programs.
- **Web backend — complete vertical slice (ADR-0019/0020)**: the
  declarative `website "N" { page P { title/heading/text/button/link/
  image/divider } }` module (contextual keywords). `kinn build
  --target web` emits a real dist: semantic HTML5 per page
  (index.html = first page), one stylesheet, focus-management-only
  app.js, copied `assets/`. Compile-time accessibility (alt text,
  non-empty labels), generated-in landmarks/aria-current/skip link.
  JS-free by design; state/forms arrive in 0.4 and are not faked.
  Verified in a real browser (a11y tree + navigation).
- **Android backend (ADR-0021)**: `kinn build --target android`
  exports a complete Gradle project (AGP 8.5.2/Kotlin 1.9.24) with a
  generated native-UI `MainActivity.kt` — no WebView/AndroidX/
  permissions. Providers: local Gradle (honest degradation, P-018),
  remote CI (real APK: export → gradle assembleDebug → apksigner
  verify → artifact), export.
- **CLI `kinn` 0.3.0**: `run`, `check`, `build --target web|android
  [--out]`, `targets`, `tokens`, `ast`, `kir`, `version`;
  `kinn.toml` manifests (strict minimal parser, ADR-0022). Site-only
  programs give run guidance instead of fake execution.
- **kinn-ffi**: adds `kinn_build_web`/`kinn_build_android` (same JSON
  shape, E3038 for site-less programs); wasm rebuilt; panic-safe.
- **KINN Studio web 0.3.0**: Build Web button, `build`/`preview`
  terminal commands, preview panel (sandboxed iframe, inlined CSS/JS,
  postMessage navigation, page selector), Download .zip (store-only
  zip, `unzip`-validated), Export Android project zip.
- **KINN Studio Android 0.3.0**: `build web`/`build android`/
  `preview`/`export android` through JNI; PreviewActivity renders the
  build offline over file:// with in-app navigation; Build Web button.
- **CI**: 3-platform target smoke (web dist + android export checks),
  Studio tests with real wasm, Android Studio APK pipeline
  (sign+verify), **backend-apk job** (exports a Kin website, builds a
  real APK on the runner, verifies signature + zero permissions,
  uploads artifact), Windows/Linux packaging unchanged.

Test totals: **281 Rust tests** (177 kinc unit incl. const/website
parser/checker/backends/consteval, 27 kinrt, 28 pipeline, 28 CLI, 16
ffi, 5 manifest) and **66 Studio vitest tests** (15 real-wasm incl.
build targets). Examples carry `// expect:` blocks; error examples
declare codes; `examples/web/site.kin` is the website example;
`examples/modules/` demonstrates multi-file projects with const
exports (0.4.0 S1).

Planned, not implemented (do not pretend otherwise): site state/forms
(0.4), package manager/registry (spec in MODULE_SYSTEM.md), native
codegen for console programs (0.4+), string interpolation
(ADR-0013), graphics (0.5), Android `tokens` command, on-device
backend-APK install (P-019), Studio desktop shell.

## Repository structure

```
compiler/                  Rust workspace (cargo test = 250 tests)
  crates/kinrt/            KIR instruction set, stack VM, disassembler
  crates/kinc/             lexer, parser, project, checker, lower,
                           site (SiteModel), backend/{web,android}
  crates/kinn/             CLI binary (kinn) + kinn.toml manifest
  crates/kinn-ffi/         C-ABI bridge: check/run/build_web/build_android
  crates/kinn-android/     JNI glue for the app (only crate with a dep)
android/                   KINN Studio app (Kotlin, AGP 8.5.2, minSdk 26)
studio/                    KINN Studio web shell (npm test = 66 tests)
  src/build/               preview builder + store-only zip writer
examples/web/              declarative website example
.github/workflows/         ci, android (studio-app + backend-apk), windows, linux
docs/                      design documents; BACKEND_WEB/BACKEND_ANDROID/
                           MODULE_SYSTEM/MULTI_TARGET_ARCHITECTURE are new
```

## Known problems (see docs/PROBLEM_REGISTRY.md for all)

- P-018 local Android build provider untested on a real desktop SDK.
- P-019 backend APK not yet installed on a physical device.
- P-020 no per-site theming tokens yet (designed in ADR-0027, ships in 0.4.0).
- P-004 string interpolation deferred (ADR-0013). P-012 license.
  P-014 narrowed to selective/renamed exports (value exports landed
  as `const`, 0.4.0 S1 / ADR-0028).
- P-024 packaging CI still names 0.2.0 artifacts (linux/windows).
- P-025 doc drift (SYNTAX 0.1-only, COMPILER_ARCHITECTURE crate count, CLI_DESIGN plan).
- P-026 Studio VFS/.kinnproj are flat — blocks 0.4 multi-folder projects.
- P-027 preview lacks runtime error/console forwarding; CLI/FFI E3038 asymmetry.

## Current task and next milestone

0.3.0 is complete and committed (web vertical slice, Android backend,
Studio integration, package foundation, CI for real targets). Current:
**0.4.0 — Dynamic web applications (owner directive, 2026-11-22)**.
Phase 1–4 delivered as `docs/DYNAMIC_WEB_ARCHITECTURE.md` and
ADR-0023–0027; **the owner approved D1–D6 on 2026-11-22 and Phase 5
started: S1 (cross-module value exports, `const`, ADR-0028) is landed
— 281 Rust tests green, 0.3.0 web dist byte-identical. Next: S2
(`app` module grammar, parser + AST).** Carry-overs: string
casts/interpolation decision, native codegen spike, structs/records
ADR (D2 stays out of 0.4.0 per the owner).

## Conventions

- Commits: `feat(scope):`, `fix(scope):`, `docs:`, `chore:`,
  conventional. Branches: `develop` (work) → `main` (release merge).
- Rust: edition 2021, zero deps in the core; kinn-android is the only
  crate with a dependency (jni, ADR-0014).
- Diagnostics carry stable codes (E1xxx lexer, E2xxx parser, E3xxx
  semantic, E4xxx runtime, E5xxx FFI, W/N warnings/notes) — 0.3 codes
  in LANGUAGE_SPEC §B2.
- Every example doubles as a test; broken examples declare their
  expected error code. Generated dists are deterministic (tested).
- Build artifacts (`compiler/target`, `studio/node_modules`,
  `studio/dist`, `studio/public/kinn.wasm`, `android/.gradle`,
  `android/app/build`, `android/local.properties`,
  `android/app/src/main/jniLibs`, per-project `build/`) are
  gitignored. Never commit secrets; SECURITY.md describes token and
  signing hygiene.

## Most relevant documents

- docs/LANGUAGE_SPEC_0.1.md — normative grammar incl. §B1/B2 (0.3)
- docs/MULTI_TARGET_ARCHITECTURE.md — backend architecture + gap analysis
- docs/BACKEND_WEB.md / docs/BACKEND_ANDROID.md — target contracts
- docs/MODULE_SYSTEM.md — ecosystem layering
- docs/KINN_STUDIO.md — Studio features and limits
- AUDIT_REPORT.md — verified / partial / planned / broken
