//! The compile pipeline: source → tokens → AST → checked AST → KIR.

use crate::ast::Mode;
use crate::checker;
use crate::diagnostics::{has_errors, Diagnostic};
use crate::lexer;
use crate::lower;
use crate::parser;
use kinrt::Module;

/// A successfully compiled program plus the mode used to compile it.
#[derive(Debug)]
pub struct Compiled {
    pub module: Module,
    pub mode: Mode,
}

/// Full pipeline result: the module when there were no errors, plus all
/// diagnostics (warnings and notes are kept even on success).
pub fn compile_full(src: &str) -> (Option<Compiled>, Vec<Diagnostic>) {
    let (tokens, mut diags) = lexer::tokenize(src);
    if has_errors(&diags) {
        return (None, diags);
    }
    match parser::parse(tokens) {
        Ok(program) => {
            let mode = program.mode;
            diags.extend(checker::check(&program));
            if has_errors(&diags) {
                return (None, diags);
            }
            (Some(Compiled { module: lower::lower(&program), mode }), diags)
        }
        Err(parse_diags) => {
            diags.extend(parse_diags);
            (None, diags)
        }
    }
}

/// Convenience wrapper that discards warnings on success.
pub fn compile(src: &str) -> Result<Compiled, Vec<Diagnostic>> {
    let (compiled, diags) = compile_full(src);
    match compiled {
        Some(c) => Ok(c),
        None => Err(diags),
    }
}
