//! kinn-ffi — C-ABI bridge to the Kin compiler and VM.
//!
//! One small surface, three hosts:
//!
//! - **WebAssembly** (`wasm32-unknown-unknown`): KINN Studio loads the
//!   `kinn.wasm` build of this crate and runs the real compiler in the
//!   browser, offline.
//! - **Android** (`aarch64-linux-android`): the KINN Studio app bundles
//!   `libkinn_ffi.so` and calls the same functions through JNI.
//! - **Native** (desktop tests): Rust integration tests call the
//!   functions directly, so the bridge itself is tested like everything
//!   else in KINN.
//!
//! ## Memory protocol
//!
//! Strings cross the boundary as `(ptr, len)` UTF-8 byte ranges the host
//! wrote into memory obtained from [`kinn_alloc`]. Results come back as
//! a length-prefixed buffer: 4 little-endian bytes with the byte length,
//! followed by that many UTF-8 bytes (JSON). Every result pointer must
//! be released with [`kinn_free`].
//!
//! ## Modules
//!
//! Single-file compilation is always available. Multi-file projects work
//! by pre-registering module sources with [`kinn_set_module`] (paths as
//! they appear in `import "..."` statements, resolved by the compiler's
//! own project loader) and clearing them with [`kinn_clear_modules`]
//! between runs.
//!
//! The exported functions never unwind: panics are caught and reported
//! as an `E5xxx` diagnostic inside the JSON result.

use kinc::ast::Mode;
use kinc::diagnostics::{render_project, Diagnostic};
use kinc::driver;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

/// Result layout: [u32 length (LE)][UTF-8 JSON].
const LEN_PREFIX: usize = 4;

// ---------------------------------------------------------------------------
// Module registry
// ---------------------------------------------------------------------------

fn modules() -> &'static Mutex<HashMap<String, String>> {
    static MODULES: std::sync::OnceLock<Mutex<HashMap<String, String>>> =
        std::sync::OnceLock::new();
    MODULES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a module source under a project-relative path
/// (e.g. `"math.kin"`). Later registrations replace earlier ones.
///
/// # Safety
/// `src` must point to `src_len` readable bytes. The bytes are copied
/// before returning.
#[no_mangle]
pub unsafe extern "C" fn kinn_set_module(path_ptr: *const u8, path_len: usize, src_ptr: *const u8, src_len: usize) {
    let path = String::from_utf8_lossy(std::slice::from_raw_parts(path_ptr, path_len)).into_owned();
    let src = String::from_utf8_lossy(std::slice::from_raw_parts(src_ptr, src_len)).into_owned();
    modules().lock().unwrap().insert(path, src);
}

/// Forget all registered modules.
#[no_mangle]
pub extern "C" fn kinn_clear_modules() {
    modules().lock().unwrap().clear();
}

// ---------------------------------------------------------------------------
// Allocation
// ---------------------------------------------------------------------------

/// Allocate `len` bytes the host can write source text into. Release
/// with [`kinn_free`] (result buffers own their memory and are freed
/// separately).
///
/// # Safety
/// The returned pointer is valid for `len` bytes until freed. A zero
/// length returns a dangling-but-aligned pointer.
#[no_mangle]
pub extern "C" fn kinn_alloc(len: usize) -> *mut u8 {
    let mut buf: Vec<u8> = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Free a buffer obtained from [`kinn_alloc`] or a result pointer.
///
/// # Safety
/// `ptr` must come from [`kinn_alloc`] or a result of [`kinn_check`] /
/// [`kinn_run`] / [`kinn_check_text`], with the exact total length it
/// was created with, and must not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn kinn_free(ptr: *mut u8, len: usize) {
    if len == 0 {
        return;
    }
    drop(Vec::from_raw_parts(ptr, len, len));
}

// ---------------------------------------------------------------------------
// JSON output
// ---------------------------------------------------------------------------

/// Minimal JSON string escaping (control characters become \u escapes).
fn json_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    json_escape(s, &mut out);
    out.push('"');
    out
}

fn diags_json(diags: &[Diagnostic]) -> String {
    let mut out = String::from("[");
    for (i, d) in diags.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"severity\":{},\"code\":{},\"message\":{},\"file\":{},\"line\":{},\"col\":{},\"len\":{}}}",
            json_str(d.severity.label()),
            json_str(d.code),
            json_str(&d.message),
            json_str(&d.file),
            d.line,
            d.col,
            d.len
        ));
    }
    out.push(']');
    out
}

