//! The interfaces behind the operators *(see `ideas/language/classes.md`)*:
//! an object gets `+` by implementing `IAdd`, `==` by `IEquals`... They can
//! be used without `use`, like the built-in exceptions.

/// The signature of an operator's method, in words the checker reads: a type
/// parameter of the interface, `Self`, `bool` or `number`.
pub struct Spec {
    pub generics: &'static [&'static str],
    /// The names and types of the parameters.
    pub params: &'static [(&'static str, &'static str)],
    pub ret: &'static str,
}

const fn binary(_: &str) -> Spec {
    Spec { generics: &["R", "O"], params: &[("other", "R")], ret: "O" }
}

/// Each interface, its only method, and the method's signature.
pub const INTERFACES: &[(&str, &str, Spec)] = &[
    ("IEquals", "equals?", Spec { generics: &[], params: &[("other", "Self")], ret: "bool" }),
    ("IOrdered", "compare", Spec { generics: &[], params: &[("other", "Self")], ret: "number" }),
    ("IAdd", "add", binary("+")),
    ("ISub", "sub", binary("-")),
    ("IMul", "mul", binary("*")),
    ("IDiv", "div", binary("/")),
    ("IMod", "mod", binary("%")),
    ("IPow", "pow", binary("**")),
    ("INeg", "neg", Spec { generics: &["O"], params: &[], ret: "O" }),
    ("IIndex", "get", Spec { generics: &["K", "V"], params: &[("key", "K")], ret: "V" }),
    ("IContains", "contains", Spec { generics: &["T"], params: &[("value", "T")], ret: "bool" }),
];

/// The interface and method behind a binary operator on an object.
pub fn for_binary(op: crate::ast::BinaryOp) -> Option<(&'static str, &'static str)> {
    use crate::ast::BinaryOp::*;
    Some(match op {
        Add => ("IAdd", "add"),
        Sub => ("ISub", "sub"),
        Mul => ("IMul", "mul"),
        Div => ("IDiv", "div"),
        Rem => ("IMod", "mod"),
        Pow => ("IPow", "pow"),
        Lt | Gt | LtEq | GtEq => ("IOrdered", "compare"),
        Eq | NotEq => ("IEquals", "equals?"),
        In | NotIn => ("IContains", "contains"),
        _ => return None,
    })
}
