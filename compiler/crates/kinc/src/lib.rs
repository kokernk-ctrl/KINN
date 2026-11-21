//! kinc — the KINN compiler front end and KIR backend.
//!
//! Pipeline: [`lexer`] → [`parser`] → [`project`] (multi-file loading) →
//! [`checker`] → [`lower`], driven by [`driver`]. Diagnostics are produced
//! by [`diagnostics`]. The declarative `website` module additionally
//! lowers to a target-independent [`site::SiteModel`], which the
//! [`backend`] generators turn into real artifacts (web, android).

pub mod ast;
pub mod backend;
pub mod checker;
pub mod consteval;
pub mod diagnostics;
pub mod driver;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod project;
pub mod site;
pub mod token;
