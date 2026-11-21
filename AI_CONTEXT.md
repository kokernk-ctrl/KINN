# AI Context

Status snapshot for AI assistants (and humans) working on KINN.
**Last updated: 2026-11-21, release 0.2.0.** Update this file after
significant milestones. Keep it between 500 and 900 words.

## What KINN is

An offline-first programming language (Kin) and development environment
(KINN Studio). No accounts, no cloud, no online-only compilation.
Android ARM64 ships real on-device compilation. The project rule that
matters most: never claim untested functionality works. Distinguish
implemented / experimental / planned / unsupported everywhere.

## Actual implementation status (0.2.0)

Fully implemented and tested, end to end:

- **Kin 0.2 language**: everything from 0.1 plus `import "file.kin" as
  m;` modules (qualified `m.f(...)` calls, cycle detection E3018, path
  confinement E3017), arrays (`[T]`, literals, indexing, element
  assignment, reference semantics, deep `==`, bounds checks E4005),
  `for` over `a..b` / `a..=b` / arrays, casts (`as int/float/string`,
  E3025), natives `len`/`abs`/`min`/`max` (E3016 reserved).
- **Compiler pipeline** (Rust, zero external dependencies): lexer →
  parser (multi-error recovery, statement sync points, E2099 cap) →
  project loader (`kinc::project`, loader-callback based) → checker →
  KIR lowering (slot reuse after scope exit).
- **Control-flow analysis**: `while true` without reachable `break` is
  diverging (no false E3012 — P-002 fixed); if/else return analysis;
  W3001 unreachable.
- **Runtime**: `kinrt` stack VM — arrays, casts, native table, E4005
  bounds, E4001 div-zero, E4004 overflow, E4002 depth.
- **CLI** `kinn` 0.2.0: `run`, `check`, `tokens`, `ast`, `kir`,
  `version`; exit 0/1/2/3; multi-file projects with imports.
- **kinn-ffi**: C-ABI bridge (`kinn_check`/`kinn_run`/`kinn_check_text`
  JSON, `kinn_set_module` registry, `kinn_alloc`/`kinn_free`); builds
  for wasm32-unknown-unknown and native; panic-safe (E5000).
- **KINN Studio web 0.2.0**: REAL in-browser check/run via
  `public/kinn.wasm` (same compiler as the CLI), multi-file projects in
  the browser, IndexedDB persistence, .kinnproj import/export,
  terminal (19 commands), problems panel, editor checks as fallback.
- **KINN Studio Android** (`android/`, Kotlin): projects on device,
  editor with highlighting + gutter, problems, terminal, offline docs
  bundled from `docs/`, SAF import/export, on-device compile/run via
  JNI `libkinn_android.so` (arm64-v8a); graceful degradation when the
  library is missing. No permissions.
- **CI**: Rust matrix (ubuntu/windows/macos) with release smoke tests;
  Studio tests incl. 10 real-wasm tests; Android APK pipeline (signing
  from `ANDROID_KEYSTORE_*` secrets only, apksigner verification,
  UNSIGNED label otherwise); Windows zip + NSIS; Linux DEB (installed
  and exercised on the runner) + AppImage (experimental) + tarball.

Test totals: **190 Rust tests** (26 kinrt, 111 kinc unit, 22 pipeline,
19 CLI, 12 ffi) and **51 Studio vitest tests**. Examples carry
`// expect:` blocks; error examples declare `// error:` /
`// runtime-error:` codes; the modules example is exercised through the
loader in pipeline and CLI tests.

Planned, not implemented (do not pretend otherwise): native codegen
(0.3), string interpolation (ADR-0013 deferred), string→number casts,
graphics (0.5), Studio desktop shell, `tokens` command on Android,
Android UI tests/emulator verification.

## Repository structure

```
compiler/                  Rust workspace (cargo test = 190 tests)
  crates/kinrt/            KIR instruction set, stack VM, disassembler
  crates/kinc/             lexer, parser, project, checker, lower, driver
  crates/kinn/             CLI binary (kinn)
  crates/kinn-ffi/         C-ABI bridge for wasm + native (cdylib+rlib)
  crates/kinn-android/     JNI glue for the app (only crate with a dep)
android/                   KINN Studio app (Kotlin, AGP 8.5.2, minSdk 26)
studio/                    KINN Studio web shell (npm test = 51 tests)
  src/wasm/                kinn.wasm loader (real compiler in browser)
  src/storage/             IndexedDB persistence + .kinnproj format
examples/                  runnable spec: // expect: output blocks
examples/errors/           // error: CODE and // runtime-error: CODE
examples/modules/          multi-file project example
scripts/                   NSIS installer, Linux desktop entry + icon
.github/workflows/         ci, android, windows, linux pipelines
docs/                      all design documents (index in README)
```

Root files: README (status table + quick start), CHANGELOG, SECURITY,
AUDIT_REPORT, INSTALL, this file.

## Known problems (see docs/PROBLEM_REGISTRY.md for all)

- P-004 string interpolation deferred (ADR-0013).
- AppImage packaging is experimental until verified on a clean machine.
- Android app: no `tokens` command; highlighting debounced (250 ms).
- P-012 license undecided.

## Current task and next milestone

0.2.0 is complete and committed. Next milestone **0.3.0 — native code
generation investigation** (docs/ROADMAP.md): backend spike (Cranelift
vs LLVM vs C transpilation, ADR-0002), constant folding, timing
reports, Studio desktop shell candidate. Close the v0.3.0 GitHub
milestone issues when verified.

## Conventions

- Commits: `feat(scope):`, `fix(scope):`, `docs:`, `chore:`, conventional.
- Rust: edition 2021, zero deps in the core; kinn-android is the only
  crate with a dependency (jni, ADR-0014); tests inside each module
  plus `crates/*/tests/`.
- Diagnostics carry stable codes (E1xxx lexer, E2xxx parser, E3xxx
  semantic, E4xxx runtime, E5xxx FFI, W/N warnings/notes, E21xx
  Studio-only) — see docs/LANGUAGE_SPEC_0.1.md.
- Every example doubles as a test; broken examples declare their
  expected error code.
- Build artifacts (`compiler/target`, `studio/node_modules`,
  `studio/dist`, `studio/public/kinn.wasm`, `android/.gradle`,
  `android/app/build`, `android/local.properties`,
  `android/app/src/main/jniLibs`) are gitignored. Never commit secrets;
  SECURITY.md describes token and signing hygiene.

## Most relevant documents

- docs/LANGUAGE_SPEC_0.1.md — normative grammar, semantics, codes
- docs/COMPILER_ARCHITECTURE.md — pipeline and testing strategy
- docs/RUNTIME_ARCHITECTURE.md — KIR and the VM
- docs/KINN_STUDIO.md — Studio web + Android features and limits
- docs/ROADMAP.md, docs/DECISIONS.md, docs/PROBLEM_REGISTRY.md
- AUDIT_REPORT.md — what is verified / partial / planned / broken
