# Installing KINN

Every package below is built **and tested on its own platform** by
GitHub Actions; artifacts are attached to the workflow runs (Actions →
pick a run → Artifacts). Nothing here is claimed to work where it has
not been built.

KINN is offline-first: no installer contacts any KINN server (there
are none), requires an account, or phones home.

## Linux (x86_64)

### DEB package (verified in CI: installed and exercised on the runner)

```bash
# from a workflow run's "kinn-linux" artifact
sudo apt install ./kinn_0.2.0_amd64.deb
kinn version
kinn run /usr/share/kinn/examples/for_loops.kin
```

Installs `/usr/local/bin/kinn`, docs under `/usr/share/doc/kinn/` and
the example programs under `/usr/share/kinn/examples/`.

### AppImage (experimental)

```bash
chmod +x kinn-0.2.0-x86_64.AppImage
./kinn-0.2.0-x86_64.AppImage version
```

The AppImage is built with linuxdeploy in CI. It is labeled
**experimental** until verified on a clean machine; prefer the DEB or
the tarball.

### Tarball

```bash
tar xzf kinn-0.2.0-x86_64-linux.tar.gz
./kinn version
```

## Windows (x86_64)

Both packages are built on a Windows runner after the test suite
passes there.

### Portable zip

Unzip `kinn-0.2.0-x86_64-windows.zip` anywhere; run `kinn.exe` from a
terminal (`cmd` or PowerShell):

```powershell
.\kinn.exe version
.\kinn.exe run examples\modules\main.kin
```

### NSIS installer

Run `kinn-0.2.0-x86_64-setup.exe`. It installs `kinn.exe` into
`%PROGRAMFILES%\KINN\bin`, adds that directory to the system `PATH`,
and ships the docs and examples. Uninstall from "Add or remove
programs". The installer requests administrator rights only to write
into Program Files and the registry.

## Android (ARM64, Android 8.0+)

1. Download the `kinn-studio-android` artifact from a successful
   `android` workflow run. It contains:
   - `kinn-studio-0.2.0-debug.apk` — debug-signed (safe for testing;
     certificate fingerprints are in `signature-report.txt`)
   - `kinn-studio-0.2.0-release.apk` — signed release build, present
     **only** when the repository has the `ANDROID_KEYSTORE_*` secrets
     configured; otherwise `kinn-studio-0.2.0-release-UNSIGNED.apk`,
     which must be signed before installing (see
     [docs/SECURITY.md](docs/SECURITY.md))
   - `SHA256SUMS` and `signature-report.txt` (apksigner output)
2. Verify the checksum: `sha256sum -c SHA256SUMS`.
3. Install: `adb install kinn-studio-0.2.0-debug.apk`, or copy the APK
   to the device and open it (enable "install unknown apps" for your
   file manager first).
4. Open KINN Studio. Projects, editing, docs, import/export work
   everywhere. `check` and `run` use the **on-device compiler**
   (bundled `libkinn_android.so`, arm64-v8a). On non-ARM64 devices the
   app states plainly that the native compiler is unavailable.

The app requests no permissions and works fully offline.

## KINN Studio (web)

No installation: build it and open it.

```bash
cd studio
npm install
npm run build:wasm   # compiles the real compiler to public/kinn.wasm (needs Rust)
npm run build        # dist/ — open dist/index.html or serve it
```

`dist/` works from `file://` or any static server; the compiler runs
inside your browser. Without `npm run build:wasm`, Studio still works
but honestly falls back to editor-level checks.

## Building everything from source

Requirements: Rust stable (rustup), Node 18+; for Android also JDK 17,
Android SDK (platform 34, build-tools 34.0.0) and NDK 27.

```bash
git clone https://github.com/kokernk-ctrl/KINN.git
cd KINN

# compiler + CLI (all desktop platforms)
cd compiler
cargo build --release          # ./target/release/kinn[.exe]
cargo test                     # 190 tests

# web Studio
cd ../studio
npm install
npm run build:wasm && npm test && npm run build

# Android app
cargo install cargo-ndk
cd ../compiler
cargo ndk -t arm64-v8a --platform 26 -o ../android/app/src/main/jniLibs build --release -p kinn-android
cd ../android
./gradlew assembleDebug assembleRelease
```

The wasm build needs the target installed once:
`rustup target add wasm32-unknown-unknown`.

## Verifying artifacts

- Checksums: every workflow publishes `SHA256SUMS`.
- Android signature: `signature-report.txt` contains the apksigner
  certificate output; verify locally with
  `apksigner verify --print-certs app.apk`.
- CI never publishes a GitHub Release by itself — that step requires
  explicit owner approval.
