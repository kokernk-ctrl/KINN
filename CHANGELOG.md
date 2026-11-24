# Changelog

All notable changes to KINN are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow semantic versioning once there is a stable language surface.

## [Unreleased] — 0.4.0 (Phase 5 implementation started)

Dynamic web applications (owner directive). Design phases 1–4 landed
as `docs/DYNAMIC_WEB_ARCHITECTURE.md` + ADR-0023…0027; **the owner
approved all Part E decisions (D1–D6) on 2026-11-22 and Phase 5
implementation began with S1.**

### Added — Studio NEXT: experimental terminal-first interface (0.4)

- **A second, isolated Studio interface** (`studio/next.html` +
  `studio/src/next/`, an entirely new module) alongside the unchanged
  classic Studio (`index.html`, one additive `Next` link in its top
  bar; both link to each other; both ship in `dist/` via two Vite
  inputs). Dark developer environment: `#0B0C0E` background, graphite
  surfaces, thin `#2B3037` borders, discreet `#FF8A32` accent,
  monospace-first, 3px radii, minimal motion, `prefers-reduced-motion`
  respected.
- **Main workspace**: top bar (identity, `project › file` breadcrumb,
  live build-status pill driven by real command statuses, Run/Build,
  layout focus switch, palette, settings, classic link), collapsible
  explorer (real tree of the flat project + rename/delete + project
  info facts), editor tabs (rendered from workspace state, modified
  dots), code editor (same overlay technique as classic + current-line
  highlight, configurable indent, find bar with match count and
  next/prev), **multi-session terminal** (up to 4 sessions, per-session
  history/scrollback/status dots), problems panel (cross-file jump),
  output panel (real build artifacts + .zip/preview/export actions),
  command palette (Ctrl+K, commands + go-to-file, fuzzy ranked,
  only-working commands), settings modal (editor/terminal font, indent,
  auto-check — all applied immediately, persisted, validated), bottom
  status bar (auto-save state, compiler version, file, Ln/Col, last
  command status, ONLINE/OFFLINE informational dot).
- **Terminal-first experience**: the terminal is a first-class surface
  with layout focus modes (editor / split / terminal, Ctrl+1/2/3),
  Ctrl+` panel toggle, Ctrl+B explorer toggle, and a dedicated
  terminal focus shortcut set; the command engine (`termcmd.ts`) is
  headless and drives the **real compiler/VM/backends** through the
  wasm bridge: `check`, `run`, `build web`, `build android`, `tokens`,
  `files`, plus file management and project import/export. Unavailable
  capabilities state exactly what is missing (kinn.wasm build
  instructions; Android = Gradle project, not an APK) — nothing is
  simulated.
- **Mobile**: single-view layout switched by a bottom navigation
  (files/editor/terminal/output), 44px touch targets, visual-viewport
  tracking so the keyboard never covers the terminal input, 16px
  terminal input font (no zoom-on-focus), condensed top bar/status
  bar; same visual identity as desktop.
- Tests: +58 vitest (palette fuzzy matching, settings validation,
  workspace state, terminal engine fallbacks with a fake sink, and the
  engine against the **real kinn.wasm** — run stdout/exit codes,
  compile errors driving PROBLEMS, web/android build artifacts,
  preview hand-off). The classic Studio suite passes untouched.
- Docs: `docs/KINN_STUDIO_NEXT.md` (architecture, components,
  terminal/editor/compiler integration, responsive layout, honest
  limitations, rollback path); `AI_CONTEXT.md` updated so future
  agents can work on the Studio without rescanning.

### Added — Studio NEXT on Android: the 0.4.0 APK (launcher default)

- **`NextActivity` (android/)**: the app now launches directly into
  Studio NEXT. The built web bundle (`studio/dist`: `next.html` +
  classic entry + JS/CSS + `kinn.wasm`) is bundled into APK assets
  (`studio-web/`) and served through `WebViewAssetLoader` on a real
  `https://appassets.androidplatform.net` origin — the piece that makes
  IndexedDB, localStorage and WebAssembly work inside a WebView. Every
  byte loads from the APK; the app still has **no INTERNET permission**.
  The compiler inside NEXT on-device is `kinn.wasm` (the same
  `kinn-ffi` crate the `kinn` CLI is built from) — check, run, build
  web and build android all execute for real, offline.
- **Host bridge `window.kinnAndroid`** (`NextActivity` injects,
  `studio/src/host.ts` feature-detects, inert in every browser): SAF
  `saveFile` for exports (.kinnproj, web dist .zip, android project
  .zip — a WebView silently drops `<a download>` clicks) and
  `openClassicStudio()` which the NEXT "classic" links and palette use
  to open the native classic app. Import keeps the standard
  `<input type=file>` flow, completed by `onShowFileChooser`.
- **Classic Studio preserved**: the native 0.3 app (`MainActivity`,
  JNI compiler `libkinn_android.so`, docs, preview) stays bundled and
  reachable through the bridge; the web classic entry also ships in
  the bundle. Home-screen shortcuts pinned to the 0.3 launcher need
  one re-pin after updating.
- **Gradle**: `copyStudioWeb` bundles `studio/dist` and **fails the
  build** (with build instructions) when it is missing or incomplete —
  an APK with an empty WebView can never be produced. Version
  0.4.0 / versionCode 4; `androidx.webkit` added for the asset loader.
- **CI (`android.yml`)**: builds the web bundle (Node 22 +
  `npm run build:wasm` + `npm run build`) before Gradle, then verifies
  the APK itself — `assets/studio-web/next.html`, `kinn.wasm`,
  `lib/arm64-v8a/libkinn_android.so` present, launchable activity is
  `NextActivity`, versionName 0.4.0, no permissions — before uploading
  `kinn-studio-0.4.0-debug.apk` and the release APK (signed only when
  the `ANDROID_KEYSTORE_*` secrets exist; otherwise clearly UNSIGNED).

### Fixed — project persistence actually works now (web + Android)

Two silent IndexedDB bugs in `storage.ts` had cancelled each other out
since 0.2.0 — no project ever persisted across a reload:

- `saveProject` passed the key as the second `put()` argument on a
  keyPath (in-line keys) store — a spec `DataError` swallowed by
  `persistNow`'s catch, so **nothing was ever written**.
- The row stored the raw project object, but `loadProject` parses
  through `parseProject` (the versioned `.kinnproj` format) — with a
  row present, every load would have thrown inside the success handler.

Fixed together (rows now hold `serializeProject` output); pinned by a
new regression suite (`storage.idb.test.ts`) against a spec-fake
IndexedDB that reproduces both failure modes. Verified end-to-end:
create/edit/run a file, reload, the project (files, content, active
tab) is restored.

### Fixed — mobile polish and hardening

