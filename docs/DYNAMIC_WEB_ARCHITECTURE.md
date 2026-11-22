# KINN 0.4.0 — Dynamic Web Applications: Technical Audit & Architecture Proposal

**Status: APPROVED + IN IMPLEMENTATION (Phase 5).** Per the agreed 0.4.0
process, this document covers **Phase 1 (technical audit), Phase 2
(language design), Phase 3 (runtime architecture) and Phase 4 (roadmap
with tests and acceptance criteria)**. **The owner approved all Part E
decisions (D1–D6) on 2026-11-22 and authorized Phase 5**; S1
(cross-module `const`, B.1) and S2 (`app` grammar B.2–B.9: parser,
AST, `kinn ast`, presence rules E3051/E3052, honest CLI guards) are
landed on `develop` — see CHANGELOG and LANGUAGE_SPEC §C1/§C3 for the
normative wording that the implementation follows.

Companion decisions: ADR-0023 … ADR-0027 in `docs/DECISIONS.md`.
Companion verification: this audit was re-verified on `develop` @ `c84a8f0`
("Merge develop: 0.3.0 audit report") with the full test suite green
(see A.1).

Related documents: `docs/MULTI_TARGET_ARCHITECTURE.md` (0.3.0 pipeline),
`docs/BACKEND_WEB.md`, `docs/BACKEND_ANDROID.md`, `docs/MODULE_SYSTEM.md`,
`docs/RUNTIME_ARCHITECTURE.md` (kinrt VM), `docs/DECISIONS.md`.

---

## 0. Requirements baseline (owner directive, restated)

0.4.0 evolves the web target from static pages into a **dynamic application
platform** with the following ten requirements (R1–R10):

- **R1 — Multi-file projects.** `kinn.toml` + entry source + `pages/`,
  `components/`, `styles/` layout; modules and components communicate
  through the official module/component system; imports/exports, reusable
  components, shared application data.
- **R2 — MPA.** Multi-page output (one independent HTML per page), page
  navigation, route parameters, navigation state, shared application state.
- **R3 — SPA.** Navigation without full page reloads; routes, dynamic route
  parameters, navigation, a not-found page, route transitions, navigation
  state.
- **R4 — Reactive state.** Local component state, shared application state,
  state updates, conditional rendering, data binding, derived values;
  avoid unnecessary DOM updates.
- **R5 — Events & interaction.** click, input, change, submit, keyboard,
  pointer, focus, blur — integrated into the **official KINN grammar**.
- **R6 — Component system.** Properties, local state, events, composition,
  conditional rendering, reusable layouts.
- **R7 — Animation.** Enter/exit transitions, page transitions, hover/press,
  component animation, scroll animation, state-driven animation, keyframes,
  timing/easing — realized through generated CSS/JS.
- **R8 — Forms & data.** Forms, validation, submission, reactive input
  values, local persistence, JSON handling. **Local persistence is never
  presented as a backend database.**
- **R9 — KINN Web Runtime.** A lightweight runtime responsible for reactive
  updates, event dispatch, component lifecycle, routing, shared state and
  animation coordination. **The runtime depends on no framework — React
  (or Vue, or any other framework) never becomes a requirement.**
- **R10 — Developer experience.** Studio gains multi-file project browsing,
  source editing, build, preview, runtime error reporting, browser console
  integration and project export.

**Architecture rules (owner directive):**

- A1. Preserve the official KINN grammar; do not create a parallel language.
- A2. Existing 0.3.0 projects keep working (backwards compatibility).
- A3. All targets share one KINN front end.
- A4. The web backend and the Android backend remain independent.
- A5. Generated JavaScript is an implementation detail, never the user's
  language.
- A6. Avoid unnecessary dependencies.
- A7. Consider mobile-browser performance and memory.
- A8. Deliver incrementally: audit → design → runtime architecture → roadmap
  → **approval gate** → implementation. No big-bang change.

---

## Part A — Phase 1: technical audit

### A.1 Verification basis (this session)

Everything claimed below was re-verified on `develop` @ `c84a8f0`, working
tree clean, Rust 1.100.0:

| Check | Result |
| --- | --- |
| `cargo test --workspace` | **250 passed, 0 failed** (153 kinc, 22 pipeline, 28 CLI, 16 kinn-ffi, 26 kinrt, 5 manifest) |
| `kinn build examples/web/site.kin --target web --out /tmp/kinn-dist` | succeeded: `index.html` (1,084 B), `about.html` (990 B), `styles.css` (2,813 B), `app.js` (545 B) |
| Inspected `app.js` | focus management only; header comment: *"There is no application JavaScript in a KINN 0.3 site (ADR-0020); every feature works with JavaScript disabled"* |
| CI on `c84a8f0` | all workflows `success` (GitHub API, authenticated) |

### A.2 Compiler front end (`compiler/crates/kinc`, `kinrt`)

Verified structure (line counts from `wc -l`):

| File | Lines | Role |
| --- | --- | --- |
| `lexer.rs` | 583 | tokenizer, E1001–E1006 |
| `parser.rs` | 1,628 | recursive descent + error recovery + website grammar |
| `ast.rs` | 574 | AST, `Type`, `Mode`, dump |
| `checker.rs` | 1,822 | two-pass semantic checks, website rules |
| `project.rs` | 472 | multi-file import loading (loader callback) |
| `lower.rs` | 698 | AST → KIR + `site_model` |
| `site.rs` | 62 | target-independent `SiteModel` |
| `driver.rs` | 76 | `compile_project`/`compile_full` → `Compiled { module, mode, sources, site }` |
| `backend/web.rs` | 506 | static web generator, 12 tests |
| `backend/android.rs` | 647 | Gradle project generator, 9 tests |
| `kinrt` (KIR + VM) | 1,224 | stack VM for console programs; **not used by the site backends** |

**Language surface today.** A `.kin` file holds: a mode directive
(`*mode = B;` first line), imports (`import "path.kin" as alias;`),
functions, and at most one `website` declaration (entry file only).
Statements: `fin/var` (`fin` = final value; renamed from `let`, ADR-0029), assignment (`=`, `+=`, `-=`), index assignment, `if`
(no `else if` chains limit), `while` (break/continue), `for` (exclusive and
inclusive ranges, arrays), `return`, blocks. Expressions: literals, arrays,
indexing, calls (bare and qualified `m.f(...)`), casts (`as`), unary
`not/-`, binary operators with a fixed precedence ladder. Builtins:
`print`, `len`, `abs`, `min`, `max`.

**The type system is the hard constraint for 0.4.0 design** (verified
absence in `ast.rs`, `checker.rs`, `kinrt/src/value.rs`):

- Types: `Int (i64)`, `Float`, `Bool`, `Str`, `Void`, `Array(T)`, plus
  checker-internal `Error`. Arrays have reference semantics and deep
  equality (ADR-0014); strings are immutable values (ADR-0009); arithmetic
  is checked (overflow E4004, div/mod-zero E4001).
- **No records/structs, no maps, no enums, no closures or function-typed
  values** (functions are call targets, never values), no generics, no
  nullable/option types, no top-level constants, no heterogeneous arrays.
- No string interpolation (ADR-0013, deferred; P-004) and no string→number
  casts (P-013). Strings concatenate only via `+`.

Consequence: **state, props and events in 0.4.0 must be expressible with
scalars and homogeneous arrays of scalars, and handlers must be declarative
statement blocks, not callbacks.** Extending the type system itself is
explicitly *out of scope* for 0.4.0 (see Part E, D2).

