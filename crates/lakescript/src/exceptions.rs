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
    ("ValueError", Some("Exception")),
    ("TypeError", Some("Exception")),
    ("IndexError", Some("Exception")),
    ("ArithmeticError", Some("Exception")),
    ("CommandFailed", Some("Exception")),
];

pub const TYPE_ERROR: &str = "TypeError";
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
    Rc::new(ExceptionValue { kind: Rc::from(kind), message: message.into(), span })
}