- The bottom tab row (TERMINAL/PROBLEMS/OUTPUT + counters + actions)
  overflowed 412px phone viewports by 24px; on ≤880px the redundant
  err/warn mini counters now hide (the PROBLEMS badge keeps the count)
  and the tab paddings shrink — zero horizontal overflow in every
  mobile view (verified at 412x915).
- Preview navigator regression test pins the injected selector to a
  valid `a[href]` (guards against silent corruption of the injected
  script string).

### Added — S4: AppModel lowering + android rejection (E3070)

- Language/pipeline: **`app` programs lower now**. `lower::app_model`
  turns the checked `app` declaration into the **AppModel**
  (`crates/kinc/src/appmodel.rs`) — the position-free,
  target-independent sibling of the site model (C.9), exposed as
  `Compiled.app` (the raw-AST exposure is gone; no AST crosses the
  pipeline boundary). The model carries:
  - **Component templates** with validated prop signatures (names,
    types, optional position-free defaults) and component state —
    templates, never runtime values; first declaration wins after
    E3065.
  - **The binding graph**: every dynamic view binding (owner page or
    component, element path, role) with its **static dependency set** —
    exact state-field names, computed-value references resolved
    transitively through the acyclic graph E3044 guarantees, `each`
    loop variables and component props/state shadowing respected
    (they are per-instance, not shared-state dependencies). Static
    literals are inlined into templates, not binding sites (C.6).
  - **Handler KIR function names** — deterministic
    `app:<owner>:<event>:<n>` (per-(owner, event) ordinal in the walk
    order) — with event association, key filters, source ordering and
    the validated bodies as position-free statements. Computed values
    likewise carry `app:computed:<name>`.
  - **Route table** (validated paths, targets, `:param` name lists),
    **animation table** (concrete integer milliseconds and easing
    shapes — E3050/E3049 ran; `cubic` keeps its four numbers),
    **store keys**, theme tokens, navigation mode, `not_found` and the
    app-level page transition.
  - Determinism is part of the contract and tested: `AppModel` derives
    `PartialEq`, re-lowering the same program yields an identical
    model (pipeline test included).
  - **Honest boundary:** handler/computed `FuncDef` emission waits for
    the runtime ABI — the manifest contract is S6's (C.2). The
    AppModel carries the KIR names, association, ordering and bodies;
    S5 lowers them into `FuncDef`s for the KIR→JS translation.
    Source lines are retained in the lowered bodies (the same data
    `FuncDef.lines` carries) because C.7 maps runtime errors back to
    `.kin` lines; columns and lengths are gone.
