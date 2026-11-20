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
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "kinn 0.1.0");
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
