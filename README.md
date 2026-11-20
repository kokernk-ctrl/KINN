# KINN

KINN is an independent programming language and development ecosystem,
built **offline-first**: no accounts, no cloud, no servers, no
online-only compilation. You write Kin programs, compile them locally,
and run them locally — on desktop today, with Android ARM64 as a
first-class target as the project grows.

KINN Studio is the terminal-inspired environment around the language:
dark graphite surfaces, one discreet orange accent, monospace
everywhere, honest about what works and what does not.

```
*mode = B;

func main() {
    let name: string = "world";
    print("Hello, " + name + "!");
}
```

## Status — what actually works today (0.1.0)

| Area | Status | Notes |
| --- | --- | --- |
| Lexer, parser, AST | **Implemented** | full Kin 0.1 subset, tested |
| Type checker | **Implemented** | no implicit int/float conversion, short-circuit `and`/`or` |
| KIR (intermediate representation) | **Implemented** | flat stack-based IR with source lines |
| Backend + runtime | **Implemented** | interpreter backend: stack VM (`kinrt`), checked overflow |
| CLI (`kinn`) | **Implemented** | `run`, `check`, `tokens`, `ast`, `kir`, `version` |
| Programming modes B/S/A | **Partially implemented** | directive parsed/validated; 0.1 changes diagnostics verbosity and Beginner lint — see `docs/PROGRAMMING_MODES.md` |
| KINN Studio (web shell) | **Implemented (shell)** | explorer, editor with highlighting, terminal, problems panel; in-memory files; no in-Studio execution yet |
| Studio in-Studio run / full type check | **Planned (0.2)** | needs kinc compiled to WebAssembly |
| Graphics (OpenGL ES 3.0) | **Planned (0.5)** | no graphics code shipped yet, by design |
| Android (Kotlin shell, on-device builds) | **Planned** | runtime is pure Rust and Android-buildable in principle; not yet verified on devices |
| Native code generation | **Planned (0.3 investigation)** | today everything runs on the KIR VM |

Every claim above is backed by tests: 90 Rust tests and 22 Studio
tests, all passing at the time of this release.

## Quick start

Requirements: Rust (stable, no external crates — builds fully offline),
Node.js 18+ for the Studio.

```bash
# build the compiler and CLI
cd compiler
cargo build --release
./target/release/kinn run ../examples/hello.kin

# run the full test suite (90 tests)
cargo test

# KINN Studio
cd ../studio
npm install
npm run dev          # development server
npm test             # 22 tests
npm run build        # production build (dist/)
```

Try the other examples: `conditions.kin`, `loops.kin`, `fibonacci.kin`,
`floats.kin`, `strings.kin`. Broken programs live in `examples/errors/`
and are used by the test suite to verify diagnostics.

## Repository layout

```
compiler/            Rust workspace (zero external dependencies)
  crates/kinrt/      runtime: KIR format, stack VM, disassembler
  crates/kinc/       compiler: lexer, parser, checker, lowering
  crates/kinn/       the kinn CLI binary
studio/              KINN Studio web shell (TypeScript, Vite, zero runtime deps)
examples/            runnable Kin programs; `// expect:` blocks are verified by tests
docs/                design documents (see index below)
```

## Documentation index

Root: [AI_CONTEXT.md](AI_CONTEXT.md) (implementation status for humans
and AI assistants), [CHANGELOG.md](CHANGELOG.md),
[SECURITY.md](SECURITY.md).

- [VISION.md](docs/VISION.md) — why KINN exists and what it refuses to be
- [LANGUAGE_DESIGN.md](docs/LANGUAGE_DESIGN.md) — language philosophy and principles
- [SYNTAX.md](docs/SYNTAX.md) — quick syntax reference with examples
- [LANGUAGE_SPEC_0.1.md](docs/LANGUAGE_SPEC_0.1.md) — normative spec: grammar, semantics, error codes
- [PROGRAMMING_MODES.md](docs/PROGRAMMING_MODES.md) — modes B / S / A: implemented vs planned behavior
- [TYPE_SYSTEM.md](docs/TYPE_SYSTEM.md) — 0.1 types and the no-hidden-conversions rule
- [COMPILER_ARCHITECTURE.md](docs/COMPILER_ARCHITECTURE.md) — pipeline, crates, diagnostics
- [RUNTIME_ARCHITECTURE.md](docs/RUNTIME_ARCHITECTURE.md) — KIR and the stack VM
- [GRAPHICS_ARCHITECTURE.md](docs/GRAPHICS_ARCHITECTURE.md) — OpenGL ES 3.0 plan (Vulkan later)
- [KINN_STUDIO.md](docs/KINN_STUDIO.md) — Studio features, design system, roadmap
- [CLI_DESIGN.md](docs/CLI_DESIGN.md) — commands, exit codes, output formats
- [ROADMAP.md](docs/ROADMAP.md) — milestones from 0.2 to 1.0
- [DECISIONS.md](docs/DECISIONS.md) — architecture decision records
- [PROBLEM_REGISTRY.md](docs/PROBLEM_REGISTRY.md) — known bugs and limitations, tracked openly

## License

To be decided by the project owner — see ADR-0012 in
[DECISIONS.md](docs/DECISIONS.md). Until then, all rights reserved.
