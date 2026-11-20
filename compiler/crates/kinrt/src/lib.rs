//! kinrt — the KINN runtime.
//!
//! This crate defines the KINN Intermediate Representation (KIR) module
//! format and the stack-based virtual machine that executes it. The `kinc`
//! compiler crate lowers type-checked Kin programs into [`Module`]s; this
//! crate knows nothing about Kin source code, only about KIR.
//!
//! The runtime is deliberately dependency-free and portable, so it can be
//! built for any target the Rust toolchain supports, including Android
//! ARM64.

pub mod kir;
pub mod listing;
pub mod value;
pub mod vm;

pub use kir::{FuncDef, Instr, Module};
pub use value::Value;
pub use vm::{run, KinError};