/// Render diagnostics in the human-readable CLI style (used by hosts
/// that want preformatted output).
fn diags_text(diags: &[Diagnostic], sources: &HashMap<String, String>, mode: Mode) -> String {
    render_project(diags, sources, mode)
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Pack a JSON payload into the length-prefixed result buffer.
fn pack(json: String) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(LEN_PREFIX + json.len());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(json.as_bytes());
    bytes
}

/// Read host-provided source bytes as UTF-8.
///
/// # Safety
/// `ptr` must point to `len` readable bytes.
unsafe fn take_string(ptr: *const u8, len: usize) -> String {
    String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned()
}

/// Internal panic-safe run of a closure producing JSON.
fn ffi_entry(f: impl FnOnce() -> String) -> *mut u8 {
    let json = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(json) => json,
        Err(_) => format!(
            "{{\"ok\":false,\"diagnostics\":[{{\"severity\":\"error\",\"code\":\"E5000\",\"message\":\"internal compiler panic (please report this)\",\"file\":\"\",\"line\":0,\"col\":0,\"len\":0}}]}}"
        ),
    };
    let mut bytes = pack(json);
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    // SAFETY contract: host frees with kinn_free(ptr, len).
    ptr
}

/// Compile a project and return diagnostics as JSON.
///
/// JSON: `{"ok": bool, "mode": "B"|"S"|"A", "diagnostics": [...],
/// "text": "<preformatted diagnostics>"}`.
///
/// # Safety
/// `src_ptr` must point to `src_len` readable bytes. Free the result
/// with [`kinn_free`] using the length stored in its first four bytes.
#[no_mangle]
pub unsafe extern "C" fn kinn_check(src_ptr: *const u8, src_len: usize, entry_ptr: *const u8, entry_len: usize) -> *mut u8 {
    ffi_entry(|| {
        let src = take_string(src_ptr, src_len);
        let entry = if entry_len == 0 {
            "main.kin".to_string()
        } else {
            take_string(entry_ptr, entry_len)
        };
        let registry = modules().lock().unwrap().clone();
        let mut loader = |path: &str| -> Result<String, String> {
            registry
                .get(path)
                .cloned()
                .ok_or_else(|| "module not registered with kinn_set_module".to_string())
        };
        let (compiled, diags) = driver::compile_project(&entry, &src, &mut loader);
        let has_err = diags.iter().any(|d| d.is_error());
        let mode = compiled.as_ref().map(|c| c.mode).unwrap_or(Mode::S);
        let sources = compiled
            .as_ref()
            .map(|c| c.sources.clone())
            .unwrap_or_default();
        format!(
            "{{\"ok\":{},\"mode\":{},\"diagnostics\":{},\"text\":{}}}",
            !has_err,
            json_str(mode.as_str()),
            diags_json(&diags),
            json_str(&diags_text(&diags, &sources, mode))
        )
    })
}

