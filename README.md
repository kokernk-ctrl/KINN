# KINN

KINN is an independent programming language and development ecosystem,
built **offline-first**: no accounts, no cloud, no servers, no
online-only compilation. You write Kin programs, compile them locally,
and run them locally — on desktop and on Android ARM64.

KINN Studio is the terminal-inspired environment around the language:
dark graphite surfaces, one discreet orange accent, monospace
everywhere, honest about what works and what does not. It exists as a
web shell (runs fully in the browser, compiler included via
WebAssembly) and as an Android app with on-device compilation.

```
*mode = B;

import "math.kin" as math;

func main() {
    let a = [10, 20, 30];
    for x in a {
        print(math.square(x));
    }
    print("total: " + (math.sum(a) as string));
}
```

## Status — what actually works today (0.3.0)

| Area | Status | Notes |
| --- | --- | --- |
| Lexer, parser, AST | **Implemented** | Kin 0.2 subset; multi-error recovery, tested |
| Type checker | **Implemented** | arrays, casts, natives, modules; no implicit int/float conversion |
| Control-flow analysis | **Implemented** | return analysis incl. `while true` divergence; unreachable warnings |
| KIR (intermediate representation) | **Implemented** | flat stack-based IR with source lines and slot reuse |
| Backend + runtime | **Implemented** | interpreter backend: stack VM (`kinrt`), checked overflow, array bounds checks |
| Modules / multi-file projects | **Implemented** | `import "file.kin" as m;` with cycle detection, path confinement |
| Language 0.2 | **Implemented** | arrays, indexing, `for` over ranges/arrays, casts, `len`/`abs`/`min`/`max` |
| Language 0.3 web module | **Implemented** | declarative `website { page { … } }`: parse/check/lower with contextual keywords — see [docs/BACKEND_WEB.md](docs/BACKEND_WEB.md) |
| **Web target backend** | **Implemented** | `kinn build --target web` → real HTML5/CSS dist (semantic markup, accessibility, navigation); verified in a browser; JS-free by design (ADR-0020) |
| **Android target backend** | **Implemented (project) / APK via CI** | `kinn build --target android` → complete Gradle project with native Kotlin UI, no permissions; real APK built + signature-verified in CI — see [docs/BACKEND_ANDROID.md](docs/BACKEND_ANDROID.md) |
| Package foundation | **Implemented (manifest)** | `kinn.toml` + module spec ([docs/MODULE_SYSTEM.md](docs/MODULE_SYSTEM.md)); manager/registry deliberately not built yet |
| CLI (`kinn`) | **Implemented** | `run`, `check`, `build --target`, `targets`, `tokens`, `ast`, `kir`, `version`; multi-file projects; `kinn.toml` manifests |
| KINN Studio (web) | **Implemented** | editor, terminal, problems; real check/run/build via `kinn.wasm`; **target build + preview panel + zip export**; IndexedDB persistence; project import/export |
| KINN Studio (Android) | **Implemented** | Kotlin app: projects, editor, problems, terminal, offline docs, SAF import/export; on-device compiler via JNI; **web build + offline preview + Android project export** |
| CI | **Implemented** | Rust matrix (Linux/macOS/Windows), Studio+wasm, **web/android target smoke**, Android Studio APK (sign+verify), **backend APK from exported Kin project**, Windows zip/installer, Linux deb/appimage |
| Programming modes B/S/A | **Partially implemented** | directive parsed/validated; changes diagnostics verbosity and Beginner lint — see `docs/PROGRAMMING_MODES.md` |
| Graphics (OpenGL ES 3.0) | **Planned (0.5)** | no graphics code shipped yet, by design |
| Native code generation | **Planned (0.4+)** | console programs run on the KIR VM; web/android targets have real backends since 0.3 |
| Studio desktop shell | **Planned** | the web shell runs in any browser; a wrapper app is 0.3+ |

Every claim above is backed by tests: **250 Rust tests** (compiler, VM,
FFI, CLI, target backends) and **66 Studio tests** (15 of them running
the real compiler and target backends in WebAssembly), all passing —
see [AUDIT_REPORT.md](AUDIT_REPORT.md).

## Quick start

### CLI (all desktop platforms)

Requirements: Rust stable. No external crates — builds fully offline.

```bash
cd compiler
cargo build --release
./target/release/kinn run ../examples/hello.kin
./target/release/kinn run ../examples/modules/main.kin   # multi-file

# 0.3: build a real website from Kin source
./target/release/kinn build ../examples/web/site.kin --target web
# → ../examples/web/build/web/ (index.html, about.html, styles.css, app.js)

cargo test          # 250 tests
```

### KINN Studio (web)

Requirements: Node 18+; Rust for the compiler backend (optional but
recommended — without it Studio honestly falls back to editor-level
checks).

```bash
cd studio
npm install
npm run build:wasm   # compiles the real compiler to public/kinn.wasm
npm test             # 66 tests (15 against the real wasm compiler + backends)
npm run dev          # development server
npm run build        # production build (dist/)
```

