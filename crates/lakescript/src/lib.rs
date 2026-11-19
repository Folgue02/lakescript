//! The Lakescript language, as a library.
//!
//! `lks` is a thin program on top of this crate, and any other program can
//! embed Lakescript the same way. Because of that, the library:
//! - keeps no global state,
//! - never prints or exits on its own: errors are returned as values,
//! - never panics because of what a script does.

pub mod ast;
mod command;
pub mod diagnostic;
mod engine;
mod exceptions;
mod host;
mod interpreter;
pub mod lexer;
mod methods;
mod modules;
pub mod parser;
mod patterns;
mod scope;
mod stdlib;
pub mod token;
mod types;
pub mod value;

pub use diagnostic::Diagnostic;
pub use engine::{CapturedOutput, Engine, Outcome};
pub use value::{NativeContext, NativeError, NativeResult, Value};

/// Reads a whole script into its syntax tree.
pub fn parse(source: &str) -> Result<Vec<ast::Stmt>, Diagnostic> {
    parser::parse(lexer::tokenize(source)?)
}

/// Like `parse`, for the file with that number *(see `Span::file`)*.
pub fn parse_file(source: &str, file: u32) -> Result<Vec<ast::Stmt>, Diagnostic> {
    parser::parse(lexer::tokenize_file(source, file)?)
}
