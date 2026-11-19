//! The modules of the standard library, under `std/` *(see `ideas/stdlib/`)*.

mod env;
mod fs;
mod io;
mod json;
mod path;
mod process;

use std::collections::HashSet;
use std::rc::Rc;

use crate::exceptions::TYPE_ERROR;
use crate::modules::Module;
use crate::scope;
use crate::value::{Function, NativeContext, NativeError, NativeFunction, NativeResult, Value};

pub const MODULES: &[&str] = &["std/env", "std/fs", "std/io", "std/json", "std/path", "std/process"];

pub fn exists(name: &str) -> bool {
    MODULES.contains(&name)
}

/// What a module of the standard library provides.
pub struct Exports {
    name: String,
    functions: Vec<(&'static str, &'static [&'static str], Box<dyn Fn(&mut NativeContext, Vec<Value>) -> NativeResult>)>,
    constants: Vec<(&'static str, Value)>,
}

impl Exports {
    fn new(name: &str) -> Self {
        Exports { name: name.to_string(), functions: Vec::new(), constants: Vec::new() }
    }

    fn function(&mut self, name: &'static str, f: impl Fn(&mut NativeContext, Vec<Value>) -> NativeResult + 'static) {
        self.functions.push((name, &[], Box::new(f)));
    }

    /// A function whose parameters can also be given by name.
    fn function_named(
        &mut self,
        name: &'static str,
        params: &'static [&'static str],
        f: impl Fn(&mut NativeContext, Vec<Value>) -> NativeResult + 'static,
    ) {
        self.functions.push((name, params, Box::new(f)));
    }

    fn constant(&mut self, name: &'static str, value: Value) {
        self.constants.push((name, value));
    }
}

pub fn build(name: &str) -> Option<Rc<Module>> {
    let mut exports = Exports::new(name);
    match name {
        "std/env" => env::register(&mut exports),
        "std/fs" => fs::register(&mut exports),
        "std/io" => io::register(&mut exports),
        "std/json" => json::register(&mut exports),
        "std/path" => path::register(&mut exports),
        "std/process" => process::register(&mut exports),
        _ => return None,
    }
    let env = scope::root_scope();
    let mut public = HashSet::new();
    let short = name.rsplit('/').next().unwrap_or(name);
    for (fname, params, func) in exports.functions {
        let full = format!("{short}::{fname}");
        let params = params.iter().map(|p| p.to_string()).collect();
        scope::declare_function(&env, fname, Rc::new(Function::Native(NativeFunction { name: full, params, func })));
        public.insert(fname.to_string());
    }
    for (cname, value) in exports.constants {
        scope::declare(&env, cname, value, true);
        public.insert(format!("${cname}"));
    }
    Some(Rc::new(Module { path: exports.name, env, public }))
}

// ---- Helpers for the arguments of native functions ----

pub(crate) struct Args<'a> {
    pub name: &'a str,
    pub values: Vec<Value>,
}

impl<'a> Args<'a> {
    pub fn new(name: &'a str, values: Vec<Value>, min: usize, max: usize) -> Result<Self, NativeError> {
        let n = values.len();
        if n < min || n > max {
            let expected = if min == max { format!("{min}") } else { format!("{min} to {max}") };
            let s = if max == 1 { "" } else { "s" };
            return Err(NativeError::new(TYPE_ERROR, format!("`{name}` takes {expected} argument{s}, but got {n}")));
        }
        Ok(Args { name, values })
    }

    fn wrong(&self, i: usize, expected: &str) -> NativeError {
        let got = self.values.get(i).map(|v| v.type_name()).unwrap_or("nothing");
        NativeError::new(TYPE_ERROR, format!("argument {} of `{}` must be {expected}, not a {got}", i + 1, self.name))
    }

    pub fn str(&self, i: usize) -> Result<String, NativeError> {
        match self.values.get(i) {
            Some(Value::Str(s)) => Ok(s.to_string()),
            _ => Err(self.wrong(i, "a string")),
        }
    }

    pub fn str_or(&self, i: usize, default: &str) -> Result<String, NativeError> {
        match self.values.get(i) {
            None | Some(Value::Null) => Ok(default.to_string()),
            _ => self.str(i),
        }
    }

    pub fn num(&self, i: usize) -> Result<f64, NativeError> {
        self.values.get(i).and_then(Value::as_f64).ok_or_else(|| self.wrong(i, "a number"))
    }

    pub fn whole(&self, i: usize) -> Result<i64, NativeError> {
        self.values.get(i).and_then(Value::as_whole).ok_or_else(|| self.wrong(i, "a whole number"))
    }

    pub fn opt_whole(&self, i: usize) -> Result<Option<i64>, NativeError> {
        match self.values.get(i) {
            None | Some(Value::Null) => Ok(None),
            _ => self.whole(i).map(Some),
        }
    }

    pub fn strings(&self, i: usize) -> Result<Vec<String>, NativeError> {
        match self.values.get(i) {
            Some(Value::List(l)) => l
                .borrow()
                .iter()
                .map(|v| match v {
                    Value::Str(s) => Ok(s.to_string()),
                    other => Err(NativeError::new(TYPE_ERROR, format!("`{}` needs a list of strings, but it has a {}", self.name, other.type_name()))),
                })
                .collect(),
            _ => Err(self.wrong(i, "a list of strings")),
        }
    }

    /// Every argument from `from` on, as strings *(for `...string` parameters)*.
    pub fn rest_strings(&self, from: usize) -> Result<Vec<String>, NativeError> {
        (from..self.values.len()).map(|i| self.str(i)).collect()
    }

    pub fn value(&self, i: usize) -> Value {
        self.values.get(i).cloned().unwrap_or(Value::Null)
    }
}

/// An I/O error as one of the built-in exceptions.
pub(crate) fn io_error(action: &str, path: &str, e: std::io::Error) -> NativeError {
    use std::io::ErrorKind::*;
    let (kind, reason) = match e.kind() {
        NotFound => ("FileNotFound", "it doesn't exist".to_string()),
        PermissionDenied => ("PermissionDenied", "permission denied".to_string()),
        AlreadyExists => ("IOError", "it already exists".to_string()),
        _ => ("IOError", e.to_string()),
    };
    NativeError::new(kind, format!("can't {action} '{path}': {reason}"))
}

pub(crate) fn strings(items: impl IntoIterator<Item = String>) -> Value {
    Value::list(items.into_iter().map(|s| Value::string(&s)).collect())
}
