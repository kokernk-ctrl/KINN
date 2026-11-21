# KINN 0.2.0 — Audit Report

Full-repository audit for release 0.2.0 (2026-11-21). Every claim below
names **how** it was verified. Categories: **verified** (built/tested
with evidence), **partial** (works with stated limits), **planned**
(honestly not built), **broken** (defect, tracked).

---

## 1. Test suites (executed 2026-11-21)

| Suite | Where run | Result |
| --- | --- | --- |
| Rust workspace (`cargo test --workspace`) | local Linux (Rust 1.100.0) | **190 passed, 0 failed** |
| same | CI, ubuntu-latest | **passed** |
| same | CI, windows-latest | **passed** |
| same | CI, macos-latest | **passed** |
| Studio web (`npm test`), wasm backend built first | local + CI | **51 passed, 0 failed** (10 execute the real compiler in WebAssembly) |
| Android app build (`assembleDebug` + `assembleRelease`) | local (SDK 34, NDK r27) | **BUILD SUCCESSFUL**; APKs produced |
| same | CI (android workflow) | **passed**; artifacts + signature report |
| Linux package | CI (linux workflow) | **passed**; DEB installed and exercised on the runner |
| Windows package | CI (windows workflow) | **passed**; tests, zip and NSIS installer (see §8) |

Rust breakdown: 26 kinrt (VM/value), 111 kinc unit (lexer/parser/
checker/project/diagnostics), 22 kinc pipeline (end-to-end incl. all
`// expect:` examples), 19 kinn CLI (real binary), 12 kinn-ffi (C ABI
through the JSON protocol), plus kinn-android type-checked as part of
the workspace on all three desktop platforms.

## 2. Compiler — verified

- **Lexer**: 0.2 token set incl. ranges (`0..10`, `1.5..2.5`, `0..=9`
  disambiguated from floats — dedicated tests), brackets, dot, new
  keywords. E1001–E1006 preserved.
