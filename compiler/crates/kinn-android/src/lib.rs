//! kinn-android — JNI glue for the KINN Studio Android app.
//!
//! Kotlin side (`dev.kinn.studio.KinnBridge`):
//!
//! ```kotlin
//! object KinnBridge {
//!     init { System.loadLibrary("kinn_android") }   // try/catch for graceful degradation
//!     external fun nVersion(): Int
//!     external fun nCheck(entry: String, src: String, paths: Array<String>, contents: Array<String>): String
//!     external fun nRun(entry: String, src: String, paths: Array<String>, contents: Array<String>): String
//!     external fun nBuildWeb(entry: String, src: String, paths: Array<String>, contents: Array<String>): String
//!     external fun nBuildAndroid(entry: String, src: String, paths: Array<String>, contents: Array<String>): String
//! }
//! ```
//!
//! Results are the same JSON as the CLI/Studio FFI (`kinn_check` /
//! `kinn_run`): `{"ok":bool,"exitCode":int,"stdout":str,"diagnostics":[...]}`.
//! Modules are registered per call and cleared afterwards, so concurrent
//! calls from the UI thread stay consistent.
//!
//! This crate is built only for Android targets (`aarch64-linux-android`)
//! by the CI pipeline; it contains no compiler logic.

use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::{jint, jstring};
use jni::JNIEnv;

const ERROR_PREFIX: &str = "JNI bridge error";

/// Read a Java String as Rust UTF-8 (the jni crate handles the conversion).
fn rust_string(env: &mut JNIEnv, s: &JString) -> Result<String, String> {
    let java_str = env
        .get_string(s)
        .map_err(|e| format!("{}: get_string: {}", ERROR_PREFIX, e))?;
    Ok(String::from(java_str))
}

/// Read a Java String[] into Rust Vec<String>.
fn rust_string_array(env: &mut JNIEnv, array: &JObjectArray) -> Result<Vec<String>, String> {
    let len = env
        .get_array_length(array)
        .map_err(|e| format!("{}: array length: {}", ERROR_PREFIX, e))?;
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let elem = env
            .get_object_array_element(array, i)
            .map_err(|e| format!("{}: array element {}: {}", ERROR_PREFIX, i, e))?;
        let js: JString = elem.into();
        out.push(rust_string(env, &js)?);
    }
    Ok(out)
}

/// Return a Java String, or an error JSON when construction fails.
fn java_string(env: &mut JNIEnv, text: &str) -> jstring {
    match env.new_string(text) {
        Ok(s) => s.into_raw(),
        Err(_) => {
            // Last resort: a plain ASCII message that cannot fail to encode.
            let fallback = "{\"ok\":false,\"exitCode\":1,\"stdout\":\"\",\"diagnostics\":[{\"severity\":\"error\",\"code\":\"E5002\",\"message\":\"JNI result conversion failed\",\"file\":\"\",\"line\":0,\"col\":0,\"len\":0}]}";
            env.new_string(fallback).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
        }
    }
}

fn error_json(message: &str) -> String {
    // Escape the message minimally for embedding into the JSON string.
    let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "{{\"ok\":false,\"exitCode\":1,\"stdout\":\"\",\"diagnostics\":[{{\"severity\":\"error\",\"code\":\"E5002\",\"message\":\"{}\",\"file\":\"\",\"line\":0,\"col\":0,\"len\":0}}]}}",
        escaped
    )
}

/// Register modules, run the FFI call, always clear the registry.
fn with_modules(
    paths: &[String],
    contents: &[String],
    f: impl FnOnce() -> *mut u8,
) -> Result<String, String> {
    if paths.len() != contents.len() {
        return Err(format!("{}: paths and contents differ in length", ERROR_PREFIX));
    }
    unsafe {
        for (p, c) in paths.iter().zip(contents.iter()) {
            kinn_ffi::kinn_set_module(p.as_ptr(), p.len(), c.as_ptr(), c.len());
        }
    }
    let ptr = f();
    // Read the length-prefixed buffer, then free it.
    let json = unsafe {
        if ptr.is_null() {
            return Err(format!("{}: null result pointer", ERROR_PREFIX));
        }
        let len = u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]) as usize;
        let bytes = std::slice::from_raw_parts(ptr.add(4), len);
        let json = String::from_utf8_lossy(bytes).into_owned();
        kinn_ffi::kinn_free(ptr, len + 4);
        json
    };
    kinn_ffi::kinn_clear_modules();
    Ok(json)
}

