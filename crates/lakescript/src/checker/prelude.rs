//! The signatures of the built-in methods and of the standard library, read
//! from `prelude.txt` *(generated from `ideas/stdlib` by `tools/docs/prelude.py`)*.

use std::collections::HashMap;
use std::rc::Rc;

use super::types::{FnTy, Ty};
use crate::ast::{FunctionDecl, TypeExpr, TypeKind};
use crate::parser::Signature;

const TEXT: &str = include_str!("prelude.txt");

/// The methods of a built-in type.
pub struct TypeSigs {
    /// The names of its type parameters *(i.e. `T` for `list<T>`)*.
    pub params: Vec<Rc<str>>,
    pub methods: HashMap<String, Rc<FnTy>>,
}

#[derive(Clone, Default)]
pub struct ModuleSig {
    pub path: String,
    pub functions: HashMap<String, Rc<FnTy>>,
    pub constants: HashMap<String, Ty>,
    /// `pub` classes, interfaces and enums *(phase 7)*.
    pub types: HashMap<String, Ty>,
    /// Where its functions (`name`) and constants (`$name`) are declared, for editors.
    pub places: HashMap<String, super::names::Place>,
}

/// A part of `prelude.txt`: the methods of a type or the functions of a
/// module, read the first time they're needed. A script uses a few of them,
/// so reading them all would be most of the time of checking a small one.
struct Section<T> {
    /// What's after `type ` or `module ` in its first line.
    header: &'static str,
    body: String,
    parsed: std::cell::OnceCell<T>,
}

pub struct Prelude {
    /// By the name of the type: `string`, `list`, `any`, `exception`...
    types: HashMap<String, Section<TypeSigs>>,
    /// `std/fs`..., and `general` for the functions available without `use`.
    modules: HashMap<String, Section<ModuleSig>>,
}

thread_local! {
    /// The same one for every engine of a thread *(i.e. the ones `lks-lsp` makes for each check)*.
    static SHARED: Rc<Prelude> = Rc::new(Prelude::load());
}

impl Prelude {
    /// The prelude, read once for each thread.
    pub fn shared() -> Rc<Prelude> {
        SHARED.with(|p| p.clone())
    }

    /// Splits `prelude.txt` into its parts, without reading them yet.
    fn load() -> Prelude {
        let mut prelude = Prelude { types: HashMap::new(), modules: HashMap::new() };
        let mut sections: Vec<(&'static str, String)> = Vec::new();
        for line in TEXT.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            if line.starts_with("type ") || line.starts_with("module ") {
                sections.push((line, String::new()));
            } else if let Some((_, body)) = sections.last_mut() {
                body.push_str(line);
                body.push('\n');
            }
        }
        for (header, body) in sections {
            if let Some(rest) = header.strip_prefix("type ") {
                let name = rest.split('<').next().unwrap_or(rest).to_string();
                prelude.types.insert(name, Section { header: rest, body, parsed: Default::default() });
            } else if let Some(path) = header.strip_prefix("module ") {
                prelude.modules.insert(path.to_string(), Section { header: path, body, parsed: Default::default() });
            }
        }
        prelude
    }

    /// The methods of a built-in type *(i.e. `string`, `list`, `any`, `exception`)*.
    pub fn type_sigs(&self, name: &str) -> Option<&TypeSigs> {
        let section = self.types.get(name)?;
        Some(section.parsed.get_or_init(|| {
            let rest = section.header;
            let params: Vec<Rc<str>> = match rest.split_once('<') {
                Some((_, p)) => p.trim_end_matches('>').split(',').map(|s| Rc::from(s.trim())).collect(),
                None => Vec::new(),
            };
            let mut methods = HashMap::new();
            for sig in signatures(&section.body) {
                if let Signature::Function(decl) = sig {
                    methods.insert(decl.name.clone(), Rc::new(signature(&decl, &params)));
                }
            }
            TypeSigs { params, methods }
        }))
    }