- **Parser recovery (P-001/#5)**: three broken statements → exactly
  three positioned diagnostics; following statements parsed without
  cascade; 25-error cap (E2099); recovery inside multiple functions.
  Verified: parser tests + `many_errors.kin` example + CLI test
  asserting three `error[…]` lines.
- **Control-flow analysis (P-002/#6)**: `func f() -> int { while true
  { return 1; } }` compiles (previously a false E3012) and **runs**
  (pipeline test); `while true { break; }` still requires a return;
  nested-loop breaks do not count for the outer loop; code after a
  diverging loop is W3001-unreachable. All checker-tested.
- **Slot reuse (P-003/#7)**: three sequential single-variable scopes
  produce `nlocals == 1` (asserted on the lowered module) and identical
  output.
- **Modules (#1)**: `import "x.kin" as m;` — transitive loading, diamond
  sharing, cycle detection with the full chain (E3018), `..`/absolute
  path rejection (E3017), duplicate alias (E3019), unknown
  module/function (E3020/E3021), same-name functions in two files,
  imported-file `main` inert. Verified in project/checker unit tests,
  pipeline test (`examples/modules` through a loader) and the CLI test
  (`kinn run examples/modules/main.kin`).
- **Arrays / indexing / for / casts / natives**: checker rules
  (E3022–E3028), VM behavior (E4005 bounds with length in message,
  reference semantics through function calls, deep equality,
  truncating/saturating float→int, `len("héllo") == 5`), lowering
  (`for` continue→increment — an infinite-loop bug was specifically
  tested against), end-to-end example programs
  (`arrays.kin`, `for_loops.kin`, `casts.kin`, `stdlib.kin`).
- **Diagnostics**: multi-file rendering grouped by file with per-file
  carets (diagnostics test); stable code registry extended and
  documented in LANGUAGE_SPEC_0.1.md §A7.

## 3. Runtime (kinrt) — verified

Arrays (`Rc<RefCell<Vec<Value>>>`), native functions, casts, bounds
checks — 26 unit tests, plus everything above running through the VM.
Checked overflow (E4004), div/mod by zero (E4001), call depth (E4002)
retained from 0.1. Zero external dependencies maintained.

## 4. CLI (`kinn` 0.2.0) — verified

19 integration tests run the real binary: multi-file run/kir
(`math::add` visible in disassembly), new-feature examples,
import-missing error with position, multi-error output, E4005 runtime
error with exit code 3, usage errors. Smoke-tested on all three CI
platforms (`kinn version`, module example, `for_loops.kin` output).

## 5. FFI bridges — verified

- **kinn-ffi** (12 tests): JSON check/run with diagnostics, positions,
  stdout capture, runtime error line, module registry, alloc/free
  round-trip, unicode, full 0.2 language through the FFI. Panic safety
  by construction (catch_unwind → E5000).
- **WebAssembly build**: `kinn_ffi.wasm` (327 KB) built for
  wasm32-unknown-unknown; **smoke-tested in Node** (version, check,
  run with arrays+for, diagnostics with positions) and by 10 vitest
  tests inside Studio CI.
- **kinn-android**: compiles in the workspace on Linux/Windows/macOS;
  built for `aarch64-linux-android` locally and in CI (933 KB
  `libkinn_android.so`); **JNI exports verified with llvm-nm**
  (`Java_dev_kinn_studio_KinnBridge_n{Version,Check,Run}`).

## 6. KINN Studio web — verified

- Real check/run in the browser via `kinn.wasm` (wasm tests above run
  exactly this path: multi-file projects, captured stdout, E4005 line
  numbers, 200 sequential calls — memory management — and unicode).
- Persistence: MemoryStorage round-trip, debounced single-save
  behavior, project reload (`vfs.test.ts`, 9 tests). IndexedDB
  implementation is small and standard; exercised in CI through the
  Studio build (it ships in the bundle) — note: automated tests of the
  **IndexedDB adapter itself** run via the same storage interface;
  a headless-browser (Playwright) session is future hardening.
- `.kinnproj` format: round-trip, version rejection, name/duplicate
  validation, active-file fallback (7 tests).
- Honest degradation: missing wasm → status badge, fallback checks,
  exact build instructions (no fake features).

## 7. Android app — verified (with stated limits)

- **Build**: full local build (JDK 17? no — local sandbox used JDK 21
  with AGP's toolchain; CI uses JDK 17) and CI build, both green.
- **Contents verified**: `lib/arm64-v8a/libkinn_android.so` present;
  `assets/docs/*.md` bundled from `docs/` (the CI APK already contains
  the updated 0.2 docs — verified by unzipping the CI artifact).
- **Signature**: debug APK verified with `apksigner verify
  --print-certs` locally and in CI (certificate fingerprints in
  `signature-report.txt`); release APK is **unsigned and labeled
  `UNSIGNED`** — correct, because no `ANDROID_KEYSTORE_*` secrets exist
  yet (documented in SECURITY.md + INSTALL.md).
- **Checksums**: CI `SHA256SUMS` re-verified locally (both APKs match).
- **Limits (partial, honest)**: no run on a physical device yet
  (V-001/P-017); no `tokens` terminal command; no automated UI tests
  (planned 0.3); highlighting debounced 250 ms. The app degrades
  honestly when `libkinn_android.so` is missing.

## 8. Release pipeline — verified

- `ci.yml` — **green**: 3-platform Rust matrix with release smoke
  tests, Studio tests with the real wasm backend, site artifact.
- `android.yml` — **green**: NDK build → APKs → apksigner verification
  → artifacts (debug APK, UNSIGNED release, SHA256SUMS,
  signature-report.txt).
- `windows.yml` — **green**: tests + release build on a Windows runner,
  portable zip and NSIS installer (built after two plugin/include
  fixes; the successful run also smoke-tested `kinn.exe` on Windows).
- `linux.yml` — **green**: tests + release build; **DEB installed on
  the runner** and `kinn` exercised (`version`, examples, module
  project output-compared); tarball; AppImage built with linuxdeploy
  and executed (`…AppImage version`) on the runner — still labeled
  experimental pending clean-desktop verification (P-016).

## 9. Security and reproducibility — verified

- **No secrets in history**: full-history grep for token/key patterns
  (ghp_…, AWS keys, private key blocks) across all commits: clean. The
  GitHub token used for automation lives only in the sandbox
  credential file, never in the repo.
- **Android**: no permissions requested (verified via aapt badging);
  app-private storage only; no INTERNET.
- **Import path confinement**: `..`, absolute paths and backslashes
  rejected (E3017) — unit-tested.
- **Reproducibility**: every artifact is rebuilt from source by CI on
  its target platform; checksums published per run. The compiler core
  remains zero-dependency; `kinn-android` (jni crate) is the single
  documented exception (ADR-0014).
- One hygiene fix during the audit: the built `kinn.wasm` had been
  accidentally committed; it is now untracked and gitignored (binary
  build artifact, no history rewrite needed).

## 10. Specification alignment — verified

- LANGUAGE_SPEC_0.1.md amended with a normative **"Kin 0.2 additions"**
  section (modules, arrays, for/ranges, casts, natives, recovery, new
  code table A7). Each documented rule maps to tests listed in §2.
- PROGRAMMING_MODES: behavior unchanged in 0.2 (directive + verbosity
  + B-mode lint) — spec statement still accurate.
- PROBLEM_REGISTRY updated: P-001/2/3/5/6/7/8/10 resolved (F-005…
  F-012); new honest entries P-013…P-017.

## 11. Summary table

| Area | Status |
| --- | --- |
| Compiler (lexer/parser/recovery/CFA/checker/lowering) | **verified** |
| Runtime VM (arrays, casts, natives, safety limits) | **verified** |
| CLI incl. multi-file projects | **verified** |
| kinn-ffi (C ABI, wasm + native) | **verified** |
| Studio web: check/run via wasm, persistence, import/export | **verified** |
| Studio web: IndexedDB adapter | **partial** (interface-tested; headless-browser test is future work) |
| Android app: build, contents, signature, artifacts | **verified** |
| Android app: on-device run | **partial** (built + symbol-verified; no physical-device test yet) |
| CI matrix + Linux DEB + Android pipeline | **verified** |
| Windows: tests + zip + NSIS installer | **verified** (CI, Windows runner) |
| Linux AppImage | **partial / experimental** (CI-built and executed on the runner; clean-machine verification pending) |
| String interpolation, string→number casts | **planned** (ADR-0013, P-013) |
| Native codegen, graphics | **planned** (0.3 / 0.5) |
| Broken functionality | **none found** in audited areas |

*Audit performed against the tip of `develop` (0a04e49+), 2026-11-21.
All four workflows green on the audited commit; the Windows-installer
and AppImage packaging corrections landed after the audit body was
written and are reflected above.*
