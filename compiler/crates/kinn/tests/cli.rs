//! CLI integration tests: run the real `kinn` binary against the example
//! programs and assert exit codes and output.

use std::process::Command;

fn kinn(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_kinn")).args(args).output().unwrap()
}

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../examples");

#[test]
fn run_hello_exits_zero_with_expected_output() {
    let out = kinn(&["run", &format!("{}/hello.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout, "Hello, Kin!\nworld\n42\n");
}

#[test]
fn run_fibonacci_exits_zero() {
    let out = kinn(&["run", &format!("{}/fibonacci.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.lines().last(), Some("55"));
}

#[test]
fn check_clean_example_exits_zero() {
    let out = kinn(&["check", &format!("{}/hello.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8(out.stdout).unwrap().contains("OK"));
}

#[test]
fn check_compile_error_exits_one_with_code() {
    let out = kinn(&["check", &format!("{}/errors/immutable.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("E3004"));
    // Beginner mode adds the N3002 "declared here" note.
    assert!(stderr.contains("N3002"));
    assert!(stderr.contains("-->"));
    assert!(stderr.contains("^"));
}

#[test]
fn run_runtime_error_exits_three_with_code() {
    let out = kinn(&["run", &format!("{}/errors/div_zero.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("E4001"));
    assert!(stderr.contains("-->"));
}

#[test]
fn missing_file_exits_two() {
    let out = kinn(&["run", "/definitely/not/here.kin"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot read"));
}

#[test]
fn unknown_command_exits_two() {
    let out = kinn(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown command"));
}

#[test]
fn command_without_file_exits_two() {
    let out = kinn(&["run"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("needs a file argument"));
}

#[test]
fn version_prints_version() {
    let out = kinn(&["version"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "kinn 0.2.0");
}

#[test]
fn tokens_prints_the_stream() {
    let out = kinn(&["tokens", &format!("{}/hello.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("STAR"));
    assert!(stdout.contains("FUNC"));
    assert!(stdout.contains("IDENT"));
    assert!(stdout.contains("STR"));
    assert!(stdout.contains("EOF"));
}

#[test]
fn ast_prints_the_tree() {
    let out = kinn(&["ast", &format!("{}/hello.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("program (mode: B)"));
    assert!(stdout.contains("func main"));
}

#[test]
fn kir_prints_the_disassembly() {
    let out = kinn(&["kir", &format!("{}/fibonacci.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("func main"));
    assert!(stdout.contains("func fib"));
    assert!(stdout.contains("Call"));
    assert!(stdout.contains("JumpIfFalse"));
}

// -- 0.2.0 -------------------------------------------------------------------

#[test]
fn run_arrays_example() {
    let out = kinn(&["run", &format!("{}/arrays.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "40\n60\n3\n99\n0\ntrue\n");
}

#[test]
fn run_for_loops_example() {
    let out = kinn(&["run", &format!("{}/for_loops.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("0\n1\n2\n3\n4\n"));
    assert!(stdout.ends_with("sum: 60\n"));
}

#[test]
fn run_casts_and_stdlib_examples() {
    let out = kinn(&["run", &format!("{}/casts.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "2.0\n2\n-2\n42\n2.0\ntrue\n7.5\n");

    let out = kinn(&["run", &format!("{}/stdlib.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "5\n3\n7\n2.5\n3\n9\n1.5\n");
}

#[test]
fn run_multi_file_module_project() {
    let out = kinn(&["run", &format!("{}/modules/main.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "7\n25\n5\n15\n");

    // `kir` shows the qualified imported function names.
    let out = kinn(&["kir", &format!("{}/modules/main.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("func math::add"));
    assert!(stdout.contains("func main"));
}

#[test]
fn missing_import_is_reported_with_the_importer_position() {
    let out = kinn(&["check", &format!("{}/errors/import_missing.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("E3017"));
    assert!(stderr.contains("no_such_module.kin"));
}

#[test]
fn parser_recovery_reports_all_errors_through_check() {
    let out = kinn(&["check", &format!("{}/errors/many_errors.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(stderr.matches("error[").count(), 3, "stderr: {}", stderr);
}

#[test]
fn array_bounds_runtime_error_exits_three() {
    let out = kinn(&["run", &format!("{}/errors/index_oob.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("E4005"));
    assert!(stderr.contains("-->"));
}
