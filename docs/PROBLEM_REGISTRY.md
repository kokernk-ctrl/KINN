# Problem Registry

Known bugs and limitations, tracked openly. Each entry has an ID, a
status, and where it is felt. Fixed entries move to the bottom with the
release that fixed them. This registry is the source for GitHub issues.

## Open

| ID | Area | Problem | Impact | Workaround |
| --- | --- | --- | --- | --- |
| P-004 | language | String interpolation deferred to 0.3 (ADR-0013) | concatenation + `as string` only | use `+` and casts |
| P-009 | checker | Unreachable-code warning (W3001) is per-block only; no cross-block control-flow graph | a few unreachable paths unreported | none |
| P-011 | lexer | Tabs count as one column; caret alignment can look off in tab-indented files | cosmetic in diagnostics | use spaces (editors insert spaces) |
| P-012 | infra | License undecided (ADR-0012) | external contributions blocked | owner decision pending |
| P-013 | language | No string-to-number casts (`"5" as int` rejected, E3025) | parse errors instead of conversion errors | 0.3 scope decision |
| P-014 | language | Selective/renamed exports unsupported; modules export **all** top-level functions and consts implicitly (value exports landed in 0.4.0 S1 / ADR-0028; the naming and hiding half stays open) | importers see every export; no `export` lists or rename syntax | pass values through functions; name consts so the implicit export is safe |
| P-015 | studio-android | Android app has no `tokens` command and no UI tests | parity gap with web Studio | use web Studio or CLI |
| P-016 | packaging | AppImage built via linuxdeploy is experimental until verified on a clean machine | may need distro-specific fixes | prefer the DEB or tarball |
| P-017 | android | On-device compiler verified by build + JNI symbol checks, **not yet on a physical device** | first device run may surface issues | CI artifacts exist for device testing |
| P-018 | backend-android | The CLI's **local** Gradle build provider is verified on the CI runner (ubuntu-latest ships Gradle+SDK: export → assembleDebug → APK, first 0.3.0 CI run) and the toolless path is tested deterministically; developer-configured SDK layouts (custom ANDROID_HOME/gradle wrappers) are untested | only the CI toolchain shape is proven | use Android Studio or CI for other setups |
| P-019 | backend-android | The CI-built backend APK is not yet installed/launched on a physical device (extends P-017) | runtime behavior on devices unconfirmed | install `kinn-android-backend-apk` artifact on a device |
| P-020 | backend-web | Site theming tokens (per-site CSS customization) are not in the grammar yet; the generated stylesheet is a single clean default | every generated site looks the same | edit the generated styles.css (it is plain CSS); tokens designed in ADR-0027, ship in 0.4.0 |
| P-021 | studio-android | No automated UI tests for the 0.3 build/preview flows (same gap as 0.2 UI) | regressions possible in app flows | CI builds the app; manual verification on device pending |
| P-022 | studio-web | The in-Studio preview is sandboxed and self-contained (inlined CSS/JS, postMessage navigation); `file://` hosting of the real dist differs slightly | preview ≈ but not == the exported dist | use Download .zip + a local server for exact rendering |
| P-023 | ffi | Binary artifacts cannot pass through the wasm/JNI build bridge (text files only); asset copying is CLI-only | Studio exports contain no binary assets | use `kinn build` locally for asset-bearing projects |
| P-024 | packaging | linux.yml/windows.yml still name artifacts and DEB control fields `0.2.0` while the release is 0.3.0 | stale artifact names mislead users | artifacts are content-correct; version fix scheduled for 0.4.0 S11 |
| P-025 | docs | Doc drift: SYNTAX.md documents only 0.1, COMPILER_ARCHITECTURE.md says "three crates" (workspace has five) and "native codegen (planned, 0.3)", CLI_DESIGN.md "Planned" list is stale | new contributors get an outdated picture | AUDIT_REPORT and README carry the accurate status; refresh scheduled for 0.4.0 S11 |
| P-026 | studio-web | Studio VFS and `.kinnproj` v1 are flat (name regex forbids `/`); the compiler/CLI accept subdirectory imports | 0.4 multi-folder projects (pages/, components/) cannot be edited in Studio yet | use flat imports (`components_nav.kin`) or the CLI; Vfs v2 scheduled 0.4.0 S10 |
| P-027 | studio-web | Preview has no runtime error/console forwarding and the CLI prints the no-website guard as a plain message while FFI emits code E3038 | runtime bugs invisible in Studio preview; diagnostic asymmetry across hosts | runtime error envelope + console forwarding designed in 0.4.0 (C.7/C.8 of DYNAMIC_WEB_ARCHITECTURE.md); E3038 alignment in S11 |
| P-028 | checker | Website diagnostics emitted inside `check_website` are attributed to the **last project file**, not the entry file, when a website program has imports (the pass-0 loop leaves `c.file` at the last unit; verified with a reproducer on 2026-11-22 — an E3032 in the entry renders under `lib.kin`) | multi-file website programs render diagnostics under the wrong filename; single-file programs are unaffected | keep website programs single-file until the one-line fix (restore `c.file` to the entry path before `check_website`, mirroring the app path) lands — found in the S3 slice 1 audit, deliberately not fixed in that slice (website checking was gated untouched) |
| P-029 | language/checker | No diagnostic code is registered for **unknown app pages** — a `route "x" -> Missing` or `not_found Missing` pointing at an undeclared page passes `kinn check` (B.9 says "E3032-family", but E3032 is website-scoped and frozen; B.11 assigns no app code). Duplicate **page** or **component** names inside an app are likewise unregistered (E3042 covers only state/computed/prop names) | app programs can reference non-existent pages undetected until the S3 name-resolution slices; duplicate page/component names go unreported | surfaced for code assignment in the next S3 slice (needs a new E3063+ code or an owner ruling); targets are parsed with positions, so the check itself is cheap once the code exists |

