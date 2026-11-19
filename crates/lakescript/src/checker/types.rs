//! Types as the checker sees them.

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    /// Not checked: any value, including `null`.
    Any,
    Void,
    /// The type of the `null` literal. Only nullable types accept it.
    Null,
    Bool,
    Number,
    Str,
    Symbol,
    Regex,
    Range,
    Command,
    /// One of the built-in exceptions *(i.e. `"ValueError"`)*.
    Exception(Rc<str>),
    Result(Box<Ty>, Box<Ty>),
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Func(Rc<FnTy>),
    Nullable(Box<Ty>),
    /// A type that isn't known yet, found out from how the value is used
    /// *(i.e. the elements of `[]`)*.
    Var(usize),
    /// A type parameter of a generic function, inside its body *(i.e. `T`)*.
    Param(Rc<str>),
}

#[derive(Debug, PartialEq)]
pub struct FnTy {
    /// For a variadic function, the last one is the type of each extra argument.
    pub params: Vec<Ty>,
    /// The names of the parameters, for named arguments. Empty if they have none.
    pub names: Vec<String>,
    /// How many parameters must be given: the ones before the first default.
    pub required: usize,
    pub variadic: bool,
    pub ret: Ty,
    /// The type parameters, replaced by new type variables at every call.
    pub generics: Vec<Rc<str>>,
}

impl FnTy {
    /// A function that takes anything and returns anything, for functions
    /// the checker knows nothing about *(i.e. registered by a host program)*.
    pub fn unknown() -> Rc<FnTy> {
        Rc::new(FnTy { params: vec![Ty::Any], names: Vec::new(), required: 0, variadic: true, ret: Ty::Any, generics: Vec::new() })
    }
}

impl Ty {
    pub fn nullable(self) -> Ty {
        match self {
            Ty::Nullable(_) | Ty::Any | Ty::Null => self,
            other => Ty::Nullable(Box::new(other)),
        }
    }

    /// Replaces type parameters *(i.e. `T` with `string`)*.
    pub fn substitute(&self, map: &HashMap<Rc<str>, Ty>) -> Ty {
        match self {
            Ty::Param(name) => map.get(name).cloned().unwrap_or_else(|| self.clone()),
            Ty::Result(a, b) => Ty::Result(Box::new(a.substitute(map)), Box::new(b.substitute(map))),
            Ty::List(a) => Ty::List(Box::new(a.substitute(map))),
            Ty::Map(a, b) => Ty::Map(Box::new(a.substitute(map)), Box::new(b.substitute(map))),
            Ty::Nullable(a) => Ty::Nullable(Box::new(a.substitute(map))),
            Ty::Func(f) => Ty::Func(Rc::new(FnTy {
                params: f.params.iter().map(|p| p.substitute(map)).collect(),
                names: f.names.clone(),
                required: f.required,
                variadic: f.variadic,
                ret: f.ret.substitute(map),
                generics: f.generics.iter().filter(|g| !map.contains_key(*g)).cloned().collect(),
            })),
            other => other.clone(),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Any | Ty::Var(_) => write!(f, "any"),
            Ty::Void => write!(f, "void"),
            Ty::Null => write!(f, "null"),
            Ty::Bool => write!(f, "bool"),
            Ty::Number => write!(f, "number"),
            Ty::Str => write!(f, "string"),
            Ty::Symbol => write!(f, "symbol"),
            Ty::Regex => write!(f, "regex"),
            Ty::Range => write!(f, "range"),
            Ty::Command => write!(f, "command"),
            Ty::Exception(kind) => write!(f, "{kind}"),
            Ty::Result(a, b) => write!(f, "result<{a}, {b}>"),
            Ty::List(a) => write!(f, "list<{a}>"),
            Ty::Map(a, b) => write!(f, "map<{a}, {b}>"),
            Ty::Nullable(a) => match **a {
                Ty::Func(_) => write!(f, "?({a})"),
                _ => write!(f, "?{a}"),
            },
            Ty::Param(name) => write!(f, "{name}"),
            Ty::Func(func) => {
                let mut params: Vec<String> = func.params.iter().map(|p| p.to_string()).collect();
                if func.variadic {
                    if let Some(last) = params.last_mut() {
                        *last = format!("...{last}");
                    }
                }
                write!(f, "({}) -> {}", params.join(", "), func.ret)
            }
        }
    }
}

/// How a value of this type is described in messages: "a string", "a list<number>".
pub fn a(ty: &Ty) -> String {
    match ty {
        Ty::Any | Ty::Var(_) => "any value".into(),
        Ty::Null => "`null`".into(),
        Ty::Void => "nothing (`void`)".into(),
        Ty::Nullable(_) => format!("a {ty}, which can be `null`"),
        other => {
            let text = other.to_string();
            let article = if text.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
            format!("{article} {text}")
        }
    }
}