/// Compile and execute, capturing program output.
///
/// JSON: `{"ok": bool, "exitCode": 0|1|3, "stdout": "...",
/// "diagnostics": [...], "text": "<preformatted diagnostics>"}`.
/// `exitCode` mirrors the CLI: 0 ok, 1 compile diagnostics, 3 runtime
/// error.
///
/// # Safety
/// See [`kinn_check`].
#[no_mangle]
pub unsafe extern "C" fn kinn_run(src_ptr: *const u8, src_len: usize, entry_ptr: *const u8, entry_len: usize) -> *mut u8 {
    ffi_entry(|| {
        let src = take_string(src_ptr, src_len);
        let entry = if entry_len == 0 {
            "main.kin".to_string()
        } else {
            take_string(entry_ptr, entry_len)
        };
        let registry = modules().lock().unwrap().clone();
        let mut loader = |path: &str| -> Result<String, String> {
            registry
                .get(path)
                .cloned()
                .ok_or_else(|| "module not registered with kinn_set_module".to_string())
        };
        let (compiled, diags) = driver::compile_project(&entry, &src, &mut loader);
        let mode = compiled.as_ref().map(|c| c.mode).unwrap_or(Mode::S);

        match compiled {
            None => format!(
                "{{\"ok\":false,\"exitCode\":1,\"stdout\":{},\"diagnostics\":{},\"text\":{}}}",
                json_str(""),
                diags_json(&diags),
                json_str(&diags_text(&diags, &HashMap::new(), mode))
            ),
            Some(c) => {
                let mut out: Vec<u8> = Vec::new();
                match kinrt::run(&c.module, &mut out) {
                    Ok(()) => format!(
                        "{{\"ok\":true,\"exitCode\":0,\"stdout\":{},\"diagnostics\":{},\"text\":{}}}",
                        json_str(&String::from_utf8_lossy(&out)),
                        diags_json(&diags),
                        json_str(&diags_text(&diags, &c.sources, mode))
                    ),
                    Err(e) => format!(
                        "{{\"ok\":false,\"exitCode\":3,\"stdout\":{},\"runtimeError\":{{\"code\":{},\"message\":{},\"line\":{}}},\"diagnostics\":{},\"text\":{}}}",
                        json_str(&String::from_utf8_lossy(&out)),
                        json_str(e.code),
                        json_str(&e.message),
                        e.line,
                        diags_json(&diags),
                        json_str(&diags_text(&diags, &c.sources, mode))
                    ),
                }
            }
        }
    })
}

/// Compile with human-readable output only (no JSON): returns the
/// rendered diagnostics, or `""` plus `ok` in JSON — see [`kinn_check`].
/// Kept as a separate entry point for hosts that only display text.
///
/// # Safety
/// See [`kinn_check`].
#[no_mangle]
pub unsafe extern "C" fn kinn_check_text(src_ptr: *const u8, src_len: usize, entry_ptr: *const u8, entry_len: usize) -> *mut u8 {
    ffi_entry(|| {
        let src = take_string(src_ptr, src_len);
        let entry = if entry_len == 0 {
            "main.kin".to_string()
        } else {
            take_string(entry_ptr, entry_len)
        };
        let registry = modules().lock().unwrap().clone();
        let mut loader = |path: &str| -> Result<String, String> {
            registry
                .get(path)
                .cloned()
                .ok_or_else(|| "module not registered with kinn_set_module".to_string())
        };
        let (compiled, diags) = driver::compile_project(&entry, &src, &mut loader);
        let mode = compiled.as_ref().map(|c| c.mode).unwrap_or(Mode::S);
        let sources = compiled
            .as_ref()
            .map(|c| c.sources.clone())
            .unwrap_or_default();
        json_str(&diags_text(&diags, &sources, mode))
    })
}

/// The KINN version string (compile-time constant of this build).
#[no_mangle]
pub extern "C" fn kinn_version() -> u32 {
    // 0x00_MM_mm_pp: major, minor, patch.
    0x00_02_00_00
}

// ---------------------------------------------------------------------------
// Safe wrappers used by tests (and available to native hosts via rlib)
// ---------------------------------------------------------------------------

/// Safe wrapper around [`kinn_check`]: returns the JSON payload.
pub fn check_json(entry: &str, src: &str) -> String {
    unsafe {
        let ptr = kinn_check(src.as_ptr(), src.len(), entry.as_ptr(), entry.len());
        let len = u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]) as usize;
        let json = String::from_utf8_lossy(std::slice::from_raw_parts(ptr.add(LEN_PREFIX), len)).into_owned();
        kinn_free(ptr, LEN_PREFIX + len);
        json
    }
}

