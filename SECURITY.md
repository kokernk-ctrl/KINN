# Security Policy

## Reporting a vulnerability

Open a private security advisory on this repository (Security tab →
"Report a vulnerability"), or contact the repository owner directly.
Please do not open public issues for vulnerabilities.

## Threat model

KINN is offline-first: the compiler, the web Studio (compiler included,
via WebAssembly) and the Android app (on-device compilation) run
entirely on your machine. Nothing in this repository phones home,
requires an account, or transmits your code anywhere. The Android app
requests **no permissions** — no INTERNET — and all project data lives
in app-private storage.

## Token and secret hygiene

- Never commit tokens, passwords or keys. This repository contains no
  secrets.
- The bootstrap GitHub token used for repository automation is
  provided out-of-band by the owner and is never written to any file,
  commit, log or document. Rotate any token shared over a chat channel
  after use.
- If you believe a secret has leaked, rotate it immediately at the
  provider — assume it is compromised, then clean history.

## Build-provider security (0.3 backends)

The Android target's remote build provider runs in GitHub Actions with
repository-scoped permissions only. It requires **no user credentials
in chat, source files, or project configuration**: users trigger CI
with their own GitHub authentication; release signing continues to use
the encrypted `ANDROID_KEYSTORE_*` secrets (see below) or nothing at
all (unsigned/UNSIGNED labeling). The exported project ships no
signing configuration, and the generated app requests no permissions
(verified by CI with `aapt dump badging`). The web backend generates
static files only — no server, no network calls, no third-party
runtime.

## Android release signing

Release APKs are signed **only** with credentials supplied as encrypted
GitHub Actions secrets:

| Secret | Meaning |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | base64 of the release keystore (`.jks`/`.keystore`) |
| `ANDROID_KEYSTORE_PASSWORD` | keystore password |
| `ANDROID_KEY_ALIAS` | key alias inside the keystore |
| `ANDROID_KEY_PASSWORD` | key password |

Generating a keystore (do this locally, never in CI):

```bash
keytool -genkeypair -v \
  -keystore kinn-release.keystore \
  -alias kinn \
  -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 kinn-release.keystore   # paste as the ANDROID_KEYSTORE_BASE64 secret
```

The workflow (`.github/workflows/android.yml`) decodes the keystore at
runtime, passes passwords through environment variables only, and
verifies every APK with `apksigner verify --print-certs`, publishing
the certificate fingerprints in the `signature-report.txt` artifact
alongside SHA-256 checksums. **When the secrets are absent, the
release APK is left unsigned and named `...-UNSIGNED.apk`** — the
pipeline never invents a signing identity, and debug APKs are always
clearly labeled as debug-signed. Installing an unsigned APK requires
signing it yourself (see INSTALL.md).

CI does not and cannot publish a GitHub Release without explicit
owner-approved workflow changes — artifacts stay on the run page.

## Supply chain

- The Rust compiler core (`kinrt`, `kinc`, `kinn`, `kinn-ffi`) has
  **zero external dependencies**. `cargo build` never touches the
  network. The single exception is `kinn-android` (JNI glue only),
  which depends on the `jni` crate (ADR-0014); it contains no compiler
  logic and is built only for Android targets in CI.
- KINN Studio (web) has zero runtime dependencies; dev-only
  dependencies are `vite`, `typescript` and `vitest`, pinned through
  `package-lock.json`.
- The Android app uses only AndroidX/Material libraries from Google's
  Maven; no analytics, no ads, no crash-reporting SDKs.
- Generated build artifacts (`compiler/target`, `studio/dist`,
  `studio/node_modules`, `studio/public/kinn.wasm`, `android/.gradle`,
  `android/app/build`, `android/local.properties`,
  `android/app/src/main/jniLibs`) are gitignored and must never be
  committed.

## Compiler safety notes

- Kin has no unsafe constructs, no FFI and no file/network access in
  the language; programs can loop forever but cannot escape the VM.
- Import paths are validated (relative `.kin` paths only, no `..`, no
  absolute paths) and cannot escape the project root; cycles and
  over-deep chains are rejected (E3017/E3018).
- The VM enforces a call-depth limit (E4002), checked integer
  arithmetic (E4004) and array bounds checks (E4005). Float division by
  zero is an error, not `inf` (a documented deviation from IEEE-754,
  ADR-0009).
- The FFI boundary (`kinn-ffi`) is panic-safe: panics are caught and
  reported as E5000 diagnostics inside the JSON result, never
  unwinding into a host (browser or Android).
- Graphics work (0.5+) will introduce native code; a security review
  is part of that milestone's exit criteria.

## Scope

This policy covers the KINN repository (compiler, runtime, CLI, FFI,
Studio web, Studio Android, CI, documentation). Third-party forks are
out of scope.
