package dev.kinn.studio

/**
 * Bridge to the native Kin compiler and VM (libkinn_android.so, built
 * from compiler/crates/kinn-android for aarch64).
 *
 * The library is optional at runtime: when it is missing (non-ARM64
 * device, or a build without the Rust .so) `available` stays false and
 * the app degrades honestly to editor-level checks — it never pretends
 * to compile.
 */
object KinnBridge {
    @Volatile
    var available: Boolean = false
        private set

    init {
        available = try {
            System.loadLibrary("kinn_android")
            true
        } catch (_: UnsatisfiedLinkError) {
            false
        } catch (_: SecurityException) {
            false
        }
    }

    /** Compiler version, 0x00MMmmpp packed as the FFI defines it. */
    external fun nVersion(): Int

    /**
     * Compile `src` (entry file named `entry`) with the project files
     * `paths`/`contents` as importable modules. Returns the FFI JSON:
     * `{"ok":bool,"mode":"S","diagnostics":[...]}`.
     */
    external fun nCheck(
        entry: String,
        src: String,
        paths: Array<String>,
        contents: Array<String>,
    ): String

    /**
     * Compile and execute; same JSON shape plus `"exitCode"` and
     * `"stdout"` (and `"runtimeError"` when the program failed at
     * runtime).
     */
    external fun nRun(
        entry: String,
        src: String,
        paths: Array<String>,
        contents: Array<String>,
    ): String

    /**
     * Build the web distribution (same backend as `kinn build --target
     * web`). JSON: `{"ok":bool,"site":str,"pages":n,"files":[...]}`.
     */
    external fun nBuildWeb(
        entry: String,
        src: String,
        paths: Array<String>,
        contents: Array<String>,
    ): String

    /**
     * Export the Android Gradle project (same backend as `kinn build
     * --target android`). The result is a project — building the APK
     * needs Android Studio, Gradle, or the remote CI workflow.
     */
    external fun nBuildAndroid(
        entry: String,
        src: String,
        paths: Array<String>,
        contents: Array<String>,
    ): String

    /** Version as "0.2.0", or null when the backend is unavailable. */
    fun version(): String? {
        if (!available) return null
        val v = nVersion()
        return "%d.%d.%d".format((v ushr 24) and 0xff, (v ushr 16) and 0xff, (v ushr 8) and 0xff)
    }
}
