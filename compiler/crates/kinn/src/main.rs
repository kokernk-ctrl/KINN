//! kinn — the KINN command-line interface.
//!
//! Commands:
//! - `kinn run <file.kin>`      compile and execute (with imports)
//! - `kinn check <file.kin>`    compile only, report diagnostics
//! - `kinn build <file.kin>`    generate target artifacts (default target: web)
//! - `kinn targets`             list supported build targets
//! - `kinn tokens <file.kin>`   print the token stream (debug)
//! - `kinn ast <file.kin>`      print the AST (debug)
//! - `kinn kir <file.kin>`      print the lowered KIR (debug)
//! - `kinn version`             print the version
//!
//! `run`, `check` and `kir` load multi-file projects: `import
//! "lib.kin" as l;` resolves relative to the importing file, staying
//! inside the entry file's directory. `build` compiles the project and
//! runs a target backend (web → static site, android → Gradle project;
//! see docs/MULTI_TARGET_ARCHITECTURE.md). A `kinn.toml` manifest next
//! to the working directory supplies name/entry/target when no file is
//! given.
//!
//! Exit codes: 0 ok, 1 compile diagnostics, 2 usage error, 3 runtime error.

mod manifest;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = "0.3.0";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some("version") | Some("--version") | Some("-v") => {
            println!("kinn {}", VERSION);
            ExitCode::SUCCESS
        }
        Some(cmd) => match cmd {
            "build" => build_command(&args[1..]),
            "targets" => targets_command(),
            _ => match args.get(1) {
                Some(path) if matches!(cmd, "run" | "check" | "tokens" | "ast" | "kir") => {
                    dispatch(cmd, path)
                }
                None if matches!(cmd, "run" | "check" | "tokens" | "ast" | "kir") => {
                    eprintln!("error: command `{}` needs a file argument", cmd);
                    eprintln!("usage: kinn {} <file.kin>", cmd);
                    ExitCode::from(2)
                }
                _ => {
                    eprintln!("error: unknown command `{}`", cmd);
                    print_help();
                    ExitCode::from(2)
                }
            },
        },
    }
}

/// Filesystem loader for `import` resolution: paths are already
/// normalized by `kinc::project` and stay inside the entry file's
/// directory.
fn fs_loader(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| e.to_string())
}

/// Collect the sources referenced by diagnostics (for rendering).
fn sources_for(diags: &[kinc::diagnostics::Diagnostic]) -> HashMap<String, String> {
    let mut sources = HashMap::new();
    for d in diags {
        if d.file.is_empty() || sources.contains_key(&d.file) {
            continue;
        }
        if let Ok(src) = fs::read_to_string(&d.file) {
            sources.insert(d.file.clone(), src);
        }
    }
    sources
}

fn dispatch(cmd: &str, path: &str) -> ExitCode {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path, e);
            return ExitCode::from(2);
        }
    };

    match cmd {
        "tokens" => tokens(&src),
        "check" => check(&src, path),
        "run" => run(&src, path),
        "ast" => ast(&src),
        "kir" => kir(&src, path),
        _ => unreachable!("dispatch only receives known commands"),
    }
}