- Backend honesty: **E3070** — the android target rejects `app`
  programs ("the android target does not support `app` programs —
  dynamic applications are web-only in 0.4.0") before any generation
  runs. The gate lives in the backend (`backend::android_gate`, C.9:
  no model is fed to a backend that did not declare support) and is
  enforced by the CLI (`kinn build --target android` on an app exits 1
  with E3070 and generates nothing) and the FFI
  (`kinn_build_android`). Static `website` android builds are
  untouched (tested). The web target's honest "dynamic apps are not
  implemented yet" guard stays (S5+ lands the dist); console programs
  keep E3038.
- E3070 registered in LANGUAGE_SPEC §C2; the S4 shape and boundary are
  recorded in §C3 and DYNAMIC_WEB_ARCHITECTURE (Part D S4 + C.9).
- Tests: +18 (13 AppModel lowering tests — tables, exact dependency
  sets for direct/indirect/independent bindings, loop-variable and
  component-scope shadowing, handler association/ordering/naming,
  line-carrying bodies, animation values, component templates,
  determinism, the E3070 gate, cycles-never-reach-lowering; 2
  pipeline; 2 CLI; 1 FFI). Workspace **457 green**. The 0.3.0 web
  dist is byte-identical (rebuilt and diffed).

### Added — S3 slice 5: app semantics complete (P-030 closed to S4 work only)

- Language: **the remaining app semantic rules are implemented** —
  every code in the B.11 table is now live and frozen by tests:
  - **E3044** — computed dependency cycles, direct and indirect.
    DFS over the computed→computed reference graph (state-field
    leaves terminate, functions/consts are not edges); anchored on
    the identifier that closes the cycle, naming the whole path
    (`computed cycle: a → b → a`; a self-reference says "depends on
    itself"); each distinct cycle reports once, independent cycles
    all report, legal chains never do. Cycle members and everything
    downstream keep the `Type::Error` poison, so nothing cascades.
  - **Computed values now type in dependency order** (Kahn's
    algorithm) — the slice-3 placeholder (source-order publication,
    forward references poison silently) is replaced exactly as that
    ruling scheduled: a computed referencing a later one gets its
    target's real type, and mismatch diagnostics downstream of
    forward references fire with the frozen ordinary codes.
  - **E3053** — component recursion (direct: "instantiates itself";
    indirect: names the path `A → B → A`), anchored on the
    cycle-closing instantiation element; each distinct cycle once;
    legal repeated use of a component never reports.
  - **E3054** — the B.3 composition cap of 32, with pages as
    composition roots: view elements start at depth 1,
    `when`/`otherwise`/`each`/`form` bodies and instantiated
    component views (cycle-guarded) nest deeper; the first element
    past the cap is reported per page.
  - **E3047** — animation references resolve: `enter`/`exit`/
    `hover`/`press`/`scroll` on every element and the app-level
    page `transition` against one namespace (keyframes names +
    transition names); the hint lists the declared animations.
  - **E3048** — the keyframe property whitelist (opacity, x, y,
    scale, rotate, blur), anchored on the property name.
  - **E3049** — easing is one of the five named easings or
    `cubic(a, b, c, d)` with exactly four numeric literal arguments;
    the rule owns its position (`cubic` never cascades into E3002).
  - **E3050** — `duration`/`delay` must be integer literals in
    milliseconds (duration positive, delay non-negative): animations
    generate static CSS, so the values are compile-time data.
  - **E3059** — store keys must name declared app state fields
    (component state does not satisfy a key); anchored on the key
    identifier, hint lists the declared fields.
  - **E3061** — field kind/state type mismatch: `number` binds
    int/float state, text/date/email/password/textarea bind string
    state, arrays bind no kind; anchored on the kind word.
  - **W3042** — `each` without a `key` warns (reorderings reuse DOM
    by position; list rendering stays legal).
  - **W3043** — a `key` expression that never references the loop
    variable gives every row the same key by construction (literals
    and state reads alike) and warns; keys derived from the loop
    variable pass.
  - **W3044** — a state field that is written (assignment, compound
    or index-assign) but never read anywhere — no view binding,
    `computed` expression, handler read or two-way bind — warns;
    index-assign bases count as writes, never-written fields are
    not its subject; reported once per field, app and component
    state both covered.
  - **N3040** — the B.11 beginner hint ("event handlers run in the
    browser, state is local") fires once per handler-bearing app in
    mode B; mode A hides notes, mode S stays silent.
- Rulings recorded in LANGUAGE_SPEC §C2 (slice-5 block) + §C3:
  **E3060 never fires in the 0.4.0 state model** (B.4's
  scalars/arrays-of-scalars make every declared field persistable —
  it becomes checkable with the structs/records decision D2);
  duplicate animation names and duplicate store keys carry no code
  in B.11 and are not reported (first declaration wins, nothing
  invented); keyframe property values and `step` percentages are
  not semantically restricted (S8's CSS contract); the S6 boundary
  (event-builtin contexts, param/store context restriction) is
  unchanged; E3067–E3069 stay reserved.
- AST: store keys carry their identifier position (`StoreKey`),
  `field` elements carry the kind word's position (E3059/E3061
  anchors). `kinn ast` output unchanged.
- Tests: +28 (25 checker — positive + negative per code, positions
  and hints asserted; 3 CLI). Workspace **439 green**. The 0.3.0
  web dist is byte-identical (rebuilt and diffed); Studio unaffected.
  `examples/web/tasks_app.kin` status header updated — its only
  diagnostic is the expected N3040 note (mode B).

### Added — S3 slice 4: component instantiation validation, handler subset (P-030 narrowed)

- Language: **component instantiation sites validate now** — the last
  silently-unchecked view construct. New checker rules on the
  app's component declarations, at every instantiation position
  (page views, component views — nested component usage included —
  and `when`/`otherwise`/`each`/`form` bodies):
  - **E3040** (unknown component), anchored on the instantiation
    name. The hint lists the declared components in declaration
    order; a name that matches a declared *page* gets the namespace
    note ("'X' is a page; views instantiate components" — the mirror
    of E3063's component note; pages and components stay separate
    namespaces). The qualified form `alias.Name(...)` is always
    E3040 today — module files cannot declare components in 0.4.0 —
    and its hint says so honestly instead of pretending the import
    path exists. Argument expressions still resolve when the
    component is unknown (slice-3 behavior preserved: unknown names
    inside arguments keep reporting E3066, and E3040 never cascades
    into E3041s).
  - **E3041** (prop mismatch), four shapes: an unknown prop name
    (anchored on the prop name; the hint lists the component's props
    as required/optional, or teaches `Name()` for prop-less
    components); a prop passed twice (anchored on the second
    occurrence); a type mismatch against the prop's declared type
    (anchored on the value expression — exact types, no implicit
    conversion: int↔float reports E3041 at instantiation sites, and
    the frozen E3011 never fires there; a `Type::Error` argument
    suppresses only the comparison, so E3066 does not cascade); and
    a required prop (no default) that was not passed (anchored on
    the instantiation name, one diagnostic per missing prop).
    Omitting a prop **with** a default stays legal — `prop completed:
    bool = false` may be left out. After E3042 (duplicate prop) or
    E3065 (duplicate component) the signature keeps the first
    declaration and matching instantiations do not cascade — the
    established "invalid declarations are not further checked"
    pattern.
- Language: **the handler subset is enforced now** (B.6):
  - **E3045** — `return` is illegal inside event handlers (handlers
    are callbacks that return to the caller automatically). Fires
    for both `return;` and `return expr;`, anchored on the `return`
    keyword; the ordinary E3006/mismatch checks are skipped in
    handlers so one statement reports one error, but a value
    expression is still checked (unknown names inside it report
    E3066). `return` in ordinary functions — including functions
    called from handlers — is untouched.
  - **E3046** — one handler per event name on one element, anchored
    on the duplicate's `on` keyword. Slice-4 ruling: the comparison
    unit is the word after `on` (B.6: "one handler per event name"),
    so two `on key` handlers collide **even with different key
    filters** (the filter is runtime data) — the hint teaches the
    `event_key` merge — while `on key "Escape"` and `on keydown` are
    different event names and coexist. Distinct events on one
    element remain legal; three same-name handlers report two
    E3046s; handler blocks on component instantiations are elements
    like any other; a duplicate's body is still checked (errors
    inside it are not hidden).
  - **W3040** — `while` inside an event handler warns ("a long
    `while` loop in an event handler freezes the page; prefer `for`
    over a range") but stays legal — a warning, never a fatal
    error; a handler whose only diagnostic is W3040 checks OK.
    Fires once per `while` statement, nested control flow included
    (`while` under `if`/blocks inside the handler warns too); `for`
    never warns; ordinary functions — including ones called from
    handlers — never see W3040.
- Example: the error corpus gains
  `examples/errors/app_unknown_component.kin` (`// error: E3040`),
  `app_prop_mismatch.kin` (`// error: E3041`),
  `app_return_in_handler.kin` (`// error: E3045`) and
  `app_duplicate_handler.kin` (`// error: E3046`); every file is
  verified by the pipeline corpus test. `examples/web/tasks_app.kin`'s
  status comment states the new checked/unchecked boundary exactly
  (it still checks clean end-to-end — its `TaskRow(label: item)`
  instantiation validates, `done` defaults).
- Docs: DYNAMIC_WEB_ARCHITECTURE B.3/B.6 mark the implemented rules
  and record the E3046 event-name ruling; B.11's E3040/E3041/E3045/
  E3046/W3040 rows carry their implementation status;
  LANGUAGE_SPEC §C2 gains the slice-4 rulings and §C3 the new
  boundary; ROADMAP and AI_CONTEXT updated; PROBLEM_REGISTRY P-030
  narrowed to what remains (computed cycles E3044, component
  recursion/depth E3053/E3054, stores/fields/animations
  E3047–E3061, W3042–W3044, N3040).
- Tests: **+14** (11 checker + 3 CLI; workspace 397 → 411, all
  green); every new code has positive and negative tests with
  position (line/col/len) and hint asserted — the task's four
  negative examples (unknown component, unknown prop, missing
  required prop, mistyped prop) and its three handler examples
  (`return`, duplicate `on click`, `while` warning) are each
  covered by name. Studio 66/66 on a freshly built wasm (the wasm
  contains the new rules); the 0.3.0 web dist is byte-identical.

### Added — S3 slice 3: app scope resolution, binding typing, param/store builtins (P-030 narrowed)

- Language: **names resolve inside apps now**. New checker code
  **E3066** (assigned from the reserved E3066–E3069 span —
  **E3067–E3069 stay reserved**): an unknown name in the app scope —
  a read in a binding expression or handler statement, a call
  target, an assignment target naming nothing, or an `input bind`/
  `field` target naming nothing — reports the **app-scoped sibling
  of E3001** (which stays frozen for ordinary function bodies,
  ADR-0011), anchored on the identifier, with a hint listing what the
  current scope declares (app state, computed values; props and
  component state first inside components).
- Resolution order, innermost first: handler local → event builtins →
  `each` loop variable (views) → component props/state (components) →
  shared state → computed values → consts → functions and natives →
  **registered app builtins**. User definitions shadow builtins (a
  fallback table consulted last — no redefinition guard); a component
  prop/state legally shadows app state (E3042 rejects duplicates only
  within one scope).
- **E3043 fires for the first time**: assigning to a prop, computed
  value or event builtin in a handler, and `field`/`input bind`
  targets that are not state fields (two-way binding writes —
  computed values, props and consts rejected). **B.4 erratum**:
  assigning to `fin` locals and `const`s keeps the frozen **E3004**;
  `+=` on a same-typed array state field appends (app scope only,
  per B.7's example — execution lands with S5).
- **E3058** (implemented): `param("…")` with a literal argument must
  name a `:param` of a valid route; the diagnostic anchors on the
  quoted argument and the hint lists the declared parameters.
  Non-literals are runtime data and pass.
- **W3041**: a handler local shadowing an app state field or
  component state field warns (legal, but visible).
- Binding typing: text/heading/title/labels/alt/legends must be
  strings, `when` conditions bool, `each` iterables arrays, field
  attributes typed (`label`/`placeholder`/`pattern`: string;
  `min_length`/`max_length`/`min_value`/`max_value`: int) — the
  ordinary **E3003/E3011** mismatch codes (E3011 keeps its int↔float
  message; `when` never borrows E3005). State-field and prop
  initializers type-check, empty array literals against a declared
  array type included.
- **A2 ruling (param/store builtins)**: `param(string) -> string`,
  `store_save`/`store_load`/`store_clear` `(string) -> void`,
  `json_of(string) -> string`, `save_json(string, string) -> bool`,
  the B.10 helpers (`str` — mirrors the `as string` cast surface —
  `upper`, `lower`, `trim`, `contains`, `split`, `join`) and the
  handler event variables (`event_value`, `event_key`,
  `event_x`/`event_y`, `form_valid`, `field_errors`) are
  **registered by name and type so the showcase compiles**.
  **Known limit: param/store names recognized; context-restricted
  invocation enforced at S6** — and the B.10 helpers' VM/JS natives
  arrive with S5 (app programs cannot build or run before S4+).
- Example: `examples/errors/app_unknown_name.kin` (`// error: E3066`)
  joins the error corpus; `examples/web/tasks_app.kin` declares the
  missing `due: string` state key (its date field binds it — the
  audit's showcase fix) and its status comment now states the
  checked/unchecked boundary exactly.
- Docs: **B.3/B.4/B.5/B.6/B.10 errata** in
  DYNAMIC_WEB_ARCHITECTURE.md — the B.3 example toggled a **prop**
  while B.4 forbids it (it now toggles the component state);
  B.4's capitalized types and its "consts are E3043" claim (E3004 is
  frozen); the full resolution order; B.6's `while`-in-handler
  warning is **W3040**, not W3044, per B.11's table; B.6/B.10
  builtin types are the real lowercase ones; B.10's `str()` is
  checker-registered in app scope (VM/JS at S5); stale store
  cross-references in B.2/B.7 now point at B.7. B.11's E3066 row is
  assigned. LANGUAGE_SPEC §C2 gains E3066/E3058/E3043/W3041 and the
  full slice-3 rulings; §C3 states the new checked/unchecked
  boundary.
- Registry: **P-030 narrowed** to the still-unchecked semantics
  (handler subset, prop signatures at instantiation sites, computed
  cycles, stores/fields/animations).
- Tests: **+17** (15 checker + 2 CLI; workspace 380 → 397, all
  green); every new code has a positive and a negative test with
  position and hint asserted; `examples/web/tasks_app.kin` checks
  clean end-to-end (0 diagnostics).

### Changed — BREAKING: binding keyword `let` renamed to `fin` (ADR-0029)

- Language: **the immutable local-binding keyword is now `fin`**
  (a final value); `var` (mutable) and `const` (top-level
  compile-time constant) are unchanged. Semantics are identical to
  `let`'s — only the keyword changes. **This is intentionally
  breaking for `let`-sources: there is no backward compatibility for
  the `let` keyword** beyond the migration diagnostic below; ADR-0029
  supersedes the A2 source-compatibility clause for this keyword
  only (everything else — including 0.3 `website` programs and their
  byte-identical web dist — keeps compiling unchanged).
- Migration: the word `let` is no longer a keyword; a
  statement-position `let x = ...;` produces the new **E2017**,
  "`let` was renamed to `fin`; use `fin`" — positioned on the `let`,
  after which the declaration parses as `fin`, so every legacy use
  reports exactly once with no error cascade. Outside the declaration
  pattern `let` remains a legal identifier (`fin let = 1;` compiles).
  Handler bodies are covered (B.6: ordinary statement parser).
- Surface: `kinn tokens` prints `FIN`; `kinn ast` prints
  `fin x = 1;`; E3004 now says "(declared with `fin`)"; the B-mode
  lint N3001 suggests `fin`; the E3022 example shows `fin a: [int]
  = [];`. Codegen/KIR/VM untouched — the 0.3 web dist and DEB stdout
  goldens are byte-identical (rebuilt and diffed).
- E2017 was the first free parser code (E2001–E2015/E2099 = 0.1/0.2,
  E2016/E2030 = 0.3, E2031–E2039 = app grammar); registered in
  LANGUAGE_SPEC §C2 + §C1a and frozen by tests.
- Repo migrated: all `.kin` examples, every compiler test source,
  Studio web (syntax highlighting, cheat sheet, samples, quick-check
  tests, real-wasm tests), Studio Android (highlighting keyword table,
  cheat sheet). Error corpus gains `examples/errors/legacy_let.kin`
  (`// error: E2017`). Tests: +10 (workspace 370 → 380, all green).

### Added — S3 slice 2: app page targets resolved, page/component name rules (P-029)

- Language: **app page targets are resolved now**. New checker codes
  per the architect ruling of 2026-11-22, using the free
  E3063–E3069 span (**E3066–E3069 stay reserved**):
  **E3063** "unknown app page target" — fires at **every page-target
  position** the app grammar admits: `route "path" -> Page`,
  `not_found Page`, and `open <Page>` on buttons and links anywhere
  in the app's views (pages and components; `when`/`otherwise`,
  `each` and `form` bodies included). It is the app-scoped sibling
  of E3032 — E3032 is website-scoped and frozen (ADR-0011), never
  reused. The diagnostic anchors on the target identifier; the hint
  lists the declared pages, and a target that names a **component**
  says "'X' is a component; routes target pages". A MPA `not_found`
  rejected by E3057 is not further resolved (the established
  "invalid declarations are not further checked" pattern).
  **E3064** duplicate page name inside an app — **case-insensitive**
  (mirrors E3031: page slugs are lowercased by the backends).
  **E3065** duplicate component name inside an app — **case-sensitive**
  (identifier semantics, matching function names). **Pages and
  components are separate namespaces**: a page and a component may
  share a name — use-site position disambiguates (`<Card>(...)`
  instantiates the component, `open Card` / `route -> Card` targets
  the page). `input bind`/`field bind` targets are state keys, not
  page targets — untouched (scope resolution, slice 3).
- Docs: B.11 table gains E3063/E3064/E3065 (+ explicit E3066–E3069
  reservations); LANGUAGE_SPEC §C2 registers all three codes with the
  implementation rulings and §C3 updates the honest limits — **app
  page targets ARE now resolved; scope/handler/prop/store/field/
  animation semantics remain unchecked (slice 3+)**, now tracked as
  **P-030** in the registry.
- Registry: **P-029 closed as F-018**; P-030 registered for the
  remaining unchecked app semantics.
- Example: `examples/errors/app_unknown_page.kin` (`// error: E3063`)
  joins the error corpus; `examples/web/tasks_app.kin` status comment
  updated to the slice 2 checking surface.
- Tests: +12 (workspace 358 → 370, all green): 10 checker (every code
  positive + negative, positions and hints asserted; all four
  page-target positions; nested-view collection; E3032-never-reused;
  namespace-separation; MPA-not_found-not-resolved; case-sensitivity
  matrix) and 2 CLI integration (E3063 with the component-target hint,
  E3064/E3065 messages through `kinn check`).

### Fixed — P-028: website diagnostics carried the wrong filename in multi-file programs

- Checker: in a website program **with imports**, every diagnostic
  emitted inside `check_website` (E3030–E3036 including E3032's
  explicit file field, W3035–W3037) was attributed to the **last
  project file** — the pass-0 loop leaves `c.file` at the last unit
  before `check_website` runs, so an entry-file E3032 ("opens page …
  which is not declared") rendered under `lib.kin`. The checker now
  restores the entry path before `check_website`, mirroring the app
  path that S3 slice 1 already had. Single-file programs are
  unchanged (the entry path is the only path). **Gate authorization**
  (architect ruling, 2026-11-22): the "website checking untouched"
  gate means no regressions, not no bug fixes.
- Tests: +1 multi-file regression
  (`website_diagnostics_are_attributed_to_the_entry_file` — E3032
  asserts file/line/col/len, W3035 asserts file/line). No existing
  test froze the buggy attribution: the only other multi-file website
  test asserts E3037, which is emitted inside the pass-0 loop itself
  with correct per-unit attribution.
- Registry: P-028 closed as F-017. 0.3.0 web dist rebuilt
  **byte-identical**; workspace 358 tests green.

### Added — S2: the `app` declaration — grammar, AST and honest guards

- Language: the full dynamic-app grammar of
  DYNAMIC_WEB_ARCHITECTURE B.2–B.9 parses: `app "Name" { ... }` with
  `navigation mpa|spa`, `not_found`, `theme` (ADR-0027 token
  whitelist), `state` blocks, `computed`, `animation` (keyframes with
  `from`/`to`/`step`, named transitions, duration/delay/easing incl.
  `cubic(...)`), `store`, `route "path/:id" -> Page`, app-level
  `transition`, `page` views and `component` declarations
  (`prop`/`state`/`view`). Views accept binding **expressions**
  wherever 0.3 elements take strings, plus `when`/`otherwise`,
  keyed `each`, `form`/`field` (6 kinds, 8 validation attributes),
  `input bind`, `on <event>` handler blocks (10 events + `on key "..."`
  sugar) and component instantiation with named props
  (`TaskRow(label: x)`, qualified `ui.Card(...)`).
- Parser: new codes **E2031–E2039** (unknown event, malformed
  each/when/instantiation, non-expression view argument, theme/field
  whitelists, animation malformations, misplaced handler blocks) — all
  positioned, all frozen by tests. **`each` vs casts**: a bare `as`
  followed by an identifier is always the loop-variable marker (cast
  types are keywords), so `each items as item` parses and casts in
  iterables need parentheses. Brace-aware recovery: a broken item
  inside a section/view/flat block never truncates its parent
  (regression-tested). Handler bodies are **official Kin statements**
  (semicolons, `if` not `when` — B.5/B.6 normative; the B.7 example's
  `when form_valid` was a doc slip, fixed).
- Checker: presence rules **E3051** (app outside the entry file) and
  **E3052** (website + app, or two apps) — pulled forward from S3 so
  an app half is never silently ignored by a website build; app
  programs do not need `main` (B.2). App internals are **not yet
  checked** (S3).
- CLI: `kinn ast` dumps the whole app tree; `kinn check` passes
  grammar + presence; `kinn run`/`kinn build` on app programs exit 1
  with honest guards ("building dynamic apps is not implemented yet")
  instead of pretending. `Compiled` carries the parsed `AppDecl`
  (lowering to an AppModel arrives with S4).
- Example: `examples/web/tasks_app.kin` — the target-shape app
  exercising every grammar construct (parses; documents its own
  intermediate status).
- Docs: LANGUAGE_SPEC §C3 (normative grammar + rules), ROADMAP S2
  checked, DYNAMIC_WEB_ARCHITECTURE Part E approval annotated.
- Tests: +54 (workspace 281 → 335, all green): 44 parser (incl. the
  70-word stays-identifier regression gate), 5 checker presence, 5
  CLI integration. 0.3.0 web dist rebuilt byte-identical; Studio
  66/66 against a freshly built wasm.

### Added — S3 slice 1: app declaration validation

- Checker: the registered B.11 meanings for app declarations, per the
  S3 plan in 2cb7ebe — **E3062** (empty/whitespace-only app name; trim
  check, positioned on the app declaration, hint included; mirrors
  E3033 exactly, including that an unnamed app is not further checked
  and E3008 pairs with it as it does for websites — E3033 was NOT
  reused, its website meaning is frozen), **E3055** (malformed route
  path: relative, non-empty, no empty segments; literal segments
  `A-Za-z0-9._-` — `.`/`..` rejected because MPA generates one file
  per route and they would escape the output directory; `:name`
  params are unique identifiers; anchored on the path string, quote
  to quote), **E3056** (duplicate route by **match shape** —
  `item/:id` vs `item/:name` match the same addresses and no router
  could tell them apart; the diagnostic names the first route), and
  **E3057** (`navigation spa` without `not_found` → error on the app
  declaration; `not_found` in MPA, default included → error on the
  target). **E3042** (duplicate names in one scope: app scope =
  state fields ∪ computed — state blocks merge; component scope =
  props ∪ state fields; cross-kind collisions count, the first
  declaration's kind and line are named). All five frozen by tests and
  registered in LANGUAGE_SPEC §C2 with the implementation-time
  rulings.
- Parser/AST (additive): `RouteDecl` records `path_line`/`path_col`
  (the path string token) so route diagnostics anchor precisely; the
  `kinn ast` dump output is unchanged.
- Checker hygiene: app diagnostics are attributed to the entry file
  (the checker restores the path after the pass-0b loop). The website
  checker has a pre-existing attribution gap in the same spot —
  registered as **P-028**, untouched in this slice per the
  website-untouched gate.
- Out of scope, honestly (later S3 slices): scope resolution, handler
  subset enforcement, prop checking, computed cycles, store/field/
  animation semantics, and route/`not_found` **target** resolution —
  no code is registered for unknown app pages (**P-029**), surfaced
  for assignment; duplicate page/component names inside an app are
  likewise unregistered (P-029).
- Example: `examples/errors/app_bad_route.kin` (`// error: E3055`)
  joins the error corpus; `examples/web/tasks_app.kin` status comment
  updated to the slice 1 checking surface.
- Tests: +21 (workspace 336 → 357, all green): 18 checker (every code
  positive + negative, positions, hints, cross-kind and cross-scope
  cases, multi-error accumulation, entry-file attribution) and 3 CLI
  integration (E3062 hint/position, the three route codes together,
  E3042 first-declaration reference). Gates re-verified: all 336
  pre-existing tests green; website checking untouched; 0.3.0 web
  dist **byte-identical** to the 77c3e6d baseline (diff -r); Studio
  **66/66** against a freshly built wasm.

### Fixed — S2 review (architect gate items)

- B.0 gate completed: the stays-identifier regression loop now freezes
  **79 words**. The S2 array held 67 (the "70-word" figure in the S2
  entry above was a miscount) and missed the six new `field kind`
  values (`text`, `number`, `date`, `email`, `password`, `textarea` —
  `text` is also a theme token) plus the six 0.3 view-element words
  the app grammar now shares (`title`, `heading`, `divider`,
  `button`, `link`, `image`). All 79 pass as names (loop test).
- `x as some_ident` outside `each` is still E2002, positioned on the
  `as`, and now carries a hint: `as <name>` is only the loop variable
  of `each <list> as <name>`; outside `each`, `as` is the cast
  operator and needs a type keyword (like `x as int`). Pinned by a
  new parser test (position, hint, the good cast spelling as
  contrast, and the short hint when no identifier follows).
- B.4 wording corrected per the B.3 ruling (errata item 6).
- CI repairs found by the first S1+S2 push (F-016): linux.yml's DEB
  smoke test golden updated to the five-line modules example output
  (S1 added the const-export line `95` but the workflow was missed);
  kinn-ffi's module-registry tests now hold a test-only lock — a
  parallel `kinn_clear_modules()` used to be able to wipe a module
  another test had just registered (macOS flake).

### Errata — S2 deviations from the approved architecture doc

Recorded in one place per the S2 review. All six are documentation
corrections or additive pull-forwards — the implemented grammar and
the approved grammar never conflicted:

1. B.7's handler example used `when form_valid` — handler bodies are
   official Kin statements, so conditionals are `if` (B.5/B.6
   normative). Architecture doc corrected in S2.
2. B.3's example types `Str`/`Bool` were informal notation — the
   standard Kin type keywords are `string`/`bool`. Doc corrected in
   S2.
3. B.3's `module::TaskRow` spelling was informal — the established
   dot form is `module.TaskRow(...)`. Doc corrected in S2.
4. Checker presence rules E3051 (app outside the entry file) and
   E3052 (website + app, or two apps) were pulled forward from S3
   into S2 so an app half is never silently ignored by a website
   build.
5. The B.0 word list in the architecture doc (45 words) is a subset
   of the implemented contextual vocabulary; the S2 regression gate
   froze 67 words (adding the event words, theme tokens, field
   attributes and store words) and the S2 commit message miscounted
   them as "70". Now 79 (see Fixed above).
6. B.4 said component state lives in the component "state block";
   the B.3 ruling is normative — component state is a per-line
   declaration exactly like `prop`, and only app-level state is a
   `{ ... }` block. B.4 wording corrected in this release.

### Errata — the first-push claim in the F-016 CI fix (77c3e6d)

The F-016 fix stated "the S1/S2 commits had never run on GitHub (they
were pushed together with the S2 review)". Both halves are wrong;
re-verified against the GitHub Actions API on 2026-11-22:

- S1 (6905f74) was pushed **alone** at 2026-11-21T21:49:13Z and four
  workflows ran on it: ci success (run 44953504346), windows success
  (44953504318), android success (44953504410) and linux **failure**
  (run 44953504257) — the DEB smoke-test golden S1 had made stale.
- S2 (699c43d, 23:09:47Z), the S2 review (711cb6e, 23:17:43Z) and the
  S3 plan (2cb7ebe, 23:18:49Z) were pushed separately; linux stayed red
  on all three (runs 44960409403, 44961075439, 44961164227). The DEB
  golden was red from 21:49:13Z to the fix push at 23:24:45Z (~1h35m),
  unnoticed across four pushes.
- The macOS race is intermittent and pre-existing — macOS passed on
  6905f74/699c43d/711cb6e and **first failed on 2cb7ebe** (ci run
  44961164278, job `rust tests (macos-latest)`).
- 77c3e6d is the first all-green push (linux 44961665490, android
  44961665523, windows 44961665547, ci 44961665552).

The commit message is immutable history; PROBLEM_REGISTRY F-016 and
this entry are the correction of record. Two standing rules were added
to AI_CONTEXT so the miss cannot recur: a push is not done until every
triggered workflow has been watched to green (run IDs reported with the
final SHA), and any commit that changes observable behavior updates all
golden assertions in the same commit, including the packaging smoke
tests.

### Decisions — Part E follow-up: D4 and D5 owner-confirmed (2026-11-22)

D1–D3 and D6 stand as previously recorded (owner directive plus the
silence clause). The owner explicitly confirmed the remaining two:

- **D4 — YES**: `kinn serve` (localhost dev server with SPA fallback)
  is in 0.4.0, landing in S11 per the roadmap.
- **D5 — YES**: Playwright (headless Chromium) joins CI as a dev
  dependency — CI only; the compiler core and kinnrt.js remain
  dependency-free. Proposed landing: wired with S6 (the first slice
  with a real app to drive), mandatory at the S11 browser gate (D.2);
  if runner flakiness would break green steps it parks as a manual log
  until S11.

Neither is implemented yet — both are recorded decisions only.

### Added — S1: cross-module value exports (`const`)

- Language: `const NAME: Type = expr;` top-level declarations with a
  mandatory type annotation (ADR-0028; normative rules in
  LANGUAGE_SPEC §C1). `const` is a contextual keyword (ADR-0019
  recipe) with the word-stays-identifier regression gate; imports must
  still precede all functions and consts (E2012 wording updated).
- Semantics: initializers are compile-time constant expressions
  (literals, array literals, operators, earlier same-file consts);
  calls, casts, indexing and qualified reads are rejected (new code
  **E3039**). Constant arithmetic follows VM rules — overflow and
  division/modulo by zero are compile-time errors. Assigning to a
  const is E3004; locals may shadow consts; void consts are E3015;
  builtin-name collisions are E3016; duplicate definitions are E3002.
- Modules export consts implicitly: declaring files read them bare,
  importers read them qualified (`m.NAME`; KIR name `m::NAME`).
  Unknown members E3020/E3021 now distinguish functions and consts
  with hints both ways; undefined variables hint at module consts.
- Compiler: new `kinc::consteval` evaluator (checker validation +
  lowering folding share one pure function); `Module` gained
  `consts: Vec<(String, Value)>` (qualified names, deterministic
  order) — **no new KIR instructions**; every const read compiles to
  `Const(value)`; `kinn kir` prints the folded constants
  (`const math::MAX = 100`).
- Examples: `examples/modules/` now demonstrates shared configuration
  values across files (`math.MAX`, entry-local `BASE`).
- Tests: +31 (workspace 250 → 281, all green): parser (contextual
  word gate, annotation error, recovery, ModConst), checker (15 tests:
  duplicates, collisions, non-constant initializers, type mismatches,
  void, arithmetic errors, assignment, shadowing, qualified reads and
  hints), pipeline (folding, qualified reads through calls,
  shared-singleton const arrays per ADR-0014, determinism, website
  compatibility), CLI (multi-file run + `kir` shows folded consts),
  kinrt listing, consteval unit tests.
- Compatibility: 0.3.0 website programs are unaffected —
  `examples/web/site.kin` dist rebuilt byte-identical (determinism
  re-verified); all 250 pre-existing tests pass unchanged (one parser
  test updated for the new `module.NAME` grammar, see S1 notes).

### Added — documentation & decisions

- `docs/DYNAMIC_WEB_ARCHITECTURE.md`: Phase 1 technical audit
  (re-verified on `c84a8f0`: 250 Rust tests green, real dist inspected),
  Phase 2 official `app` language module design (components, reactive
  state, binding expressions, events in official grammar, forms,
  animations, MPA/SPA routing, local persistence with normative honesty
  clause), Phase 3 KINN Web Runtime architecture (dependency-free
  ES2020, statically compiled binding graph, KIR→JS with differential
  testing, ≤ 16 KB budget), Phase 4 roadmap S1–S11 with tests and
  acceptance criteria, Phase 5 approval gate (decision points D1–D6).
- ADR-0023 (KINN Web Runtime; supersedes ADR-0020's no-runtime clause
  for `app` programs only), ADR-0024 (`app` module: components, state,
  events, forms), ADR-0025 (MPA default / SPA opt-in, honest `file://`
  behavior), ADR-0026 (local persistence + JSON, local-only), ADR-0027
  (declarative animation whitelist + theme tokens).
- ROADMAP re-scoped: 0.4.0 = Dynamic web applications (S1–S11);
  "breadth and developer comfort" items moved to 0.6.0 (D1).
- PROBLEM_REGISTRY: P-018–P-023 table formatting fixed; P-024
  (packaging CI still emits 0.2.0 names), P-025 (doc drift),
  P-026 (flat Studio VFS blocks 0.4 folders), P-027 (no runtime error/
  console forwarding in preview; E3038 CLI/FFI asymmetry) added.

## [0.3.0] — 2026-11-21

Multi-target delivery: the first complete backend vertical slice (web),
the Android application backend, Studio build integration on both
platforms, and the package/module foundation. The compiler core still
has **zero external dependencies**. 0.2.0 functionality is preserved
(backwards-compatibility is tested, including contextual-keyword
behavior).

### Added — targets & backends (ADR-0018)

- **Target/backend architecture**: `kinc::backend` with a stable
  contract — target identification (`kinn targets`), pure artifact
  generation (`OutputFile`), capability tables, and diagnostics before
  generation. Backends consume the validated `SiteModel`, never the
  AST.
- **Web backend (complete vertical slice)**: `kinn build --target web`
  generates a real distribution — semantic HTML5 (one file per page,
  `index.html` for the first page), one shared stylesheet, focus
  management script, copied `assets/`. Accessibility enforced at
  compile time and generated in (skip link, landmarks, aria-current,
  alt text, focus-visible, reduced-motion). JS-free by design
  (ADR-0020): state/forms/events do not exist in 0.3 and are not
  faked. Verified in a real browser (accessibility tree + navigation).
- **Android backend (project export + remote APK)**:
  `kinn build --target android` exports a complete Gradle project
  (AGP 8.5.2, Kotlin 1.9.24) whose `MainActivity.kt` renders the site
  with native Android views — no WebView, no AndroidX, no permissions.
  Build providers: local Gradle/SDK (honest degradation), remote CI
  (real APK built + `apksigner verify` + artifact), export (always).
  ADR-0021.

### Added — language (Kin 0.3, ADR-0019)

- **Declarative `website` module**: `website "Name" { page Home { … } }`
  with `title`, `heading`, `text`, `button "…" open Page`, `link "…"
  open Page`, `image "src" alt "…"`, `divider`. Contextual keywords —
  no existing identifier breaks (tested). Websites live in the entry
  file; site-only programs need no `main` (run reports guidance).
- **New diagnostics** (normative in LANGUAGE_SPEC §B2): E2016, E2030,
  E3030–E3038, W3035–W3037.

### Added — CLI

- `kinn build [file] [--target web|android] [--out DIR]` with per-file
  build report and honest statuses (`project exported` vs `debug APK`).
- `kinn targets`; `kinn.toml` manifest support (name/version/entry/
  target) with a strict minimal parser (ADR-0022).

### Added — Studio (web + Android)

- Web Studio: **Build Web** button, `build [target]` and `preview`
  terminal commands, preview panel (page selector, iframe preview with
  inlined CSS/JS and intercepted navigation), **Download .zip** of the
  real dist, **Export Android** project zip. Store-only ZIP writer
  (crc32 vector-tested; validated with `unzip`).
- Android Studio: `build web` / `build android` / `preview` /
  `export android`; **PreviewActivity** renders the generated site
  offline via file:// with in-app page navigation; Build Web button.

### Added — ecosystem foundation

- `docs/MODULE_SYSTEM.md`: module/package/registry/SDK layering; what
  exists vs what is deliberately deferred. `kinn.toml` is the first
  working piece; no registry, no lockfile, no downloads (none faked).

### Changed

- Versions across compiler, CLI, FFI, Studio and the Android app to
  0.3.0 (FFI `kinn_version` = 0x00_03_00_00).
- CI: target smoke tests (web dist + android export) on all three
  platforms; a new `backend-apk` job builds a real APK from the
  exported Kin project on a GitHub runner with signature verification
  and zero-permission check; Studio test count 51 → 66.

### Tests

- Rust: 190 → **250** tests (+12 web backend, +9 android backend,
  +13 parser, +12 checker, +4 FFI build, +9 CLI build, +5 manifest,
  +1 provider status; version test updated).
- Studio: 51 → **66** vitest tests (preview builder, zip writer, wasm
  build-web/build-android against the real backend).
- Browser verification of the generated dist (rendering, a11y tree,
  navigation) performed with headless Chromium.

## [0.2.0] — 2026-11-21

Language breadth, Studio integration, the Android app and the release
pipeline. The compiler core still has **zero external dependencies**.

### Added — language (Kin 0.2)

- **Modules / multi-file projects**: `import "lib.kin" as m;` with
  qualified calls `m.add(1, 2)`. Paths resolve relative to the importing
  file, cannot escape the entry directory (E3017), cycles are rejected
  with the full chain (E3018), duplicate aliases rejected (E3019), and a
  file imported twice is parsed once. Imported `main` is inert; only the
  entry's `main` runs.
- **Arrays**: `[1, 2, 3]` literals, `[T]` types, indexing `a[i]`, element
  assignment `a[i] = v`, nesting, `len(a)`. Reference semantics
  (assignment shares the array) with deep structural equality; bounds
  checks at runtime (E4005). Empty literals need a type annotation
  (`let a: [int] = [];`, E3022).
- **`for` loops**: `for i in 0..10` (exclusive), `for i in 0..=9`
  (inclusive), `for x in array`. `break`/`continue` work; `continue`
  jumps to the increment; the loop variable is a fresh immutable binding
  each iteration.
- **Explicit casts**: `x as float`, `x as int` (truncating toward zero,
  saturating), `x as string` for int/float/bool. Unsupported casts
  rejected (E3025).
- **Native stdlib foundations**: `len` (arrays and strings — string
  length counts characters), `abs`, `min`, `max`; reserved against
  redefinition (E3016).
- **Parser error recovery**: multiple syntax errors per file with
  statement-level synchronization and a cap (E2099); recovery tested to
  not cascade.

### Changed — compiler

- Control-flow analysis: `while true` without a reachable `break` is
  recognized as diverging — no more false E3012 for
  `while true { return x; }`; code after it is unreachable (W3001).
- KIR local slots are reused after scope exit (free-list allocator):
  smaller frames, same semantics.
- `Type` gained `Array(Box<Type>)`; diagnostics carry the source file
  (`render_project` renders multi-file diagnostics grouped by file).
- `kinc::project` (new module) implements loading, path validation and
  cycle detection; `driver::compile_project` takes a source loader used
  identically by the CLI, Studio (wasm) and Android (JNI).
- CLI 0.2.0: `run`/`check`/`kir` load multi-file projects; `kir` shows
  qualified names (`math::add`).

### Added — kinn-ffi / kinn-android (Rust)

- `kinn-ffi`: C-ABI bridge compiling the same pipeline to
  `wasm32-unknown-unknown` and native `.so`. Exports `kinn_check` /
  `kinn_run` / `kinn_check_text` (JSON results with diagnostics,
  stdout, runtime errors), `kinn_set_module`/`kinn_clear_modules` for
  multi-file projects, `kinn_alloc`/`kinn_free` memory protocol.
  Panic-safe (E5000). 12 tests.
- `kinn-android`: JNI glue for `dev.kinn.studio.KinnBridge`
  (arm64-v8a). The only crate allowed an external dependency (`jni`
  0.21); the compiler core stays zero-dep (ADR-0014).

### Added — KINN Studio (web) 0.2.0

- **Real in-Studio check and run**: the Rust compiler+VM compiled to
  `public/kinn.wasm` (via `kinn-ffi`), running multi-file projects in
  the browser, fully offline. Graceful, honest fallback with build
  instructions when the wasm is not built.
- **Persistence**: projects survive reloads via IndexedDB (debounced
  autosave); explicit `save` command.
- **Import/export**: `.kinnproj` JSON project format (versioned,
  validated) for download/upload; single `.kin` file import.
- Terminal: real `check`/`run`, `rm`, `mv`, `save`, `export`, `import`;
  Kin 0.2 cheat sheet; backend status badge in the top bar.
- Editor lexer mirrors the 0.2 token set (ranges `..`/`..=`, brackets,
  `for`/`in`/`as`/`import`).
- Tests: 22 → 51, ten of them executing the real compiler in
  WebAssembly (skipped loudly, not silently, when `kinn.wasm` is
  missing).

### Added — KINN Studio for Android

- First real Android app (Kotlin, `android/`): multi-project explorer
  with persistent local storage, editor (line numbers, Kin syntax
  highlighting), problems panel (click to jump), terminal with 19 real
  commands, offline bundled documentation, SAF import/export.
- **On-device compilation on ARM64**: check/run call the real compiler
  through JNI (`libkinn_android.so`); when the library is missing the
  app degrades honestly and says so. No permissions — no INTERNET.
- Built and verified locally: debug APK 6.6 MB bundling the 933 KB
  native compiler and the docs; signature verified with apksigner.

### Added — CI and release pipeline

- `ci.yml`: Rust tests on Ubuntu/Windows/macOS + release CLI smoke test
  on each platform; Studio tests with the wasm backend; Studio site
  artifact.
- `android.yml`: NDK + cargo-ndk build of `libkinn_android.so`,
  assembleDebug/assembleRelease; **release signing only from encrypted
  `ANDROID_KEYSTORE_*` secrets** (unsigned + clearly labeled otherwise —
  never a fake identity); apksigner verification and SHA-256 checksums
  in the `signature-report.txt` artifact.
- `windows.yml`: tests + release build on a Windows runner; portable
  zip and NSIS installer.
- `linux.yml`: tests + release build on Linux; DEB (installed and
  exercised on the runner), AppImage via linuxdeploy (experimental),
  tarball.

### Added — documentation

- `AUDIT_REPORT.md` (verified / partial / planned / broken per area),
  `INSTALL.md` (per-platform installation).

### Known limitations

See [PROBLEM_REGISTRY.md](docs/PROBLEM_REGISTRY.md): P-001/P-002/P-003
and P-005–P-008 are resolved in 0.2.0; string interpolation was decided
against for 0.2 (ADR-0013, revisit in 0.3); the Android app is a v1
shell (no `tokens` command, single-color terminal lines before colors
landed — see the registry for the current list).

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