**Contextual keywords are a proven, non-breaking pattern** (ADR-0019):
`website`, `page`, `title`, `heading`, `text`, `button`, `link`, `image`,
`divider`, `open`, `alt` are recognized by the parser only in
grammar positions (`parser.rs:249, 313, 344–449`); the regression test
`website_words_stay_identifiers_outside_the_declaration`
(`parser.rs:1584–1592`, updated for the fin rename) proves `fin website = 1;` still compiles. The same
recipe (enter grammar on `Ident(word)`, recover via sync-word lists —
`recover_page_element`, `parser.rs:478–500`) is the template for all 0.4.0
grammar.

### A.3 Module and project system

Implemented since 0.2 (verified in `project.rs`):

- `import "path.kin" as alias;` — path-relative, project-confined
  (`resolve_path`: must end `.kin`, no absolute path, no `\`, no `..`;
  **subdirectory paths like `import "components/nav.kin"` already work**).
- Loader-callback architecture: CLI uses the filesystem
  (`kinn/src/main.rs:70–72`); Studio and the Android app register every
  project file through `kinn_set_module` (FFI virtual filesystem). Import
  semantics are identical on all three hosts.
- Cycle rejection (E3018, full chain), depth cap 32, duplicate alias E3019,
  missing module E3017, unknown module function E3020/E3021.
- Modules export **functions only** (ADR-0015); imported functions get
  qualified KIR names (`math::add`). No value/const exports (that is the
  existing roadmap item "cross-module value exports").

Gaps for R1:

- Studio's virtual filesystem is **flat** (`vfs.ts` name regex
  `^[\w.-]+\.kin$` forbids `/`); `.kinnproj` v1 has no directories. The
  compiler and CLI accept subdirectories; **the Studio storage layer does
  not** (P-026).
- `kinn.toml` parses `name`, `version`, `entry`, `target` (unknown keys
  ignored — forward compatible). No `[web]`-style sections are parsed (the
  parser stops at the first `[section]` header), and no project layout
  conventions are enforced or needed — `entry = "src/main.kin"` already
  works today.

### A.4 Web backend and SiteModel

- Backends consume **only** a validated `SiteModel`
  (`SiteModel { name, pages: Vec<PageModel> }`,
  `PageModel { name, title, elements: Vec<Element> }`,
  `Element ∈ { Heading, Text, Button{label,target}, Link{…},
  Image{src,alt}, Divider }` — `site.rs:11–48`). The model is
  position-free and the enum is closed ("total coverage", ADR-0020) —
  **`site.rs` is the extension choke-point: any dynamic feature that
  reaches a backend must be expressible in a model, or the backend must
  reject it with a diagnostic** (ADR-0018 rule).
- Checker rules E3030–E3037 / W3035–W3037 fully validate websites before
  lowering; buttons/links must navigate (`open`), images need `alt`.
- Generated dist (verified this session): semantic HTML5 per page
  (`lang`, landmarks, skip link, `aria-current`), one stylesheet, and an
  `app.js` that **only manages focus**. Every text/attribute value is
  HTML-escaped; output is byte-deterministic (tested).
- The Android backend consumes the same `SiteModel` and is fully
  independent (no WebView/AndroidX/permissions in generated apps).

### A.5 Runtime and KIR

- `kinrt` is a zero-dependency stack VM over KIR
  (`Module/FuncDef/Instr`; ~25-instruction ISA: arithmetic, comparisons,
  jumps, calls, array ops, natives, casts). `docs/RUNTIME_ARCHITECTURE.md`
  and ADR-0002 define **KIR as the execution-swap contract**.
- The web/android backends never touch KIR today; website declarations are
  not lowered to KIR at all. For 0.4.0 this is an opportunity: app logic
  (handlers, computed values) written in Kin can be lowered to KIR by the
  *existing* pipeline and then translated to JavaScript — one semantics,
  two execution carriers (Rust VM for console, JS for web). See C.3.

### A.6 Host surfaces

| Surface | Verified state (evidence) | Reuse potential for 0.4.0 |
| --- | --- | --- |
| CLI (`kinn`) | `run/check/build/targets/tokens/ast/kir`; `build` reads `kinn.toml`, copies `assets/`, honest no-website error; 28 integration tests | `build` needs an app-aware guard and target descriptions; `kinn serve` is new |
| kinn-ffi | 10 C-ABI exports, JSON results, module registry, E3038 synthetic diagnostic, 16 tests | unchanged for dynamic builds (build JSON shape already generic over files) |
| kinn-android (JNI) | 5 JNI wrappers around the same FFI; per-call isolation; E5002 | unchanged |
| Studio web | wasm backend client, iframe `sandbox="allow-scripts"` + srcdoc preview + postMessage nav interception, store-only ZIP, problems panel already multi-file (`Problem.file`), 66 vitest (15 run the real compiler) | preview/nav plumbing reusable for SPA routes; needs folder VFS, runtime-error and console forwarding |
| Studio Android | on-device compiler via JNI; offline preview loads `file://…/index.html` with **JavaScript disabled** (deliberate); build→filesDir; SAF zip export | dynamic preview needs a redesign (C.8) |
| CI | rust-tests matrix (3 OS) + real-target smoke (web dist asserts, android manifest asserts), backend-build artifact, studio-tests (wasm + 66 vitest), android.yml builds real APKs with apksigner | add differential VM/JS job; keep all existing gates |

### A.7 Gap classification (exists / needs modification / must be built)

| # | Capability (R) | Status | Evidence / target |
| --- | --- | --- | --- |
| 1 | Multi-file compilation (R1) | **exists** | `project.rs` loader, FFI module registry, CLI/Studio/Android identical semantics (A.3) |
| 2 | Subdirectory module paths (R1) | **exists** (compiler/CLI) — **needs modification** (Studio) | `resolve_path`; `vfs.ts` flat regex (P-026) |
| 3 | Value/const exports across modules (R1) | **must be built** | ADR-0015 functions-only; roadmap item carried from 0.4 draft |
| 4 | MPA output: one HTML per page (R2) | **exists** | `backend/web.rs` (verified dist A.1) |
| 5 | Page navigation + nav state (R2) | **exists** (static) — **needs modification** (dynamic nav state) | anchors + `aria-current`; no state carry-over |
| 6 | Route parameters (R2/R3) | **must be built** | no route concept in grammar or backend |
| 7 | SPA mode, client router, not-found, transitions (R3) | **must be built** | nothing in repo mentions SPA/routing (grep-verified) |
| 8 | Reactive state, binding, derived values, minimal DOM updates (R4) | **must be built** | grammar rejects state today (ADR-0020); `web.rs:181` reserves "0.4 state module" |
| 9 | Event grammar (R5) | **must be built** | parser hint already promises "interactive buttons (state, forms) arrive in a later release" (`parser.rs:403–405`) |
| 10 | Components (R6) | **must be built** | no component concept anywhere |
| 11 | Animations (R7) | **must be built** (CSS generation + coordination) | only `prefers-reduced-motion` exists in the 0.3 stylesheet |
| 12 | Forms, validation, persistence, JSON (R8) | **must be built** | none exist; honesty rule required by owner |
| 13 | KINN Web Runtime (R9) | **must be built** | ADR-0020 explicitly has *no* runtime; must be superseded (ADR-0023) |
| 14 | Studio: multi-file browser, runtime errors, console (R10) | **needs modification** | editor/build/preview/export exist; VFS flat, no error/console bridge (P-026/P-027) |
| 15 | Android app dynamic preview (R10) | **needs modification** | WebView JS disabled by design (A.6) |
| 16 | Backend contract / front end unity (A1–A4) | **exists** — **needs modification** | add `AppModel` beside `SiteModel`; backends stay independent |

**Audit conclusion.** The 0.3.0 foundation (shared front end, SiteModel
choke-point, contextual-keyword recipe, module system with loader
callbacks, three-host FFI, honest-diagnostics culture) carries an unusually
large share of R1/R2. Everything dynamic (R3–R9) is new but has a clean
insertion point; the two most consequential design facts are (1) the
scalar/array-only type system, which shapes the entire state model, and
(2) ADR-0020's "no runtime" clause, which 0.4.0 must explicitly supersede.

---

## Part B — Phase 2: official language design (syntax & semantics)

### B.0 Design principles

1. **Contextual keywords only** (ADR-0019 recipe). No new reserved words;
   every 0.4.0 word (`app`, `component`, `state`, `computed`, `view`,
   `each`, `when`, `otherwise`, `on`, `route`, `store`, `animation`,
   `keyframes`, `from`, `to`, `step`, `duration`, `delay`, `easing`,
   `enter`, `exit`, `hover`, `press`, `scroll`, `transition`, `navigation`,
   `mpa`, `spa`, `not_found`, `form`, `field`, `kind`, `validate`, `bind`,
   `theme`, `prop`, `min_length`, `max_length`, `min_value`, `max_value`,
   `required`, `placeholder`, `label`, `pattern`, `save`, `load`, `clear`)
   remains a legal identifier outside grammar positions. A regression test
   mirroring `website_words_stay_identifiers_outside_the_declaration` is a
   gate for every new word.
2. **No HTML, CSS or JavaScript in user source.** Views are Kin
   declarations; animations map to a fixed CSS subset; generated JS is an
   implementation detail (A5). There is **no raw-HTML escape hatch** — the
   language cannot emit `innerHTML`, so XSS-by-construction stays
   impossible (same guarantee as 0.3).
3. **One front end** (A3): the new grammar is parsed, checked and lowered
   by `kinc`; the existing expression and statement parsers are **reused
   verbatim** inside views and handlers (binding expressions use the same
   `expr()` entry point; handler bodies use the same statement parser).
4. **Beginner modes honored**: B/S/A remain guidance (E/N diagnostics
   visibility); new Note-level diagnostics guide beginners.
5. **Honest capability boundaries** (ADR-0018 rule): a target that cannot
   express a construct rejects the program with a precise diagnostic, never
   silently degraded output.
6. **0.3.0 compatibility (A2)**: the `website` module, its grammar, its
   checker rules and its backend output are untouched. The 12 web backend
   tests + determinism test must keep passing byte-identically.

### B.1 Projects, files and value exports (R1)

- Layout conventions (documented, not enforced): `kinn.toml` with
  `entry = "src/main.kin"`; modules under `pages/`, `components/`,
  `styles/` are plain `.kin` files imported with existing path syntax
  (`import "components/nav.kin" as nav;`). The compiler already resolves
  subdirectory imports; **no new file machinery is required in 0.4.0
  beyond Studio's folder support** (C.8).
- **Cross-module value exports (new, S1)**: `const NAME: Type = expr;` at
  top level in any module; exported implicitly like functions; qualified
  reads `module::NAME` in constant contexts; KIR gains `Const` defs in
  `Module` (no instruction change — constants fold at lowering). Checker:
  duplicate const E3002-family code; const must be a compile-time constant
  expression (literals, arrays of literals, ops on consts). This closes the
  carried roadmap item and gives R1 its "shared application data" seam.
- `kinn.toml` stays minimal (ADR-0022). Navigation mode and app settings
  live in the source `app` declaration, not the manifest — the manifest
  keeps describing *what to build*, not *how the app behaves*.

### B.2 The `app` declaration (dynamic sibling of `website`)

```
*mode = B;

app "TaskTrack" {
    navigation spa              // or mpa — see B.9
    not_found NotFound          // optional; SPA only
    theme { accent "#2563eb" }  // optional theming tokens (closes P-020)

    state { … }                 // B.4
    computed { … }              // B.4
    animation { … }             // B.8
    store "tasks" { … }         // B.10
    page …  route …  component …// B.3, B.5, B.9
}
```

Semantics:

- Exactly one `website` **or** one `app` per program, entry file only —
  declaring both is E3052; declaring either outside the entry file is
  E3051 (mirrors E3037).
- `navigation` is `mpa` (default) or `spa`. The keyword selects the output
  and runtime mode (B.9/C.5); everything else in the grammar is identical
  in both modes.
- `app` is accepted by the **web** target only. The android target
  diagnoses `app` programs with E3070 ("this target builds static
  `website` declarations; `app` is not supported yet") — independence
  preserved (A4), honest rejection per ADR-0018. A future ADR may lower an
  app's static subset for Android; **not in 0.4.0**.
- Program modes B/S/A apply unchanged; `main` remains optional for app
  programs (same rule as websites).

### B.3 Components (R6)

```
component TaskRow {
    prop label: string               // types are the standard Kin
    prop done: bool = false          // keywords (int, float, bool,
    state expanded: bool = false     // string, [T]); "Str"/"Bool" in
                                     // early drafts was informal
    view {
        text label                   // binding to the prop (B.5)
        when done {
            text "done" exit "fade"
        }
        button "Toggle" {
            on click { done = not done; }
        }
    }
}
```

- A component has `prop` declarations (typed, optional default), local
  `state` (one declaration per line, exactly like `prop` — only the
  app-level `state` is a `{ ... }` block), and exactly one `view`.
  Event handlers may appear on elements inside the view (B.6).
  Components are declared inside `app` (or imported from a module file
  and instantiated as `module.TaskRow(...)` — the established dot form
  of calls and const reads; the `module::TaskRow` spelling in early
  drafts was informal).
- **Instantiation** inside a view: `TaskRow(label: item, done: is_done)` —
  same shape as function calls, parsed by position-insensitive named args.
  Unknown prop E3041; missing prop without default E3041; type mismatch
  E3041.
- **No component instances as values** (no closures in the language):
  components are template declarations; the compiler unrolls/statically
  instantiates each use site with its prop bindings — this keeps the
  runtime free of a component framework (C.2).
- Recursion: a component whose instantiation chain reaches itself
  (directly or via another component) is E3053. `each` is the loop
  construct; recursion is rejected at check time.
- Composition: views nest arbitrarily (elements, `when`, `each`, other
  components up to a depth cap of 32, E3054-style diagnostic on overflow).

### B.4 Reactive state, computed values (R4)

```
state {
    items: [Str] = []
    filter: Str = "all"
}

computed remaining = len(items) - done_count      // a binding expression
```

- **Shared state** lives in the `app` `state` block: typed fields with
  mandatory initializers. **Component state** is declared per line
  inside the component (`state expanded: bool = false`) — one
  declaration per line, exactly like `prop` (B.3); only the app-level
  `state` is a `{ ... }` block. Both are mutable only from event
  handlers (B.6); view
  code only reads. Names are resolved in strict order — handler local
  (`fin/var`) → component state → shared state → otherwise E3043 — and a
  handler local shadowing a state name triggers W3041 so beginners see the
  shadow immediately.
- **`computed`** declares a derived value: a pure binding expression over
  state, other computed values, component props/locals (in components),
  and functions. Recomputation is automatic on dependency change
  (dependency set computed **statically at compile time** — C.5). Cycles
  between computed values are E3044.
- Assignment rules: `state_field = expr`, `state_field[i] = expr`, and
  compound `+=`/`-=` are legal in handlers; assigning to anything else
  (props, loop variables, `const`s) is E3043 with a hint naming the
  nearest legal target.
- Type constraints: state fields are scalars or arrays of scalars
  (`Int/Float/Bool/Str` and one array dimension). The 0.4.0 state model
  deliberately does **not** invent records/maps — that decision stays with
  the structs/records ADR (Part E, D2).

### B.5 Views: bindings, conditionals, lists (R4)

Views reuse the element style of 0.3 pages, extended with three block
forms and dynamic arguments:

```
page Home {
    heading "Tasks"                    // static, exactly as 0.3
    text "Open: " + str(count)         // dynamic binding expression
    text filter_label                  // identifier binding

    each items as item key item {      // keyed list rendering
        TaskRow(label: item)
    }

    when len(items) == 0 {
        text "Nothing here yet."
    }
    otherwise {
        text str(len(items)) + " tasks"
    }

    form "Add task" { … }              // B.7
}
```

- **Dynamic arguments**: everywhere a 0.3 element takes a string literal,
  an app view may instead take a binding expression (same expression
  grammar as the language). `text`/`heading`/`label`/`placeholder`
  expressions must type as `Str` (`str()` cast applied explicitly —
  E3011-style diagnostics keep the no-implicit-conversion rule);
  conditions must be `Bool`; `each` iterables must be arrays.
- **`when <cond> { … } otherwise { … }`** is conditional rendering
  (element presence), not control flow — it removes/mounts DOM subtrees
  and participates in enter/exit animations (B.8). `if` remains a
  statement inside handlers; views use `when` so the two never blur.
- **`each <array-expr> as <name> [key <expr>] { … }`** renders lists. The
  loop variable is read-only (assignment E3043, mirroring `for`). `key`
  supplies the reconciliation key expression; omitted keys fall back to
  index-based reuse with W3042 ("reorderings will reuse DOM by position —
  add a stable key"). Duplicate keys within one `each` are W3043.
- Elements keep one-line declarations; blocks (`{ … }`) open after the
  line, exactly like `image … alt "…"` extends one element today. The
  parser gains `when`/`each`/component-instantiation cases inside the
  existing `element()` dispatch plus the sync-word recovery list (B.0),
  preserving multi-error recovery.

### B.6 Events (R5)

```
button "Save" {
    on click   { saved = true }                 // element events
    on key "Escape" { open_form = false }       // keyboard with key filter
}
field draft kind text {
    on input  { draft = event_value }           // manual alternative to bind
    on focus  { … }
    on blur   { … }
}
form "Add task" {
    on submit { if form_valid { … } }           // form-level (official statements)
}
```

- Supported events (complete set for 0.4.0): `click`, `input`, `change`,
  `submit`, `focus`, `blur`, `keydown`, `keyup`, `pointerdown`,
  `pointerup`. `on key "Enter"` filters keydown/keyup by key string.
  Unknown event word: E2031 (parser); unknown key filter is not checked
  (runtime data, not semantics).
- **Handler bodies are official Kin statements** (semicolons required;
  conditionals are `if` — views use `when` so the two never blur, B.5;
  an early draft of the B.7 example showed `when` inside a handler and
  is corrected in this document), parsed by the existing statement
  parser and type-checked by the existing checker, with three
  app-specific rules: assignment targets resolve per B.4; handler-only
  builtins are in scope (`event_value: Str` on input/change, `event_key:
  Str` on keydown/keyup, `event_x/event_y: Int` on pointer events,
  `form_valid: Bool` inside `on submit`); and `return` is illegal in
  handlers (E3045).
- `while` is allowed in handlers but always triggers W3044 ("a long loop
  in an event handler freezes the page; prefer `for` over a range") —
  honest guidance, not a prohibition.
- Handlers are lowered to KIR functions (same lowering as any Kin
  function) and compiled to JS (C.3). They may call any function of the
  program, including imported module functions — R1's "modules and
  components communicate through the official system".
- Multiple handlers per element and one handler per event name
  (duplicate `on click` on one element: E3046).

### B.7 Forms, validation, persistence, JSON (R8)

```
store "tasks" { items, filter }        // persisted state keys (B.10)

state { draft: Str = "" }

form "Add task" {
    field draft kind text {
        label "Task title"
        placeholder "What needs doing?"
        required
        max_length 80
    }
    field qty kind number { min_value 1 max_value 99 }
    field due kind date { label "Due" }
    field email kind email { pattern "^[^@]+@[^@]+$" }
    button "Add" {
        on submit {
            if form_valid {          // official statements: `if`, not
                items = items + [draft]  // `when` (B.5/B.6); the first
                draft = ""               // draft of this example used
                store_save("tasks")      // `when` — corrected
            }
        }
    }
}
```

- `field <state-key> kind <text|number|date|email|password|textarea>` binds
  the field **two-way** to a shared/component state key (`bind` is the
  explicit form for non-form inputs: `input bind draft`). Validation
  rules (`required`, `min_length`, `max_length`, `min_value`, `max_value`,
  `pattern`) are checked by the generated form logic **in the browser**
  and surfaced as `form_valid` plus `field_errors` (array of human-readable
  messages) in the submit scope. The same rules are re-checked in Kin at
  submit time when the handler repeats them — no server validation is
  claimed anywhere.
- **Local persistence (`store`)**: a `store` block names state keys
  persisted to the browser's `localStorage` under
  `kinn:<app>:<store>` with a schema-hash guard (shape change → old data
  discarded, never misread). `store_save("tasks")`, `store_load("tasks")`,
  `store_clear("tasks")` are handler builtins. Hydration happens
  automatically at boot for MPA and SPA alike (C.5).
- **Honesty clause (normative)**: persistence is the browser's local
  storage on one device in one browser profile. It is **not a backend
  database**: no sync, no multi-device, no multi-user, no durability
  guarantee beyond the browser's own (users can clear it). Documentation,
  generated README, Studio copy and error text state this in exactly these
  terms.
- **JSON**: `json_of("tasks")` returns the store snapshot as a JSON string
  (for export into a textarea/clipboard) and `save_json("tasks", text)`
  parses and stores one (rejecting mismatched shapes with `false`). JSON
  is the serialization format of **declared state snapshots** — it is not
  a general document API. Rich JSON documents need records/maps and are
  explicitly deferred to the structs/records decision (Part E, D2; ties to
  P-004/P-013).
- **No network in 0.4.0**: there is no fetch/http construct, and none may
  be implied by docs or examples.

### B.8 Animation (R7)

```
animation {
    keyframes "pop" {
        from { opacity 0  scale 0.96 }
        to   { opacity 1  scale 1 }
        duration 200
        easing ease_out
    }
    transition "fade" { duration 150 easing linear }   // enter/exit default
}

page Home {
    heading "Tasks" enter "pop"              // mount animation
    each items as item { … exit "fade" }     // exit before removal
    button "Save" hover "raise" press "squish"   // press feedback
    image "hero.png" alt "Hero" scroll "reveal"  // viewport reveal (once)
}
app "…" { transition "fade" }                // page/route transitions (SPA)
```

- **`animation` block** defines named `keyframes` (steps `from`, `to`,
  `step N`; properties from a fixed whitelist: `opacity`, `x`, `y`,
  `scale`, `rotate`, `blur` — mapped to `opacity`, `translate`,
  `scale()`, `rotate()`, `blur()`; no arbitrary CSS) and named
  `transition`s. `duration`/`delay` are integer milliseconds; `easing` is
  one of `linear`, `ease`, `ease_in`, `ease_out`, `ease_in_out`,
  `cubic(a, b, c, d)`. Unknown name (element side) E3047; unknown
  property E3048; bad easing E3049; non-positive duration E3050.
- **Element attributes**: `enter`, `exit`, `hover`, `press`, `scroll`,
  `transition` (page-level). `hover`/`press` compile to pure CSS
  (`:hover`/`:active`) — zero runtime cost. `enter` fires on mount and on
  `when`-block appearance; `exit` delays DOM removal until the
  transition ends (`transitionend` + timeout fallback). `scroll` uses one
  shared `IntersectionObserver` (C.6). All of it degrades silently under
  `prefers-reduced-motion` (0.3 stylesheet already ships the media query).
- **State-driven animation** is `when` + `enter`/`exit` (B.5): no
  imperative animation API is exposed — animations are declarations the
  runtime coordinates (C.5), never user-written JavaScript.

### B.9 Routing: MPA and SPA (R2, R3)

```
app "Shop" {
    navigation spa
    not_found Missing
    route "item/:id" -> ItemDetail
    page Home { … }
    page ItemDetail { … }        // reads route param via param("id")
}
```

- **MPA** (default) keeps 0.3's shape and upgrades it: one independent
  HTML per `page` (+ one per dynamic `route`), full page loads, anchors
  for navigation, `aria-current` marking. Shared state travels across
  pages through the store/hydration channel (C.5) — one honest mechanism,
  not magic. Dynamic route params in MPA arrive as query parameters
  (`item.html?id=7`); `param("id")` reads them.
- **SPA** emits one shell `index.html`; the runtime router swaps pages
  without reloads, supports dynamic path segments (`:param`, `param()`
  accessor), `not_found` rendering for unmatched paths, per-route
  `transition`, navigation history (back/forward), `aria-current`
  updates, `document.title` updates and focus restoration to the main
  heading (0.3's a11y pattern, carried over).
- **`file://` honesty (normative)**: the HTML5 history API does not work
  from `file://`. A SPA opened from disk automatically falls back to hash
  routing (`#/item/7`) and the build report says so; MPA always works
  from `file://`. Hosting a SPA "for real" means any static file server
  with the standard fallback rule (serve `index.html` for unknown paths)
  — documented in the generated README.
- `route` paths are validated at check time: segments are literals or
  `:param`s (malformed path E3055; duplicate route E3056; unknown target
  page E3032-family; `not_found` missing in SPA E3057; `param("…")` for
  an undeclared param E3058).
- Navigation state (both modes): scroll position per route (SPA:
  `sessionStorage`; MPA: browser restoration), current route identity,
  and `aria-current` are preserved/restored by the runtime.

### B.10 `str()` and small string helpers (enabling bindings)

Bindings need strings: `str(expr)` (cast — exists), `upper(s)`,
`lower(s)`, `trim(s)`, `contains(s, sub)`, `split(s, sep) -> [Str]`,
`join(arr, sep) -> Str` are added as natives (checker table + VM natives +
JS equivalents, all differentially tested — C.3). This is the minimum set
bindings realistically need; anything richer waits for interpolation
(P-004).

### B.11 Semantic rules and new diagnostics

Reserved ranges: parser **E2031–E2039** (app/view grammar), checker
**E3040–E3079**, warnings **W3040–W3049**, notes **N3040+**. Assigned in
this proposal (final wording at implementation, codes frozen by tests):

| Code | Meaning |
| --- | --- |
| E2031 | unknown event name in `on` |
| E2032 | malformed `each`/`when`/component instantiation syntax |
| E2033 | view argument must be expression or string |
| E3040 | unknown component |
| E3041 | prop mismatch (unknown / missing / type) |
| E3042 | duplicate state/computed/prop name in one scope |
| E3043 | invalid assignment target (not handler local / state) |
| E3044 | computed-cycle or handler-restricted construct |
| E3045 | `return` inside an event handler |
| E3046 | duplicate event handler on one element |
| E3047 | unknown animation name on element |
| E3048 | keyframes property outside the whitelist |
| E3049 | unknown/invalid easing |
| E3050 | invalid duration/delay |
| E3051 | `app` outside entry file |
| E3052 | both `website` and `app` declared |
| E3053 | component recursion |
| E3054 | view nesting/instantiation depth overflow |
| E3055 | malformed route path |
| E3056 | duplicate route |
| E3057 | SPA without `not_found` page (or `not_found` in MPA) |
| E3058 | `param()` for undeclared route param |
| E3059 | store key not a declared state field |
| E3060 | store type/shape mismatch against state |
| E3061 | field kind/state type mismatch (e.g. `kind number` bound to `Str`) |
| E3062 | app name empty or whitespace-only (mirrors E3033; assigned at S3, found in the S2 review — F-015) |
| E3063 | unknown app page target — any page-target position (`route "x" -> Missing`, `not_found Missing`, `open <Page>` on buttons/links in page and component views, nested views included). App-scoped sibling of E3032 (website-scoped, frozen) — never reused. Hint lists declared pages; a target that names a component says "'X' is a component; routes target pages" (assigned in slice 2 — P-029) |
| E3064 | duplicate page name inside an app — case-insensitive (mirrors E3031: page slugs are lowercased; assigned in slice 2 — P-029) |
| E3065 | duplicate component name inside an app — case-sensitive (identifier semantics, matching function names; assigned in slice 2 — P-029) |
| E3066 | (reserved — remaining S3 slices: scope/handler/prop/store/field/animation semantics) |
| E3067 | (reserved) |
| E3068 | (reserved) |
| E3069 | (reserved) |
| E3070 | target does not support `app` (android) |
| W3040 | `while` inside an event handler |
| W3041 | handler local shadows a state name |
| W3042 | `each` without stable `key` |
| W3043 | duplicate `each` key values |
| W3044 | state field written but never read / never rendered |
| N3040 | beginner hint: event handlers run in the browser, state is local |

The E3038 inconsistency found by the audit (CLI prints the no-website
error as a plain message while FFI emits code E3038) is fixed as part of
S11: the CLI prints the same code for both the website and app guards.

### B.12 Compatibility guarantees (A2)

- `website` programs compile to **byte-identical output** (the dynamic
  path is additive; `backend/web.rs` static generation is untouched).
  Gate: existing 12 web tests + determinism test + a new golden test
  that builds `examples/web/site.kin` and compares against a committed
  checksum.
- Every new contextual word gets a "stays an identifier" regression test.
- Existing error codes never change meaning; new codes live in the
  reserved ranges above (ADR-0011: codes are public interface).
- Existing `.kinnproj` v1 files keep importing (v2 adds optional
  directories; v1 files auto-upgrade).

---

## Part C — Phase 3: KINN Web Runtime architecture

### C.1 Goals and non-goals

**Goals.** One small, dependency-free JavaScript runtime (`kinnrt.js`)
that provides: reactive DOM updates from a **statically compiled binding
graph**, event dispatch to compiled handlers, component/lifecycle
mount-unmount, SPA routing and MPA hydration, shared state and stores, and
animation coordination. It ships as a static, versioned asset
(`kinnrt.js` with a `KINN <version> web runtime` header), hand-written,
hand-auditable, **no build/minification step** (the compiler core stays
zero-dependency; the runtime adds zero third-party code — A6).

**Non-goals (normative).** No React/Vue/Svelte/Preact or any framework;
no virtual-DOM library; no `eval`/`new Function`; no `innerHTML`
assignment (DOM writes go through `textContent`, `setAttribute`,
`insertBefore` etc.); no proxy-based magic; no server rendering; no
network calls; no WebComponents; no TypeScript build step for the runtime
(plain ES2020 JavaScript in-repo).

**Size budget (hard, CI-tested):** `kinnrt.js` ≤ 16 KB as shipped;
per-app `app.js` reported at build time (soft budget 4 KB + ~0.5 KB per
binding; reported, not gated). Memory goal: no per-frame allocations in
steady state; event listeners are delegated; steady-state memory target
≤ 2× static-0.3 baseline for equivalent pages (measured in the
verification matrix, D.2).

### C.2 Layered design

```
kinnrt.js (static asset, one file)
├── State        — plain object holding state + computed caches; subscribe(bindingId)
├── Flush        — microtask-batched update scheduler (no sync re-entry)
├── Binder       — compiled binding map (id → { deps, run }); writes text/attrs/
│                  visibility/list patches to live DOM nodes
├── Lists        — keyed reconciliation (map key→node, reorder/mount/unmount)
├── Events       — root-level delegated listeners; event builtins; handler calls
│                  into generated KIR-JS functions
├── Lifecycle    — mount/patch/unmount of view subtrees; enter/exit coordination
├── Router       — SPA: location ⇄ route+params, history/popstate, hash fallback,
│                  title/focus/aria-current, per-route transitions
├── Hydrator     — MPA: sessionStorage state snapshot out/in; store hydration
├── Store        — localStorage persistence with schema-hash guard
├── Anim         — class toggling, transitionend/timeout exit waits, one shared
│                  IntersectionObserver for scroll reveals
└── Reporter     — window.onerror + console.* forwarding (postMessage when
                   embedded; console.group styling otherwise)
```

The runtime has **no knowledge of any specific app**: everything app-specific
(state shapes, binding graph, component templates, routes, animation names,
KIR-JS functions) lives in the generated `app.js`, which calls
`KinnRT.mount(manifest)` with a JSON manifest. This keeps the runtime
versionable, cacheable across apps, and independently testable.

### C.3 Executing Kin logic in the browser: KIR → JS

App functions, handlers and computed expressions are **not** interpreted
from source and not re-implemented: the existing pipeline lowers them to
KIR (same lowering as console programs), and a new `backend/webkir.rs`
translates KIR functions to JavaScript.

- Value mapping: `Int` → JS number (see precision note), `Float` → number,
  `Bool` → boolean, `Str` → string, `Array` → JS array (reference
  semantics match ADR-0014 exactly), `Unit` → undefined.
- **Checked arithmetic**: JS numbers are float64; Kin `Int` is i64. The
  generated code uses helper functions in `kinnrt.js`
  (`k_add/k_sub/k_mul/k_div/k_mod`) with a fast in-range path and
  E4004/E4001-equivalent throws on overflow/zero-division — the browser
  behaves like the VM, not like IEEE slop.
- Casts, `Value::format` printing parity (for `print`), array deep
  equality and string helpers (B.10) are implemented in `kinnrt.js` and
  **differentially tested**: the test corpus runs each program through
  `kinrt::run` (Rust) and through the generated JS under Node, comparing
  stdout byte-for-byte (D.2). KIR carries source lines
  (`FuncDef.lines`), so JS errors map back to `.kin` lines (C.7).
- This reuses ADR-0002's "KIR is the swap contract" in the direction it
  was designed for: a second execution carrier without a second semantics.

### C.4 Generated artifact contract (dynamic web target)

| Mode | Files |
| --- | --- |
| MPA | `index.html` + one `.html` per page/route + `styles.css` + `app.js` + `kinnrt.js` + `assets/` |
| SPA | `index.html` (shell: landmarks, nav, mount node) + `styles.css` + `app.js` + `kinnrt.js` + `assets/` |

- `app.js` (generated, deterministic): manifest JSON (state defaults,
  binding graph with static dependency sets, component templates, routes,
  animation names, store keys) + KIR-JS functions for handlers/computed.
  Sorted keys, stable ordering everywhere; byte-equality rebuild test.
- `styles.css` (generated): 0.3 base stylesheet + theme tokens (B.2) +
  generated keyframes/transitions + hover/press classes. The 0.3 static
  stylesheet itself is untouched.
- `index.html`/pages keep 0.3's semantic HTML5 skeleton (landmarks, skip
  link, `aria-current` for MPA nav, escaping). Every dynamic text/attr
  write goes through the runtime's escaping-equivalent DOM APIs.
- Integrity: the runtime file records its version in
  `KinnRT.version`; `app.js` asserts compatibility (mismatch = visible
  error, not silent misbehavior).

### C.5 Update pipeline (why DOM updates stay minimal — R4)

1. A handler runs as plain JS and mutates the state object (no tracking
   proxies, no per-property getters).
2. On completion, `Flush` schedules one microtask; all dirty bindings are
   collected from a **statically compiled dependency map** (binding → set
   of state fields, computed at check/lower time — no runtime dependency
   discovery, no surprises).
3. `Binder` re-evaluates each dirty binding (a KIR-JS call), compares with
   the cached last value (scalars by value, arrays by length+contents
   fingerprint), and patches only changed text nodes/attributes/list
   ranges.
4. `Lists` reconciles keyed children (mount/reorder/unmount), honoring
   exit animations before removal.
5. One rAF-aligned paint; no polling, no timers in steady state
   (`visibilitychange` pauses observers; listeners are passive where
   allowed).

Computed values cache on their dependency set; a state write that no
binding depends on costs a set-membership check — batching makes
N-handler-updates cost one DOM pass.

### C.6 Mobile constraints (A7)

- ES2020 baseline (Chromium ≥ 90, Firefox ≥ 90, Safari ≥ 14 — matches
  minSdk 26 devices with updated WebView); no transpilation.
- One delegated listener set per document (not per element); one shared
  `IntersectionObserver` for all scroll reveals; `requestAnimationFrame`
  used only inside active transitions.
- No layout thrash patterns: bindings batch reads/writes within the
  microtask flush; animation classes toggle transform/opacity only
  (compositor-friendly properties, mirroring the keyframe whitelist).
- Memory: state object + binding cache + DOM only; no per-binding
  closures for static content (static strings are inlined into templates).
- `prefers-reduced-motion` disables enter/exit/scroll JS paths entirely
  (not just visually), per the 0.3 stylesheet's precedent.

### C.7 Runtime error reporting and diagnostics (R10)

- Kin-level runtime errors (E4000–E4005 family, thrown by KIR-JS helpers
  with `FuncDef.lines` mapping) surface as
  `console.error` +, when embedded, `postMessage({kinnRuntimeError:
  {code, message, kinFile, kinLine}}, …)`.
- `window.onerror` and unhandled promise rejections produce the same
  envelope (labeled `E4099` — unexpected browser error, honest label).
- Studio consumes these into the existing Problems panel (which already
  carries `file`/`line`); `console.*` forwarding appears in the Studio
  terminal (`{kinnConsole: {level, text}}`), fulfilling the "browser
  console integration" requirement without a real DevTools bridge.
- Generated `app.js` emits `//@kinn:file:line` markers per handler so the
  mapping table is auditable by hand.

### C.8 Host integration deltas

- **Studio web**: Vfs v2 (path-aware names, tree explorer, `.kinnproj` v2
  with `files: [{path, content}]`, v1 auto-upgrade); preview builds on the
  existing srcdoc + postMessage channel — the SPA router inside the
  preview emits `{kinnPreviewRoute: {path}}` and the Studio page selector
  becomes a route selector (MPA keeps the page selector); runtime-error
  and console forwarding wired into Problems/terminal; build tab gains
  the dynamic target with the same honest per-file report.
- **Studio Android**: dynamic preview redesign without AndroidX — load the
  built page via `loadDataWithBaseURL("https://kinn.local/", html, …)`
  (inlined CSS/JS like the web preview) with **JavaScript enabled but all
  file access disabled** (`allowFileAccess=false`,
  `allowFileAccessFromFileURLs=false`), keeping the no-network/no-permission
  posture; runtime errors surface in the in-app terminal; static
  `website` preview keeps the current JS-disabled flow.
- **CLI**: target description updated ("static website or dynamic web
  app"); no-site/no-app guard prints E3038 (alignment, B.11); build
  report notes SPA `file://` hash fallback; optional `kinn serve`
  (localhost static server with SPA fallback) gated on decision D4.

### C.9 Relationship to static targets (A4)

`AppModel` (new lowering product, sibling of `SiteModel`:
`Compiled { module, mode, sources, site, app }`) is consumed **only** by
the web dynamic path. `backend/android.rs` keeps consuming `SiteModel`
exactly as today and rejects app programs with E3070. `backend/web.rs`
static generation keeps consuming `SiteModel` for `website` programs —
byte-identical output. No model is ever fed to a backend that did not
declare support for it.

---

## Part D — Phase 4: implementation roadmap (steps, tests, acceptance)

Sequencing rules: every step lands with tests green on `develop`; no step
claims a feature without a test or a logged verification; each step ends
with CHANGELOG/AI_CONTEXT/PROBLEM_REGISTRY updated (standing rules).
Estimates are ordering and dependency statements, not dates.

### S1 — Cross-module value exports (`const`)

- Scope: `const NAME: Type = expr;` (B.1); lexer/parser/checker/lowering
  (constants fold into KIR); qualified reads `module::NAME`.
- Tests: parser (syntax + recovery), checker (duplicate, non-constant
  expr, type mismatch), pipeline (import a const into a function), CLI
  (run + kir shows folded constant).
- **Acceptance:** a program can share configuration values across files;
  all 250 existing tests still green; new tests ≥ 12.

### S2 — App grammar (parser + AST)

- Scope: `app` declaration and all view grammar (B.2–B.9): state,
  computed, component/prop/view, when/otherwise, each/key, on-events,
  form/field, animation block, keyframes/transition, store, route,
  not_found, theme, navigation; sync-word error recovery in views;
  contextual-word regression tests.
- Tests: parser unit tests per construct + recovery + word-stays-
  identifier set; `kinn ast` dump; examples added
  (`examples/web/tasks_app.kin` target shape).
- **Acceptance:** every grammar error produces a positioned E2xxx
  diagnostic and parsing continues (multi-error pages); 0.3 grammar
  untouched (all existing parser tests green).

### S3 — App checker (semantics)

- Scope: all B.11 rules — scope resolution (locals/state/computed/props),
  binding typing, handler subset enforcement, prop checking at every
  instantiation site, route/store/animation/field validation, E3052/
  E3051, target capability pre-checks (E3070 knowledge lives here as
  "app rejected for android"), W3040–W3044, N3040.
- Tests: two checker tests per code (positive + negative) — the code
  table in B.11 is frozen by this test suite.
- **Acceptance:** every E3040–E3070 has ≥ 2 tests; static website
  checking unchanged (all website checker tests green).

### S4 — AppModel lowering + static-target honesty

- Scope: `lower::app_model` → `AppModel` (templates, binding graph with
  static dependency sets, handler KIR function names, route table,
  animation table, store keys); `Compiled.app`; android target rejects
  app programs with E3070; **static `website` lowering untouched**.
- Tests: lowering golden tests (model shapes per feature); android
  rejection test; binding-dependency extraction tests (the dependency
  map is data — assert exact sets).
- **Acceptance:** `Compiled.app` fully position-free; no AppModel path
  reaches android; site model byte-identical for 0.3 programs.

### S5 — KIR → JS codegen + differential harness

- Scope: `backend/webkir.rs` translating the full KIR ISA to JS using
  `kinnrt.js` arithmetic/cast/format helpers; string helpers (B.10) in
  VM + JS; Node-based differential harness comparing `kinrt::run`
  stdout against generated-JS output over the corpus
  (`examples/*.kin` + errors/*.kin + synthesized edge cases: overflow,
  div-zero, deep equality, unicode, float formatting).
- Tests: golden JS snapshots per ISA construct; differential corpus run
  (both engines agree byte-for-byte or the test fails with a diff).
- **Acceptance:** 100% of the corpus agrees between VM and JS; KIR-JS
  output is deterministic; no `eval` in output (asserted by scan).

### S6 — KINN Web Runtime v1 (reactivity core)

- Scope: `kinnrt.js` layers State/Flush/Binder/Lists/Events/Lifecycle/
  Reporter (C.2, C.5, C.7); manifest contract; MPA boot path.
- Tests: vitest suites driving a real DOM (jsdom) — binding updates,
  batching, keyed reconciliation cases (append/remove/reorder/dup-key),
  event dispatch + builtins, error reporting envelope; size-budget CI
  assertion (≤ 16 KB).
- **Acceptance:** an MPA todo app (state + events + each + when + form)
  works end-to-end in the preview; DOM mutation count per update ==
  expected minimal set (asserted in tests); all Studio tests green.

### S7 — Router (SPA) + hydration (MPA)

- Scope: Router (history + hash fallback, params, not_found, title/
  focus/aria-current, transitions) and Hydrator/Store (sessionStorage
  snapshots, localStorage stores with schema hash, `file://` fallback
  reporting) — B.9/B.10/C.5.
- Tests: routing table tests (push/replace/back/forward/unknown),
  param access, not-found rendering, transition sequencing; hydration
  round-trip (MPA page A → page B state visibility; corrupted snapshot
  → clean reset); schema-hash migration test.
- **Acceptance:** SPA navigation never reloads the document (test
  asserts singleton document); `file://` simulation uses hash fallback;
  MPA passes state across pages through the documented channel only.

### S8 — Animations

- Scope: CSS generation for keyframes/transitions/hover/press; Anim
  coordinator (enter/exit waits, scroll observer); reduced-motion
  behavior (C.6).
- Tests: generated-CSS golden tests; enter/exit sequencing (exit waits
  for transitionend or timeout); scroll-once reveal; reduced-motion
  disables JS animation paths.
- **Acceptance:** no layout-thrash pattern in the runtime (asserted by
  batching tests); animations are pure declarations in source; 0.3
  static stylesheet byte-identical.

### S9 — Forms, validation, stores, JSON

- Scope: form/field rendering, two-way binds, validation rules →
  `form_valid`/`field_errors`, submit flow; store builtins; `json_of`/
  `save_json` (B.7); honesty strings in READMEs/diagnostics.
- Tests: validation matrix per rule; bind round-trips; store
  persistence + clear + shape mismatch; JSON export/import round-trip;
  honesty-claim tests (docs contain the "not a backend database"
  statement; no fetch/http in runtime — grep-asserted).
- **Acceptance:** a store-backed form app survives a simulated reload
  (state restored); no network API appears anywhere in generated output.

### S10 — Studio + Android app integration

- Scope: C.8 — Vfs v2 folders + `.kinnproj` v2, route/page preview
  selector, runtime-error/console forwarding into Problems/terminal,
  build/zip/export for dynamic dists; Android app preview v2
  (loadDataWithBaseURL, JS on, file access off) + error surfacing.
- Tests: vitest (vfs v2 round-trip incl. v1 import, preview route
  events, error envelope → problems panel); Android unit tests for the
  new preview settings; toolless degradation unchanged.
- **Acceptance:** a multi-folder project builds and previews in Studio;
  a runtime error thrown by a handler appears in Problems with the right
  `.kin` file/line; Android preview shows the dynamic app with no file
  access granted (settings asserted in tests).

### S11 — CLI, docs, verification, release hygiene

- Scope: CLI guards/report updates (E3038 alignment, SPA file:// note);
  `kinn serve` (D4 — owner-confirmed 2026-11-22); docs: BACKEND_WEB
  rewrite (0.4 section),
  LANGUAGE_SPEC additions, RUNTIME_ARCHITECTURE (web runtime chapter),
  SYNTAX refresh (P-025), MODULE_SYSTEM (consts), COMPILER_ARCHITECTURE
  crate-count fix; CHANGELOG/AI_CONTEXT/PROBLEM_REGISTRY; packaging CI
  version fix (P-024); AUDIT_REPORT 0.4 section with browser evidence.
- Tests: CLI integration (dynamic build, manifest-driven build, android
  rejection); CI additions (D.2).
- **Acceptance:** `kinn build --target web` on an app produces the full
  dynamic dist; `--target android` on the same program fails with E3070;
  all docs state capability honestly; headless-browser verification log
  committed (D.2).

### D.2 Verification matrix (release gate for 0.4.0)

| Level | What | Where |
| --- | --- | --- |
| Unit | parser/checker/lowering/codegen per construct | `kinc` tests (target ≥ +120 tests over the release) |
| Differential | VM vs generated JS, byte-identical stdout | S5 harness, in CI (studio job has Node) |
| Runtime | jsdom-driven reactivity/router/store/animation suites | Studio vitest |
| Browser | headless Chromium: todo-app flows, a11y tree (landmarks, aria-current, focus), no console errors, route deep-link, reduced-motion | manual log until the Playwright harness lands (D5 — owner-confirmed 2026-11-22; harness proposed with S6, mandatory here at S11) |
| Budget | kinnrt.js ≤ 16 KB; app.js reported; no eval/innerHTML (source scan) | CI assertion |
| Compat | 0.3 dist golden checksum + full existing suite | CI |
| Perf sanity | update-batching assertions; steady-state allocation check in jsdom; manual low-end device note (honest: emulator only, P-017 analog) | S6/S10 tests + audit report |
| Hosts | CLI + wasm (Studio) + JNI (Android app) all build the same dynamic dist from the same sources | three-host parity tests |

### D.3 CI deltas

- `rust-tests`: unchanged + new unit suites.
- `backend-build`: add a dynamic app build artifact (tasks app dist).
- `studio-tests`: add differential job (Node), size-budget assertion.
- `android.yml`: unchanged (backend APK provider still builds the static
  example; dynamic apps are web-target only in 0.4.0).
- Packaging workflows: version strings 0.2.0 → 0.3.0 → keep in sync with
  the release (P-024 fix lands in S11).

---

## Part E — Decisions required from the owner (Phase 5 gate)

> **RESOLVED 2026-11-22.** D1–D3 and D6 stand per the owner directive
> and the silence clause (recommendation = decision): D1 renumber (0.4.0
> dynamic web; graphics 0.5.0; developer comfort 0.6.0), D2 no
> records/structs in 0.4.0, D3 MPA default, D6 animation whitelist
> freeze. **D4 and D5 were explicitly owner-confirmed on 2026-11-22**:
> D4 include `kinn serve` with the documented security boundary (lands
> in S11), D5 add Playwright to CI as a dev dependency, CI only — the
> compiler core and kinnrt.js stay dependency-free (harness proposed to
> land with S6; mandatory at the S11 browser gate, D.2). Phase 5 runs
> S1→S11 in order; ADR-0023…0028 encode the decisions and the S1/S2
> landing.

Implementation starts after these are answered. Each carries a
recommendation; silence on a point = the recommendation stands.

- **D1 — ROADMAP renumbering.** Make "Dynamic web applications" the
  0.4.0 milestone (this proposal), moving "breadth and developer comfort"
  items (VM debugging, emulator UI tests, AppImage verification) to a new
  0.6.0; graphics stays 0.5.0. *Recommended: yes* — matches the owner
  directive that named 0.4.0 "Dynamic Web Applications".
- **D2 — Records/structs stay out of 0.4.0.** State = scalars/arrays of
  scalars; JSON = state snapshots (B.4/B.7). Rich state shapes wait for
  the structs/records ADR (already a carried roadmap item). *Recommended:
  defer* — keeps the runtime tiny and the semantics total; a records ADR
  can layer on later without breaking the state model.
- **D3 — SPA vs MPA default.** `navigation mpa` is the default for `app`
  (static-friendly, `file://`-safe), SPA is opt-in via `navigation spa`.
  *Recommended: mpa default* — safest upgrade path from 0.3 and simplest
  hosting story; SPA remains fully supported.
- **D4 — `kinn serve` (localhost dev server with SPA fallback) in 0.4.0
  or later?** *Recommended: include* (it is small and makes SPA hosting
  behavior testable), but it is separable without touching anything else.
- **D5 — Playwright (headless browser) in CI.** The verification matrix
  needs real-browser evidence; adding a Playwright step introduces a dev
  dependency in CI (not in the compiler, not in the runtime). *Recommended:
  add it*; alternatively 0.4.0 ships with a manual headless-verification
  log like 0.3.0 did.
- **D6 — Animation scope freeze.** The whitelist (B.8) is intentionally
  small (opacity/transform/filter only). Anything outside waits for
  0.5+. *Recommended: freeze* — keeps generated CSS auditable and
  mobile-compositor friendly.

**What is explicitly NOT in 0.4.0** (honest scope line): records/maps,
closures, string interpolation (P-004 decision unchanged), string→number
casts beyond `str()` usage above (P-013 decision unchanged), network/fetch,
SSR, theming beyond the token whitelist, WebComponents, TypeScript in the
runtime, any third-party frontend framework, Android dynamic rendering.

---

*End of proposal. ADR-0023…ADR-0027 in `docs/DECISIONS.md` encode the
decisions above; the Phase 5 implementation sequence begins at S1 once
the owner approves.*
