# KINN Studio

KINN Studio is the terminal-inspired environment around the Kin
language. It exists in two shipped forms — the **web shell** (runs in
any browser, compiler included via WebAssembly) and the **Android app**
(on-device compilation on ARM64) — plus a future desktop shell. This
document describes what exists today (0.2.0, verified) and what is
planned, component by component.

## Design system

| Token | Value | Use |
| --- | --- | --- |
| background surfaces | `#0b0d10` → `#151922` | app → panels |
| accent | `#ff7a1a` | brand, caret, prompt, keywords, focus |
| ink | `#d7dce3` / `#9aa3af` / `#67707c` | text / dim / faint |
| syntax | orange keywords, teal types, sand numbers, olive strings, mauve bools | muted terminal palette |
| type | monospace everywhere (JetBrains Mono → Cascadia → ui-monospace stack) | 13px editor, 12–14px UI |
| motion | 120ms color/border transitions only | `prefers-reduced-motion` respected |

No gradients, no shadows-as-decoration, no emoji, one accent color. The
Studio should feel like a well-kept terminal, not a marketing page.
Both the web shell and the Android app follow this system; the app uses
the same palette (see `android/app/src/main/res/values/colors.xml`).

## Web Studio — implemented in 0.2.0 (verified)

- **Project explorer** — file tree over the project, active-file
  marking, in-app file creation (validated `.kin` names), delete/rename
  via the terminal (`rm`, `mv`).
- **Code editor** — textarea-over-highlight architecture with an
  editor-side TypeScript lexer mirroring the Rust lexer (0.2 token set:
  `for`/`in`/`as`/`import`, brackets, `..`/`..=` ranges); line-number
  gutter; Tab inserts two spaces; Enter auto-indents; Ln/Col status
  bar; `MODE B|S|A` badge.
- **Real compiler in the browser** — `check` and `run` use
  `public/kinn.wasm` (the `kinn-ffi` crate compiled for
  `wasm32-unknown-unknown`): the same Rust compiler and VM as the CLI,
  multi-file projects included (`import` resolves inside the project).
  Program output lands in the terminal; diagnostics (with file, line,
  column) in the problems panel.
- **Honest degradation** — when `kinn.wasm` is not built, the top-bar
  badge says `compiler: not built`, check/run fall back to editor
  checks and print the exact build command. Nothing pretends.
- **Persistence** — the project persists in IndexedDB (debounced
  autosave after edits, `save` command, Ctrl+S); reloads restore it.
- **Import/export** — `export` downloads a `.kinnproj` JSON project
  file; `import` loads a `.kinnproj` project or a single `.kin` file.
  The format is versioned and validated (same format as Android).
- **Problems panel** — full compiler diagnostics or editor checks,
  clickable to jump to the line.
- **Integrated terminal** — command loop with history (↑/↓) and 19
  implemented commands: `help`, `version`, `about`, `clear`, `ls`,
  `cat`, `open`, `new`, `rm`, `mv`, `tokens`, `check`, `mode`,
  `syntax`, `run`, `save`, `export`, `import`.
- **Responsive layout** — three-pane grid ≥901px; below 900px a tab
  bar (Files / Editor / Terminal) switches panes.
- **Keyboard** — `Ctrl+Enter` checks; `Ctrl+S` saves to the browser.

### Editor checks (fallback scope)

Editor-grade checks — lexical errors (E1xxx mirrors), delimiter balance
(E2101/E2102), mode-directive validation (E2110), duplicate functions
(E3002). Real and tested; used automatically only when the wasm
backend is unavailable.

## Android app — implemented in 0.2.0 (verified)

`android/` — Kotlin, AGP 8.5.2, minSdk 26 (Android 8.0), targetSdk 34,
**no permissions** (fully offline, no INTERNET permission).

- **Projects** — multiple projects in app-private storage
  (`filesDir/projects`), create/delete with confirmation, active
  project remembered; new projects start with a runnable `main.kin`.
