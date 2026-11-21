//! kinn — the KINN command-line interface.
//!
//! Commands:
//! - `kinn run <file.kin>`      compile and execute (with imports)
//! - `kinn check <file.kin>`    compile only, report diagnostics
//! - `kinn tokens <file.kin>`   print the token stream (debug)
//! - `kinn ast <file.kin>`      print the AST (debug)
//! - `kinn kir <file.kin>`      print the lowered KIR (debug)
//! - `kinn version`             print the version
//!
//! `run`, `check` and `kir` load multi-file projects: `import
//! "lib.kin" as l;` resolves relative to the importing file, staying
//! inside the entry file's directory.
//!
//! Exit codes: 0 ok, 1 compile diagnostics, 2 usage error, 3 runtime error.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

const VERSION: &str = "0.2.0";

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
        Some(cmd) => match args.get(1) {
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

fn print_help() {
    println!("kinn {} — the KINN compiler CLI", VERSION);
    println!();
    println!("USAGE:");
    println!("  kinn <command> [file]");
    println!();
    println!("COMMANDS:");
    println!("  run <file.kin>     compile and execute the program (imports resolve");
    println!("                     relative to the entry file)");
    println!("  check <file.kin>   compile and report diagnostics");
    println!("  tokens <file.kin>  print the token stream (debug)");
    println!("  ast <file.kin>     print the parsed AST (debug)");
    println!("  kir <file.kin>     print the lowered KIR (debug)");
    println!("  version            print the version");
    println!();
    println!("EXIT CODES:");
    println!("  0 ok, 1 compile diagnostics, 2 usage error, 3 runtime error");
}
