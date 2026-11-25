//! The modules of the standard library, under `std/` *(see `ideas/stdlib/`)*.

#[cfg(feature = "archive")]
mod archive;
#[cfg(feature = "csv")]
mod csv_format;
#[cfg(feature = "dates")]
mod dates;
mod bytes;
mod dirs;
mod env;
mod fs;
#[cfg(feature = "html")]
mod html;
#[cfg(feature = "http")]
mod http;
mod io;
mod json;
mod path;
mod process;
mod random;
mod template;
#[cfg(feature = "terminal")]
mod terminal;
mod test;
mod time;

use std::collections::HashSet;
use std::rc::Rc;

use crate::exceptions::TYPE_ERROR;
use crate::modules::{Globals, Module};
use crate::value::{Function, NativeContext, NativeError, NativeFunction, NativeResult, Value, VmFn, VmFunction};

/// The modules this build has. The ones that need big crates are features of
/// this crate, all on by default, so a program that embeds Lakescript can
/// leave them out *(i.e. `default-features = false, features = ["dates"]`)*.
pub const MODULES: &[&str] = &[
    #[cfg(feature = "archive")]
    "std/archive",
    "std/bytes",
    "std/dirs",
    "std/env",
    #[cfg(feature = "csv")]
    "std/format/csv",
    "std/format/json",
    "std/fs",
    #[cfg(feature = "html")]
    "std/html",
    #[cfg(feature = "http")]
    "std/http",
    "std/io",
    "std/path",
    "std/process",
    "std/process/Platform",
    "std/random",
    "std/template",
    #[cfg(feature = "terminal")]
    "std/terminal",
    #[cfg(feature = "terminal")]
    "std/terminal/Style",
    #[cfg(feature = "dates")]
    "std/time/zone",
    "std/test",
    "std/time",
];

/// The modules that are features of the crate, with their feature, to say
/// why one is missing.
const FEATURES: &[(&str, &str)] = &[
    ("std/archive", "archive"),
    ("std/format/csv", "csv"),
    ("std/html", "html"),
    ("std/http", "http"),
    ("std/terminal", "terminal"),
    ("std/terminal/Style", "terminal"),
    ("std/time/zone", "dates"),
];

/// The feature of the crate a module needs, when this build doesn't have it.
pub fn left_out(name: &str) -> Option<&'static str> {
    FEATURES.iter().find(|(module, _)| *module == name && !MODULES.contains(module)).map(|(_, feature)| *feature)
}

/// The Rust parts of modules written in Lakescript, which only those modules use.
const NATIVE: &[&str] = &[
    #[cfg(feature = "dates")]
    "std/fs/native",
    #[cfg(feature = "archive")]
    "std/archive/native",
    #[cfg(feature = "html")]
    "std/html/native",
    #[cfg(feature = "http")]
    "std/http/native",
    #[cfg(feature = "terminal")]
    "std/terminal/native",
    "std/dirs/native",
    "std/process/native",
    "std/random/native",
    "std/time/native",
];

/// Modules written in Lakescript, which come with the interpreter.
const SOURCES: &[(&str, &str)] = &[
    // With dates, `fs::modified()` gives one, so `std/fs` is Lakescript over its Rust part
    #[cfg(feature = "dates")]
    ("std/fs", include_str!("fs.lks")),
    #[cfg(feature = "archive")]
    ("std/archive", include_str!("archive.lks")),
    ("std/dirs", include_str!("dirs.lks")),
    ("std/process", include_str!("process.lks")),
    ("std/random", include_str!("random.lks")),
    ("std/process/Platform", include_str!("process/Platform.lks")),
    #[cfg(feature = "html")]
    ("std/html", include_str!("html.lks")),
    #[cfg(feature = "http")]
    ("std/http", include_str!("http.lks")),
    #[cfg(feature = "terminal")]
    ("std/terminal", include_str!("terminal.lks")),
    #[cfg(feature = "terminal")]
    ("std/terminal/Style", include_str!("terminal/Style.lks")),
    #[cfg(feature = "dates")]
    ("std/time", include_str!("time.lks")),
    // Without dates, `std/time` is only the current time
    #[cfg(not(feature = "dates"))]
    ("std/time", include_str!("time_basic.lks")),
    #[cfg(feature = "dates")]
    ("std/time/zone", include_str!("time/zone.lks")),
];

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

/// The source of a module of the standard library written in Lakescript *(i.e. `std/http`)*.
pub fn source_of(name: &str) -> Option<&'static str> {
    SOURCES.iter().find(|(n, _)| *n == name).map(|(_, source)| *source)
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
        #[cfg(feature = "archive")]
        "std/archive/native" => archive::register(&mut exports),
        "std/bytes" => bytes::register(&mut exports),
        "std/dirs/native" => dirs::register(&mut exports),
        "std/env" => env::register(&mut exports),
        #[cfg(not(feature = "dates"))]
        "std/fs" => fs::register(&mut exports),
        #[cfg(feature = "dates")]
        "std/fs/native" => fs::register(&mut exports),
        "std/io" => io::register(&mut exports),
        #[cfg(feature = "csv")]
        "std/format/csv" => csv_format::register(&mut exports),
        "std/format/json" => json::register(&mut exports),
        #[cfg(feature = "html")]
        "std/html/native" => html::register(&mut exports),
        #[cfg(feature = "http")]
        "std/http/native" => http::register(&mut exports),
        #[cfg(feature = "terminal")]
        "std/terminal/native" => terminal::register(&mut exports),
        "std/path" => path::register(&mut exports),
        "std/process/native" => process::register(&mut exports),
        "std/random/native" => random::register(&mut exports),
        "std/template" => template::register(&mut exports),
        "std/test" => test::register(&mut exports),
        "std/time/native" => time::register(&mut exports),
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

    /// A string argument, shared with the script, not copied.
    pub fn str(&self, i: usize) -> Result<Rc<str>, NativeError> {
        match self.values.get(i) {
            Some(Value::Str(s)) => Ok(s.clone()),
            _ => Err(self.wrong(i, "a string")),
        }
    }

    pub fn str_or(&self, i: usize, default: &str) -> Result<Rc<str>, NativeError> {
        match self.values.get(i) {
            None | Some(Value::Null) => Ok(Rc::from(default)),
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
        (from..self.values.len()).map(|i| self.str(i).map(|s| s.to_string())).collect()
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
