//! kinc — the KINN compiler front end and KIR backend.
//!
//! Pipeline: [`lexer`] → [`parser`] → [`project`] (multi-file loading) →
//! [`checker`] → [`lower`], driven by [`driver`]. Diagnostics are produced
//! by [`diagnostics`].

pub mod ast;
pub mod checker;
pub mod diagnostics;
pub mod driver;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod project;
pub mod token;
