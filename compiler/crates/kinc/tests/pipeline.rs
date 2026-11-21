//! End-to-end pipeline tests: source text through compilation and VM
//! execution, plus verification of every `examples/*.kin` program against
//! its `// expect:` output block.

use kinc::ast::Mode;
use kinc::diagnostics;
use kinc::driver;
use kinrt::vm;

fn run_src(src: &str) -> String {
    let c = driver::compile(src).expect("compile failed");
    let mut buf = Vec::new();
    vm::run(&c.module, &mut buf).expect("vm failed");
    String::from_utf8(buf).unwrap()
}

/// Extract the `// expect:` block: every following `//`-prefixed line is
/// one expected output line (`//` alone means an empty line).
fn expected_output(src: &str) -> Option<Vec<String>> {
    let mut expected: Option<Vec<String>> = None;
    for line in src.lines() {
        if expected.is_none() {
            if line.trim_end() == "// expect:" {
                expected = Some(Vec::new());
            }
        } else if let Some(rest) = line.strip_prefix("//") {
            expected
                .as_mut()
                .unwrap()
                .push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        } else {
            break;
        }
    }
    expected
}

const EXAMPLES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../examples");

#[test]
fn every_expect_example_compiles_and_runs() {
    let mut verified = 0;
    for entry in std::fs::read_dir(EXAMPLES_DIR).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("kin") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        if let Some(expected) = expected_output(&src) {
            let out = run_src(&src);
            let produced: Vec<&str> = out.trim_end_matches('\n').split('\n').collect();
            assert_eq!(
                produced,
                expected,
                "example {} produced wrong output",
                path.display()
            );
            verified += 1;
        }
    }
    assert!(verified >= 6, "expected at least 6 verified examples, found {}", verified);
}

#[test]
fn error_examples_fail_with_the_declared_code() {
    let dir = format!("{}/errors", EXAMPLES_DIR);
    let mut verified = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("kin") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        // Files marked `// runtime-error:` compile cleanly; they are
        // exercised by the CLI tests instead.
        if src.lines().any(|l| l.starts_with("// runtime-error:")) {
            continue;
        }
        let code = src
            .lines()
            .find_map(|l| l.strip_prefix("// error: "))
            .map(|s| s.trim().to_string())
            .expect("every file in examples/errors must declare // error: CODE or // runtime-error: CODE");
        let diags = driver::compile(&src).expect_err("error example must fail to compile");
        let rendered =
            diagnostics::render(&diags, &src, path.to_str().unwrap(), Mode::S);
        assert!(
            rendered.contains(&code),
            "expected {} in diagnostics for {}:\n{}",
            code,
            path.display(),
            rendered
        );
        verified += 1;
    }
    assert!(verified >= 2, "expected at least 2 error examples, found {}", verified);
}

#[test]
fn recursion_fibonacci_15() {
    let src = "*mode = A;\n\
               func fib(n: int) -> int {\n    \
               if n < 2 { return n; }\n    \
               return fib(n - 1) + fib(n - 2);\n\
               }\n\
               func main() { print(fib(15)); }\n";
    assert_eq!(run_src(src), "610\n");
}

#[test]
fn short_circuit_evaluation_avoids_division_by_zero() {
    // If `and` were eager, `100 / x` would divide by zero.
    let src = "func main() {\n    \
               let x = 0;\n    \
               if x != 0 and 100 / x > 1 { print(\"unreachable\"); }\n    \
               print(\"safe\");\n\
               }\n";
    assert_eq!(run_src(src), "safe\n");
}

#[test]
fn scoping_and_shadowing() {
    let src = "func main() {\n    \
               let x = 1;\n    \
               {\n        let x = 2.5;\n        print(x);\n    }\n    \
               print(x);\n\
               }\n";
    assert_eq!(run_src(src), "2.5\n1\n");
}

#[test]
fn nested_calls_and_multiple_arguments() {
    let src = "func add(a: int, b: int) -> int { return a + b; }\n\
               func main() { print(add(add(1, 2), add(3, 4))); }\n";
    assert_eq!(run_src(src), "10\n");
}

#[test]
fn string_concat_and_equality() {
    let src = "func main() {\n    \
               let s = \"KIN\" + \"N\";\n    \
               print(s);\n    \
               if s == \"KINN\" { print(\"match\"); }\n\
               }\n";
    assert_eq!(run_src(src), "KINN\nmatch\n");
}

#[test]
fn runtime_division_by_zero_reports_line() {
    let src = "func main() {\n    let a = 1;\n    let b = 0;\n    print(a / b);\n}\n";
    let c = driver::compile(src).expect("compiles");
    let mut buf = Vec::new();
    let err = vm::run(&c.module, &mut buf).expect_err("must fail at runtime");
    assert_eq!(err.code, "E4001");
    assert_eq!(err.line, 4);
}

#[test]
fn beginner_mode_lint_survives_the_pipeline() {
    let src = "*mode = B;\nfunc main() { var x = 1; print(x); }";
    let (compiled, diags) = driver::compile_full(src);
    assert!(compiled.is_some());
    assert!(diags.iter().any(|d| d.code == "N3001"));
}

