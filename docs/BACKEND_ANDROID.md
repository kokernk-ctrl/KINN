# Android target backend (Kin 0.3 — architecture + incremental implementation)

Turns a Kin `website` declaration into a real, installable Android
application project. Status in 0.3.0: **project export implemented and
tested; APK produced by building that project** through one of the
build providers below (local Gradle, remote CI, or Android Studio).
Never claim an APK from an export.

---

## 1. Not to be confused with the JNI compiler integration

Two different components, both present in the repository:

| Component | What it is | Where |
| --- | --- | --- |
| **JNI compiler integration** | the Rust compiler *runs inside* the KINN Studio app so you can check/run Kin code on-device | `compiler/crates/kinn-android` + `KinnBridge` |
| **Android application backend** | generates an Android *app project* from Kin source | `compiler/crates/kinc/src/backend/android.rs` |

The backend consumes the validated `SiteModel` (ADR-0018), never the
AST, and emits a complete Gradle project (ADR-0021).

## 2. Generated project layout

`kinn build site.kin --target android` (default output
`<entry-dir>/build/android`, override with `--out`):

```
build/android/
  settings.gradle.kts            AGP 8.5.2 / Kotlin 1.9.24 plugin pins
  build.gradle.kts
  gradle.properties              useAndroidX=false (framework widgets only)
  gradle/wrapper/gradle-wrapper.properties   Gradle 8.7
  app/build.gradle.kts           minSdk 26, targetSdk 34, no signing config
  app/src/main/AndroidManifest.xml           launcher activity, NO permissions
  app/src/main/java/dev/kinn/site/<pkg>/MainActivity.kt
  app/src/main/res/values/{strings,colors,themes}.xml
  app/src/main/assets/…          files copied from the project assets/ dir
  README.md                      build instructions + honest status note
```

- Package: `dev.kinn.site.<slug>` (site name sanitized; Kotlin-keyword
  safe — tested).
- `MainActivity.kt` is **generated native UI code**: one function per
  page rendering the elements with framework widgets (`TextView`,
  `Button`, `ImageView`, plain `android.app.Activity`). No WebView, no
  AndroidX, no external dependencies.
- Navigation happens in place (`renderPage(index)`); the system back
  button exits (0.3.0 behavior, documented in the generated code).
- Image elements resolve `assets/…` sources from the project's
  `app/src/main/assets/` with the (compile-time enforced) alt text as
  the fallback when a file is missing.
- Lifecycle: single `Activity`, `onCreate` renders the home page. The
  UI module mapping is intentionally minimal and matches the web
  element set exactly.

## 3. Build-provider abstraction

The generator produces a *project*. The APK comes from a provider —
hosts must report which one actually ran:

| Provider | When | How | Status reported |
| --- | --- | --- | --- |
| **local** | Gradle + Android SDK installed | `gradle assembleDebug` in the exported project | `local build` (+ APK path) — verified on the CI runner; other toolchain layouts untested (P-018) |
| **remote** | no local tooling | `.github/workflows/android-app.yml` (GitHub Actions, SDK preinstalled, `apksigner verify`, artifact upload) | `remote build pending` → `APK built (signature verified by CI)` |
| **export** | always available | the exported project / .zip | `project exported` |

CLI behavior (honest by construction): `kinn build --target android`
exports the project and prints the exact next steps; it attempts a
local Gradle build only when `gradle` is on PATH, and says `exported`
otherwise. The remote path never requires users to paste personal
GitHub tokens into chat, source, or project configuration — CI secrets
or the user's own GitHub authentication only (SECURITY.md, ADR-0017).

## 4. Packaging and signing

- Debug APKs: `gradle assembleDebug` — debug-signed, for local testing.
- Release APKs: governed by ADR-0017 — signed only from the owner's
  encrypted `ANDROID_KEYSTORE_*` CI secrets, otherwise labeled
  UNSIGNED. The generated project ships **no** signing config.
- The exported project contains no Gradle wrapper jar (ADR-0021):
  Android Studio provides its own Gradle, CI uses the preinstalled
  one, and `gradle-wrapper.properties` pins the version for local
  builds.

## 5. Studio integration

- **Studio web**: `build android` (terminal) or the *Export Android*
  button downloads the project as a .zip — explicitly labeled a
  project.
- **Studio Android app (0.4.0)**: the app launches **Studio NEXT**
  (`NextActivity`: WebView + `kinn.wasm`, see
  docs/KINN_STUDIO_NEXT.md); the classic native Studio 0.3 remains
  bundled as the fallback. In NEXT, `build android` produces the
  project through the wasm backend and `export android`/download saves
  it via the host bridge (SAF); in the classic app the same commands
  run through the native JNI bridge and `export android` shares the
  .zip via SAF; `build web` + `preview` give the on-device offline
  preview. Statuses use the vocabulary above.

## 6. Verification performed

- 9 codegen unit tests: file set, manifest (no permissions, launcher
  activity), package sanitization, Kotlin escaping, navigation
  indices, assets mapping, no-AndroidX/no-WebView, honest README.
- CLI integration test exports a real project to disk; the toolless
  provider path degrades honestly (deterministic test), and the
  CI-verified local provider produced a real debug APK on the runner
  (first 0.3.0 `ci` run).
- FFI/wasm test exports the same project through the bridge.
- **End-to-end APK**: the `android-app` CI workflow builds the
  exported project on a GitHub runner (real SDK), runs `apksigner
  verify`, and uploads the APK artifact — see AUDIT_REPORT.md for the
  run evidence. On-device installation remains planned (P-017).
