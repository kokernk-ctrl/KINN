# AI Context

Status snapshot for AI assistants (and humans) working on KINN.
**Last updated: 2026-11-21, release 0.1.0.** Update this file after
significant milestones. Keep it between 500 and 900 words.

## What KINN is

An offline-first programming language (Kin) and development environment
(KINN Studio). No accounts, no cloud, no online-only compilation.
Android ARM64 is a first-class future target. The project rule that
matters most: never claim untested functionality works. Distinguish
implemented / experimental / planned / unsupported everywhere.

## Actual implementation status (0.1.0)

Fully implemented and tested, end to end:

- **Kin 0.1 language subset**: `func`, `let`/`var` with inference,
  `=`/`+=`/`-=`, `if`/`else if`/`else`, `while`, `break`, `continue`,
  `return`, expression statements, bare blocks, types `int` (i64),
  `float` (f64), `bool`, `string`, `void`, builtin `print`,
  short-circuit `and`/`or`/`not`, string concatenation and equality.
- **Compiler pipeline** (Rust, zero external dependencies):
  lexer → parser → AST → semantic checker → KIR lowering.
- **Runtime**: `kinrt` stack VM with call frames, per-instruction
  source lines, checked integer overflow (E4004), division by zero
  (E4001), call-depth limit (E4002).
- **CLI** `kinn`: `run`, `check`, `tokens`, `ast`, `kir`, `version`;
  exit codes 0 ok / 1 diagnostics / 2 usage / 3 runtime error.
- **Programming modes** `*mode = B|S|A;`: parsed, validated, carried
  through the pipeline. 0.1 differences: B adds the N3001 `var` lint;
  A suppresses notes (terse diagnostics). Nothing deeper yet — by
  design and documented.
- **KINN Studio** (TypeScript, Vite, zero runtime deps): explorer over
  an in-memory vfs, editor with syntax highlighting and line numbers,
  13-command terminal, problems panel with editor checks (lexical
  errors, delimiter balance, mode validation, duplicate functions),
  responsive desktop/mobile layouts.

Test totals: **90 Rust tests** (15 kinrt, 53 kinc unit, 10 kinc
pipeline, 12 CLI) and **22 Studio vitest tests**, all passing. The six
`examples/*.kin` programs carry `// expect:` blocks and are executed
with output comparison in CI-style integration tests.

Planned, not implemented (do not pretend otherwise): modules and
multi-file projects, arrays, casts, `for`, stdlib beyond `print`,
native codegen (0.3 investigation), graphics (0.5, OpenGL ES 3.0
first), Android shells, in-Studio execution (needs kinc→WASM, 0.2).

## Repository structure

```
compiler/                  Rust workspace (cargo test = 90 tests)
  crates/kinrt/            KIR instruction set, stack VM, disassembler
  crates/kinc/             lexer, parser, ast, checker, lower, driver
  crates/kinn/             CLI binary (kinn)
studio/                    KINN Studio web shell (npm test = 22 tests)
  src/syntax/              editor lexer + highlight (mirrors Rust lexer)
  src/diagnostics/         editor quick checks
  src/ui/                  explorer, editor, terminal, problems, tabs
examples/                  runnable spec: // expect: output blocks
examples/errors/           // error: CODE (compile) and // runtime-error:
docs/                      all design documents (index in README)
```

Root files: README (status table + quick start), CHANGELOG, SECURITY,
this file.

## Known problems (see docs/PROBLEM_REGISTRY.md for all)

- P-001 parser stops at the first syntax error.
- P-002 `while true` not recognized as diverging (E3012 false positive
  for `while true { return x; }`).
- P-003 KIR local slots are not reused after scope exit (memory only).
- P-005 Studio files are in-memory only.
- P-006 Studio `check` is editor-grade, not the full type checker.
- P-007 Studio cannot run programs (Run disabled by design, 0.2 = WASM).
- P-008 no CI yet.
- P-012 license undecided.

## Current task and next milestone

0.1.0 is complete and committed. Next milestone **0.2.0 — language
breadth and Studio integration** (docs/ROADMAP.md): multi-error parser
recovery, control-flow return analysis, slot reuse, modules/import,
casts, arrays, `for`, stdlib functions, kinc→WebAssembly for in-Studio
check/run, persistent Studio files, CI workflow. Issues for these exist
on GitHub under the v0.2.0 milestone.

## Conventions

- Commits: `feat(scope):`, `fix(scope):`, `docs:`, conventional style.
- Rust: edition 2021, zero deps, tests inside each module plus
  integration tests in `crates/*/tests/`.
- Diagnostics carry stable codes (E1xxx lexer, E2xxx parser, E3xxx
  semantic, E4xxx runtime, W/N warnings/notes, E21xx Studio-only) —
  see docs/LANGUAGE_SPEC_0.1.md for the registry.
- Every example doubles as a test; broken examples declare their
  expected error code.
- Build artifacts (`compiler/target`, `studio/node_modules`,
  `studio/dist`) are gitignored. Never commit secrets; SECURITY.md
  describes token hygiene.

## Most relevant documents

- docs/LANGUAGE_SPEC_0.1.md — normative grammar, semantics, codes
- docs/COMPILER_ARCHITECTURE.md — pipeline and testing strategy
- docs/RUNTIME_ARCHITECTURE.md — KIR and the VM
- docs/PROGRAMMING_MODES.md — what B/S/A actually do today
- docs/KINN_STUDIO.md — Studio features and honest limitations
- docs/ROADMAP.md, docs/DECISIONS.md, docs/PROBLEM_REGISTRY.md
