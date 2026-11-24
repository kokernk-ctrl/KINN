# KINN Studio NEXT — Experimental Terminal-First Interface (0.4)

Studio NEXT is the experimental redesign of KINN Studio: a dark
developer environment inspired by terminals, command-line interfaces
and resource managers. It ships **alongside** the classic Studio —
both entries are built from the same sources, share the same project
format and the same compiler backends, and can be switched in both
directions at any time.

- Classic Studio: `studio/index.html` — **unchanged behavior**, got one
  additive `Next` link in its top bar.
- Studio NEXT: `studio/next.html` — the interface described here; its
  settings modal and its top bar both link back to `index.html`.

NEXT is isolated in `studio/src/next/` and reuses the proven core
services (`vfs`, `syntax`, `diagnostics`, `wasm`, `storage`, `build`)
without modifications. The old UI components (`src/ui/*`,
`theme.css`) are not imported by NEXT, so a rollback is a deletion of
one directory plus two config lines — nothing else ever needs to
change.

## Design identity

| Token | Value | Use |
| --- | --- | --- |
| Background | `#0B0C0E` | app, editor, terminal scrollback |
| Primary surface | `#121417` | top bar, explorer, bottom panel, status bar |
| Secondary | `#191C20` | buttons, hover states, echo lines |
| Elevated | `#20242A` | find bar, palette selection, modals |
| Border | `#2B3037` | 1px separators everywhere |
| Primary text | `#E6E8EB` | code, output, titles |
| Secondary text | `#9299A3` | metadata, hints, status items |
| Accent | `#FF8A32` | brand mark, keywords, active tabs, prompt, caret |
| Success / Warning / Error | `#39C47A` / `#E6B450` / `#E05252` | statuses and severities |

Typography: monospace (UI mono stack) for terminal, code, tree, status
bar and diagnostics; the system sans stack for nothing — NEXT is
monospace-first, with letter-spaced small caps for panel headers.
Radii are 3px, motion is limited to 120–160 ms color/opacity
transitions plus the pulse of a running status dot; everything
respects `prefers-reduced-motion`.

## Architecture

```
studio/
  index.html                 classic Studio (kept, + additive Next link)
  next.html                  NEXT entry (separate Vite input)
  src/
    next/
      main.ts                bootstrap: wiring, shortcuts, boot sequence
      next.css               NEXT design system (independent of theme.css)
      state.ts               workspace state (tabs, modified, layout) + persistence
      settings.ts            settings model, validation, localStorage, line metrics
      termcmd.ts             headless terminal command engine (real compiler calls)
      palette.ts             fuzzy matcher + PaletteCommand model (headless)
      layout.ts              desktop grid/resizers, mobile view switch, viewport
      ui/
        topbar.ts            identity, breadcrumb, build pill, actions
        explorer.ts          collapsible tree + file management + project info
        editortabs.ts        tab strip rendered from WorkspaceState
        codeeditor.ts        overlay editor + current-line + find bar
        terminal.ts          multi-session terminal view (engine per session)
        problems.ts          diagnostics list, cross-file jump
        output.ts            real build artifacts + actions
        statusbar.ts         measured status items only
        settings.ts          settings modal (every option really applies)
        paletteview.ts       command/file palette (keyboard driven)
        preview.ts           sandboxed preview of the real `build web` bytes
    vfs.ts                   shared: in-memory project persisted to IndexedDB
    syntax/lexer.ts          shared: Kin tokenizer (mirrors the Rust lexer)
    syntax/highlight.ts      shared: tokens -> colored segments
    diagnostics/checks.ts    shared: editor-level checks + mode detection
    wasm/kinnwasm.ts         shared: kinn-ffi wasm bridge (check/run/build)
    storage/storage.ts       shared: IndexedDB + .kinnproj format
    build/preview.ts, zip.ts shared: page assembly and .zip download
```

Vite builds both entries (`vite.config.ts` → `rollupOptions.input`);
both ship in `dist/` side by side.

## Component responsibilities

### Top bar
Identity (`KINN STUDIO · NEXT`), `project › file` breadcrumb, a build
status pill driven by **real** command statuses (idle / pulsing
running / green ok / red fail), `Run` and `Build` buttons that execute
terminal commands, the layout focus switch (editor/split/terminal),
the palette button, settings, and the way back to classic Studio.

