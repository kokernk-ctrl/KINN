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
| P-014 | language | Cross-module globals/selective exports unsupported (ADR-0015 boundary) | modules export functions only | pass values through functions |
| P-015 | studio-android | Android app has no `tokens` command and no UI tests | parity gap with web Studio | use web Studio or CLI |
| P-016 | packaging | AppImage built via linuxdeploy is experimental until verified on a clean machine | may need distro-specific fixes | prefer the DEB or tarball |
| P-017 | android | On-device compiler verified by build + JNI symbol checks, **not yet on a physical device** | first device run may surface issues | CI artifacts exist for device testing |

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

| P-018 | backend-android | The CLI's **local** Gradle build provider is verified on the CI runner (ubuntu-latest ships Gradle+SDK: export → assembleDebug → APK, first 0.3.0 CI run) and the toolless path is tested deterministically; developer-configured SDK layouts (custom ANDROID_HOME/gradle wrappers) are untested | only the CI toolchain shape is proven | use Android Studio or CI for other setups |
| P-019 | backend-android | The CI-built backend APK is not yet installed/launched on a physical device (extends P-017) | runtime behavior on devices unconfirmed | install `kinn-android-backend-apk` artifact on a device |
| P-020 | backend-web | Site theming tokens (per-site CSS customization) are not in the grammar yet; the generated stylesheet is a single clean default | every generated site looks the same | edit the generated styles.css (it is plain CSS); state module planned 0.4 |
| P-021 | studio-android | No automated UI tests for the 0.3 build/preview flows (same gap as 0.2 UI) | regressions possible in app flows | CI builds the app; manual verification on device pending |
| P-022 | studio-web | The in-Studio preview is sandboxed and self-contained (inlined CSS/JS, postMessage navigation); `file://` hosting of the real dist differs slightly | preview ≈ but not == the exported dist | use Download .zip + a local server for exact rendering |
| P-023 | ffi | Binary artifacts cannot pass through the wasm/JNI build bridge (text files only); asset copying is CLI-only | Studio exports contain no binary assets | use `kinn build` locally for asset-bearing projects |
