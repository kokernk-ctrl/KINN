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

## ADR-0013 — String interpolation: deferred to 0.3

**Status: accepted (2026-11-21, 0.2.0).** The 0.2 roadmap carried
"string interpolation decision (ADR) + implementation". The decision:
**defer**. Rationale: 0.2 already lands modules, arrays, `for` and
casts; `as string` casts plus `+` concatenation cover the honest
need meanwhile, and interpolation deserves its own design pass
(escaping rules, expression grammar inside holes, mode-B friendliness)
rather than being rushed. Not silently dropped — tracked as P-004 in
the registry and re-listed under 0.3.

## ADR-0014 — Arrays use reference semantics with deep equality

**Status: accepted (2026-11-21, 0.2.0).** `let b = a;` makes `b` share
`a`'s array (Java/Python model, matching most beginner expectations for
"containers"), while `==` compares **contents deeply** (documented).
Value/copy semantics were rejected for 0.2: they would make element
assignment through parameters impossible and invite hidden-copy
surprises later. Element mutation is allowed even through `let`-bound
variables: the binding is immutable, the contents are not.

## ADR-0015 — Modules: path-based imports, functions-only exports

**Status: accepted (2026-11-21, 0.2.0).** `import "lib.kin" as m;`
(quoted relative path, alias mandatory, declared before functions).
Exports are all top-level functions; imported `main` is inert. Cross-
file globals, renaming exports and selective exports are deliberately
out of scope for 0.2 (registry-tracked). Qualified names in KIR use
the entry-relative path stem (`math::add`) so identical names in
different files cannot collide.

## ADR-0016 — One FFI crate, three hosts; the `jni` exception

**Status: accepted (2026-11-21, 0.2.0).** `kinn-ffi` (C ABI, zero
deps) exposes the same `kinn_check`/`kinn_run` to WebAssembly and
native hosts, so the CLI, web Studio and Android app share one
compiler implementation and one diagnostics format (JSON). The
Android-only glue crate `kinn-android` is the single permitted
external dependency (`jni` 0.21) in the workspace: hand-rolled JNI
struct layouts were judged riskier than a well-audited crate. The
compiler core (kinrt/kinc/kinn/kinn-ffi) remains dependency-free.

## ADR-0017 — Release signing: secrets or honestly unsigned

**Status: accepted (2026-11-21, 0.2.0).** CI signs release APKs only
when the owner-provided `ANDROID_KEYSTORE_*` encrypted secrets exist;
otherwise the release APK is named `...-UNSIGNED.apk` and the
signature report says so. The pipeline never generates its own
"release" key, and never labels a debug-signed build as production.
Public GitHub Releases remain an explicit owner decision, not a CI
side effect.

## ADR-0018 — Multi-target backends over a shared site model

**Status: accepted (2026-11-21, 0.3.0).** Targets (`web`, `android`)
consume a target-independent `SiteModel` (`kinc::site`) produced by the
shared front end, never the AST and never per-target forks of the
language. Backends are pure generators returning `Vec<OutputFile>`.
KIR + `kinrt` remain the execution path for console programs. A backend
that cannot express something says so with a diagnostic; it never
emits output that silently misrepresents the source.

## ADR-0019 — The web module is a language module (`website` declarations)

**Status: accepted (2026-11-21, 0.3.0).** Declarative sites are parsed,
checked and lowered by the shared front end (`website "Name" { page P {
… } }`, B-mode-friendly syntax). `website`/`page`/element words are
contextual keywords, so no existing identifier breaks. Sites live in
the entry file only; `kinn run` of a site-only program fails with
guidance toward `kinn build --target web`.

## ADR-0020 — Web output is semantic static HTML/CSS, nearly JS-free

**Status: accepted (2026-11-21, 0.3.0).** The web backend emits one
semantic HTML5 file per page plus one stylesheet; `app.js` only adds
focus management after navigation. There is no generic JS runtime that
pretends to support state, forms or events: those arrive with the 0.4
state module, and until then the checker/grammar simply do not accept
them. Accessibility is enforced at compile time (alt text E3034,
non-empty labels E3035) and generated in (landmarks, skip link,
`aria-current`, focus-visible).

## ADR-0021 — Android backend: exported Gradle project + build providers

**Status: accepted (2026-11-21, 0.3.0).** An Android build is produced
from a generated Gradle project (native `MainActivity` rendering the
SiteModel with Android views). Three providers, reported honestly:
local Gradle/SDK when present, remote CI build (GitHub Actions,
artifact upload, apksigner verification), and always the exported
project itself. The wrapper jar is not embedded in the CLI; generated
projects use the system Gradle (CI) or Android Studio. Signing stays
governed by ADR-0017.

## ADR-0022 — Package foundation: `kinn.toml` manifest, no registry yet

**Status: accepted (2026-11-21, 0.3.0).** The first package-manager
step is a minimal manifest (`kinn.toml`: name, version, entry) read by
`kinn build`, plus the MODULE_SYSTEM.md specification of modules,
packages, registry, network and FFI layers. No lockfiles, no
dependency downloads, no registry — those ship only with real
implementation and tests in later releases.

