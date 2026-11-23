# KINN Studio

Terminal-inspired, offline-first editor shell for the Kin language.
Dark graphite surfaces, one discreet orange accent, monospace everywhere,
minimal motion.

**Two interfaces ship side by side** and share all core services,
the project format and the compiler backends:

- `index.html` — the classic Studio (this README's main topic; got one
  additive `Next` link, nothing else changed).
- `next.html` — **Studio NEXT (0.4, experimental)**: a substantially
  different, terminal-first redesign — top bar with live build status,
  collapsible explorer with project info, editor tabs with current-line
  highlight and a find bar, a prominent multi-session terminal running
  the real compiler, problems/output panels, command palette (Ctrl+K),
  settings, and a bottom status bar; desktop grid with resizable
  panels and a drawer-based mobile layout. See
  `docs/KINN_STUDIO_NEXT.md` for its architecture, honest limitations
  and rollback path; its components live in `src/next/`.

## Status (0.1) — implemented

- **Project explorer** over an in-memory project seeded with Kin samples;
  create new `.kin` files from the explorer or the terminal.
- **Code editor** with Kin syntax highlighting (editor-side lexer in
  TypeScript that mirrors the reference Rust lexer), line numbers,
  tab/auto-indent, Ln/Col status bar and mode badge.
- **Problems panel** fed by real editor checks: lexical errors, delimiter
  balance, mode-directive validation, duplicate functions. Clicking a
  problem jumps to its line.
- **Integrated terminal** with a real command loop: `help`, `version`,
  `about`, `clear`, `ls`, `cat`, `open`, `new`, `tokens`, `check`,
  `mode`, `syntax`, `run`.
- **Responsive layout**: three-pane desktop grid; below 900px the panes
  switch through a tab bar (Files / Editor / Terminal).

## Status (0.1) — honest limitations

- Files live in memory only; persistence arrives with the desktop shell
  (File System Access API) in 0.2.
- `check` runs editor-side lexical/structural checks, **not** the full
  type checker; run `kinn check <file>` locally for semantics.
- The `Run` button is disabled and explains itself: executing programs
  inside Studio needs the compiler compiled to WebAssembly (planned 0.2).

## Develop

```bash
npm install
npm run dev       # dev server
npm test          # vitest unit tests (editor lexer + checks)
npm run build     # tsc typecheck + production build in dist/
npm run preview   # serve the production build
```

The build has **zero runtime dependencies** (dev-only: vite, typescript,
vitest) and uses a relative base path, so `dist/` also works from
`file://` and inside future desktop (Tauri) / Android (Capacitor) shells.

## Architecture

```
src/
  main.ts              bootstrap + wiring (classic Studio)
  theme.css            design tokens and layout (classic)
  vfs.ts               in-memory project file system
  syntax/lexer.ts      Kin tokenizer (editor side, mirrors kinc)
  syntax/highlight.ts  tokens -> colored segments
  diagnostics/checks.ts editor quick checks + mode detection
  ui/explorer.ts       file tree (classic)
  ui/editor.ts         textarea + highlight overlay editor (classic)
  ui/terminal.ts       command loop (classic)
  ui/problems.ts       diagnostics list (classic)
  ui/tabs.ts           mobile tab navigation (classic)
  next/                Studio NEXT (experimental, isolated module):
                       next.css design system, state.ts, settings.ts,
                       termcmd.ts terminal engine, palette.ts, layout.ts,
                       ui/{topbar,explorer,editortabs,codeeditor,terminal,
                       problems,output,statusbar,settings,paletteview,preview}
samples/               bundled .kin sample projects
```

See `docs/KINN_STUDIO.md` for the design system and roadmap and
`docs/KINN_STUDIO_NEXT.md` for the experimental interface.
