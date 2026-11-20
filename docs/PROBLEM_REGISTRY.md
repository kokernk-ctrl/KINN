# Problem Registry

Known bugs and limitations, tracked openly. Each entry has an ID, a
status, and where it is felt. Fixed entries move to the bottom with the
release that fixed them. This registry is the source for GitHub issues.

## Open

| ID | Area | Problem | Impact | Workaround |
| --- | --- | --- | --- | --- |
| P-001 | parser | Parsing stops at the first syntax error; no recovery | one error reported per compile | fix and re-run |
| P-002 | checker | Return analysis is conservative: `while true { ... }` divergence not recognized, so `func f() -> int { while true { return 1; } }` is rejected with E3012 | classic infinite-loop-with-return pattern needs a trailing `return` | add a final `return` |
| P-003 | lowering | Local slots are never reused after a scope exits; `nlocals` equals the number of declarations | memory only; no semantic effect | none needed |
| P-004 | runtime | Float division by zero is an error, not IEEE-754 `inf` (deliberate, ADR-0008) | numeric code expecting `inf`/`NaN` propagation differs | guard divisors or wait for a mode-gated opt-out |
| P-005 | studio | Files live in memory only; no persistence | reload loses edits (samples are read-only seeds) | keep sources in a real directory; use kinn CLI |
| P-006 | studio | `check` runs editor-side lexical/structural checks only, not the full type checker | type errors invisible in Studio until 0.2 | run `kinn check <file>` locally |
| P-007 | studio | No in-Studio program execution; Run button disabled by design | must use CLI to run | `kinn run <file>` |
| P-008 | infra | No CI workflow yet; tests run manually | regressions caught only locally | run `cargo test` and `npm test` before commits |
| P-009 | checker | Unreachable-code warning (W3001) is per-block only; no cross-block control-flow graph | a few unreachable paths unreported | none |
| P-010 | language | No `for` loop, arrays, casts, modules, or stdlib beyond `print` (0.1 scope) | expected 0.1 boundary, not a bug | — |
| P-011 | lexer | Tabs count as one column; caret alignment can look off in tab-indented files | cosmetic in diagnostics | use spaces (editor inserts spaces) |
| P-012 | infra | License undecided (ADR-0012) | external contributions blocked | owner decision pending |

## Verification debts

| ID | Area | Debt |
| --- | --- | --- |
| V-001 | android | `kinrt` is pure Rust and should build for `aarch64-linux-android`, but no Android build has been verified on a device |
| V-002 | graphics | Zero graphics code exists (planned 0.5); no window/context verification yet |

## Fixed

| ID | Release | Fix |
| --- | --- | --- |
| F-001 | 0.1.0 | Studio gutter collapsed line numbers (missing `white-space: pre`) — caught in visual QA |
| F-002 | 0.1.0 | Studio editor check mistook the `*` in `6 * 7` for a mode directive (E2110 false positive) — caught in QA, regression test added |
| F-003 | 0.1.0 | Desktop→mobile resize left all Studio panes visible; panes now rely on media queries only |
| F-004 | 0.1.0 | Kinrt VM tests initially lacked the `Const(Unit); Return` epilogue, exposing the E4999 guard — fixed before first commit |
