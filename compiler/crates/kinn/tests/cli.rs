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
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "kinn 0.3.0");
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

// -- 0.3.0 -------------------------------------------------------------------

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

// -- 0.3.0 build targets ------------------------------------------------------

fn tempdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kinn-cli-test-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn build_web_generates_a_real_dist() {
    let out = kinn(&[
        "build",
        &format!("{}/web/site.kin", EXAMPLES),
        "--target",
        "web",
        "--out",
        "/tmp/kinn-cli-test-web-dist",
    ]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("built web site \"MyWebsite\" (2 pages)"));
    assert!(stdout.contains("index.html"));
    assert!(stdout.contains("styles.css"));
    let dist = std::path::Path::new("/tmp/kinn-cli-test-web-dist");
    for f in ["index.html", "about.html", "styles.css", "app.js"] {
        assert!(dist.join(f).exists(), "missing {}", f);
    }
    let index = std::fs::read_to_string(dist.join("index.html")).unwrap();
    assert!(index.contains("<title>Welcome to KINN · MyWebsite</title>"));
    assert!(index.contains("aria-current=\"page\""));
}

#[test]
fn build_android_exports_a_gradle_project() {
    let dir = tempdir("android");
    let out = kinn(&[
        "build",
        &format!("{}/web/site.kin", EXAMPLES),
        "--target",
        "android",
        "--out",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let manifest = std::fs::read_to_string(dir.join("app/src/main/AndroidManifest.xml")).unwrap();
    assert!(!manifest.contains("uses-permission"));
    let kt = std::fs::read_to_string(
        dir.join("app/src/main/java/dev/kinn/site/mywebsite/MainActivity.kt"),
    )
    .unwrap();
    assert!(kt.contains("class MainActivity"));
    assert!(dir.join("README.md").exists());
}

#[test]
fn build_without_target_defaults_to_web() {
    let dir = tempdir("default");
    let out = kinn(&[
        "build",
        &format!("{}/web/site.kin", EXAMPLES),
        "--out",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert!(dir.join("index.html").exists());
}

#[test]
fn build_rejects_unknown_targets_and_lists_supported() {
    let out = kinn(&[
        "build",
        &format!("{}/web/site.kin", EXAMPLES),
        "--target",
        "ios",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown build target `ios`"));
    // The supported-target list is printed to stdout (targets_command).
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("web"));
    assert!(stdout.contains("android"));
}

#[test]
fn build_on_a_console_program_explains_the_missing_website() {
    let dir = tempdir("nosite");
    let out = kinn(&[
        "build",
        &format!("{}/hello.kin", EXAMPLES),
        "--out",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("declares no `website`"));
}

#[test]
fn run_of_a_website_only_program_gives_guidance() {
    let out = kinn(&["run", &format!("{}/web/site.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("website \"MyWebsite\""));
    assert!(stderr.contains("--target web"));
}

#[test]
fn targets_command_lists_both_targets() {
    let out = kinn(&["targets"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("web"));
    assert!(stdout.contains("android"));
}

#[test]
fn build_reads_the_kinn_toml_manifest() {
    let dir = tempdir("manifest");
    std::fs::write(
        dir.join("kinn.toml"),
        "name = \"manifest-test\"\nentry = \"site.kin\"\ntarget = \"web\"\n",
    )
    .unwrap();
    let site = std::fs::read_to_string(format!("{}/web/site.kin", EXAMPLES)).unwrap();
    std::fs::write(dir.join("site.kin"), site).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kinn"))
        .args(["build"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("project `manifest-test`"));
    assert!(dir.join("build/web/index.html").exists());
}

#[test]
fn android_export_degrades_honestly_without_a_toolchain() {
    // The local android build provider must degrade honestly when
    // Gradle/the SDK are absent (they are absent in CI and sandboxes).
    let missing = kinn(&["build", &format!("{}/web/site.kin", EXAMPLES), "--target", "android", "--out", "/tmp/kinn-cli-test-provider"]);
    assert_eq!(missing.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&missing.stdout);
    assert!(stdout.contains("status: project exported"), "{}", stdout);
}
