# Multi-target architecture (0.3.0)

Proposal + gap analysis against the 0.2.0 baseline. Status markers follow
the project rule: **implemented** (built and tested), **partial** (works
with stated limits), **planned** (designed here, not built yet), **not
started**.

---

## 1. Where 0.2.0 stands (verified baseline, 2026-11-21)

Verified by running the suites locally on the 0.2.0 tip (`6845496`):

| Component | Evidence at 0.2.0 |
| --- | --- |
| Compiler core (`kinc`) | 111 unit + 22 pipeline tests green; zero external deps |
| Runtime (`kinrt`) | 26 tests; KIR stack VM (arrays, casts, natives) |
| CLI (`kinn`) | 19 integration tests; `run` / `check` / `tokens` / `ast` / `kir` |
| FFI (`kinn-ffi`) | 12 tests; wasm build smoke-tested; one crate, three hosts |
| JNI glue (`kinn-android`) | builds for `aarch64-linux-android`; verified symbols |
| Studio web | 51 vitest tests; real wasm compiler, VFS, IndexedDB, terminal |
| Studio Android | Kotlin app; on-device compile/run via JNI; SAF import/export |
| CI | 4 workflows (ci, android, windows, linux), all green at 0.2.0 |

Baseline totals: **190 Rust tests + 51 Studio tests**, re-run locally for
this document. No code was replaced or restarted.

## 2. Gap analysis: 0.2.0 → multi-target ecosystem

| Requirement | 0.2.0 state | Gap |
| --- | --- | --- |
| Write an app in Kin only | CLI language only (console programs) | No declarative UI module, no non-VM targets |
| `kinn build --target web` | No `build` command, no targets | New CLI surface |
| Real website output | none | Web backend (HTML/CSS generation) |
| Target capability declaration + diagnostics | single implicit target (KIR VM) | Target registry + capability rules |
| Studio build target selection | run/check only | Build panel + preview + export |
| Android *application* output | Android *Studio app* + JNI compiler (different thing) | Android backend + build providers |
| SDK / package manager / registry | `import "file.kin"` modules only | Spec + manifest foundation |
| CI testing actual targets | tests compiler+Studio+APK of the Studio app | web smoke + backend APK job |

Key distinction kept explicit: the existing JNI integration (running the
Rust compiler *inside* the Android Studio app) is **not** an Android
application backend. It stays exactly as it is.

## 3. Target/backend architecture

### 3.1 Pipeline after 0.3.0

```
 source .kin ──lexer→ parser → AST ──checker→ ┌─ lower → KIR → kinrt VM   (target "vm", existing)
                                              └─ site model ──backend→ artifacts
                                                 (target "web", target "android", ...)
```

- **KIR stays the execution representation** for console programs.
- The declarative `website` module lowers to a **`SiteModel`**
  (`kinc::site`) — a small, target-independent UI tree. Backends consume
  the `SiteModel`, never the AST, so every target sees identical,
  already-validated semantics.
- `driver::Compiled` carries both: `module: Module` and
  `site: Option<SiteModel>`. A program can have either or both.

### 3.2 Backend contract (implemented in `kinc::backend`)

Every backend is a pure function from validated compiler output to
artifact files, plus a static capability declaration:

| Contract item | Form |
| --- | --- |
| Target identification | `backend::Target` enum + `id()`/`description()` (used by CLI and Studio UIs) |
| Capabilities | per-target supported element/feature table, published in the backend docs (BACKEND_WEB.md, BACKEND_ANDROID.md) |
| Semantic validation | shared checker rules (website rules E3030–E3037 are target-independent) + backend-specific rejection with a clear diagnostic instead of silent wrong output |
| Compilation | `generate(&SiteModel, …) -> Vec<OutputFile>` (bytes, so binaries like wrapper jars are possible) |
| Artifacts | `OutputFile { path, data }` — written under the build output dir |
| Diagnostics | compile errors surface before generation; generation itself is total (no partial silent output) |
| Runtime/preview | web: dist is directly hostable + Studio preview; android: exported Gradle project builds via local SDK or the CI remote-build workflow |

Backends declare what they support. Anything outside the declared
feature set cannot be *expressed* in the source (the grammar rejects
it with a precise code), and features the grammar accepts but a target
cannot use are rejected by that target's build with an explicit error —
never by generating wrong output.

### 3.3 Target 1 — Web (this release, complete vertical slice)

- Language: declarative `website "Name" { page Home { … } }` module
  (grammar in BACKEND_WEB.md; codes E2016, E2030, E3030–E3037, W3035,
  W3036).