#[test]
fn mode_is_reported_on_the_compiled_program() {
    let c = driver::compile("*mode = A;\nfunc main() { }").unwrap();
    assert_eq!(c.mode, Mode::A);
    let c = driver::compile("func main() { }").unwrap();
    assert_eq!(c.mode, Mode::S);
}

// -- 0.2.0 end-to-end --------------------------------------------------------

#[test]
fn while_true_divergence_compiles_and_runs() {
    // The exact program from PROBLEM_REGISTRY P-002: no false E3012.
    let src = "func spin() -> int { while true { return 1; } }\nfunc main() { print(spin()); }";
    assert_eq!(run_src(src), "1\n");
}

#[test]
fn slots_are_reused_after_scope_exit() {
    // P-003: three sequential one-variable scopes must share one slot.
    let src = "func main() {\n    { let a = 1; print(a); }\n    { let b = 2; print(b); }\n    { let c = 3; print(c); }\n}";
    let c = driver::compile(src).expect("compiles");
    let main = &c.module.funcs[c.module.find("main").unwrap()];
    assert_eq!(main.nlocals, 1, "expected slot reuse, got {} locals", main.nlocals);
    assert_eq!(run_src(src), "1\n2\n3\n");
}

#[test]
fn for_loops_over_ranges_and_arrays() {
    let src = "func main() { for i in 0..5 { print(i); } for i in 1..=3 { print(i); } }";
    assert_eq!(run_src(src), "0\n1\n2\n3\n4\n1\n2\n3\n");

    let src = "func main() { for s in [\"a\", \"b\"] { print(s); } }";
    assert_eq!(run_src(src), "a\nb\n");
}

#[test]
fn for_continue_jumps_to_the_increment() {
    // If `continue` jumped to the condition without incrementing, this
    // would loop forever.
    let src = "func main() {\n    for i in 0..6 {\n        if i % 2 == 0 { continue; }\n        print(i);\n    }\n}";
    assert_eq!(run_src(src), "1\n3\n5\n");
}

#[test]
fn for_break_exits_the_loop() {
    let src = "func main() { for i in 0..100 { if i == 3 { break; } print(i); } }";
    assert_eq!(run_src(src), "0\n1\n2\n");
}

#[test]
fn arrays_end_to_end() {
    let src = "func main() {\n    let a = [1, 2];\n    let b = a;\n    b[0] = 9;\n    print(a[0]);\n    if a == [9, 2] { print(true); }\n    print(len(a) + len(\"ab\"));\n}";
    assert_eq!(run_src(src), "9\ntrue\n4\n");
}

#[test]
fn array_bounds_errors_carry_the_line() {
    let src = "func main() {\n    let a = [1];\n    print(a[3]);\n}";
    let c = driver::compile(src).expect("compiles");
    let mut buf = Vec::new();
    let err = vm::run(&c.module, &mut buf).expect_err("must fail");
    assert_eq!(err.code, "E4005");
    assert_eq!(err.line, 3);
}

#[test]
fn arrays_pass_through_functions_by_reference() {
    let src = "func fill(xs: [int], v: int) { xs[0] = v; }\nfunc main() { let a = [0, 0]; fill(a, 7); print(a[0]); }";
    assert_eq!(run_src(src), "7\n");
}

#[test]
fn casts_end_to_end() {
    let src = "func main() { print(2 as float); print(2.9 as int); print(42 as string); print(true as string); }";
    assert_eq!(run_src(src), "2.0\n2\n42\ntrue\n");
}

#[test]
fn natives_end_to_end() {
    let src = "func main() { print(abs(-5)); print(abs(-1.5)); print(min(4, 2)); print(max(1.5, 2.5)); print(len(\"héll\")); }";
    assert_eq!(run_src(src), "5\n1.5\n2\n2.5\n4\n");
}

#[test]
fn modules_link_and_run() {
    let dir = format!("{}/modules", EXAMPLES_DIR);
    let mut loader = |path: &str| std::fs::read_to_string(path).map_err(|e| e.to_string());
    let entry = format!("{}/main.kin", dir);
    let entry_src = std::fs::read_to_string(&entry).unwrap();
    let (compiled, diags) = kinc::driver::compile_project(&entry, &entry_src, &mut loader);
    let compiled = compiled.expect("module example must compile");
    let errs: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert!(errs.is_empty(), "{:#?}", errs);

    let mut buf = Vec::new();
    vm::run(&compiled.module, &mut buf).expect("module example must run");
    let out = String::from_utf8(buf).unwrap();
    assert_eq!(out, "7\n25\n5\n15\n");

    // Imported functions appear under qualified names in the module.
    assert!(compiled.module.find("math::add").is_some());
    assert!(compiled.module.find("triple").is_some());
    assert!(compiled.module.find("main").is_some());
}

#[test]
fn multiple_syntax_errors_are_reported_through_the_driver() {
    let src = "func main() {\n    let x = ;\n    let y = ;\n}\n";
    let diags = driver::compile(src).expect_err("must fail");
    let errs: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    assert_eq!(errs.len(), 2, "{:#?}", diags);
}