### Explorer
A collapsible tree over the real project: root node + files (no fake
folders — the Kin project format is flat `name.kin`), per-file
rename/delete (with confirmation), `+ new file`, and a PROJECT INFO
section (project name, file count, active file, compiler version,
storage state). Modified files get the orange dot (see "Modified"
below).

### Editor tabs + code editor
Tabs are rendered strictly from `WorkspaceState`; the dot means
"modified since the last explicit save (Ctrl+S / `save`)" — the Vfs
already persists every keystroke to IndexedDB automatically, so the
dot is provenance, not risk. The editor is the same proven overlay
technique as classic (transparent textarea over a highlighted `<pre>`)
plus: current-line highlight bar, configurable indent, font-size
metrics shared through CSS variables, and a find bar with match count
and next/prev navigation (Ctrl+F, Enter/Shift+Enter, Esc).

### Terminal — the primary surface
Multiple sessions (up to 4; each has its own scrollback, input,
command history, and status dot: gray idle / yellow pulsing running /
green ok / red fail). The engine behind every session is
`termcmd.ts`, which drives the **actual compiler and VM** through the
wasm backend — the same Rust code the `kinn` CLI runs:

| Command | What really happens |
| --- | --- |
| `check [file]` | full compiler check (all diagnostics; falls back to editor checks only when kinn.wasm is missing — and says so) |
| `run [file]` | compile + execute in the VM; stdout rendered; exit 0 / compile error / runtime error with line |
| `build web` | real web backend; artifact list printed; OUTPUT panel gets the files |
| `build android` | real Android Gradle project export (honest: a project, not an APK) |
| `preview` | opens the exact built pages in the preview overlay |
| `tokens [file]` | token stream from the editor lexer |
| `ls / files <q> / cat / open / new / rm / mv` | live project file management |
| `mode / syntax / version / about / help / history` | information, all truthful |
| `save / export / import` | IndexedDB flush / .kinnproj download / host picker |

Anything unavailable states what is missing and how to fix it
(e.g. `kinn.wasm` build instructions) — nothing is simulated.

### Problems panel
Real compiler diagnostics (including warnings/notes) or editor-level
checks when the backend is missing. Clicking a problem opens its file
tab (diagnostics carry their own file name in multi-file projects) and
jumps to the line. Badges show total / errors / warnings.

### Output panel
Structured results of the last real build: target, site name, page
count, per-file sizes, plus session log lines of statuses. Actions
work only on what exists: preview web, download .zip, export android.

### Command palette (Ctrl+K)
Two modes: commands and `Go to file…`. The fuzzy matcher
(`palette.ts`) ranks substring hits above scattered subsequence hits.
**Only working commands are registered** — there is no disabled or
decorative entry; commands that need a prior build say so when run.

### Settings
Editor font (12/13/14/16), terminal font (11/12/13/15), indent width
(2/4), auto-check on edit (on/off — runs the real compiler debounced
on every edit). Applied immediately, persisted in
`localStorage`, validated on load; corrupt data falls back to
defaults.

### Status bar
`auto-save state · compiler version · current file · Ln/Col · lines ·
last command status · ONLINE/OFFLINE` plus desktop-only toggles for
explorer and bottom panel. The network dot is informational: NEXT is
offline-first, compilation/builds/runs are local wasm and keep working
offline (the tooltip says exactly that).

## Compiler integration

`wasm/kinnwasm.ts` (shared with classic Studio) loads
`studio/public/kinn.wasm` — produced by `npm run build:wasm` from
`compiler/crates/kinn-ffi` (`wasm32-unknown-unknown`). Every
check/run/build goes through the same FFI boundary the CLI exposes,
with project files registered as modules. When the wasm file is
absent, the UI degrades honestly: editor-level checks, "compiler not
built" badges, and instructions instead of fake results.

## Responsive layout

