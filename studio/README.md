# KINN Studio

Terminal-inspired, offline-first editor shell for the Kin language.
Dark graphite surfaces, one discreet orange accent, monospace everywhere,
minimal motion.

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
  main.ts              bootstrap + wiring
  theme.css            design tokens and layout
  vfs.ts               in-memory project file system
  syntax/lexer.ts      Kin tokenizer (editor side, mirrors kinc)
  syntax/highlight.ts  tokens -> colored segments
  diagnostics/checks.ts editor quick checks + mode detection
  ui/explorer.ts       file tree
  ui/editor.ts         textarea + highlight overlay editor
  ui/terminal.ts       command loop
  ui/problems.ts       diagnostics list
  ui/tabs.ts           mobile tab navigation
samples/               bundled .kin sample projects
```

See `docs/KINN_STUDIO.md` for the design system and roadmap.