/// Safe wrapper around [`kinn_run`]: returns the JSON payload.
pub fn run_json(entry: &str, src: &str) -> String {
    unsafe {
        let ptr = kinn_run(src.as_ptr(), src.len(), entry.as_ptr(), entry.len());
        let len = u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]) as usize;
        let json = String::from_utf8_lossy(std::slice::from_raw_parts(ptr.add(LEN_PREFIX), len)).into_owned();
        kinn_free(ptr, LEN_PREFIX + len);
        json
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = "*mode = S;\nfunc main() { print(\"hi\"); }";

    #[test]
    fn version_encodes_0_2_0() {
        assert_eq!(kinn_version(), 0x00_02_00_00);
    }

    #[test]
    fn check_ok_returns_clean_json() {
        let json = check_json("main.kin", HELLO);
        assert!(json.contains("\"ok\":true"), "{}", json);
        assert!(json.contains("\"mode\":\"S\""), "{}", json);
        assert!(json.contains("\"diagnostics\":[]"), "{}", json);
    }

    #[test]
    fn check_reports_diagnostics_with_positions() {
        let json = check_json("main.kin", "func main() { let x = 1; x = 2; }");
        assert!(json.contains("\"ok\":false"), "{}", json);
        assert!(json.contains("E3004"), "{}", json);
        assert!(json.contains("\"line\""), "{}", json);
        // JSON-escaped message contains the variable name
        assert!(json.contains("immutable"), "{}", json);
    }

    #[test]
    fn check_text_renders_the_caret_style_output() {
        // Single file: text uses render_project with the entry source.
        let json = unsafe {
            let ptr = kinn_check_text(HELLO.as_ptr(), HELLO.len(), std::ptr::null(), 0);
            let len = u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]) as usize;
            let out = String::from_utf8_lossy(std::slice::from_raw_parts(ptr.add(LEN_PREFIX), len)).into_owned();
            kinn_free(ptr, LEN_PREFIX + len);
            out
        };
        assert_eq!(json, "\"\""); // no diagnostics -> empty text
    }

    #[test]
    fn run_captures_stdout() {
        let json = run_json("main.kin", HELLO);
        assert!(json.contains("\"ok\":true"), "{}", json);
        assert!(json.contains("\"exitCode\":0"), "{}", json);
        assert!(json.contains("\"stdout\":\"hi\\n\""), "{}", json);
    }

    #[test]
    fn run_reports_compile_errors() {
        let json = run_json("main.kin", "func main() { nope(1); }");
        assert!(json.contains("\"exitCode\":1"), "{}", json);
        assert!(json.contains("E3001"), "{}", json);
    }

    #[test]
    fn run_reports_runtime_errors_with_lines() {
        let json = run_json("main.kin", "func main() {\n    print(1 / 0);\n}");
        assert!(json.contains("\"exitCode\":3"), "{}", json);
        assert!(json.contains("E4001"), "{}", json);
        assert!(json.contains("\"line\":2"), "{}", json);
    }

    #[test]
    fn modules_register_and_resolve() {
        let math = "func add(a: int, b: int) -> int { return a + b; }".to_string();
        unsafe {
            kinn_set_module(
                "math.kin".as_ptr(),
                "math.kin".len(),
                math.as_ptr(),
                math.len(),
            );
        }
        let json = run_json(
            "main.kin",
            "import \"math.kin\" as math;\nfunc main() { print(math.add(2, 3)); }",
        );
        assert!(json.contains("\"ok\":true"), "{}", json);
        assert!(json.contains("\"stdout\":\"5\\n\""), "{}", json);
        kinn_clear_modules();
    }

    #[test]
    fn missing_module_is_an_e3017() {
        kinn_clear_modules();
        let json = run_json(
            "main.kin",
            "import \"absent.kin\" as a;\nfunc main() { }",
        );
        assert!(json.contains("E3017"), "{}", json);
    }

    #[test]
    fn alloc_free_roundtrip() {
        unsafe {
            let ptr = kinn_alloc(16);
            for i in 0..16u8 {
                *ptr.add(i as usize) = i;
            }
            let slice = std::slice::from_raw_parts(ptr, 16);
            assert_eq!(slice[7], 7);
            kinn_free(ptr, 16);
        }
    }

    #[test]
    fn unicode_sources_survive_the_boundary() {
        let json = run_json(
            "main.kin",
            "*mode = S;\nfunc main() { print(\"héllo → 世界\"); }",
        );
        assert!(json.contains("héllo → 世界"), "{}", json);
    }

    #[test]
    fn run_supports_the_full_0_2_language() {
        let src = "func main() {\n    let a = [1, 2, 3];\n    for x in a { print(x * 2); }\n    print(len(a) + abs(-4));\n    print(2 as float + 0.5);\n}";
        let json = run_json("main.kin", src);
        assert!(json.contains("\"ok\":true"), "{}", json);
        assert!(json.contains("\"stdout\":\"2\\n4\\n6\\n7\\n2.5\\n\""), "{}", json);
    }
}