### KINN Studio (Android)

Grab an APK from the Actions artifacts ("kinn-studio-android" — build
the `android` workflow or use a run from `main`), or build it yourself:

```bash
# prerequisites: JDK 17, Android SDK (platform 34, build-tools 34, NDK 27), Rust
cargo install cargo-ndk
cd compiler
cargo ndk -t arm64-v8a --platform 26 -o ../android/app/src/main/jniLibs build --release -p kinn-android
cd ../android
./gradlew assembleDebug      # app/build/outputs/apk/debug/app-debug.apk
```

The debug APK is debug-signed for testing. Release signing uses the
encrypted `ANDROID_KEYSTORE_*` GitHub secrets — see
[docs/SECURITY.md](docs/SECURITY.md). Install with
`adb install app-debug.apk` (enable "install from unknown sources" on
the device). See [INSTALL.md](INSTALL.md) for details.

## Installation (per platform)

See [INSTALL.md](INSTALL.md): Linux (deb / AppImage / tarball), Windows
(zip / NSIS installer), Android (APK), plus build-from-source for
everything. CI builds and verifies each package on its own platform —
nothing is claimed to work until it has been built and tested there.

## Repository layout

```
compiler/            Rust workspace (zero external dependencies in the core)
  crates/kinrt/      runtime: KIR format, stack VM, disassembler
  crates/kinc/       compiler: lexer, parser, project loader, checker, lowering
  crates/kinn/       the kinn CLI binary
  crates/kinn-ffi/   C-ABI bridge: same compiler for wasm and native hosts
  crates/kinn-android/  JNI glue for the Android app (only crate with a dep)
android/             KINN Studio Android app (Kotlin)
studio/              KINN Studio web shell (TypeScript, Vite, zero runtime deps)
examples/            runnable Kin programs; `// expect:` blocks are verified by tests
examples/errors/     programs that must fail with the declared diagnostic code
examples/modules/    multi-file project example
examples/web/        declarative website example (kinn build --target web)
scripts/             NSIS installer script, Linux desktop entry + icon
.github/workflows/   CI: test matrix, Android APK pipeline, Windows/Linux packages
docs/                design documents (see index below)
```

## Documentation index

Root: [AI_CONTEXT.md](AI_CONTEXT.md) (implementation status for humans
and AI assistants), [CHANGELOG.md](CHANGELOG.md),
[SECURITY.md](SECURITY.md), [AUDIT_REPORT.md](AUDIT_REPORT.md),
[INSTALL.md](INSTALL.md).

- [VISION.md](docs/VISION.md) — why KINN exists and what it refuses to be
- [LANGUAGE_DESIGN.md](docs/LANGUAGE_DESIGN.md) — language philosophy and principles
- [SYNTAX.md](docs/SYNTAX.md) — quick syntax reference with examples
- [LANGUAGE_SPEC_0.1.md](docs/LANGUAGE_SPEC_0.1.md) — normative spec: grammar, semantics, error codes (0.1) + 0.2/0.3 additions
- [MULTI_TARGET_ARCHITECTURE.md](docs/MULTI_TARGET_ARCHITECTURE.md) — target/backend architecture and gap analysis
- [BACKEND_WEB.md](docs/BACKEND_WEB.md) — the web module and the web target contract
- [BACKEND_ANDROID.md](docs/BACKEND_ANDROID.md) — the Android target and build providers
- [MODULE_SYSTEM.md](docs/MODULE_SYSTEM.md) — modules, packages, registry, SDK layering
- [PROGRAMMING_MODES.md](docs/PROGRAMMING_MODES.md) — modes B / S / A: implemented vs planned behavior
- [TYPE_SYSTEM.md](docs/TYPE_SYSTEM.md) — types and the no-hidden-conversions rule
- [COMPILER_ARCHITECTURE.md](docs/COMPILER_ARCHITECTURE.md) — pipeline, crates, diagnostics
- [RUNTIME_ARCHITECTURE.md](docs/RUNTIME_ARCHITECTURE.md) — KIR and the stack VM
- [GRAPHICS_ARCHITECTURE.md](docs/GRAPHICS_ARCHITECTURE.md) — OpenGL ES 3.0 plan (Vulkan later)
- [KINN_STUDIO.md](docs/KINN_STUDIO.md) — Studio features, design system, roadmap (web + Android)
- [CLI_DESIGN.md](docs/CLI_DESIGN.md) — commands, exit codes, output formats
- [ROADMAP.md](docs/ROADMAP.md) — milestones from 0.3 to 1.0
- [DECISIONS.md](docs/DECISIONS.md) — architecture decision records
- [PROBLEM_REGISTRY.md](docs/PROBLEM_REGISTRY.md) — known bugs and limitations, tracked openly

## License

To be decided by the project owner — see ADR-0012 in
[DECISIONS.md](docs/DECISIONS.md). Until then, all rights reserved.
