//! The modules of the standard library, under `std/` *(see `ideas/stdlib/`)*.

mod csv_format;
mod env;
mod fs;
mod http;
mod io;
mod json;
mod path;
mod process;
mod template;
mod test;
mod time;

use std::collections::HashSet;
use std::rc::Rc;

use crate::exceptions::TYPE_ERROR;
use crate::modules::{Globals, Module};
use crate::value::{Function, NativeContext, NativeError, NativeFunction, NativeResult, Value, VmFn, VmFunction};

pub const MODULES: &[&str] = &[
    "std/env",
    "std/format/csv",
    "std/format/json",
    "std/fs",
    "std/http",
    "std/io",
    "std/path",
    "std/process",
    "std/template",
    "std/test",
    "std/time",
];

/// The Rust parts of modules written in Lakescript, which only those modules use.
const NATIVE: &[&str] = &["std/http/native"];

/// Modules written in Lakescript, which come with the interpreter.
const SOURCES: &[(&str, &str)] = &[("std/http", include_str!("http.lks"))];

/// Where the files of the modules written in Lakescript seem to be, in errors.
pub const EMBEDDED: &str = "<std>/";

pub fn exists(name: &str) -> bool {
    MODULES.contains(&name) || NATIVE.contains(&name)
}

/// `true` for the Rust part of a module written in Lakescript *(i.e. `std/http/native`)*.
pub fn native(name: &str) -> bool {
    NATIVE.contains(&name)
}

/// The file a module written in Lakescript seems to be in *(i.e. `<std>/http.lks`)*.
pub fn embedded_path(name: &str) -> Option<std::path::PathBuf> {
    SOURCES.iter().any(|(n, _)| *n == name).then(|| format!("{EMBEDDED}{}.lks", &name["std/".len()..]).into())
}

/// The source of the module written in Lakescript that seems to be in `path`.
pub fn embedded(path: &str) -> Option<&'static str> {
    let name = path.strip_prefix(EMBEDDED)?.strip_suffix(".lks")?;
    SOURCES.iter().find(|(n, _)| n.strip_prefix("std/") == Some(name)).map(|(_, source)| *source)
}

/// What a module of the standard library provides.
pub struct Exports {
    name: String,
    functions: Vec<(&'static str, &'static [&'static str], Box<dyn Fn(&mut NativeContext, Vec<Value>) -> NativeResult>)>,
    constants: Vec<(&'static str, Value)>,
    vm: Vec<(&'static str, &'static [&'static str], VmFn)>,
}

impl Exports {
    fn new(name: &str) -> Self {
        Exports { name: name.to_string(), functions: Vec::new(), constants: Vec::new(), vm: Vec::new() }
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

    /// A function that runs Lakescript code, so it needs the VM.
    fn vm_function(&mut self, name: &'static str, params: &'static [&'static str], f: VmFn) {
        self.vm.push((name, params, f));
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
        "std/format/csv" => csv_format::register(&mut exports),
        "std/format/json" => json::register(&mut exports),
        "std/http/native" => http::register(&mut exports),
        "std/path" => path::register(&mut exports),
        "std/process" => process::register(&mut exports),
        "std/template" => template::register(&mut exports),
        "std/test" => test::register(&mut exports),
        "std/time" => time::register(&mut exports),
        _ => return None,
    }
    let globals = Rc::new(Globals::default());
    let mut public = HashSet::new();
    let short = name.rsplit('/').next().unwrap_or(name);
    for (fname, params, func) in exports.functions {
        let full = format!("{short}::{fname}");
        let params = params.iter().map(|p| p.to_string()).collect();
        let function = Value::Function(Rc::new(Function::Native(NativeFunction { name: full, params, func })));
        globals.define(fname, function, true);
        public.insert(fname.to_string());
    }
    for (fname, params, func) in exports.vm {
        let name = format!("{short}::{fname}");
        let params = params.iter().map(|p| p.to_string()).collect();
        globals.define(fname, Value::Function(Rc::new(Function::Vm(VmFunction { name, params, func }))), true);
        public.insert(fname.to_string());
    }
    for (cname, value) in exports.constants {
        globals.define(&format!("${cname}"), value, true);
        public.insert(format!("${cname}"));
    }
    Some(Rc::new(Module { path: exports.name, globals, public }))
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
