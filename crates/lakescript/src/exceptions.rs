//! The built-in exceptions *(see `ideas/language/errors.md`)*.

use std::rc::Rc;

use crate::token::Span;
use crate::value::ExceptionValue;

/// Every built-in exception and its parent.
pub const TREE: &[(&str, Option<&str>)] = &[
    ("Exception", None),
    ("IOError", Some("Exception")),
    ("FileNotFound", Some("IOError")),
    ("PermissionDenied", Some("IOError")),
    // A request of `std/http` that got no response
    ("NetworkError", Some("IOError")),
    ("ValueError", Some("Exception")),
    // Arguments that don't match a command *(see `cli.rs`)*
    ("UsageError", Some("ValueError")),
    ("TypeError", Some("Exception")),
    ("IndexError", Some("Exception")),
    ("ArithmeticError", Some("Exception")),
    ("CommandFailed", Some("Exception")),
    // An `assert` whose condition was false
    ("AssertionError", Some("Exception")),
    // Ctrl+C: beside `Exception`, so `catch ($e)` doesn't catch it *(see ideas/language/errors.md)*
    ("Interrupted", None),
];

pub const TYPE_ERROR: &str = "TypeError";
pub const ASSERTION_ERROR: &str = "AssertionError";
pub const INTERRUPTED: &str = "Interrupted";
pub const VALUE_ERROR: &str = "ValueError";
pub const INDEX_ERROR: &str = "IndexError";
pub const ARITHMETIC_ERROR: &str = "ArithmeticError";

pub fn is_exception_type(name: &str) -> bool {
    TREE.iter().any(|(n, _)| *n == name)
}

/// `true` if `kind` is `ancestor` or one of the exceptions under it.
pub fn is_a(kind: &str, ancestor: &str) -> bool {
    let mut current = Some(kind);
    while let Some(name) = current {
        if name == ancestor {
            return true;
        }
        current = TREE.iter().find(|(n, _)| *n == name).and_then(|(_, parent)| *parent);
    }
    false
}

pub fn new(kind: &str, message: impl Into<String>, span: Span) -> Rc<ExceptionValue> {
    let kind: Rc<str> = Rc::from(kind);
    Rc::new(ExceptionValue { base: kind.clone(), kind, object: None, message: message.into(), span, trace: Default::default() })
}
