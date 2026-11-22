# Programming Modes

Kin has three programming modes, selected by the first statement of the
file:

```kin
*mode = B;   // Beginner
*mode = S;   // Standard (the default when the directive is absent)
*mode = A;   // Advanced
```

The directive is parsed and validated by the compiler (E2010/E2011 on
mistakes, must be first, E2110-level guidance in Studio), carried
through the whole pipeline, and reported by `driver::compile`.

## Design intent

- **B — Beginner.** The language reduces configuration and increases
  guidance. A beginner project is a single `main.kin`; nothing else is
  required. Diagnostics speak plainly and add hints.
- **S — Standard.** Balanced defaults for everyday programs.
- **A — Advanced.** Terse output and, as the language grows, more
  technical control (explicit memory hints, lower-level graphics APIs,
  performance toggles).

**Invariant:** modes change guidance and defaults, never syntax. Any
program valid in B is valid unchanged in S and A.

## What is implemented in 0.1 (tested)

| Behavior | B | S | A |
| --- | --- | --- | --- |
| Extra lint: `var` never reassigned → note N3001 suggesting `fin` | yes | no | no |
| `note`-severity diagnostics (e.g. "declared here" N3002) shown | yes | yes | **hidden** (terse output) |
| Error/warning rendering identical | yes | yes | yes |

That is the entire 0.1 difference, stated plainly. The `kinn check`
output for a given file differs only in those ways.

## Planned differentiation (designed, not implemented)

- **B**
  - Project templates: `main.kin` only; module system stays invisible
    until needed.
  - Softer error vocabulary and "what to try next" hints on the most
    common first-week errors (missing `;`, `=` vs `==`, int/float mix).
  - Float division by zero explained inline (why not `inf`).
- **S**
  - Default diagnostics, warnings and lints as today.
- **A**
  - `kinn` exposes compiler internals: KIR dump flags, timing,
    optimization reports.
  - First access to explicit-control features as they are added
    (explicit casts, allocation hints, raw GPU resource control in the
    graphics layer).
  - Notes suppressed by default (implemented already), with an opt-in.

Modes will never gate correctness: an A program and its B twin either
both compile or both fail, with the same semantics.