## Verification debts

| ID | Area | Debt |
| --- | --- | --- |
| V-001 | android | `kinn-android` builds for `aarch64-linux-android` (CI + local NDK build, JNI exports verified with llvm-nm), but no run on a physical device yet |
| V-002 | graphics | Zero graphics code exists (planned 0.5); no window/context verification yet |
| V-003 | packaging | NSIS installer and AppImage artifacts are produced by CI; manual installation has not been exercised by a human on Windows/a clean Linux desktop yet |

## Fixed

| ID | Release | Fix |
| --- | --- | --- |
| F-001 | 0.1.0 | Studio gutter collapsed line numbers (missing `white-space: pre`) — caught in visual QA |
| F-002 | 0.1.0 | Studio editor check mistook the `*` in `6 * 7` for a mode directive (E2110 false positive) — caught in QA, regression test added |
| F-003 | 0.1.0 | Desktop→mobile resize left all Studio panes visible; panes now rely on media queries only |
| F-004 | 0.1.0 | Kinrt VM tests initially lacked the `Const(Unit); Return` epilogue, exposing the E4999 guard — fixed before first commit |
| F-005 | 0.2.0 | P-001: parser stops at the first syntax error → statement-level recovery with sync points, E2099 cap, tests |
| F-006 | 0.2.0 | P-002: `while true` divergence not recognized → diverging-loop analysis (break-aware), tests |
| F-007 | 0.2.0 | P-003: KIR slots never reused → free-list allocator per function, slot-reuse test |
| F-008 | 0.2.0 | P-005: Studio files in-memory only → IndexedDB persistence with debounced autosave |
| F-009 | 0.2.0 | P-006/P-007: Studio check editor-grade, run disabled → real compiler via kinn.wasm (check + run + multi-file) |
| F-010 | 0.2.0 | P-008: no CI → four workflows (test matrix, Android sign+verify, Windows, Linux), all green on develop |
| F-011 | 0.2.0 | P-010 (0.1 boundary): for loops, arrays, casts, modules, stdlib natives all shipped in 0.2.0 |
| F-012 | 0.2.0 | Android MainActivity termPrint lost its color parameters during a rewrite — caught by the local gradle build, fixed before merge |
| F-013 | 0.4.0 | P-014 (value half): cross-module value exports did not exist — top-level `const` with qualified reads `m.NAME` shipped in S1 (ADR-0028); selective/renamed exports remain open under the narrowed P-014 |
| F-014 | 0.4.0 | S2 grammar findings, fixed before landing: (1) `each items as item` collided with the cast operator — `as` + identifier is now unambiguously the loop marker (cast types are keywords; casts in iterables need parens); (2) nested-brace recovery initially consumed a section's own `}` as the app's — all section/view/flat-block parsers now recover inside their own braces (regression tests); (3) the architecture doc's B.7 example used `when` inside a handler and capitalized type words — B.6/B.5 normative text (official statements, `if`) and the real lowercase types win; doc examples corrected with S2 |
| F-015 | 0.4.0 | S2 review finding: `app ""` (and whitespace-only names) pass `kinn check` silently — the parser accepts any string and no S2 rule validates the content. **Fixed in S3 slice 1**: new checker code **E3062** "the app name must not be empty", mirroring E3033 (trim check, positioned, hint, frozen by tests). Ruled out: reusing E3033 (registered as website-scoped, ADR-0011 frozen meanings) and folding into E3051/E3052 (frozen presence-rule meanings — placement/uniqueness, not content) |
| F-016 | 0.4.0 | CI findings exposed by the S1/S2 pushes, fixed in 77c3e6d. **Errata (2026-11-22): the fix's commit message claimed the S1/S2 commits "had never run on GitHub" — false.** Every push ran on GitHub (Actions API, verified 2026-11-22): S1 (6905f74, pushed 2026-11-21T21:49:13Z) triggered four workflows — ci success (run 44953504346), windows success (44953504318), android success (44953504410), linux **failure** (run 44953504257): linux.yml's DEB smoke test still asserted the pre-S1 four-line output of `examples/modules/main.kin` — S1's const-export line (`95`) was added to the example but the workflow golden was not updated. That red linux workflow went unnoticed through the S2 (699c43d, 23:09:47Z, run 44960409403), S2-review (711cb6e, 23:17:43Z, run 44961075439) and S3-plan (2cb7ebe, 23:18:49Z, run 44961164227) pushes — red from 21:49:13Z to the fix at 23:24:45Z (~1h35m). The macOS race in kinn-ffi's module-registry tests (`missing_module_is_an_e3017`'s `kinn_clear_modules()` could wipe the module `modules_register_and_resolve` had just registered, under Rust's parallel test threads) is intermittent and pre-existing — macOS passed on 6905f74/699c43d/711cb6e and **first failed on 2cb7ebe** (ci run 44961164278, job `rust tests (macos-latest)`). Fix verified green on all four workflows (linux 44961665490, android 44961665523, windows 44961665547, ci 44961665552): golden updated to five lines; registry-touching tests hold a test-only lock for their whole body. Commit messages are immutable history — this entry and the CHANGELOG errata are the correction of record; two standing rules added to AI_CONTEXT prevent recurrence (watch every workflow to green; update all goldens in the same commit) |
