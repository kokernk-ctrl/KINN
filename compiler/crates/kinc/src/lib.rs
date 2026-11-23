//! kinc — the KINN compiler front end and KIR backend.
//!
//! Pipeline: [`lexer`] → [`parser`] → [`project`] (multi-file loading) →
//! [`checker`] → [`lower`], driven by [`driver`]. Diagnostics are produced
//! by [`diagnostics`]. The declarative `website` module lowers to a
//! target-independent [`site::SiteModel`]; the dynamic `app` module
//! lowers to the position-free [`appmodel::AppModel`] (the S4 lowering
//! product). Backend generators turn models into real artifacts (web,
//! android).

pub mod appmodel;
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
