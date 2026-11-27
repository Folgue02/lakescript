//! The Lakescript language, as a library.
//!
//! `lks` is a thin program on top of this crate, and any other program can
//! embed Lakescript the same way. Because of that, the library:
//! - keeps no global state,
//! - never prints or exits on its own: errors are returned as values,
//! - never panics because of what a script does.

mod ast;
mod bytecode;
mod bytes;
mod checker;
mod cli;
mod command;
mod compiler;
mod diagnostic;
pub mod docs;
mod engine;
mod exceptions;
mod host;
mod interrupt;
mod interpreter;
mod lexer;
mod methods;
mod modules;
mod operators;
mod parser;
mod paths;
#[cfg(feature = "bundle")]
mod bundle;
mod siblings;
mod unused;
mod small_alloc;
mod patterns;
mod stack;
mod stdlib;
mod strings;
pub mod terminal;
mod testing;
mod token;
mod types;
mod value;

pub use checker::complete::{Completion, Item as CompletionItem, ItemKind as CompletionKind};
pub use checker::names::{Definition, Name};
pub use diagnostic::{Call, Diagnostic, json_message, json_string};
pub use engine::{CapturedOutput, Engine, Outcome};
pub use interrupt::on_ctrl_c;
pub use paths::{canonical, without_verbatim};
pub use small_alloc::SmallAlloc;
#[cfg(feature = "bundle")]
pub use bundle::{BundleInfo, is_bundle, write_bundle};
pub use testing::{TestResult, render_result, render_summary, test_names};
pub use token::Span;
pub use value::{Key, NativeContext, NativeError, NativeResult, Value};

/// The modules of the standard library this build has *(i.e. `std/fs`)*.
pub fn std_modules() -> &'static [&'static str] {
    stdlib::MODULES
}

/// The source of a module of the standard library written in Lakescript
/// *(i.e. `std/http`)*, to read its docs and types. `None` for the ones in Rust.
pub fn std_source(module: &str) -> Option<&'static str> {
    stdlib::source_of(module)
}

/// Reads a whole script into its syntax tree.
pub fn parse(source: &str) -> Result<Vec<ast::Stmt>, Diagnostic> {
    parser::parse(lexer::tokenize(source)?)
}

/// Like `parse`, for the file with that number *(see `Span::file`)*.
pub fn parse_file(source: &str, file: u32) -> Result<Vec<ast::Stmt>, Diagnostic> {
    parser::parse(lexer::tokenize_file(source, file)?)
}

/// Like `parse_file`, with the stack budget of the engine.
pub(crate) fn parse_budgeted(source: &str, file: u32, stack_budget: usize) -> Result<Vec<ast::Stmt>, Diagnostic> {
    parser::parse_with_budget(lexer::tokenize_file(source, file)?, stack_budget)
}

/// The `use` lines of `program` that are never used, as warnings, from the
/// text of its file *(see `unused`)*.
pub(crate) fn unused_imports(source: &str, file: u32, program: &[ast::Stmt]) -> Vec<Diagnostic> {
    match lexer::tokenize_file(source, file) {
        Ok(tokens) => unused::imports(program, &tokens),
        Err(_) => Vec::new(),
    }
}

/// Like `parse_budgeted`, for the file at `path`: the types of its folder
/// that it uses without `use` get one *(see `siblings`)*. With `recover`, it
/// goes on after syntax errors, and gives them.
pub(crate) fn parse_in(
    source: &str,
    file: u32,
    stack_budget: usize,
    loader: &modules::Loader,
    path: Option<&std::path::Path>,
    recover: bool,
) -> Result<(Vec<ast::Stmt>, Vec<Diagnostic>), Diagnostic> {
    let tokens = lexer::tokenize_file(source, file)?;
    let names = if path.is_some() { siblings::type_names(&tokens) } else { Vec::new() };
    let (mut program, errors) = if recover {
        parser::parse_recovering(tokens, stack_budget)
    } else {
        (parser::parse_with_budget(tokens, stack_budget)?, Vec::new())
    };
    if let Some(path) = path {
        loader.add_siblings(&mut program, names, path);
    }
    Ok((program, errors))
}
