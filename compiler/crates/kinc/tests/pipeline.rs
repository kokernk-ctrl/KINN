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
