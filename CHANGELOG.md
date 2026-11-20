# Changelog

All notable changes to KINN are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow semantic versioning once there is a stable language surface.

## [0.1.0] — 2026-11-21

First verified milestone: the complete Kin 0.1 pipeline from source text
to execution, plus the first KINN Studio shell.

### Added — language and compiler (`compiler/`)

- Kin 0.1 language subset: `func`, `let`/`var` (with inference),
  assignment and compound assignment (`=` `+=` `-=`), `if` / `else if` /
  `else`, `while`, `break`, `continue`, `return`, expression statements,
  bare blocks; types `int`, `float`, `bool`, `string`, `void`; the
  `print` builtin; `and` / `or` / `not` with short-circuit evaluation.
- Programming mode directive `*mode = B | S | A;` (parsed, validated,
  carried through the pipeline; see PROGRAMMING_MODES for 0.1 behavior).
- Lexer with full position tracking, line and block comments, string
  escapes, precise E1xxx diagnostics.
- Recursive-descent parser producing a typed AST; explicit braces and
  required semicolons; chained comparisons rejected (E2004).
- Semantic analysis: block scoping with shadowing, function signatures,
  `main()` requirements, void-value protection, conservative
  missing-return analysis (E3012), unreachable-code warning (W3001),
  no implicit int/float conversion (E3011), Beginner-mode `var` lint
  (N3001).
- KIR: flat stack-based intermediate representation with per-instruction
  source lines.
- `kinrt` runtime: stack VM with call frames, checked integer overflow
  (E4004), division/modulo by zero (E4001), call depth limit (E4002),
  float formatting that keeps `2.0` visible as `2.0`.
- `kinn` CLI: `run`, `check`, `tokens`, `ast`, `kir`, `version`; exit
  codes 0/1/2/3; caret-style diagnostics with source snippets.
- Example programs with machine-verified expected output
  (`// expect:` blocks) and error programs with expected diagnostic
  codes (`// error:` / `// runtime-error:` markers).
- 90 Rust tests (unit + integration), zero external dependencies.

### Added — KINN Studio (`studio/`)

- Terminal-inspired web shell: project explorer, code editor with Kin
  syntax highlighting, integrated terminal (13 real commands), problems
  panel with clickable editor checks, responsive desktop/mobile layout.
- Editor-side TypeScript lexer mirroring the Rust lexer; 22 vitest
  tests; production build with zero runtime dependencies.

### Added — documentation

- Full document set (see README index): vision, language design, spec,
  modes, type system, compiler/runtime/graphics architecture, Studio,
  CLI, roadmap, decisions, problem registry, security, AI context.

### Known limitations

See [PROBLEM_REGISTRY.md](docs/PROBLEM_REGISTRY.md): single-syntax-error
parsing, conservative return analysis, no slot reuse, Studio files are
in-memory only, no in-Studio execution, no CI yet.