    /// The functions and constants of a module written in Rust *(i.e.
    /// `std/fs`)*, or `general` for the functions available without `use`.
    pub fn module(&self, path: &str) -> Option<&ModuleSig> {
        let section = self.modules.get(path)?;
        Some(section.parsed.get_or_init(|| {
            let mut module = ModuleSig { path: section.header.to_string(), ..Default::default() };
            for sig in signatures(&section.body) {
                match sig {
                    Signature::Function(decl) => {
                        module.functions.insert(decl.name.clone(), Rc::new(signature(&decl, &[])));
                    }
                    Signature::Constant { ty, name } => {
                        module.constants.insert(name, convert(&ty, &[], &mut Vec::new()));
                    }
                }
            }
            module
        }))
    }
}

fn signatures(body: &str) -> Vec<Signature> {
    let tokens = crate::lexer::tokenize(body).expect("prelude.txt is valid");
    crate::parser::parse_signatures(tokens).expect("prelude.txt is valid")
}

/// The type of a signature. Names like `T` or `U` that aren't types are the
/// type parameters of the method.
fn signature(decl: &FunctionDecl, receiver: &[Rc<str>]) -> FnTy {
    let mut generics: Vec<Rc<str>> = decl.generics.iter().map(|g| Rc::from(g.as_str())).collect();
    let mut params = Vec::new();
    let mut names = Vec::new();
    let mut required = 0;
    let mut variadic = false;
    for (i, p) in decl.params.iter().enumerate() {
        let ty = p.ty.as_ref().map(|t| convert(t, receiver, &mut generics)).unwrap_or(Ty::Any);
        params.push(ty);
        names.push(p.name.clone());
        if p.variadic {
            variadic = true;
        } else if p.default.is_none() && required == i {
            required = i + 1;
        }
    }
    let ret = decl.ret.as_ref().map(|t| convert(t, receiver, &mut generics)).unwrap_or(Ty::Any);
    FnTy { params, names, required, variadic, ret, generics }
}

/// A type written in the prelude.
fn convert(ty: &TypeExpr, receiver: &[Rc<str>], generics: &mut Vec<Rc<str>>) -> Ty {
    match &ty.kind {
        TypeKind::Nullable(inner) => convert(inner, receiver, generics).nullable(),
        TypeKind::Function { params, variadic, ret } => Ty::Func(Rc::new(FnTy {
            params: params.iter().map(|p| convert(p, receiver, generics)).collect(),
            names: Vec::new(),
            required: params.len() - usize::from(*variadic),
            variadic: *variadic,
            ret: convert(ret, receiver, generics),
            generics: Vec::new(),
        })),
        TypeKind::Named { name, args } => {
            let arg = |i: usize, generics: &mut Vec<Rc<str>>| args.get(i).map(|a| convert(a, receiver, generics)).unwrap_or(Ty::Any);
            match name.as_str() {
                "type" if !args.is_empty() => Ty::TypeOf(Box::new(arg(0, generics))),
                "any" | "type" => Ty::Any,
                "void" => Ty::Void,
                "never" => Ty::Never,
                "bool" => Ty::Bool,
                "number" => Ty::Number,
                "string" => Ty::Str,
                "regex" => Ty::Regex,
                "bytes" => Ty::Bytes,
                "range" => Ty::Range,
                "command" => Ty::Command,
                "list" => Ty::List(Box::new(arg(0, generics))),
                "map" => Ty::Map(Box::new(arg(0, generics)), Box::new(arg(1, generics))),
                "result" => Ty::Result(Box::new(arg(0, generics)), Box::new(arg(1, generics))),
                n if crate::exceptions::is_exception_type(n) => Ty::Exception(Rc::from(n)),
                n => {
                    let n: Rc<str> = Rc::from(n);
                    if !receiver.contains(&n) && !generics.contains(&n) {
                        generics.push(n.clone());
                    }
                    Ty::Param(n)
                }
            }
        }
    }
}
