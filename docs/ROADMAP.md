# Roadmap

Milestones are verifiable: each one closes with everything building,
all tests green, docs updated, and a tagged summary of what is real.
Dates are not promised — scope and honesty are.

## 0.1.0 — Bootstrap (DONE, 2026-11-21)

- [x] Repository structure, examples as executable specification
- [x] Rust workspace, zero dependencies: kinrt (KIR + VM), kinc
      (lexer/parser/checker/lowering), kinn CLI
- [x] Kin 0.1 subset with modes B/S/A (directive + diagnostics
      differentiation)
- [x] 90 compiler tests + 22 Studio tests
- [x] KINN Studio web shell (explorer, editor, terminal, problems)
- [x] Full documentation set + AI_CONTEXT

## 0.2.0 — Language breadth and Studio integration (DONE, 2026-11-21)

- [x] Parser: recover from multiple syntax errors (P-001)
- [x] Checker: full control-flow return analysis, `while true`
      divergence (P-002)
- [x] KIR: local slot reuse after scope exit (P-003)
- [x] Language: modules and multi-file projects (`import`, #1)
- [x] Language: explicit casts, arrays, `for` over ranges and arrays
- [x] Language: string interpolation decision (ADR-0013: deferred to
      0.3 — decided, not silently dropped)
- [x] Stdlib: `len`, `abs`, `min`, `max` as a real native function table
- [x] Studio: kinc → WebAssembly for full in-Studio `check` and `run` (#8)
- [x] Studio: persistent files (IndexedDB + `.kinnproj` export/import) (#9)
- [x] CI: GitHub Actions build+test matrix (#10)
- [x] KINN Studio Android app: Kotlin source, on-device compiler on
      ARM64, signed release pipeline with secrets-only signing (#11, #12)
- [x] Windows (zip + NSIS installer) and Linux (DEB + AppImage
      experimental) packages, built and tested on their platforms

## 0.3.0 — Multi-target: web vertical slice, Android backend architecture

Re-scoped 2026-11-21 (product direction: KINN Studio becomes the
development environment for real multi-target apps; details in
docs/MULTI_TARGET_ARCHITECTURE.md):

- [x] Target/backend architecture: SiteModel, backend contract,
      capability declaration (ADR-0018)
- [x] Language: declarative `website` module (ADR-0019) with full
      parse/check/lower tests
- [x] Web backend: `kinn build --target web` producing a real dist
      (HTML/CSS, accessibility, navigation; ADR-0020), verified in a
      browser
- [x] Studio integration (web + Android): target select, build,
      diagnostics, preview, export
- [x] Android backend: exported Gradle project + build providers
      (local / remote CI / export; ADR-0021), APK built in CI with
      apksigner verification
- [x] Ecosystem foundation: `kinn.toml` manifest + MODULE_SYSTEM spec
      (ADR-0022)
- [x] CI tests the actual supported targets (web smoke, backend APK)
- [ ] String interpolation and string→number casts (ADR-0013) —
      carried to 0.4 to keep this release focused; decision unchanged
- [ ] Native code generation investigation — moved to 0.4+ (re-scoped
      out of 0.3 by the multi-target direction; ADR-0002 spike still
      pending)

## 0.4.0 — Breadth and developer comfort

- [ ] Cross-module value exports (consts, not only functions)
- [ ] Structs/records decision (ADR) or a deliberate "arrays only"
      extension statement
- [ ] Debugging support in the VM (step, breakpoints) exposed in Studio
- [ ] Android app hardening: UI tests on an emulator in CI
- [ ] AppImage verification on clean machines (promote from experimental)

## 0.5.0 — Graphics foundations (OpenGL ES 3.0)

- [ ] Window/context initialization (EGL + desktop GL), verified on
      real hardware
- [ ] Render loop driven from the runtime
- [ ] Geometry, cubes, meshes; cameras and transformations
- [ ] Textures, materials, shaders (GLSL ES 3.00)
- [ ] GPU resource management with leak reporting
- [ ] Android ARM64 device verification of the run path
- [ ] C/C++ integration layer with security review

## 1.0.0 — Preview

- [ ] Graphics API stable enough for a beginner tutorial series
- [ ] Documentation complete: tutorials per mode
- [ ] Release process with signed artifacts (with owner approval)
- [ ] License decision executed (ADR-0012)

## Standing rules

- One graphics backend before a second; no parallel backends.
- No feature ships claiming to work without a test or a verification
  log.
- Every milestone updates CHANGELOG, AI_CONTEXT and PROBLEM_REGISTRY.
- The compiler core stays zero-dependency; new host glue (JNI, future
  desktop shells) must justify itself in an ADR (see ADR-0014).
- Release signing only ever uses real secrets supplied by the owner —
  unsigned artifacts are labeled UNSIGNED, never faked.