- **Desktop (>880px)**: grid `explorer | editor / bottom` with drag
  handles (explorer width 170–480px, bottom height 120–900px), sizes
  and layout mode persisted in `localStorage`. Layout modes: `editor`
  (bottom hidden), `split` (default), `terminal` (editor hidden,
  terminal maximized). Keyboard: Ctrl+1/2/3, Ctrl+B (explorer),
  Ctrl+` (panel), Ctrl+K (palette), Ctrl+S (save), Ctrl+Enter (check),
  Ctrl+F (find).
- **Mobile (≤880px)**: single-view mode switched by the bottom
  navigation (files / editor / terminal / output); compact top bar
  (Run/Build/palette reachable), 44px touch targets, `editor`
  stays usable (gutter + highlighting intact), terminal input grows
  to 16px to prevent mobile zoom-on-focus, and the visual viewport is
  tracked (`--nx-vvh`) so the keyboard never covers the input.
  `interactive-widget=resizes-content` is set in the viewport meta.

## Testing

- `palette.test.ts`, `settings.test.ts`, `state.test.ts` — pure model
  tests (matching, validation, tab/layout state).
- `termcmd.test.ts` — the engine against a fake sink without wasm:
  command dispatch, file management, honest fallbacks, statuses.
- `termcmd.wasm.test.ts` — the engine against the **real** kinn.wasm:
  `run` stdout/exit codes, compile errors driving PROBLEMS, `build
  web/android` artifacts, preview hand-off, version reporting.
- The classic suite (`vfs`, `syntax`, `diagnostics`, `storage`,
  `build`, `wasm`) keeps passing untouched.

## Android packaging (0.4.0)

The Android app (`android/`) launches **NEXT by default** since 0.4.0:

- `NextActivity` (new) hosts the built web bundle inside a WebView
  served through `WebViewAssetLoader` on
  `https://appassets.androidplatform.net/assets/studio-web/next.html`.
  A real https origin — not `file://` — is what makes IndexedDB,
  localStorage and WebAssembly work in the WebView; every byte still
  comes from the APK (the app has no INTERNET permission).
- The compiler inside NEXT is `kinn.wasm` (same `kinn-ffi` as the CLI)
  — real checking, running and building on-device, offline.
- The web bundle is a build artifact: Gradle's `copyStudioWeb` task
  copies `studio/dist` into the APK assets and fails the build with
  build instructions when it is missing or incomplete.
- `window.kinnAndroid` (injected by `NextActivity`, feature-detected by
  `studio/src/host.ts`): `saveFile` routes exports through the system
  document picker (a WebView drops `<a download>` silently), and
  `openClassicStudio` opens the native classic Studio. In a plain
  browser the bridge is absent and the exact previous behavior runs.
- Import uses the standard `<input type=file>` flow, completed by
  `onShowFileChooser`.
- The classic **native** Studio 0.3.0 (`MainActivity` + JNI compiler)
  stays bundled and reachable; the launcher filter moved to
  `NextActivity`. Web classic also ships (`assets/studio-web/index.html`).
- Keyboard: `adjustResize` + the page's visual-viewport tracking; back
  navigation: WebView history first, then finish.

## Known limitations (honest)

- The terminal is an in-browser command engine, not a system shell: it
  runs the real compiler, VM and target backends, but not arbitrary
  OS commands (`cd`, `git`, `npm`) — those belong to a desktop shell
  (Tauri) later.
- Projects are flat (`name.kin`); the explorer does not invent
  folders. The Kin module system resolves flat file names today.
- The compiler wasm still reports its own build version (0.3.0); the
  NEXT shell is 0.4.0-next. `version` shows both, separately.
- Android *export* (`build android` in the terminal) is a Gradle
  project, not an APK — building that APK needs Android Studio /
  gradle / CI (docs/BACKEND_ANDROID.md). (Unrelated: the KINN Studio
  app itself is packaged by `.github/workflows/android.yml`.)
- Editor find is per-file, without highlighting-all or replace.
- Single Vfs project per browser profile (shared with classic Studio,
  same IndexedDB key — opening the same project in both is supported,
  last write wins).
- No Vim/emacs keymaps, no LSP, no multi-cursor — out of scope for the
  experimental slice.

## Rollback / adoption path

1. NEXT is additive: delete `src/next/`, `next.html`, the vite input
   line and the classic `Next` link — classic Studio is byte-identical
   again.
2. Promotion (only after validation): swap the two entries' roles in
   `vite.config.ts` or redirect `index.html` to `next.html` — no
   compiler or storage migration is needed (same .kinnproj, same
   IndexedDB key, same wasm bridge).