- Checker validates: single website, page-name uniqueness
  (case-insensitive), navigation targets resolve, non-empty names,
  image `alt` present, labels non-empty, websites only in the entry
  file.
- `kinc::backend::web::generate` produces `index.html` (first page),
  one `<slug>.html` per page, `styles.css`, `app.js`. Honest
  engineering decisions (ADR-0020): semantic HTML5, generated
  server-side `aria-current`, accessibility enforced by the checker
  (alt text, heading order, skip link, focus-visible), **no JavaScript
  framework** — `app.js` only does focus management after navigation;
  interactive state is a planned 0.4 module and the language does not
  pretend otherwise.
- CLI: `kinn build <file.kin> --target web [--out DIR]` (default
  `build/web`), plus `kinn targets`. The `assets/` directory next to
  the entry file is copied into `dist/assets/` when present.
- Studio (web + Android): same backend through
  `kinn_build_web` (FFI/JNI) → target select, build, diagnostics, file
  list, in-Studio preview (page switcher), download/export as zip.
  Verified in a real browser from the generated dist (see
  BACKEND_WEB.md §8).

### 3.4 Target 2 — Android (architecture now, incremental implementation)

Not to be confused with the JNI compiler integration. The Android
backend turns a Kin website into a **real installable APK**:

1. `kinc::backend::android::generate` exports a complete Gradle project
   (`build/android/`): manifest, resources, `MainActivity.kt` with the
   site rendered as **native Android views** (TextView/Button/ImageView,
   in-app page navigation), README.
2. Build providers (CLI + Studio), in order of availability:
   - **local**: Gradle + Android SDK detected → build and report the
     APK path (when the toolchain is absent the CLI says so honestly
     and stops after export);
   - **remote**: `.github/workflows/android-app.yml` builds the
     exported project on a GitHub runner (SDK preinstalled), verifies
     the signature with apksigner, uploads the APK artifact. Users
     trigger it with their own GitHub authentication; no tokens ever
     enter chat, source, or project config (SECURITY.md);
   - **export**: the project itself is always the fallback artifact.
3. Status vocabulary everywhere: `exported` / `local build pending` /
   `remote build pending` / `APK built (verified signature)` — outputs
   are never mislabeled.

### 3.5 Ecosystem foundation (small by design this release)

- `docs/MODULE_SYSTEM.md`: module/package/registry/network/graphics/FFI
  layering and what is intentionally *not* in the language (no
  third-party syntax).
- `kinn.toml` manifest (name, version, entry) + `kinn build` reads it
  from the working directory; spec in MODULE_SYSTEM.md. Package
  manager, registry, network APIs: **planned**, documented, not faked.

## 4. Programming modes

Unchanged rule set (directive + verbosity + B-mode lint). The website
module is available in **all three modes** — it is a language module,
not a mode feature; modes do not fork the language. B-mode lint notes
apply to website programs like anywhere else.

## 5. Compatibility

- `website`, `page`, `button`, … are **contextual keywords**: recognized
  only in their grammatical positions, so existing 0.2 programs that
  use them as identifiers keep compiling (tested).
- Console programs, modules, arrays, casts, natives: untouched; the
  full 190-test suite keeps passing plus new tests.
- `kinn run` on a website-only program fails with guidance toward
  `kinn build --target web` (no fake execution).

## 6. Implementation sequence (executed in this order)

1. `chore`: version 0.3.0 across crates/CLI/Studio/app.
2. `feat(kinc)`: website declarations in AST + parser (+tests).
3. `feat(kinc)`: website semantics, SiteModel lowering (+tests).
4. `feat(kinc)`: web backend codegen (+tests).
5. `feat(cli)`: `kinn build --target web`, `kinn targets`, `kinn.toml`
   (+tests).
6. `feat(ffi)`: `kinn_build_web` + wasm rebuild (+tests).
7. `feat(studio)`: build panel, preview, zip export (+vitest).
8. `feat(kinc)`: android backend project export (+tests).
9. `feat(cli)`: android build-provider statuses (+tests).
10. `feat(android)`: app target select, web build + preview, project
    export (Kotlin + JNI).
11. `ci`: web smoke job; backend APK job in the android workflow.
12. `docs`: spec additions, BACKEND_WEB, BACKEND_ANDROID,
    MODULE_SYSTEM, KINN_STUDIO, SECURITY, README, CHANGELOG,
    AI_CONTEXT, AUDIT_REPORT (0.3.0).

Each step: implement → build → test → docs → commit.
