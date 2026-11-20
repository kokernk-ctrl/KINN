# KINN Studio

KINN Studio is the terminal-inspired environment around the Kin
language. This document describes what exists today (0.1, verified) and
what is planned, component by component.

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

## Implemented in 0.1 (verified)

- **Project explorer** — file tree over the in-memory project
  (`samples`: hello, loops, broken demos), active-file marking, dirty
  dots, in-app file creation (`+ new file`, validated `.kin` names).
- **Code editor** — textarea-over-highlight architecture with an
  editor-side TypeScript lexer mirroring the Rust lexer; line-number
  gutter; Tab inserts two spaces; Enter auto-indents (deeper after
  `{`); Ln/Col status bar; `MODE B|S|A` badge detected from the source.
- **Problems panel** — real editor checks (see below), clickable to
  jump to the line.
- **Integrated terminal** — command loop with history (↑/↓) and 13
  implemented commands: `help`, `version`, `about`, `clear`, `ls`,
  `cat`, `open`, `new`, `tokens`, `check`, `mode`, `syntax`, `run`.
- **Responsive layout** — three-pane grid ≥901px; below 900px a tab
  bar (Files / Editor / Terminal) switches panes.
- **Keyboard** — `Ctrl+Enter` runs checks; `Ctrl+S` marks the
  in-memory project saved (with an honest toast about 0.2 persistence).

## Editor checks (honest scope)

The Studio runs **editor-grade** checks only — lexical errors (E1xxx
mirrors), delimiter balance (E2101/E2102), mode-directive validation
(E2110), duplicate functions (E3002). These are real checks, tested
with 22 vitest cases.

The **full type checker runs in the compiler** (`kinn check`), not in
the browser. The `Run` button is visibly disabled and, when clicked,
explains exactly what to use today and what 0.2 will bring. No fake
features.

## Planned

| Milestone | Feature |
| --- | --- |
| 0.2 | kinc compiled to WebAssembly: full `check` and `run` inside Studio, offline |
| 0.2 | persistent projects (File System Access API in the desktop shell) |
| 0.2 | multi-file projects with the module system as it lands in the language |
| 0.3 | desktop shell (Tauri-style) wrapping the same web build |
| 0.5 | Android shell (Kotlin/Capacitor), graphics preview when the GLES backend lands |
| later | integrated docs viewer, diff view, project templates per mode |

The build already targets this future: zero runtime dependencies and a
relative base path mean `dist/` runs from `file://`, inside a desktop
webview, and inside an Android webview unchanged.

## Architecture

```
studio/src/
  main.ts               bootstrap and wiring
  theme.css             design tokens, layout, syntax colors
  vfs.ts                in-memory project file system
  syntax/lexer.ts       Kin tokenizer (editor side; mirrors kinc's lexer)
  syntax/highlight.ts   tokens → per-line colored segments
  diagnostics/checks.ts editor quick checks + mode detection
  ui/explorer.ts        file tree
  ui/editor.ts          editor component
  ui/terminal.ts        command loop
  ui/problems.ts        diagnostics list
  ui/tabs.ts            mobile tab navigation
samples/                bundled .kin demo projects
```

Testing: `npm test` (22 vitest tests), `npm run build` (strict `tsc` +
vite production build). Visual QA at 0.1 included desktop (1366px) and
mobile (390px) screenshot review; three defects found and fixed during
that pass (gutter line-number collapsing, multiplication `*` mistaken
for a mode directive, desktop→mobile resize pane state).