## ADR-0023 — KINN Web Runtime: a real, dependency-free runtime for dynamic web apps

**Status: accepted (2026-11-22, 0.4.0 design; implementation gated on
the 0.4.0 Part E approval).** 0.4.0 introduces a genuine web runtime
(`kinnrt.js`) for the new dynamic `app` module: reactive updates from a
statically compiled binding graph, event dispatch, lifecycle, SPA/MPA
routing, stores, animation coordination. This **supersedes the "no
runtime" clause of ADR-0020 for `app` programs only**; static
`website` output stays byte-identical (no runtime, focus-management
`app.js` only). The runtime is hand-written ES2020 JavaScript with zero
third-party code, no eval, no innerHTML, no frameworks — React (or any
framework) is never a dependency. Size budget ≤ 16 KB, CI-tested. Full
architecture: `docs/DYNAMIC_WEB_ARCHITECTURE.md` Part C.

## ADR-0024 — The `app` language module: components, state, events, forms

**Status: accepted (2026-11-22, 0.4.0 design; implementation gated).**
Dynamic web apps are a sibling declaration of `website` (`app "Name" { …
}`), parsed/checked/lowered by the shared front end via the ADR-0019
contextual-keyword recipe (no new reserved words; regression tests per
word). Components declare `prop`/`state`/`view`; views use binding
expressions, `when`/`otherwise` conditional rendering and keyed `each`
lists; events are `on <event> { … }` handler blocks whose bodies are
official Kin statements (restricted: assignment targets resolve to
handler locals → component state → shared state); handlers and computed
values lower to KIR and are translated to JavaScript (one semantics, two
carriers — KIR remains the swap contract per ADR-0002). State is
scalars/arrays of scalars; records/maps stay out of 0.4.0. New
diagnostics live in reserved ranges (parser E2031–E2039, checker
E3040–E3079, W3040–W3049). The android target rejects `app` programs
with E3070 — backends stay independent (ADR-0018).

## ADR-0025 — Routing: MPA default, SPA opt-in, honest `file://` behavior

**Status: accepted (2026-11-22, 0.4.0 design; implementation gated).**
`navigation mpa` (default) keeps one independent HTML per page/route
with shared state carried by an explicit hydration channel;
`navigation spa` emits one shell with a runtime router (dynamic `:param`
routes, `not_found`, per-route transitions, history, `aria-current`,
title/focus updates). SPAs opened from `file://` automatically fall back
to hash routing and say so in the build report; hosting requirements are
documented in the generated README. No claim is made that a SPA deep
link "just works" without the documented static-host fallback rule.

## ADR-0026 — Local persistence and JSON: local-only, honestly bounded

**Status: accepted (2026-11-22, 0.4.0 design; implementation gated).**
`store` blocks persist declared state keys to the browser's
`localStorage` (schema-hash guarded) with `store_save/load/clear`
builtins; JSON is the serialization format of state snapshots
(`json_of`/`save_json`), not a general document API. Normative honesty
clause: local persistence is one device, one browser profile — **it is
never presented as a backend database**; no sync/multi-device/multi-user
claims. There is no network/fetch construct in 0.4.0.

## ADR-0027 — Animation and theming: declarative whitelist mapped to generated CSS

**Status: accepted (2026-11-22, 0.4.0 design; implementation gated).**
Animations are declarations (`animation { keyframes/transition }`,
element attributes `enter/exit/hover/press/scroll`), never imperative
JS: keyframe properties are whitelisted to compositor-friendly
opacity/transform/filter; hover/press compile to pure CSS; exit
animations delay DOM removal; scroll reveals share one
IntersectionObserver; everything honors `prefers-reduced-motion`. The
same mechanism ships the per-site `theme` tokens (closes P-020):
a whitelisted token set (`accent`, `bg`, `surface`, `text`, `muted`,
`font`, `radius`, `max_width`) generated as CSS custom properties.

## ADR-0028 — Cross-module value exports: top-level `const`, folded at lowering

**Status: accepted (2026-11-22, 0.4.0 S1 — the first implemented step
of the owner-approved 0.4.0 Phase 5 scope; docs/DYNAMIC_WEB_ARCHITECTURE.md
B.1).** Modules export top-level constants in addition to functions:
`const NAME: Type = expr;` with a mandatory type annotation. This
**partially supersedes the "functions-only exports" clause of
ADR-0015**; everything else in ADR-0015 (path-based imports, mandatory
aliases, qualified KIR names, inert imported `main`) is unchanged.
Initializers are compile-time constant expressions over literals,
operators and earlier same-file consts (E3039 elsewhere); constants
fold to plain `Const(value)` instructions at lowering — no new KIR
instructions — and the folded values are recorded in `Module.consts`
for `kinn kir` to show. Importers read consts qualified (`m.NAME`).
Selective/renamed exports remain out of scope (P-014 narrowed).
`const` is a contextual keyword per the ADR-0019 recipe, with the
word-stays-identifier regression gate.
