//! The Lakescript language, as a library.
//!
//! `lks` is a thin program on top of this crate, and any other program can
//! embed Lakescript the same way. Because of that, the library:
//! - keeps no global state,
//! - never prints or exits on its own: errors are returned as values,
//! - never panics because of what a script does.

pub mod ast;
pub mod diagnostic;
mod engine;
mod interpreter;
pub mod lexer;
mod methods;
pub mod parser;
mod scope;
pub mod token;
pub mod value;

pub use diagnostic::Diagnostic;
pub use engine::{CapturedOutput, Engine};
pub use value::{NativeContext, Value};

/// Reads a whole script into its syntax tree.
pub fn parse(source: &str) -> Result<Vec<ast::Stmt>, Diagnostic> {
    parser::parse(lexer::tokenize(source)?)
}