fn call_ffi<'local>(
    env: &mut JNIEnv<'local>,
    run: Option<bool>,
    entry: JString<'local>,
    src: JString<'local>,
    paths: JObjectArray<'local>,
    contents: JObjectArray<'local>,
) -> jstring {
    let result: Result<String, String> = (|| {
        let entry_s = rust_string(env, &entry)?;
        let src_s = rust_string(env, &src)?;
        let paths_v = rust_string_array(env, &paths)?;
        let contents_v = rust_string_array(env, &contents)?;
        with_modules(&paths_v, &contents_v, || unsafe {
            match run {
                Some(true) => kinn_ffi::kinn_run(src_s.as_ptr(), src_s.len(), entry_s.as_ptr(), entry_s.len()),
                Some(false) => kinn_ffi::kinn_check(src_s.as_ptr(), src_s.len(), entry_s.as_ptr(), entry_s.len()),
                None => kinn_ffi::kinn_build_web(src_s.as_ptr(), src_s.len(), entry_s.as_ptr(), entry_s.len()),
            }
        })
    })();
    match result {
        Ok(json) => java_string(env, &json),
        Err(msg) => java_string(env, &error_json(&msg)),
    }
}

fn call_ffi_android<'local>(
    env: &mut JNIEnv<'local>,
    entry: JString<'local>,
    src: JString<'local>,
    paths: JObjectArray<'local>,
    contents: JObjectArray<'local>,
) -> jstring {
    let result: Result<String, String> = (|| {
        let entry_s = rust_string(env, &entry)?;
        let src_s = rust_string(env, &src)?;
        let paths_v = rust_string_array(env, &paths)?;
        let contents_v = rust_string_array(env, &contents)?;
        with_modules(&paths_v, &contents_v, || unsafe {
            kinn_ffi::kinn_build_android(src_s.as_ptr(), src_s.len(), entry_s.as_ptr(), entry_s.len())
        })
    })();
    match result {
        Ok(json) => java_string(env, &json),
        Err(msg) => java_string(env, &error_json(&msg)),
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_kinn_studio_KinnBridge_nVersion(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    kinn_ffi::kinn_version() as jint
}

#[no_mangle]
pub extern "system" fn Java_dev_kinn_studio_KinnBridge_nCheck<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    entry: JString<'local>,
    src: JString<'local>,
    paths: JObjectArray<'local>,
    contents: JObjectArray<'local>,
) -> jstring {
    call_ffi(&mut env, Some(false), entry, src, paths, contents)
}

#[no_mangle]
pub extern "system" fn Java_dev_kinn_studio_KinnBridge_nRun<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    entry: JString<'local>,
    src: JString<'local>,
    paths: JObjectArray<'local>,
    contents: JObjectArray<'local>,
) -> jstring {
    call_ffi(&mut env, Some(true), entry, src, paths, contents)
}

#[no_mangle]
pub extern "system" fn Java_dev_kinn_studio_KinnBridge_nBuildWeb<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    entry: JString<'local>,
    src: JString<'local>,
    paths: JObjectArray<'local>,
    contents: JObjectArray<'local>,
) -> jstring {
    call_ffi(&mut env, None, entry, src, paths, contents)
}

#[no_mangle]
pub extern "system" fn Java_dev_kinn_studio_KinnBridge_nBuildAndroid<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    entry: JString<'local>,
    src: JString<'local>,
    paths: JObjectArray<'local>,
    contents: JObjectArray<'local>,
) -> jstring {
    call_ffi_android(&mut env, entry, src, paths, contents)
}