- **Editor** — monospace editor with line-number gutter, Kin syntax
  highlighting (debounced 250 ms, skipped for files >60 KB), saves to
  device storage on every edit.
- **Terminal** — 19 commands mirroring the web Studio (`projects` /
  `project <name>` replace the web's single-project commands; `docs`
  opens the bundled documentation), history via the keyboard, colored
  output lines.
- **Problems** — compiler diagnostics (when the native backend is
  present) with file:line:col, click to jump into the editor.
- **Docs** — the repository's markdown docs are bundled into the APK
  at build time (gradle copy task) and readable offline.
- **Import/export** — SAF pickers: import a `.kinnproj` project or a
  `.kin` file; export the active project as `.kinnproj`.
- **On-device compiler (ARM64)** — `check`/`run` call
  `libkinn_android.so` through JNI (`dev.kinn.studio.KinnBridge` →
  `kinn-android` crate → `kinn-ffi`): the real compiler, in-process,
  offline. On non-ARM64 devices or builds without the library, the app
  states plainly that the native compiler is unavailable.

### Android verification status

Built and exercised locally in this sandbox: SDK 34 + NDK r27, full
gradle build, `libkinn_android.so` (933 KB) with verified JNI exports
(`llvm-nm`), debug APK (6.6 MB) containing the native library and the
docs, signature verified with `apksigner` (Android Debug certificate).
CI reproduces the build and additionally signs the release APK when
the `ANDROID_KEYSTORE_*` secrets exist. **Not yet verified on a
physical device** — that is the honest limit of this claim; on-device
verification is scheduled with the 0.3/0.5 device work.

## Planned

| Milestone | Feature |
| --- | --- |
| 0.3 | desktop shell (webview wrapper) around the same web build |
| 0.3 | Android UI tests on an emulator in CI |
| 0.4 | VM debugging in Studio (step, breakpoints) |
| 0.5 | graphics preview when the GLES backend lands |
| later | diff view, project templates per mode, Android `tokens` command |

The build targets this future: the web shell has zero runtime
dependencies and a relative base path, so `dist/` runs from `file://`,
inside a desktop webview, and inside an Android webview unchanged.

## Architecture

```
studio/src/
  main.ts               bootstrap and wiring (storage + wasm backend)
  theme.css             design tokens, layout, syntax colors
  vfs.ts                project file system + persistence hooks
  storage/storage.ts    ProjectStorage (IndexedDB + memory), .kinnproj format
  wasm/kinnwasm.ts      kinn.wasm backend: check/run, module registration
  syntax/lexer.ts       Kin tokenizer (editor side; mirrors kinc's lexer)
  syntax/highlight.ts   tokens → per-line colored segments
  diagnostics/checks.ts editor quick checks + mode detection
  ui/explorer.ts        file tree
  ui/editor.ts          editor component
  ui/terminal.ts        command loop (real check/run)
  ui/problems.ts        diagnostics list
  ui/tabs.ts            mobile tab navigation
scripts/build-wasm.mjs  compiles kinn-ffi → public/kinn.wasm
samples/                bundled .kin demo files

android/app/src/main/java/dev/kinn/studio/
  MainActivity.kt       tabs, editor, terminal, SAF import/export
  KinnBridge.kt         JNI bridge (graceful when lib is missing)
  ProjectStore.kt       on-device projects + .kinnproj parsing
  CompilerResults.kt    FFI JSON → diagnostics
  KinSyntax.kt          editor highlighting
  Adapters.kt           project/file/problem lists
  DocsActivity.kt       offline docs viewer
```

Testing: web `npm test` (51 vitest tests — 10 run the real compiler in
WebAssembly; they are skipped loudly, not silently, when `kinn.wasm`
is not built) plus `npm run build` (strict `tsc` + vite). Android:
built locally and in CI (`assembleDebug` + `assembleRelease`, apksigner
verification); UI tests are planned for 0.3, and on-device testing is
explicitly not claimed until it happens.
