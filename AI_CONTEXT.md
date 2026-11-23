# AI Context

Status snapshot for AI assistants (and humans) working on KINN.
**Last updated: 2026-11-23, release 0.3.0 + 0.4.0 S1–S3 slice 3 +
the let→fin rename (ADR-0029, develop past the slice-3 push).** Update this
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

Test totals: **397 Rust tests** (280 kinc unit incl. app grammar,
const, website, app validation, scope resolution, the fin rename; 28
pipeline, 41 CLI incl. 5 manifest, 16 ffi) and **66 Studio vitest
tests**
(15 real-wasm incl. build targets). Examples carry `// expect:`
blocks; error examples declare codes; `examples/web/site.kin` is the
website example; `examples/web/tasks_app.kin` is the 0.4.0 app
grammar showcase (parses and checks — scope resolution since slice 3;
build guards say what is not implemented
yet); `examples/modules/` demonstrates multi-file projects with const
exports (0.4.0 S1).

Planned, not implemented (do not pretend otherwise): the remaining
app semantics — prop signatures at instantiation sites (E3040/E3041),
handler subset (E3045/E3046, W3040), computed cycles (E3044),
stores/fields/animations (E3047–E3061, W3042–W3044) (0.4.0 S3 slice
4+, P-030 narrowed) — app lowering/AppModel (S4), KIR→JS + runtime
(S5–S6), routing/forms/persistence (S7–S9), package manager/registry
(spec in MODULE_SYSTEM.md), native codegen for console programs
(0.4+), string interpolation (ADR-0013), graphics (0.5), Android
`tokens` command, on-device backend-APK install (P-019), Studio
desktop shell. An `app` program parses and checks (names, targets,
scope resolution since slice 3) but does not build — the CLI says so.

## Repository structure

```
compiler/                  Rust workspace (cargo test = 397 tests)
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
- P-030 narrowed 2026-11-23: app scope resolution IS checked (slice 3
  — E3066/E3043/E3058/W3041, binding typing, param/store builtins by
  name and type); still unchecked: handler subset, prop signatures at
  instantiation sites, computed cycles, stores/fields/animations
  (slice 4+). Page targets resolved since slice 2 (P-028/P-029 closed
  as F-017/F-018). Param/store context restriction enforced at S6.

## Current task and next milestone

0.3.0 is complete and committed (web vertical slice, Android backend,
Studio integration, package foundation, CI for real targets). Current:
**0.4.0 — Dynamic web applications (owner directive, 2026-11-22)**.
Phase 1–4 delivered as `docs/DYNAMIC_WEB_ARCHITECTURE.md` +
ADR-0023–0028. Part E: D1–D3/D6 by directive + silence clause; **D4
(`kinn serve`) and D5 (Playwright in CI) explicitly owner-confirmed
2026-11-22** — Playwright proposed to land with S6, mandatory at the
S11 browser gate; neither implemented yet. Phase 5 landed S1
(cross-module `const`, ADR-0028), S2 (`app` grammar E2031–E2039,
presence E3051/E3052, honest guards), the S2 review pass (B.0 = 79
words, E2002 `as` hint, B.4 fix) and the F-016 CI fix — errata
recorded: the S1/S2 commits DID run on GitHub (77c3e6d's message
claimed otherwise; linux was red ~1h35m from the S1 push, the macOS
race first failed at 2cb7ebe). **S3 slice 1 landed: app declaration
validation — E3062 (empty app name, F-015 closed), E3055–E3057
(route rules), E3042 (duplicate state/computed/prop names per
scope); registered in LANGUAGE_SPEC §C2; +21 tests, workspace 357
green, 0.3.0 web dist byte-identical, Studio 66/66; P-028 (website
diagnostic file attribution) and P-029 (no code for unknown app
pages / duplicate page names) surfaced in the audit. S3 slice 2
landed (P-028 fix + P-029 code assignment, architect ruling
2026-11-22): P-028 fixed as F-017 (website diagnostics name the
entry file in multi-file programs); E3063 (unknown app page target
at every page-target position), E3064 (duplicate page name,
case-insensitive), E3065 (duplicate component name, case-sensitive)
assigned from the free E3063–E3069 span, E3066–E3069 reserved;
pages and components are separate namespaces; registered in B.11 +
LANGUAGE_SPEC §C2; +12 tests (incl. the P-028 regression), workspace
370 green, 0.3.0 web dist byte-identical; P-030 registered for the
remaining unchecked app semantics. **Owner decision 2026-11-23
(ADR-0029): `let` was renamed to `fin`** (fin = final value;
`var`/`const` unchanged; codegen identical, 0.3 web dist
byte-identical). Intentionally breaking, no backward compatibility
for the keyword: legacy `let x = ...;` gets **E2017** ("`let` was
renamed to `fin`; use `fin`") then parses as `fin` (one diagnostic
per use, no cascade); `let` stays a legal identifier otherwise.
First free parser code (F-019). Repo fully migrated (examples,
tests, docs, Studio web + Android); workspace 380 green. **S3 slice
3 landed (audit approved 2026-11-23): scope resolution + binding
typing — E3066 (unknown name in the app scope, the E3001 sibling),
E3043 live (props/computed/event builtins read-only in handlers;
E3004 stays for `fin`/consts), E3058 (`param("…")` must name a
declared route parameter), W3041 (handler local shadows state),
`field`/`input bind` targets must be state keys, param/store/B.10
builtins registered by name and type (known limit: param/store names
recognized; context-restricted invocation enforced at S6), array
`+`/`+=` app-scope-only (B.7); B.3/B.4/B.5/B.6/B.10 errata; the
showcase's `due` state key added; +17 tests, workspace 397 green,
0.3.0 web dist byte-identical; P-030 narrowed.** Next:
**slice 4 = handler subset (E3045/E3046, W3040) + prop checking at
instantiation sites (E3040/E3041); E3067–E3069 stay reserved;
Playwright-with-S6 APPROVED.**
Carry-overs: string casts/interpolation decision, native codegen
spike, structs/records ADR (D2 stays out of 0.4.0).

## Standing rules

- **A push is not done until every triggered workflow has been watched
to green.** Report the run IDs together with the final SHA; no success
claim before GitHub confirms. (Added 2026-11-22 after the F-016
errata: a red linux workflow went unnoticed for ~1h35 across four
pushes.)
- **Any commit that changes observable behavior updates all golden
assertions in the same commit**, including the packaging smoke tests
(linux DEB, windows, android). S1's example change without the
linux.yml golden update was exactly this miss.
- **Credentials come from the environment, never from chat**;
environment injection is preferred. When the sandbox cannot persist
env between calls, a transient 0600 file OUTSIDE the repo — sourced
without echoing and deleted at session end — is acceptable, and its
use must be disclosed. Never in the repo, git config, remote URLs,
or logs. If a token is pasted into a conversation anyway: use it at
most once for the requested operation, and the owner revokes it
immediately after the push. A pasted token is a compromised token.
(Amended 2026-11-22, architect ruling: the transient-file exception
replaces the previous no-files-at-all wording; the earlier PAT was
revoked after chat exposure.)

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
