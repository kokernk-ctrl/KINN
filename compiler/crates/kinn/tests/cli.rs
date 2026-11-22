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
fn check_legacy_let_reports_the_e2017_migration() {
    // ADR-0029: `let` was renamed to `fin`; the legacy spelling gets a
    // positioned migration diagnostic naming the new keyword.
    let out = kinn(&["check", &format!("{}/errors/legacy_let.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("E2017"));
    assert!(stderr.contains("`let` was renamed to `fin`; use `fin`"));
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
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "7\n25\n5\n15\n95\n");

    // `kir` shows the qualified imported function names and the folded
    // constants (0.4.0 S1): the entry const keeps its plain name, the
    // imported one is qualified with its module stem.
    let out = kinn(&["kir", &format!("{}/modules/main.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("func math::add"));
    assert!(stdout.contains("func main"));
    assert!(stdout.contains("const math::MAX = 100"), "{}", stdout);
    assert!(stdout.contains("const BASE = 5"), "{}", stdout);
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
fn ast_command_dumps_the_dynamic_app_grammar() {
    // 0.4.0 S2: `kinn ast` shows the full app tree (the checkable
    // surface of the grammar; see examples/web/tasks_app.kin).
    let out = kinn(&["ast", &format!("{}/web/tasks_app.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("app \"TaskTrack\""));
    assert!(stdout.contains("navigation spa"));
    assert!(stdout.contains("not_found Missing"));
    assert!(stdout.contains("keyframes \"pop\""));
    assert!(stdout.contains("store \"tasks\" (items, done, filter)"));
    assert!(stdout.contains("route \"item/:id\" -> ItemDetail"));
    assert!(stdout.contains("each items as item key item"));
    assert!(stdout.contains("TaskRow(label: item)"));
    assert!(stdout.contains("field draft kind text"));
    assert!(stdout.contains("on key \"Escape\""));
    assert!(stdout.contains("component TaskRow"));
    assert!(stdout.contains("prop label: string"));
}

#[test]
fn check_app_example_passes_grammar_and_presence_rules() {
    // S2 scope: the example is grammatically valid and satisfies the
    // presence rules (one app, entry file, no main needed).
    let out = kinn(&["check", &format!("{}/web/tasks_app.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8(out.stdout).unwrap().contains("OK"));
}

#[test]
fn build_on_an_app_program_reports_the_honest_guard() {
    // No pretending: dynamic app builds are not implemented in S2, and
    // the CLI says exactly that instead of silently ignoring the app.
    let dir = tempdir("appguard");
    let out = kinn(&[
        "build",
        &format!("{}/web/tasks_app.kin", EXAMPLES),
        "--target",
        "web",
        "--out",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("building dynamic apps is not implemented yet"));
    assert!(stderr.contains("app (\"TaskTrack\")"));
}

#[test]
fn run_of_an_app_only_program_gives_guidance() {
    let out = kinn(&["run", &format!("{}/web/tasks_app.kin", EXAMPLES)]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("app \"TaskTrack\""));
    assert!(stderr.contains("0.4.0"));
}

#[test]
fn website_and_app_in_one_program_is_e3052_at_the_cli() {
    let dir = tempdir("e3052");
    let path = dir.join("both.kin");
    std::fs::write(
        &path,
        "website \"W\" { page Home { text \"x\" } }\napp \"A\" { page Home { text \"y\" } }\n",
    )
    .unwrap();
    let out = kinn(&["check", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("E3052"));
}

#[test]
fn empty_app_name_is_e3062_at_the_cli_with_hint_and_position() {
    // S3 slice 1: `kinn check` renders E3062 with the caret under the
    // app declaration and the how-to-fix hint.
    let dir = tempdir("e3062");
    let path = dir.join("unnamed.kin");
    std::fs::write(&path, "app \"\" {\n    page Home { text \"x\" }\n}\n").unwrap();
    let out = kinn(&["check", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("E3062"));
    assert!(stderr.contains("the app name must not be empty"));
    assert!(stderr.contains("hint: name the app"));
    assert!(stderr.contains("-->"));
    assert!(stderr.contains("^"));
}

#[test]
fn app_route_rules_are_reported_at_the_cli() {
    // S3 slice 1: malformed path (E3055) + duplicate route (E3056) +
    // SPA without not_found (E3057) all surface through `kinn check`.
    let dir = tempdir("e305x");
    let path = dir.join("routes.kin");
    std::fs::write(
        &path,
        concat!(
            "app \"A\" {\n",
            "    navigation spa\n",
            "    route \"items/:id\" -> Home\n",
            "    route \"items/:id\" -> Home\n",
            "    route \"/bad\" -> Home\n",
            "    page Home { text \"x\" }\n",
            "}\n",
        ),
    )
    .unwrap();
    let out = kinn(&["check", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("E3056"));
    assert!(stderr.contains("duplicate route"));
    assert!(stderr.contains("E3055"));
    assert!(stderr.contains("must not start with `/`"));
    assert!(stderr.contains("E3057"));
    assert!(stderr.contains("not_found"));
}

#[test]
fn duplicate_app_names_are_e3042_at_the_cli() {
    // S3 slice 1: E3042 names the duplicate and where the first one
    // was declared.
    let dir = tempdir("e3042");
    let path = dir.join("dup.kin");
    std::fs::write(
        &path,
        concat!(
            "app \"A\" {\n",
            "    state { n: int = 0 }\n",
            "    computed n = 1\n",
            "    page Home { text \"x\" }\n",
            "}\n",
        ),
    )
    .unwrap();
    let out = kinn(&["check", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("E3042"));
    assert!(stderr.contains("first declared as a state field at line 2"));
}

#[test]
fn unknown_app_page_targets_are_e3063_at_the_cli() {
    // S3 slice 2 (P-029): unknown page targets resolve now — routes,
    // not_found and open. A target that names a component gets the
    // namespace note in the hint.
    let dir = tempdir("e3063");
    let path = dir.join("targets.kin");
    std::fs::write(
        &path,
        concat!(
            "app \"A\" {\n",
            "    navigation spa\n",
            "    not_found Ghost\n",
            "    route \"items\" -> Missing\n",
            "    component Row { view { text \"r\" } }\n",
            "    route \"other\" -> Row\n",
            "    page Home { button \"Go\" open Home }\n",
            "}\n",
        ),
    )
    .unwrap();
    let out = kinn(&["check", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("E3063"));
    assert!(stderr.contains("is not declared in this app"));
    assert!(stderr.contains("declared pages:"));
    assert!(stderr.contains("'Row' is a component; routes target pages"));
}

#[test]
fn duplicate_app_page_and_component_names_are_e3064_e3065_at_the_cli() {
    // S3 slice 2 (P-029): duplicate app pages (case-insensitive) and
    // duplicate components (case-sensitive) are registered errors.
    let dir = tempdir("e3064");
    let path = dir.join("dups.kin");
    std::fs::write(
        &path,
        concat!(
            "app \"A\" {\n",
            "    page Home { text \"a\" }\n",
            "    page home { text \"b\" }\n",
            "    component Card { view { text \"x\" } }\n",
            "    component Card { view { text \"y\" } }\n",
            "}\n",
        ),
    )
    .unwrap();
    let out = kinn(&["check", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("E3064"));
    assert!(stderr.contains("duplicate page name `home` (first defined at line 2)"));
    assert!(stderr.contains("E3065"));
    assert!(stderr.contains("duplicate component name `Card` (first defined at line 4)"));
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
fn android_export_without_a_toolchain_reports_exported() {
    // Deterministic on every machine: with PATH stripped, the local
    // build provider cannot find gradle, so the CLI must degrade to
    // the honest "project exported" status (never an APK claim).
    // (With a real toolchain — e.g. GitHub runners — the same command
    // builds a debug APK; that path is exercised by the android.yml
    // backend job and was verified there.)
    let dir = tempdir("provider");
    let out = Command::new(env!("CARGO_BIN_EXE_kinn"))
        .args(["build", &format!("{}/web/site.kin", EXAMPLES), "--target", "android", "--out", dir.to_str().unwrap()])
        .env("PATH", "/definitely/not/here")
        .env_remove("ANDROID_HOME")
        .env_remove("ANDROID_SDK_ROOT")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("status: project exported"), "{}", stdout);
}
