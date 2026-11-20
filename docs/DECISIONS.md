# Decisions

Architecture Decision Records. Each ADR is immutable once accepted;
superseding an ADR adds a new one that references it.

## ADR-0001 — Zero-dependency Rust workspace

**Status: accepted.** The compiler workspace (kinrt, kinc, kinn) uses
no external crates.

Why: offline-first is a product promise; `cargo build` must never need
the network. It also keeps audit surface at zero and build times tiny.
Cost: hand-rolled arg parsing and no ecosystem conveniences — accepted.

## ADR-0002 — KIR stack VM as the first backend

**Status: accepted.** 0.1 lowers to KIR and interprets it on a stack
VM. Native codegen is deferred to 0.3 with candidates Cranelift, LLVM,
or C transpilation; the KIR format is the contract that survives any
backend swap.

Why: proves the whole pipeline with the least machinery; VM execution
is portable to Android today; IR-first keeps the backend replaceable.

## ADR-0003 — Required semicolons

**Status: accepted.** Statements end with `;`; no automatic insertion.

Why: deterministic parsing and errors that point at real positions,
which matters more for beginners than saving a keystroke.

## ADR-0004 — No implicit numeric conversion

**Status: accepted.** `int` and `float` never mix silently (E3011);
explicit casts arrive in 0.2.

Why: hidden promotions are a classic source of beginner confusion;
forcing `1.0` states intent. Also keeps the checker simple and exact.

## ADR-0005 — Stack policy per component

**Status: accepted.** Rust for compiler/runtime/CLI/core; C/C++ only
for native graphics integration; Kotlin only for the Android shell;
TypeScript only for Studio and editor integrations; Python optional for
tooling tests. No other languages.

## ADR-0006 — Mode directive syntax `*mode = B|S|A;`

**Status: accepted.** A single first-line directive, parsed by the
compiler itself; 0.1 semantics = diagnostic verbosity + Beginner lint.
Modes change guidance, never syntax; B programs are valid unchanged in
S and A.

## ADR-0007 — Studio is a web-tech shell

**Status: accepted.** Studio 0.1 is TypeScript + Vite with zero runtime
dependencies, built with a relative base so `dist/` runs from `file://`
and inside desktop (Tauri) / Android (Capacitor) webviews. Native
widget toolkits were rejected for now: one codebase, three targets,
smallest possible team maintenance cost.

## ADR-0008 — Checked integer arithmetic; float div-by-zero is an error

**Status: accepted.** Integer overflow is a runtime error (E4004), not
wraparound. Float division by zero errors instead of producing IEEE
`inf` — a documented deviation chosen for beginner clarity. Revisit if
numeric-workload needs emerge (a mode-gated opt-out is the likely
shape).

## ADR-0009 — Strings are immutable values

**Status: accepted.** `+` concatenates; there is no mutation API on
strings in 0.1. Interpolation and indexing decisions are deferred to a
0.2 ADR.

## ADR-0010 — OpenGL ES 3.0 first, Vulkan later

**Status: accepted.** One backend before a second; GLES 3.0 covers
Android + desktop GL. The 0.5 API designs a `GfxBackend` seam from day
one, but no Vulkan code exists until GLES is complete and verified.

## ADR-0011 — Error codes are part of the public interface

**Status: accepted.** Stable codes (E1xxx lexer, E2xxx parser, E3xxx
semantic, E4xxx runtime, W/N warnings/notes, E21xx Studio editor
checks) are documented and tested. Tools may match on them.

## ADR-0012 — License pending owner decision

**Status: open.** No license is chosen yet; the repository is
all-rights-reserved until the owner decides. Blocks external
contributions; tracked as a GitHub issue.