fn tokens(src: &str) -> ExitCode {
    let (toks, diags) = kinc::lexer::tokenize(src);
    for t in &toks {
        println!("{}", kinc::lexer::token_summary(t));
    }
    if kinc::diagnostics::has_errors(&diags) {
        eprintln!();
        eprint!("{}", kinc::diagnostics::render(&diags, src, "<tokens>", kinc::ast::Mode::S));
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn ast(src: &str) -> ExitCode {
    let (toks, diags) = kinc::lexer::tokenize(src);
    if kinc::diagnostics::has_errors(&diags) {
        eprint!("{}", kinc::diagnostics::render(&diags, src, "<ast>", kinc::ast::Mode::S));
        return ExitCode::from(1);
    }
    match kinc::parser::parse(toks) {
        Ok(program) => {
            print!("{}", kinc::ast::dump(&program));
            ExitCode::SUCCESS
        }
        Err(diags) => {
            eprint!("{}", kinc::diagnostics::render(&diags, src, "<ast>", kinc::ast::Mode::S));
            ExitCode::from(1)
        }
    }
}

/// Shared implementation for `check` and `run`: compile the project.
fn compile_entry(src: &str, path: &str) -> (Option<kinc::driver::Compiled>, Vec<kinc::diagnostics::Diagnostic>) {
    kinc::driver::compile_project(path, src, &mut fs_loader)
}

fn render_diags(
    diags: &[kinc::diagnostics::Diagnostic],
    compiled: Option<&kinc::driver::Compiled>,
    entry_src: &str,
    path: &str,
) -> String {
    match compiled {
        Some(c) if !c.sources.is_empty() => {
            kinc::diagnostics::render_project(diags, &c.sources, c.mode)
        }
        _ => {
            let sources = sources_for(diags);
            if sources.is_empty() {
                kinc::diagnostics::render(diags, entry_src, path, kinc::ast::Mode::S)
            } else {
                kinc::diagnostics::render_project(diags, &sources, kinc::ast::Mode::S)
            }
        }
    }
}

fn check(src: &str, path: &str) -> ExitCode {
    let (compiled, diags) = compile_entry(src, path);
    let has_err = kinc::diagnostics::has_errors(&diags);
    if !diags.is_empty() {
        eprint!("{}", render_diags(&diags, compiled.as_ref(), src, path));
    }
    if has_err {
        ExitCode::from(1)
    } else {
        println!("OK: no errors found ({} diagnostic{} shown)", diags.len(), if diags.len() == 1 { "" } else { "s" });
        ExitCode::SUCCESS
    }
}

fn run(src: &str, path: &str) -> ExitCode {
    let (compiled, diags) = compile_entry(src, path);
    let compiled = match compiled {
        Some(c) => {
            if !diags.is_empty() {
                eprint!("{}", render_diags(&diags, Some(&c), src, path));
            }
            c
        }
        None => {
            eprint!("{}", render_diags(&diags, None, src, path));
            return ExitCode::from(1);
        }
    };

    // Website-only and app-only programs have nothing for the console
    // VM to run: report guidance instead of a fake execution.
    if let Some(site) = &compiled.site {
        if compiled.module.find("main").is_none() {
            eprintln!(
                "note: this program declares website \"{}\" and defines no `main` to run.",
                site.name
            );
            eprintln!("  build it with: kinn build {} --target web", path);
            return ExitCode::from(1);
        }
    }
    if let Some(app) = &compiled.app {
        if compiled.module.find("main").is_none() {
            eprintln!(
                "note: this program declares app \"{}\" and defines no `main` to run.",
                app.name
            );
            eprintln!(
                "  dynamic app builds arrive later in 0.4.0 (the runtime is under construction);"
            );
            eprintln!("  today only `website` programs build with `kinn build --target web`");
            return ExitCode::from(1);
        }
    }

    let mut out = io::stdout().lock();
    if let Err(e) = kinrt::run(&compiled.module, &mut out) {
        let _ = out.flush();
        eprintln!();
        eprintln!("error[{}]: {}", e.code, e.message);
        eprintln!("  --> {}:{}", path, e.line);
        return ExitCode::from(3);
    }
    if let Err(e) = out.flush() {
        eprintln!("error: cannot write output: {}", e);
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

fn kir(src: &str, path: &str) -> ExitCode {
    match kinc::driver::compile_project(path, src, &mut fs_loader) {
        (Some(compiled), diags) => {
            if !diags.is_empty() {
                eprint!("{}", render_diags(&diags, Some(&compiled), src, path));
            }
            print!("{}", kinrt::listing::listing(&compiled.module));
            ExitCode::SUCCESS
        }
        (None, diags) => {
            eprint!("{}", render_diags(&diags, None, src, path));
            ExitCode::from(1)
        }
    }
}

// -- build + targets ---------------------------------------------------------

fn targets_command() -> ExitCode {
    println!("supported build targets:");
    for (id, desc) in kinc::backend::TARGETS {
        println!("  {:<8} {}", id, desc);
    }
    ExitCode::SUCCESS
}

/// `kinn build [file.kin] [--target web|android] [--out DIR]`
fn build_command(args: &[String]) -> ExitCode {
    let mut file: Option<String> = None;
    let mut target: Option<String> = None;
    let mut out: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" | "-t" => {
                i += 1;
                match args.get(i) {
                    Some(v) => target = Some(v.clone()),
                    None => {
                        eprintln!("error: `--target` needs a value (see `kinn targets`)");
                        return ExitCode::from(2);
                    }
                }
            }
            "--out" | "-o" => {
                i += 1;
                match args.get(i) {
                    Some(v) => out = Some(v.clone()),
                    None => {
                        eprintln!("error: `--out` needs a directory argument");
                        return ExitCode::from(2);
                    }
                }
            }
            f if f.starts_with('-') => {
                eprintln!("error: unknown flag `{}` for build", f);
                eprintln!("usage: kinn build [file.kin] [--target web|android] [--out DIR]");
                return ExitCode::from(2);
            }
            f => {
                if file.is_some() {
                    eprintln!("error: build takes a single file (got `{}` too)", f);
                    return ExitCode::from(2);
                }
                file = Some(f.to_string());
            }
        }
        i += 1;
    }

    // No file argument: read the manifest in the working directory.
    let mut project_note = String::new();
    let file = match file {
        Some(f) => f,
        None => {
            let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let mpath = cwd.join("kinn.toml");
            match manifest::load(&mpath) {
                Ok(m) => {
                    match m.entry {
                        Some(e) => {
                            if target.is_none() {
                                target = m.target.clone();
                            }
                            project_note = format!(" (project `{}`, manifest kinn.toml)", m.name);
                            e
                        }
                        None => {
                            eprintln!("error: kinn.toml has no `entry` — add `entry = \"file.kin\"`");
                            return ExitCode::from(2);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("error: no input file given and {}: {}", mpath.display(), e);
                    eprintln!("usage: kinn build <file.kin> --target web [--out DIR]");
                    return ExitCode::from(2);
                }
            }
        }
    };

    let target = target.unwrap_or_else(|| "web".to_string());
    if !kinc::backend::TARGETS.iter().any(|(id, _)| *id == target) {
        eprintln!("error: unknown build target `{}`", target);
        let _ = targets_command();
        return ExitCode::from(2);
    }

    let src = match fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", file, e);
            return ExitCode::from(2);
        }
    };

    let (compiled, diags) = kinc::driver::compile_project(&file, &src, &mut fs_loader);
    let compiled = match compiled {
        Some(c) => {
            if !diags.is_empty() {
                eprint!("{}", render_diags(&diags, Some(&c), src.as_str(), &file));
            }
            c
        }
        None => {
            eprint!("{}", render_diags(&diags, None, src.as_str(), &file));
            return ExitCode::from(1);
        }
    };

    // App programs: the dynamic web dist (0.4.0 S6) — the android gate
    // stays (E3070, C.9).
    if compiled.site.is_none() {
        if let Some(app) = &compiled.app {
            if target == "android" {
                let err = kinc::backend::android_gate(compiled.site.as_ref(), Some(app))
                    .expect_err("an app program is always rejected for android");
                eprintln!("error[{}]: {}", err.code, err.message);
                if let Some(h) = &err.hint {
                    eprintln!("hint: {}", h);
                }
                return ExitCode::from(1);
            }
            let entry_dir = Path::new(&file)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let out_dir = out
                .map(PathBuf::from)
                .unwrap_or_else(|| entry_dir.join("build").join("web"));
            let generated = match kinc::backend::webapp::generate_app(app, &compiled.module, &file) {
                Ok(files) => {
                    let mut files = files;
                    copy_assets(&entry_dir, &out_dir, &mut files);
                    files
                }
                Err(d) => {
                    eprint!("{}", render_diags(&[d], Some(&compiled), src.as_str(), &file));
                    return ExitCode::from(1);
                }
            };
            if let Err(e) = write_files(&generated, &out_dir) {
                eprintln!("error: cannot write output: {}", e);
                return ExitCode::from(2);
            }
            println!(
                "built web app \"{}\" ({} page{}, {}) → {}{}",
                app.name,
                app.pages.len(),
                if app.pages.len() == 1 { "" } else { "s" },
                if app.navigation == kinc::ast::NavigationMode::Spa { "SPA" } else { "MPA" },
                out_dir.display(),
                project_note,
            );
            for f in &generated {
                println!("  {} ({} B)", f.path, f.data.len());
            }
            println!(
                "open {}/index.html in a browser — it needs JavaScript enabled (the dynamic runtime)",
                out_dir.display()
            );
            return ExitCode::SUCCESS;
        }
        eprintln!(
            "error: `{}` declares no `website` and no `app`; the {} target builds website or app declarations",
            file, target
        );
        eprintln!("  see docs/BACKEND_WEB.md for the web module syntax");
        return ExitCode::from(1);
    }

    let site = match &compiled.site {
        Some(s) => s,
        None => unreachable!("app programs returned above"),
    };
    if compiled.module.find("main").is_some() {
        eprintln!("note: `func main()` is not part of the {} build (it runs on the VM with `kinn run`).", target);
    }

    let entry_dir = Path::new(&file)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let out_dir = out
        .map(PathBuf::from)
        .unwrap_or_else(|| entry_dir.join("build").join(&target));

    let generated = match target.as_str() {
        "web" => {
            let mut files = kinc::backend::web::generate(site);
            copy_assets(&entry_dir, &out_dir, &mut files);
            files
        }
        "android" => {
            let assets = read_assets(&entry_dir);
            kinc::backend::android::generate(site, &assets)
        }
        _ => unreachable!("validated above"),
    };

    if let Err(e) = write_files(&generated, &out_dir) {
        eprintln!("error: cannot write output: {}", e);
        return ExitCode::from(2);
    }

    println!(
        "built {} site \"{}\" ({} page{}) → {}{}",
        target,
        site.name,
        site.pages.len(),
        if site.pages.len() == 1 { "" } else { "s" },
        out_dir.display(),
        project_note,
    );
    for f in &generated {
        println!("  {} ({} B)", f.path, f.data.len());
    }
    match target.as_str() {
        "web" => println!(
            "open {}/index.html in a browser, or host the folder as-is — it is plain HTML/CSS",
            out_dir.display()
        ),
        "android" => match try_local_gradle_build(&out_dir) {
            Some(apk) => println!(
                "local build provider: debug APK at {} (debug-signed — verify with apksigner before any distribution)",
                apk.display()
            ),
            None => println!(
                "status: project exported — build the APK with Android Studio, `gradle assembleDebug`, or the remote CI workflow (see {}/README.md)"
            , out_dir.display()),
        },
        _ => {}
    }
    ExitCode::SUCCESS
}

/// Local build provider (docs/BACKEND_ANDROID.md §3): when Gradle and
/// the Android SDK are installed, build the exported project's debug
/// APK and return its path. Any missing piece or failure degrades to
/// the honest `project exported` status — never an APK claim.
fn try_local_gradle_build(out_dir: &Path) -> Option<PathBuf> {
    let gradle = find_on_path("gradle")?;
    let sdk = env::var("ANDROID_HOME")
        .or_else(|_| env::var("ANDROID_SDK_ROOT"))
        .ok()?;
    if !Path::new(&sdk).exists() {
        return None;
    }
    println!(
        "local build provider found: gradle + ANDROID_HOME={} — assembling debug APK…",
        sdk
    );
    let status = std::process::Command::new(gradle)
        .args(["-p", &out_dir.to_string_lossy(), "assembleDebug", "--no-daemon"])
        .status();
    match status {
        Ok(s) if s.success() => {
            let apk_dir = out_dir.join("app/build/outputs/apk/debug");
            let apk = std::fs::read_dir(&apk_dir)
                .ok()?
                .filter_map(|e| e.ok())
                .find(|e| e.path().extension().map(|x| x == "apk").unwrap_or(false))
                .map(|e| e.path());
            if apk.is_none() {
                println!("gradle reported success but produced no APK — status stays: project exported");
            }
            apk
        }
        Ok(_) => {
            println!("local Gradle build failed — status stays: project exported");
            None
        }
        Err(e) => {
            println!("cannot launch gradle: {} — status stays: project exported", e);
            None
        }
    }
}

/// Minimal PATH search (no external dependencies).
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Write generated files, creating directories as needed.
fn write_files(files: &[kinc::backend::OutputFile], out_dir: &Path) -> io::Result<()> {
    for f in files {
        let path = out_dir.join(&f.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &f.data)?;
    }
    Ok(())
}

/// Copy `<entry_dir>/assets` into the web dist (files only, recursive),
/// recording the copies in `files` for the build report.
fn copy_assets(entry_dir: &Path, out_dir: &Path, files: &mut Vec<kinc::backend::OutputFile>) {
    let assets_dir = entry_dir.join("assets");
    for path in collect_files(&assets_dir, "") {
        let full = assets_dir.join(&path);
        if let Ok(data) = fs::read(&full) {
            files.push(kinc::backend::OutputFile {
                path: format!("assets/{}", path),
                data,
            });
        }
    }
    let _ = out_dir; // write_files handles directories; kept for clarity
}

/// Read `<entry_dir>/assets` recursively for the android export.
fn read_assets(entry_dir: &Path) -> Vec<(String, Vec<u8>)> {
    let assets_dir = entry_dir.join("assets");
    collect_files(&assets_dir, "")
        .into_iter()
        .filter_map(|p| fs::read(assets_dir.join(&p)).ok().map(|d| (p, d)))
        .collect()
}

/// Recursively list files under `dir` with forward-slash relative paths.
fn collect_files(dir: &Path, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    let mut names: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    names.sort_by_key(|e| e.file_name());
    for entry in names {
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() { name.clone() } else { format!("{}/{}", prefix, name) };
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            out.extend(collect_files(&entry.path(), &rel));
        } else {
            out.push(rel);
        }
    }
    out
}

fn print_help() {
    println!("kinn {} — the KINN compiler CLI", VERSION);
    println!();
    println!("USAGE:");
    println!("  kinn <command> [file] [--flags]");
    println!();
    println!("COMMANDS:");
    println!("  run <file.kin>     compile and execute the program (imports resolve");
    println!("                     relative to the entry file)");
    println!("  check <file.kin>   compile and report diagnostics");
    println!("  build [file.kin]   generate target artifacts (--target web|android,");
    println!("                     --out DIR; default target web; reads kinn.toml");
    println!("                     when no file is given)");
    println!("  targets            list the supported build targets");
    println!("  tokens <file.kin>  print the token stream (debug)");
    println!("  ast <file.kin>     print the parsed AST (debug)");
    println!("  kir <file.kin>     print the lowered KIR (debug)");
    println!("  version            print the version");
    println!();
    println!("EXIT CODES:");
    println!("  0 ok, 1 compile diagnostics, 2 usage error, 3 runtime error");
}
