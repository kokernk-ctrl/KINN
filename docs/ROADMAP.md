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
- [x] Labels, milestones, issues on GitHub

## 0.2.0 — Language breadth and Studio integration

- [ ] Parser: recover from multiple syntax errors (P-001)
- [ ] Checker: full control-flow return analysis, `while true`
      divergence (P-002)
- [ ] KIR: local slot reuse after scope exit (P-003)
- [ ] Language: modules and multi-file projects (`import`)
- [ ] Language: explicit casts, arrays, `for` over ranges
- [ ] Language: string interpolation decision (ADR) + implementation
- [ ] Stdlib: `len`, `abs`, `min`, `max` as a real native function table
- [ ] Studio: kinc → WebAssembly for full in-Studio `check` and `run`
- [ ] Studio: persistent file system (desktop shell first)
- [ ] CI: GitHub Actions build+test matrix (Linux/macOS/Windows) (P-008)

## 0.3.0 — Native code generation investigation

- [ ] Backend spike: Cranelift vs LLVM vs C transpilation (ADR-0002
      decision)
- [ ] Constant folding and trivial optimizations over KIR
- [ ] Timing/size reports in `kinn` for Advanced mode
- [ ] Studio desktop shell (webview wrapper) candidate

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

- [ ] Studio Android shell (Kotlin/Capacitor)
- [ ] Graphics API stable enough for a beginner tutorial series
- [ ] Documentation complete: tutorials per mode
- [ ] Release process with signed artifacts (with owner approval)

## Standing rules

- One graphics backend before a second; no parallel backends.
- No feature ships claiming to work without a test or a verification
  log.
- Every milestone updates CHANGELOG, AI_CONTEXT and PROBLEM_REGISTRY.
- License decision (ADR-0012) must happen before accepting external
  contributions.
