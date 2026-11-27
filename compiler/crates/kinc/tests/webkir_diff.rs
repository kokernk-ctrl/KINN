//! Differential testing: KIR → JS against the Rust VM (0.4.0 S5, C.3).
//!
//! Each corpus entry compiles once, then runs twice — `kinrt::run` (the
//! VM) and the generated JavaScript under Node — and both stdout and the
//! error line must match **byte for byte**. The corpus covers every
//! example program, the runtime-error examples, the modules pair,
//! hand-built KIR fixtures exercising every ISA instruction (including
//! the S5 string natives and the array `+` arm), synthesized edge cases
//! (overflow, division by zero, deep equality, unicode, float
//! formatting, array reference semantics) and app fixtures driving
//! handlers/computeds through generated driver `main`s.
//!
//! Skipped when `node` is absent (every GitHub runner has it; minimal
//! local environments still get the rest of the suite).

use std::path::{Path, PathBuf};
use std::process::Command;

use kinrt::kir::{Instr, ValKind};
use kinrt::{FuncDef, Module, Value};
use std::cell::RefCell;
use std::rc::Rc;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

fn node_available() -> bool {
    Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn have_node() -> bool {
    static HAVE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HAVE.get_or_init(node_available)
}

fn load_example(rel: &str) -> String {
    let p = repo_root().join("examples").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {}", p.display(), e))
}

fn compile_kin(src: &str) -> Module {
    kinc::driver::compile(src).expect("compile failed").module
}

/// One differential run: VM vs JS, stdout + error line byte-equal.
fn diff_case(name: &str, module: &Module, file: &str) {
    if !have_node() {
        eprintln!("SKIP (no node): {}", name);
        return;
    }
    // 1. VM run.
    let mut vm_out: Vec<u8> = Vec::new();
    let vm_err = kinrt::run(module, &mut vm_out).err();
    let vm_text = String::from_utf8(vm_out).expect("vm output utf8");
    let vm_err_line = vm_err.as_ref().map(|e| e.to_string()).unwrap_or_default();

    // 2. JS run under node.
    let dir = std::env::temp_dir().join(format!("kinn-webdiff-{}", sanitize(name)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    std::fs::write(dir.join("kinnrt.js"), kinc::backend::webkir::kinnrt_dist())
        .expect("write kinnrt");
    let js = kinc::backend::webkir::js_module(module, file);
    std::fs::write(dir.join("module.cjs"), js.as_bytes()).expect("write module");
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/webdiff_run.mjs");
    let out = Command::new("node")
        .arg(driver)
        .arg(dir.join("kinnrt.js"))
        .arg(dir.join("module.cjs"))
        .output()
        .expect("node run");
    let js_text = String::from_utf8(out.stdout).expect("js output utf8");
    let js_err_line = String::from_utf8_lossy(&out.stderr).trim_end().to_string();
    let js_exit = out.status.code().unwrap_or(-1);

    assert_eq!(
        vm_text, js_text,
        "STDOUT MISMATCH in `{}`:\n--- vm ---\n{}\n--- js ---\n{}\n--- generated module ---\n{}\n",
        name, vm_text, js_text, js
    );
    assert_eq!(
        vm_err_line.trim_end(),
        js_err_line,
        "ERROR LINE MISMATCH in `{}` (vm: {:?}, js: {:?})\n--- generated module ---\n{}\n",
        name, vm_err_line, js_err_line, js
    );
    // Exit-code parity: 0 for clean, 3 for a Kin runtime error.
    match (&vm_err, js_exit) {
        (None, 0) => {}
        (Some(_), 3) => {}
        (v, j) => panic!("EXIT MISMATCH in `{}`: vm error {:?}, node exit {}", name, v, j),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

// ===========================================================================
// Corpus: examples
// ===========================================================================

#[test]
fn webdiff_examples() {
    for name in [
        "hello.kin",
        "fibonacci.kin",
        "arrays.kin",
        "casts.kin",
        "conditions.kin",
        "floats.kin",
        "for_loops.kin",
        "loops.kin",
        "stdlib.kin",
        "strings.kin",
    ] {
        let src = load_example(name);
        let module = compile_kin(&src);
        diff_case(&format!("examples/{}", name), &module, name);
    }
}

#[test]
fn webdiff_errors() {
    for name in ["div_zero.kin", "index_oob.kin"] {
        let src = load_example(&format!("errors/{}", name));
        let module = compile_kin(&src);
        diff_case(&format!("errors/{}", name), &module, &format!("errors/{}", name));
    }
}

#[test]
fn webdiff_modules() {
    let main = load_example("modules/main.kin");
    let math = load_example("modules/math.kin");
    let (compiled, diags) = kinc::driver::compile_project(
        "main.kin",
        &main,
        &mut |path| {
            if path == "math.kin" {
                Ok(math.clone())
            } else {
                Err(format!("unknown module {}", path))
            }
        },
    );
    let c = compiled.unwrap_or_else(|| panic!("modules compile failed: {:?}", diags));
    diff_case("examples/modules", &c.module, "main.kin");
}

// ===========================================================================
// Corpus: hand-built KIR fixtures (every ISA instruction)
// ===========================================================================

fn fdef(
    name: &str,
    nparams: u16,
    nlocals: u16,
    code: Vec<Instr>,
    param_kinds: Vec<ValKind>,
    ret: ValKind,
) -> FuncDef {
    FuncDef {
        name: name.to_string(),
        nparams,
        nlocals,
        lines: vec![1; code.len()],
        code,
        param_kinds,
        ret_kind: ret,
    }
}

fn main_only(code: Vec<Instr>) -> Module {
    Module {
        funcs: vec![fdef("main", 0, 4, code, vec![], ValKind::Unit)],
        consts: Vec::new(),
    }
}

#[test]
fn webdiff_kir_scalars_and_print_tags() {
    let m = main_only(vec![
        Instr::Const(Value::Int(42)),
        Instr::Print,
        Instr::Const(Value::Float(2.5)),
        Instr::Const(Value::Float(1.25)),
        Instr::Add,
        Instr::Print, // 3.75
        Instr::Const(Value::Float(2.0)),
        Instr::Print, // 2.0
        Instr::Const(Value::Bool(true)),
        Instr::Print,
        Instr::Const(Value::Str("s".to_string())),
        Instr::Print,
    ]);
    diff_case("kir: scalars and print tags", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_comparisons() {
    let m = main_only(vec![
        Instr::Const(Value::Int(1)),
        Instr::Const(Value::Int(2)),
        Instr::Lt,
        Instr::Print, // true
        Instr::Const(Value::Int(1)),
        Instr::Const(Value::Int(2)),
        Instr::Gt,
        Instr::Print, // false
        Instr::Const(Value::Float(1.5)),
        Instr::Const(Value::Float(1.5)),
        Instr::Le,
        Instr::Print, // true
        Instr::Const(Value::Float(2.5)),
        Instr::Const(Value::Float(1.5)),
        Instr::Ge,
        Instr::Print, // true
        Instr::Const(Value::Bool(false)),
        Instr::Not,
        Instr::Print, // true
    ]);
    diff_case("kir: comparisons", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_deep_equality() {
    let a = Value::Array(Rc::new(RefCell::new(vec![
        Value::Int(1),
        Value::Array(Rc::new(RefCell::new(vec![Value::Str("x".to_string())]))),
    ])));
    let b = Value::Array(Rc::new(RefCell::new(vec![
        Value::Int(1),
        Value::Array(Rc::new(RefCell::new(vec![Value::Str("x".to_string())]))),
    ])));
    let c = Value::Array(Rc::new(RefCell::new(vec![Value::Int(1)])));
    let m = Module {
        funcs: vec![fdef(
            "main",
            0,
            0,
            vec![
                Instr::Const(a.clone()),
                Instr::Const(b.clone()),
                Instr::Eq,
                Instr::Print, // true (deep)
                Instr::Const(b.clone()),
                Instr::Const(c),
                Instr::Ne,
                Instr::Print, // true
            ],
            vec![],
            ValKind::Unit,
        )],
        consts: vec![("A".to_string(), a.clone())],
    };
    diff_case("kir: deep equality", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_array_concat_and_mutation() {
    let m = main_only(vec![
        Instr::Const(Value::Array(Rc::new(RefCell::new(vec![Value::Int(1)])))),
        Instr::Const(Value::Array(Rc::new(RefCell::new(vec![Value::Int(2)])))),
        Instr::Add,
        Instr::Store(0),
        Instr::Load(0),
        Instr::Print, // [1, 2]
        Instr::Load(0),
        Instr::Const(Value::Int(1)),
        Instr::Const(Value::Int(9)),
        Instr::ArrSet,
        Instr::Load(0),
        Instr::Print, // [1, 9]
    ]);
    diff_case("kir: array concat and mutation", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_string_natives() {
    let m = main_only(vec![
        Instr::Const(Value::Str("Kin 𐐨 ß".to_string())),
        Instr::Native(kinrt::NativeId::Upper),
        Instr::Print, // KIN 𐐀 ß
        Instr::Const(Value::Str("İNFO".to_string())),
        Instr::Native(kinrt::NativeId::Lower),
        Instr::Print, // İnfo
        Instr::Const(Value::Str(" \u{a0} padded \u{feff}\t".to_string())),
        Instr::Native(kinrt::NativeId::Trim),
        Instr::Print,
        Instr::Const(Value::Str("hello kinn".to_string())),
        Instr::Const(Value::Str("kinn".to_string())),
        Instr::Native(kinrt::NativeId::Contains),
        Instr::Print, // true
        Instr::Const(Value::Str("a,,b".to_string())),
        Instr::Const(Value::Str(",".to_string())),
        Instr::Native(kinrt::NativeId::Split),
        Instr::Print, // [a, , b]
        Instr::Const(Value::Str("a".to_string())),
        Instr::Const(Value::Str("b".to_string())),
        Instr::ArrNew(2),
        Instr::Const(Value::Str("-".to_string())),
        Instr::Native(kinrt::NativeId::Join),
        Instr::Print, // a-b
        Instr::Const(Value::Float(2.0)),
        Instr::Native(kinrt::NativeId::Str),
        Instr::Print, // 2.0
    ]);
    diff_case("kir: string natives", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_console_natives() {
    let m = main_only(vec![
        Instr::Const(Value::Str("hï𐐀".to_string())),
        Instr::Native(kinrt::NativeId::Len),
        Instr::Print, // 3
        Instr::Const(Value::Array(Rc::new(RefCell::new(vec![
            Value::Int(1),
            Value::Int(2),
        ])))),
        Instr::Native(kinrt::NativeId::Len),
        Instr::Print, // 2
        Instr::Const(Value::Int(-5)),
        Instr::Native(kinrt::NativeId::Abs),
        Instr::Print, // 5
        Instr::Const(Value::Float(-2.5)),
        Instr::Native(kinrt::NativeId::Abs),
        Instr::Print, // 2.5
        Instr::Const(Value::Int(3)),
        Instr::Const(Value::Int(9)),
        Instr::Native(kinrt::NativeId::Min),
        Instr::Print, // 3
        Instr::Const(Value::Float(2.5)),
        Instr::Const(Value::Float(f64::NAN)),
        Instr::Native(kinrt::NativeId::Max),
        Instr::Print, // 2.5 (Rust f64::max NaN rule)
        Instr::Const(Value::Str("ab".to_string())),
        Instr::Const(Value::Str("".to_string())),
        Instr::Native(kinrt::NativeId::Split),
        Instr::Native(kinrt::NativeId::Len),
        Instr::Print, // 2
    ]);
    diff_case("kir: console natives", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_casts() {
    let m = main_only(vec![
        Instr::Const(Value::Int(2)),
        Instr::Cast(kinrt::kir::CastTarget::Float),
        Instr::Print, // 2.0
        Instr::Const(Value::Float(-2.9)),
        Instr::Cast(kinrt::kir::CastTarget::Int),
        Instr::Print, // -2
        Instr::Const(Value::Float(1e300)),
        Instr::Cast(kinrt::kir::CastTarget::Int),
        // Saturated beyond ±2^53: printing the int itself would hit the
        // documented f64 precision limit, so cast back to float (the
        // documented exact round-trip) and print that.
        Instr::Cast(kinrt::kir::CastTarget::Float),
        Instr::Print, // 9223372036854775808
        Instr::Const(Value::Float(-1e300)),
        Instr::Cast(kinrt::kir::CastTarget::Int),
        Instr::Cast(kinrt::kir::CastTarget::Float),
        Instr::Print, // -9223372036854775808
        Instr::Const(Value::Float(0.5)),
        Instr::Cast(kinrt::kir::CastTarget::Str),
        Instr::Print, // 0.5
        Instr::Const(Value::Int(42)),
        Instr::Cast(kinrt::kir::CastTarget::Str),
        Instr::Print, // 42
    ]);
    diff_case("kir: casts", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_calls_and_recursion() {
    let fib = fdef(
        "fib",
        1,
        1,
        vec![
            Instr::Load(0),
            Instr::Const(Value::Int(2)),
            Instr::Lt,
            Instr::JumpIfFalse(6),
            Instr::Load(0),
            Instr::Return,
            Instr::Load(0), // 8
            Instr::Const(Value::Int(1)),
            Instr::Sub,
            Instr::Call(1),
            Instr::Load(0),
            Instr::Const(Value::Int(2)),
            Instr::Sub,
            Instr::Call(1),
            Instr::Add,
            Instr::Return,
        ],
        vec![ValKind::Int],
        ValKind::Int,
    );
    let mut main_code = vec![
        Instr::Const(Value::Int(10)),
        Instr::Call(1),
        Instr::Print, // 55
    ];
    main_code.push(Instr::Const(Value::Unit));
    main_code.push(Instr::Return);
    let main = fdef("main", 0, 0, main_code, vec![], ValKind::Unit);
    let m = Module { funcs: vec![main, fib], consts: Vec::new() };
    diff_case("kir: fib via calls", &m, "fixture.kin");

    // Infinite recursion → E4002 at the same call-site line.
    let rec = fdef(
        "rec",
        1,
        1,
        vec![
            Instr::Load(0),
            Instr::Const(Value::Int(1)),
            Instr::Sub,
            Instr::Call(1),
            Instr::Return,
        ],
        vec![ValKind::Int],
        ValKind::Int,
    );
    let mut main_code = vec![
        Instr::Const(Value::Int(100000)),
        Instr::Call(1),
        Instr::Print,
    ];
    main_code.push(Instr::Const(Value::Unit));
    main_code.push(Instr::Return);
    let main = fdef("main", 0, 0, main_code, vec![], ValKind::Unit);
    let m = Module { funcs: vec![main, rec], consts: Vec::new() };
    diff_case("kir: call depth overflow", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_overflow_family() {
    let cases: Vec<(&str, Vec<Instr>)> = vec![
        (
            "overflow add",
            vec![
                Instr::Const(Value::Int(i64::MAX)),
                Instr::Const(Value::Int(1)),
                Instr::Add,
                Instr::Print,
            ],
        ),
        (
            "overflow sub",
            vec![
                Instr::Const(Value::Int(i64::MIN)),
                Instr::Const(Value::Int(1)),
                Instr::Sub,
                Instr::Print,
            ],
        ),
        (
            "overflow neg",
            vec![Instr::Const(Value::Int(i64::MIN)), Instr::Neg, Instr::Print],
        ),
        (
            "overflow abs",
            vec![
                Instr::Const(Value::Int(i64::MIN)),
                Instr::Native(kinrt::NativeId::Abs),
                Instr::Print,
            ],
        ),
        (
            "overflow div",
            vec![
                Instr::Const(Value::Int(i64::MIN)),
                Instr::Const(Value::Int(-1)),
                Instr::Div,
                Instr::Print,
            ],
        ),
        (
            "overflow mod",
            vec![
                Instr::Const(Value::Int(i64::MIN)),
                Instr::Const(Value::Int(-1)),
                Instr::Mod,
                Instr::Print,
            ],
        ),
        (
            "overflow mul",
            vec![
                Instr::Const(Value::Int(3037000500)),
                Instr::Const(Value::Int(3037000500)),
                Instr::Mul,
                Instr::Print,
            ],
        ),
        (
            "int div zero",
            vec![
                Instr::Const(Value::Int(1)),
                Instr::Const(Value::Int(0)),
                Instr::Div,
                Instr::Print,
            ],
        ),
        (
            "float div zero",
            vec![
                Instr::Const(Value::Float(1.0)),
                Instr::Const(Value::Float(-0.0)),
                Instr::Div,
                Instr::Print,
            ],
        ),
        (
            "mod zero",
            vec![
                Instr::Const(Value::Int(1)),
                Instr::Const(Value::Int(0)),
                Instr::Mod,
                Instr::Print,
            ],
        ),
        (
            "index oob get",
            vec![
                Instr::Const(Value::Array(Rc::new(RefCell::new(vec![Value::Int(1)])))),
                Instr::Const(Value::Int(5)),
                Instr::ArrGet,
                Instr::Print,
            ],
        ),
        (
            "index oob set",
            vec![
                Instr::Const(Value::Array(Rc::new(RefCell::new(vec![Value::Int(1)])))),
                Instr::Const(Value::Int(-1)),
                Instr::Const(Value::Int(0)),
                Instr::ArrSet,
            ],
        ),
    ];
    for (name, code) in cases {
        let m = main_only(code);
        diff_case(&format!("kir: {}", name), &m, "fixture.kin");
    }
}

#[test]
fn webdiff_kir_float_formatting_zoo() {
    let m = main_only(vec![
        Instr::Const(Value::Float(0.1 + 0.2)),
        Instr::Print, // 0.30000000000000004
        Instr::Const(Value::Float(-0.0)),
        Instr::Print, // -0.0
        Instr::Const(Value::Float(1e16)),
        Instr::Print, // 10000000000000000
        Instr::Const(Value::Float(1e21)),
        Instr::Print, // plain, no exponent
        Instr::Const(Value::Float(1.5e-7)),
        Instr::Print, // 0.00000015
        Instr::Const(Value::Float(f64::INFINITY)),
        Instr::Print, // inf
        Instr::Const(Value::Float(f64::NEG_INFINITY)),
        Instr::Print, // -inf
        Instr::Const(Value::Float(f64::NAN)),
        Instr::Print, // NaN
        Instr::Const(Value::Float(1e15)),
        Instr::Print, // 1000000000000000.0
        Instr::Const(Value::Float(-1.5e-7)),
        Instr::Print, // -0.00000015
    ]);
    diff_case("kir: float formatting zoo", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_unicode_and_escapes() {
    let m = main_only(vec![
        Instr::Const(Value::Str("hï𐐀\n\t\"\\\u{1}".to_string())),
        Instr::Print,
        Instr::Const(Value::Str("".to_string())),
        Instr::Native(kinrt::NativeId::Len),
        Instr::Print, // 0
    ]);
    diff_case("kir: unicode and escapes", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_shared_array_const() {
    let shared = Value::Array(Rc::new(RefCell::new(vec![Value::Int(1)])));
    let m = Module {
        funcs: vec![fdef(
            "main",
            0,
            1,
            vec![
                Instr::Const(shared.clone()),
                Instr::Const(shared.clone()),
                Instr::Eq,
                Instr::Print, // true — same reference
                Instr::Const(shared.clone()),
                Instr::Const(Value::Int(0)),
                Instr::Const(Value::Int(7)),
                Instr::ArrSet,
                Instr::Const(shared.clone()),
                Instr::Print, // [7] — visible through every push
            ],
            vec![],
            ValKind::Unit,
        )],
        consts: vec![("SHARED".to_string(), shared)],
    };
    diff_case("kir: shared array const", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_loop_break_continue_and_unreachable() {
    let code = vec![
        Instr::Const(Value::Int(0)),  // 0
        Instr::Store(0),              // 1
        Instr::Load(0),               // 2  L0:
        Instr::Const(Value::Int(6)),  // 3
        Instr::Lt,                    // 4
        Instr::JumpIfFalse(21),       // 5  → END
        Instr::Load(0),               // 6
        Instr::Const(Value::Int(3)),  // 7
        Instr::Eq,                    // 8
        Instr::JumpIfFalse(13),       // 9  → INC (continue)
        Instr::Load(0),               // 10
        Instr::Print,                 // 11 prints 3
        Instr::Jump(21),              // 12 break → END
        Instr::Load(0),               // 13 INC:
        Instr::Const(Value::Int(1)),  // 14
        Instr::Add,                   // 15
        Instr::Store(0),              // 16
        Instr::Jump(2),               // 17 → L0
        Instr::Const(Value::Int(99)), // 18 unreachable but well-formed
        Instr::Print,                 // 19
        Instr::Jump(2),               // 20
        Instr::Const(Value::Unit),    // 21 END:
        Instr::Return,                // 22
    ];
    let m = Module {
        funcs: vec![fdef("main", 0, 1, code, vec![], ValKind::Unit)],
        consts: Vec::new(),
    };
    diff_case("kir: loop with break/continue", &m, "fixture.kin");
}

#[test]
fn webdiff_kir_sibling_scope_slot_reuse() {
    // P-003: if/else branches reuse the same slot for different-typed
    // variables — the analysis joins slot kinds instead of asserting.
    let src = r#"*mode = S;
func main() {
    if 1 < 2 {
        var t = 1;
        print(t);
    } else {
        var u = 2.5;
        print(u);
    }
    print("done");
    return;
}"#;
    let module = compile_kin(src);
    diff_case("kir: sibling scope slot reuse", &module, "fixture.kin");
}

// ===========================================================================
// Corpus: app fixtures (handler/computed ABI via driver mains)
// ===========================================================================

/// Build a driver `main` that: runs `app:init`, feeds the given state
/// params (by sorted name) from the init array, calls the handler
/// (supplying builtin args), and prints every write-back element through
/// per-element typed helpers.
fn driver_diff(
    name: &str,
    c: &kinc::driver::Compiled,
    handler: &str,
    state_params: &[&str], // sorted handler state params
) {
    let app = c.app.as_ref().expect("app model");
    let appf = kinc::backend::webkir::app_funcs(app, &c.module, "driver.kin").expect("app funcs");
    let mut all: Vec<FuncDef> = c.module.funcs.clone();
    all.extend(appf);

    let init_idx = all.iter().position(|f| f.name == "app:init").expect("init");
    let h_idx = all.iter().position(|f| f.name == handler).expect("handler fn");
    let h = &all[h_idx];
    let n_state = state_params.len();
    let n_builtins = h.param_kinds.len().saturating_sub(n_state);

    let mut main_code: Vec<Instr> = vec![Instr::Call(init_idx as u16), Instr::Store(0)];
    // state args, in the handler's (sorted) param order
    for p in state_params {
        let decl = app.state.iter().position(|s| &s.name == p).expect("state field");
        main_code.push(Instr::Load(0));
        main_code.push(Instr::Const(Value::Int(decl as i64)));
        main_code.push(Instr::ArrGet);
    }
    // builtin args after the state params
    for b in 0..n_builtins {
        match &h.param_kinds[n_state + b] {
            ValKind::Str => main_code.push(Instr::Const(Value::Str("typed text".to_string()))),
            ValKind::Int => main_code.push(Instr::Const(Value::Int(5))),
            ValKind::Bool => main_code.push(Instr::Const(Value::Bool(true))),
            _ => main_code.push(Instr::Const(Value::Int(0))),
        }
    }
    main_code.push(Instr::Call(h_idx as u16));
    main_code.push(Instr::Store(1));
    // print each write-back element through a typed helper
    for (i, _p) in state_params.iter().enumerate() {
        main_code.push(Instr::Load(1));
        main_code.push(Instr::Const(Value::Int(i as i64)));
        main_code.push(Instr::ArrGet);
        let helper_idx = all.len() + i;
        main_code.push(Instr::Call(helper_idx as u16));
    }
    main_code.push(Instr::Const(Value::Unit));
    main_code.push(Instr::Return);
    let main = fdef("main", 0, 2, main_code, vec![], ValKind::Unit);

    // typed print helpers
    for (i, p) in state_params.iter().enumerate() {
        let ty = &app.state.iter().find(|s| &s.name == *p).expect("state").ty;
        let kind = kinc::backend::webkir::kind_of_type(ty);
        let code = vec![Instr::Load(0), Instr::Print, Instr::Const(Value::Unit), Instr::Return];
        all.push(fdef(&format!("app:print{}", i), 1, 1, code, vec![kind], ValKind::Unit));
    }
    all.push(main);
    let module = Module { funcs: all, consts: c.module.consts.clone() };
    diff_case(name, &module, "driver.kin");
}

/// Drive a computed function: init, feed its params, call, print the
/// value through a typed helper.
fn driver_computed(name: &str, c: &kinc::driver::Compiled, computed: &str) {
    let app = c.app.as_ref().expect("app model");
    let appf = kinc::backend::webkir::app_funcs(app, &c.module, "driver.kin").expect("app funcs");
    let mut all: Vec<FuncDef> = c.module.funcs.clone();
    all.extend(appf);

    let init_idx = all.iter().position(|f| f.name == "app:init").expect("init");
    let cf_idx = all.iter().position(|f| f.name == computed).expect("computed fn");
    let cf = &all[cf_idx];
    // Params of the computed = sorted free names that are state fields.
    let comp = app.computed.iter().find(|c| c.kir == computed).expect("computed model");
    let mut params: Vec<String> = comp
        .expr
        .free_names()
        .into_iter()
        .filter(|n| app.state.iter().any(|s| &s.name == n))
        .collect();
    params.sort();

    let mut main_code: Vec<Instr> = vec![Instr::Call(init_idx as u16), Instr::Store(0)];
    for p in &params {
        let decl = app.state.iter().position(|s| &s.name == p).expect("state field");
        main_code.push(Instr::Load(0));
        main_code.push(Instr::Const(Value::Int(decl as i64)));
        main_code.push(Instr::ArrGet);
    }
    main_code.push(Instr::Call(cf_idx as u16));
    let kind = cf.ret_kind.clone();
    main_code.push(Instr::Call(all.len() as u16)); // print helper
    main_code.push(Instr::Const(Value::Unit));
    main_code.push(Instr::Return);
    let main = fdef("main", 0, 1, main_code, vec![], ValKind::Unit);
    let code = vec![Instr::Load(0), Instr::Print, Instr::Const(Value::Unit), Instr::Return];
    all.push(fdef("app:print0", 1, 1, code, vec![kind], ValKind::Unit));
    all.push(main);
    let module = Module { funcs: all, consts: c.module.consts.clone() };
    diff_case(name, &module, "driver.kin");
}

#[test]
fn webdiff_app_todo_handler_and_computed() {
    let src = r#"*mode = B;
app "Todo" {
    state {
        items: [string] = []
        done: [bool] = []
        count: int = 0
    }
    computed total = len(items) + count
    page Home {
        text "Items: " + str(len(items))
        button "Add" {
            on click {
                items = items + ["task"];
                done = done + [true];
                count = count + 1;
            }
        }
        button "Reset" {
            on click { count = 0; }
        }
    }
}"#;
    let c = kinc::driver::compile(src).expect("compile");
    driver_diff("app: todo add handler", &c, "app:Home:click:0", &["count", "done", "items"]);
    driver_diff("app: todo reset handler", &c, "app:Home:click:1", &["count"]);
    driver_computed("app: computed total", &c, "app:computed:total");
}

#[test]
fn webdiff_app_event_value_handler() {
    let src = r#"*mode = B;
app "Draft" {
    state { draft: string = "" n: int = 0 }
    page Home {
        input bind draft
        button "Type" {
            on input { draft = event_value; n = n + 1; }
        }
    }
}"#;
    let c = kinc::driver::compile(src).expect("compile");
    driver_diff("app: event_value handler", &c, "app:Home:input:0", &["draft", "n"]);
}

#[test]
fn webdiff_app_user_fn_and_const() {
    let src = r#"*mode = B;
const LIMIT: int = 3;

func bump(x: int) -> int {
    return x + 1;
}

app "Counter" {
    state { n: int = 0 }
    computed limit = LIMIT
    page Home {
        button "Go" { on click { n = bump(n); } }
    }
}"#;
    let c = kinc::driver::compile(src).expect("compile");
    driver_diff("app: user fn + const", &c, "app:Home:click:0", &["n"]);
    driver_computed("app: computed const", &c, "app:computed:limit");
}

#[test]
fn webdiff_app_index_assign_mutation() {
    // Index-assign mutates through the array reference: the write-back
    // carries the same (mutated) array; both engines print it identically.
    let src = r#"*mode = B;
app "Mut" {
    state { xs: [int] = [1, 2, 3] }
    page Home {
        button "Set" {
            on click {
                xs[1] = 20;
            }
        }
    }
}"#;
    let c = kinc::driver::compile(src).expect("compile");
    driver_diff("app: index assign mutation", &c, "app:Home:click:0", &["xs"]);
}

#[test]
fn webdiff_app_loops_and_branching() {
    let src = r#"*mode = B;
func sum(xs: [int]) -> int {
    var t = 0;
    for x in xs {
        if x > 1 {
            t = t + x;
        }
    }
    return t;
}

app "Loop" {
    state { xs: [int] = [1, 2, 3] total: int = 0 }
    computed derived = sum(xs) * 2
    page Home {
        button "Sum" {
            on click {
                total = sum(xs);
            }
        }
    }
}"#;
    let c = kinc::driver::compile(src).expect("compile");
    driver_diff("app: loops and branching", &c, "app:Home:click:0", &["total", "xs"]);
    driver_computed("app: computed derived", &c, "app:computed:derived");
}

#[test]
fn webdiff_app_concat_built_arrays_join_kinds() {
    // Regression (S6 Phase C): a handler that rebuilds an array through
    // an empty literal + concat chain inside a `for` loop, wrapped in an
    // `if`. The empty literal widens the slot to Array(Any) while the
    // param path carries Array(Str): the worklist analysis must JOIN the
    // kinds (monotone widening, array-aware) instead of panicking — and
    // the generated JS must still produce byte-identical output to the
    // VM through the shared concat arm.
    let src = r#"*mode = B;
app "Rebuild" {
    state { rows: [string] = ["a", "b", "c"] }
    page Home {
        button "Drop last" {
            on click {
                if len(rows) > 0 {
                    var next: [string] = [];
                    for i in 0..len(rows) - 1 {
                        next = next + [rows[i]];
                    }
                    rows = next;
                }
            }
        }
    }
}"#;
    let c = kinc::driver::compile(src).expect("compile");
    driver_diff("app: concat-built array joins kinds", &c, "app:Home:click:0", &["rows"]);
}

#[test]
fn webapp_dist_ships_kinnrt_css_and_links_it() {
    // S6 Phase C: the dynamic dist carries the predefined animation
    // stylesheet, links it from every page, and the animation attrs ride
    // the manifest templates.
    let src = r#"*mode = B;
app "Anim" {
    animation {
        transition "fade" { duration 300 }
        transition "scale" { duration 300 }
        keyframes "pop" { from { opacity 0 } to { opacity 1 } duration 300 }
    }
    transition "fade"
    state { rows: [string] = ["a"] }
    page Home {
        title "Anim"
        text "hi" enter "fade" exit "fade"
        button "B" hover "pop" {
            on click { rows = rows + ["x"]; }
        }
        each rows as row key row {
            text row scroll "scale"
        }
    }
}"#;
    let c = kinc::driver::compile(src).expect("compile");
    let app = c.app.as_ref().expect("app model");
    let files = kinc::backend::webapp::generate_app(app, &c.module, "anim.kin").expect("dist");
    let text = |name: &str| {
        let f = files.iter().find(|f| f.path == name).unwrap_or_else(|| panic!("{} emitted", name));
        String::from_utf8_lossy(&f.data).into_owned()
    };
    let css = text("kinnrt.css");
    assert!(css.contains(".kinn-enter-fade"));
    assert!(css.contains(".kinn-exit-slide-up"));
    assert!(css.contains(".kinn-hover-pop:hover"));
    assert!(css.contains(".kinn-scroll-scale.kinn-in"));
    assert!(css.contains("prefers-reduced-motion"));
    let index = text("index.html");
    assert!(index.contains("<link rel=\"stylesheet\" href=\"kinnrt.css\">"));
    let js = text("app.js");
    assert!(js.contains("\"enter\":\"fade\""));
    assert!(js.contains("\"exit\":\"fade\""));
    assert!(js.contains("\"scroll\":\"scale\""));
    // The app-level page transition rides the manifest.
    assert!(js.contains("\"transition\":\"fade\""));
}

// ===========================================================================
// Corpus: synthesized .kin edge cases in tests/fixtures/webdiff/
// ===========================================================================

#[test]
fn webdiff_synthesized() {
    let dir = repo_root().join("compiler/crates/kinc/tests/fixtures/webdiff");
    if !dir.exists() {
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("webdiff dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "kin").unwrap_or(false))
        .collect();
    entries.sort();
    for p in entries {
        let src = std::fs::read_to_string(&p).unwrap();
        let name = format!("webdiff/{}", p.file_name().unwrap().to_string_lossy());
        let module = compile_kin(&src);
        diff_case(&name, &module, &name);
    }
}

// ===========================================================================
// Determinism and budget gates
// ===========================================================================

#[test]
fn webdiff_determinism() {
    // Byte-equal rebuild: same module in, same JS out.
    let src = load_example("fibonacci.kin");
    let module = compile_kin(&src);
    let a = kinc::backend::webkir::js_module(&module, "fibonacci.kin");
    let b = kinc::backend::webkir::js_module(&module, "fibonacci.kin");
    assert_eq!(a, b);
}

#[test]
fn kinnrt_size_budget() {
    // The delivered (minified) runtime is the budgeted artifact.
    assert!(
        kinc::backend::webkir::kinnrt_dist().len() <= kinc::backend::webkir::KINNRT_BUDGET,
        "minified kinnrt.js is {} bytes, budget is {}",
        kinc::backend::webkir::kinnrt_dist().len(),
        kinc::backend::webkir::KINNRT_BUDGET
    );
}

#[test]
fn webdiff_module_kinds_json() {
    // The app ABI surface used by the manifest: param_kinds/ret_kind ride
    // on FuncDef and survive the full pipeline.
    let src = r#"*mode = B;
func add(a: int, b: float) -> float {
    return a as float + b;
}
func main() {
    print(add(1, 2.5));
}"#;
    let module = compile_kin(src);
    let add = module.funcs.iter().find(|f| f.name == "add").unwrap();
    assert_eq!(add.param_kinds, vec![ValKind::Int, ValKind::Float]);
    assert_eq!(add.ret_kind, ValKind::Float);
}
