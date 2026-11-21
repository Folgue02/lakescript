//! The type checker (phase 4 of `ideas/roadmap.md`). It runs before a script
//! does, and a type error stops the script before its first line runs.
//!
//! It's conservative: when it can't be sure of a type, it uses `any`, so it
//! never rejects a script that would run fine. Types are inferred like in
//! TypeScript, with type variables for what isn't known yet *(i.e. the
//! elements of `[]`)*.

mod classes;
pub mod complete;
pub mod names;
pub mod prelude;
pub mod types;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::exceptions;
use crate::token::Span;
use prelude::{ModuleSig, Prelude};
use types::{ClassRef, FnTy, Ty, a};

/// What the checker remembers between runs of the same engine: the types of
/// the globals, so a later `run` can use them.
#[derive(Default)]
pub struct State {
    globals: Scope,
    vars: Vec<Option<Ty>>,
}

/// Where `use` finds modules, and the ones already checked.
pub struct Modules<'a> {
    pub loader: &'a crate::modules::Loader,
    pub cache: &'a mut HashMap<PathBuf, Result<ModuleSig, Diagnostic>>,
    pub checking: Vec<PathBuf>,
    /// Every class of every module, by its file and name, so a class is the
    /// same type in all the modules that use it, also when they use each other.
    pub classes: &'a mut HashMap<(PathBuf, String), ClassRef>,
    /// The signatures of modules read from their declarations, without
    /// checking their code, for modules that import each other.
    pub declared: &'a mut HashMap<PathBuf, ModuleSig>,
    pub declaring: Vec<PathBuf>,
    /// Where the names the checkers find go, when an editor wants them *(see `names.rs`)*.
    pub names: Option<&'a mut Vec<names::Found>>,
    /// Where completion is wanted, when an editor wants it *(see `complete.rs`)*.
    pub complete: Option<&'a mut complete::Request>,
    /// Going on after syntax errors, for code that's being written *(see `parser::parse_recovering`)*.
    pub recover: bool,
}

#[derive(Default, Clone)]
struct Scope {
    vars: HashMap<String, VarInfo>,
    funcs: HashMap<String, Rc<FnTy>>,
    modules: HashMap<String, ModuleSig>,
    /// Types narrowed by a condition *(i.e. not `null` after `$x != null`)*.
    narrowed: HashMap<String, Ty>,
    /// Types imported with `use m::{T}`.
    types: HashMap<String, Ty>,
    generics: HashSet<String>,
    /// Functions of this block whose declaration hasn't been reached yet.
    /// Other functions can call them, but the block's own statements can't.
    pending: HashSet<String>,
    /// How many functions deep the statements of this scope are *(see
    /// `Checker::returns`)*, set when its functions are declared.
    depth: usize,
    /// Where its functions are declared.
    places: HashMap<String, names::Place>,
}

#[derive(Clone)]
struct VarInfo {
    ty: Ty,
    constant: bool,
    place: Option<names::Place>,
    /// What it is, for editors *(i.e. "A parameter of `greet`.")*.
    about: Option<String>,
}

/// The class whose method or constructor is being checked.
#[derive(Clone)]
struct This {
    class: ClassRef,
    /// While an interface's signatures are read, `Self` stands for the class
    /// that implements it.
    self_param: bool,
    /// A static method, which has no `@`.
    is_static: bool,
    constructor: bool,
}

/// What a `return` must give.
enum Return {
    Declared(Ty),
    /// An anonymous function: its return type is found out from its `return`s.
    Inferred(Ty),
}

/// Facts a condition proves: variables and the type they have when it's true or false.
type Facts = Vec<(String, Ty)>;

pub struct Checker<'a> {
    prelude: &'a Prelude,
    modules: Modules<'a>,
    state: &'a mut State,
    scopes: Vec<Scope>,
    returns: Vec<Return>,
    loop_depth: usize,
    /// The file being checked, for errors in modules.
    file: Option<String>,
    host_functions: Vec<String>,
    /// The module being checked, for its classes' place in `Modules::classes`.
    path: Option<PathBuf>,
    /// Only reading the declarations of a module *(see `declarations`)*.
    declaring: bool,
    /// The classes whose code is being checked, the innermost last: what `@` is.
    this: Vec<This>,
    /// Where the stack was when checking started, so deeply nested code is an error instead of a crash.
    stack_base: usize,
    pub stack_budget: usize,
    /// The names found, when they're wanted.
    names: Option<Vec<names::Raw>>,
    /// The scope at the place completion is wanted at, and the members before its `.`.
    snapshot: Option<complete::Snapshot>,
    members: Option<Vec<complete::Item>>,
}

type Check<T> = Result<T, Diagnostic>;

impl<'a> Checker<'a> {
    pub fn new(prelude: &'a Prelude, modules: Modules<'a>, state: &'a mut State, host_functions: &[String]) -> Self {
        let mut builtins = Scope::default();
        if let Some(general) = prelude.modules.get("general") {
            builtins.funcs.extend(general.functions.clone());
        }
        for (kind, _) in exceptions::TREE {
            let ctor = FnTy {
                params: vec![Ty::Str],
                names: vec!["message".into()],
                required: 0,
                variadic: false,
                ret: Ty::Exception(Rc::from(*kind)),
                generics: Vec::new(),
            };
            builtins.funcs.insert(kind.to_string(), Rc::new(ctor));
        }
        for name in host_functions {
            builtins.funcs.insert(name.clone(), FnTy::unknown());
        }
        for (name, interface) in classes::operator_interfaces() {
            builtins.types.insert(name, interface);
        }
        let globals = std::mem::take(&mut state.globals);
        let names = modules.names.is_some().then(Vec::new);
        Checker {
            prelude,
            modules,
            state,
            scopes: vec![builtins, globals],
            returns: Vec::new(),
            loop_depth: 0,
            file: None,
            host_functions: host_functions.to_vec(),
            path: None,
            declaring: false,
            this: Vec::new(),
            stack_base: crate::stack::position(),
            stack_budget: crate::stack::DEFAULT_BUDGET,
            names,
            snapshot: None,
            members: None,
        }
    }

    /// Checks a whole script. The globals it declares are kept for the next run.
    pub fn check_program(mut self, program: &[Stmt]) -> Check<()> {
        let result = self.block(program);
        self.finish_names();
        self.finish_completion();
        let globals = self.scopes.remove(1);
        self.state.globals = globals;
        result
    }

    // ---- Errors ----

    fn fail<T>(&mut self, message: impl Into<String>, span: Span) -> Check<T> {
        let mut d = Diagnostic::new(message, span);
        d.file = self.file.clone();
        Err(d)
    }

    // ---- Type variables ----

    fn fresh(&mut self) -> Ty {
        self.state.vars.push(None);
        Ty::Var(self.state.vars.len() - 1)
    }

    /// Follows bound type variables, all the way down.
    fn resolve(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(v) => match &self.state.vars[*v] {
                Some(bound) => self.resolve(bound),
                None => ty.clone(),
            },
            Ty::List(e) => Ty::List(Box::new(self.resolve(e))),
            Ty::Map(k, v) => Ty::Map(Box::new(self.resolve(k)), Box::new(self.resolve(v))),
            Ty::Result(t, e) => Ty::Result(Box::new(self.resolve(t)), Box::new(self.resolve(e))),
            Ty::Nullable(t) => self.resolve(t).nullable(),
            Ty::Object(c, args) => Ty::Object(c.clone(), args.iter().map(|a| self.resolve(a)).collect()),
            Ty::Func(f) => Ty::Func(Rc::new(FnTy {
                params: f.params.iter().map(|p| self.resolve(p)).collect(),
                names: f.names.clone(),
                required: f.required,
                variadic: f.variadic,
                ret: self.resolve(&f.ret),
                generics: f.generics.clone(),
            })),
            other => other.clone(),
        }
    }

    fn shallow(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(v) => match &self.state.vars[*v] {
                Some(bound) => self.shallow(bound),
                None => ty.clone(),
            },
            other => other.clone(),
        }
    }

    /// `true` if a value of type `from` can go where `to` is expected. Unknown
    /// types (variables) take the type they're used as.
    fn assignable(&mut self, from: &Ty, to: &Ty) -> bool {
        let (from, to) = (self.shallow(from), self.shallow(to));
        match (&from, &to) {
            (Ty::Any, _) | (_, Ty::Any) => true,
            (Ty::Var(a), Ty::Var(b)) if a == b => true,
            (Ty::Var(v), other) | (other, Ty::Var(v)) => {
                // `null` makes an unknown type nullable instead of `null` itself
                let bound = if *other == Ty::Null { self.fresh().nullable() } else { other.clone() };
                self.state.vars[*v] = Some(bound);
                true
            }
            (_, Ty::Void) => true,
            (Ty::Null, Ty::Nullable(_)) => true,
            (Ty::Null, Ty::Null) => true,
            (Ty::Null, _) => false,
            (Ty::Nullable(f), Ty::Nullable(t)) => self.assignable(f, t),
            (Ty::Nullable(_), _) => false,
            (f, Ty::Nullable(t)) => self.assignable(f, t),
            (Ty::Exception(k), Ty::Exception(j)) => exceptions::is_a(k, j),
            (Ty::List(f), Ty::List(t)) => self.assignable(f, t) && self.assignable(t, f),
            (Ty::Map(fk, fv), Ty::Map(tk, tv)) => {
                self.assignable(fk, tk) && self.assignable(tk, fk) && self.assignable(fv, tv) && self.assignable(tv, fv)
            }
            (Ty::Result(ft, fe), Ty::Result(tt, te)) => self.assignable(ft, tt) && self.assignable(fe, te),
            (Ty::Func(f), Ty::Func(t)) => {
                let (f, t) = (f.clone(), t.clone());
                // The function must accept as many arguments as the expected type gives
                let given = t.params.len();
                if given < f.required || (!f.variadic && given > f.params.len()) {
                    return false;
                }
                for (i, tp) in t.params.iter().enumerate() {
                    let fp = f.params.get(i).or(f.params.last()).cloned().unwrap_or(Ty::Any);
                    if !self.assignable(tp, &fp) {
                        return false;
                    }
                }
                self.assignable(&f.ret, &t.ret)
            }
            (Ty::Param(a), Ty::Param(b)) => a == b,
            (Ty::Object(f, _), Ty::Exception(k)) => f.0.body.borrow().exception.as_ref().is_some_and(|e| exceptions::is_a(e, k)),
            (Ty::Object(f, fa), Ty::Object(t, ta)) => {
                let (f, fa, t, ta) = (f.clone(), fa.clone(), t.clone(), ta.clone());
                // The types of the parameters must be the same, like in lists
                f.ancestor_args(&fa, &t).iter().any(|args| {
                    ta.is_empty() || args.len() == ta.len() && args.iter().zip(ta.iter()).all(|(x, y)| self.assignable(x, y) && self.assignable(y, x))
                })
            }
            (f, t) => f == t,
        }
    }

    /// The type of two values that can be in the same place *(i.e. both
    /// branches of an `if`)*. Different types give `any`.
    fn join(&mut self, a: &Ty, b: &Ty) -> Ty {
        let (a, b) = (self.shallow(a), self.shallow(b));
        match (&a, &b) {
            (Ty::Any, _) | (_, Ty::Any) => Ty::Any,
            (Ty::Var(_), _) => {
                self.assignable(&a, &b);
                b
            }
            (_, Ty::Var(_)) => {
                self.assignable(&b, &a);
                a
            }
            (Ty::Null, Ty::Null) => Ty::Null,
            (Ty::Null, other) | (other, Ty::Null) => other.clone().nullable(),
            (Ty::Nullable(x), y) | (y, Ty::Nullable(x)) => {
                let y = match y {
                    Ty::Nullable(inner) => (**inner).clone(),
                    other => other.clone(),
                };
                self.join(x, &y).nullable()
            }
            (Ty::Object(x, _), Ty::Object(y, _)) if x.is_a(y) || y.is_a(x) => {
                if x.is_a(y) { b.clone() } else { a.clone() }
            }
            (Ty::Exception(x), Ty::Exception(y)) => {
                if exceptions::is_a(x, y) {
                    b.clone()
                } else if exceptions::is_a(y, x) {
                    a.clone()
                } else {
                    Ty::Exception(Rc::from("Exception"))
                }
            }
            _ => {
                let (ra, rb) = (self.resolve(&a), self.resolve(&b));
                if ra == rb {
                    a
                } else if self.same_shape(&ra, &rb) && self.assignable(&a, &b) {
                    b
                } else {
                    Ty::Any
                }
            }
        }
    }

    fn same_shape(&self, a: &Ty, b: &Ty) -> bool {
        std::mem::discriminant(a) == std::mem::discriminant(b)
    }

    fn non_null(&self, ty: &Ty) -> Ty {
        match self.shallow(ty) {
            Ty::Nullable(inner) => *inner,
            Ty::Null => Ty::Any,
            other => other,
        }
    }

    // ---- Scopes ----

    fn push(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn scoped<T>(&mut self, facts: &Facts, f: impl FnOnce(&mut Self) -> Check<T>) -> Check<T> {
        self.push();
        for (name, ty) in facts {
            self.scopes.last_mut().unwrap().narrowed.insert(name.clone(), ty.clone());
        }
        let result = f(self);
        self.pop();
        result
    }

    fn narrow_here(&mut self, facts: &Facts) {
        for (name, ty) in facts {
            self.scopes.last_mut().unwrap().narrowed.insert(name.clone(), ty.clone());
        }
    }

    fn lookup_var(&self, name: &str) -> Option<Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.narrowed.get(name) {
                return Some(ty.clone());
            }
            if let Some(info) = scope.vars.get(name) {
                return Some(info.ty.clone());
            }
        }
        None
    }

    fn declared_var(&self, name: &str) -> Option<VarInfo> {
        self.scopes.iter().rev().find_map(|s| s.vars.get(name).cloned())
    }

    fn lookup_func(&self, name: &str) -> Option<Rc<FnTy>> {
        self.scopes.iter().rev().find_map(|s| s.funcs.get(name).cloned())
    }

    /// Whether `name` is a function of a block whose declaration hasn't been
    /// reached, used by that block's own statements: it doesn't exist yet
    /// when they run. Inside another function it's fine, since that function
    /// runs later.
    fn not_declared_yet(&self, name: &str) -> bool {
        self.scopes.iter().rev().find(|s| s.funcs.contains_key(name)).is_some_and(|s| s.pending.contains(name) && s.depth == self.returns.len())
    }

    fn lookup_module(&self, name: &str) -> Option<ModuleSig> {
        self.scopes.iter().rev().find_map(|s| s.modules.get(name).cloned())
    }

    fn declare(&mut self, name: &str, ty: Ty, constant: bool, span: Span) -> Check<()> {
        self.declare_at(name, ty, constant, span, span)
    }

    /// Declares a variable whose name is written in `at`.
    fn declare_at(&mut self, name: &str, ty: Ty, constant: bool, span: Span, at: Span) -> Check<()> {
        if self.declared_var(name).is_some() {
            return self.fail(format!("`${name}` already exists. Variables can't be declared twice"), span);
        }
        let place = self.recording().then(|| self.here(at, format!("${name}"), true));
        if let Some(place) = &place {
            let show = names::Show::Var { constant, name: name.to_string(), ty: ty.clone() };
            self.note(at, at.start, "", &place.needle.clone(), show, None, Some(place.clone()));
        }
        self.scopes.last_mut().unwrap().vars.insert(name.to_string(), VarInfo { ty, constant, place, about: None });
        Ok(())
    }

    // ---- Types written in the code ----

    fn ty(&mut self, ty: &TypeExpr) -> Check<Ty> {
        Ok(match &ty.kind {
            TypeKind::Nullable(inner) => self.ty(inner)?.nullable(),
            TypeKind::Function { params, variadic, ret } => {
                let params = params.iter().map(|p| self.ty(p)).collect::<Check<Vec<_>>>()?;
                let required = params.len() - usize::from(*variadic);
                Ty::Func(Rc::new(FnTy { params, names: Vec::new(), required, variadic: *variadic, ret: self.ty(ret)?, generics: Vec::new() }))
            }
            TypeKind::Named { name, args } => {
                let arg = |c: &mut Self, i: usize| -> Check<Ty> {
                    match args.get(i) {
                        Some(a) => c.ty(a),
                        None => Ok(Ty::Any),
                    }
                };
                let count = |c: &mut Self, n: usize| -> Check<()> {
                    if !args.is_empty() && args.len() != n {
                        let s = if n == 1 { "" } else { "s" };
                        return c.fail(format!("`{name}` takes {n} type parameter{s}"), ty.span);
                    }
                    Ok(())
                };
                match name.as_str() {
                    "any" => Ty::Any,
                    "void" => Ty::Void,
                    "bool" => Ty::Bool,
                    "number" => Ty::Number,
                    "string" => Ty::Str,
                    "symbol" => Ty::Symbol,
                    "regex" => Ty::Regex,
                    "range" => Ty::Range,
                    "command" => Ty::Command,
                    "list" => {
                        count(self, 1)?;
                        Ty::List(Box::new(arg(self, 0)?))
                    }
                    "map" => {
                        count(self, 2)?;
                        Ty::Map(Box::new(arg(self, 0)?), Box::new(arg(self, 1)?))
                    }
                    "result" => {
                        count(self, 2)?;
                        Ty::Result(Box::new(arg(self, 0)?), Box::new(arg(self, 1)?))
                    }
                    n if exceptions::is_exception_type(n) => {
                        self.note_exception(n, ty.span);
                        Ty::Exception(Rc::from(n))
                    }
                    // A subcommand *(i.e. `Git::Commit`)*
                    n if n.split_once("::").is_some_and(|(left, _)| crate::parser::is_type_name(left)) => {
                        match self.scopes.iter().rev().find_map(|s| s.types.get(n).cloned()) {
                            Some(t) => t,
                            None => return self.fail(format!("unknown type `{n}`"), ty.span),
                        }
                    }
                    n if n.contains("::") => {
                        let (module, member) = n.split_once("::").unwrap();
                        match self.scopes.iter().rev().find_map(|s| s.modules.get(module)) {
                            None => return self.fail(format!("there's no module named `{module}` here: import it with `use`"), ty.span),
                            Some(m) => match m.types.get(member) {
                                Some(t) => t.clone(),
                                None => return self.fail(format!("`{}` has no `pub` type named `{member}`", m.path), ty.span),
                            },
                        }
                    }
                    "Self" => match self.this.last().cloned() {
                        Some(this) if this.self_param => {
                            self.note_class(&this.class, ty.span, ty.span.start, "Self");
                            Ty::Param(Rc::from("Self"))
                        }
                        Some(this) => {
                            self.note_class(&this.class, ty.span, ty.span.start, "Self");
                            this.class.this_type()
                        }
                        None => return self.fail("`Self` can only be used inside a class", ty.span),
                    },
                    n => {
                        if let Some(Ty::Object(class, _)) = self.scopes.iter().rev().find_map(|s| s.types.get(n).cloned()) {
                            self.note_class(&class, ty.span, ty.span.start, n);
                            let expected = class.0.generics.len();
                            if !args.is_empty() && args.len() != expected {
                                let s = if expected == 1 { "" } else { "s" };
                                return self.fail(format!("`{n}` takes {expected} type parameter{s}"), ty.span);
                            }
                            let given = (0..expected).map(|i| arg(self, i)).collect::<Check<Vec<_>>>()?;
                            Ty::Object(class, given)
                        } else if self.scopes.iter().any(|s| s.generics.contains(n)) {
                            if self.recording() {
                                self.note(ty.span, ty.span.start, "", n, names::Show::Text(n.to_string()), Some("A type parameter.".into()), None);
                            }
                            Ty::Param(Rc::from(n))
                        } else {
                            return self.fail(format!("unknown type `{n}`"), ty.span);
                        }
                    }
                }
            }
        })
    }

    // ---- Statements ----

    /// Functions are declared when their block starts, so they can call each
    /// other in any order. `use` and `type` go first too, since the
    /// signatures need them. The bodies are checked where they are.
    fn block(&mut self, stmts: &[Stmt]) -> Check<()> {
        let mut early = HashSet::new();
        for (i, stmt) in stmts.iter().enumerate() {
            let inner = match &stmt.kind {
                StmtKind::Pub(inner) => &**inner,
                _ => stmt,
            };
            if let StmtKind::Use(_) = &inner.kind {
                self.stmt(stmt)?;
                early.insert(i);
            }
        }
        self.declare_classes(stmts)?;
        for stmt in stmts {
            let inner = match &stmt.kind {
                StmtKind::Pub(inner) => &**inner,
                _ => stmt,
            };
            if let StmtKind::Function(decl) = &inner.kind {
                self.hoist(decl, inner.span)?;
            }
        }
        for (i, stmt) in stmts.iter().enumerate() {
            if !early.contains(&i) {
                self.stmt(stmt)?;
            }
        }
        Ok(())
    }

    /// Gives a block's classes their names, and then their signatures, so
    /// their fields and methods can use any of them.
    fn declare_classes(&mut self, stmts: &[Stmt]) -> Check<()> {
        let mut classes = Vec::new();
        for stmt in stmts {
            let (inner, public) = match &stmt.kind {
                StmtKind::Pub(inner) => (&**inner, true),
                _ => (stmt, false),
            };
            if let StmtKind::Class(decl) = &inner.kind {
                let class = self.declare_class(decl, inner.span)?;
                classes.push((decl.clone(), class, public, inner.span));
            }
        }
        // A superclass first, so its subclasses know its fields
        let mut done: Vec<String> = Vec::new();
        while done.len() < classes.len() {
            let before = done.len();
            for (decl, class, _, span) in &classes {
                if done.contains(&decl.name) {
                    continue;
                }
                let waits = decl.kind == ClassKind::Class
                    && decl.extends.first().is_some_and(|p| {
                        let TypeKind::Named { name, .. } = &p.kind else { return false };
                        classes.iter().any(|(d, ..)| d.name == *name && !done.contains(name)) && *name != decl.name
                    });
                if !waits {
                    self.class_body(decl, class, *span)?;
                    done.push(decl.name.clone());
                }
            }
            if done.len() == before {
                // Classes that extend each other: the first one says so
                let (decl, class, _, span) = classes.iter().find(|(d, ..)| !done.contains(&d.name)).unwrap();
                self.class_body(decl, class, *span)?;
                done.push(decl.name.clone());
            }
        }
        Ok(())
    }

    /// Declares a function's signature before its block runs.
    fn hoist(&mut self, decl: &FunctionDecl, span: Span) -> Check<()> {
        if self.lookup_func(&decl.name).is_some() && self.scopes.len() > 2 || self.scopes.last().unwrap().funcs.contains_key(&decl.name) {
            return self.fail(format!("a function named `{}` already exists", decl.name), span);
        }
        // The type parameters are known inside the signature
        self.push();
        self.scopes.last_mut().unwrap().generics.extend(decl.generics.iter().cloned());
        let sig = self.signature(decl, span);
        self.pop();
        let depth = self.returns.len();
        let sig = Rc::new(sig?);
        let place = self.here(span, decl.name.clone(), true);
        if self.recording() {
            let show = names::Show::Func { name: decl.name.clone(), ty: Ty::Func(sig.clone()) };
            self.note(span, span.start, "", &decl.name, show, None, Some(place.clone()));
        }
        let scope = self.scopes.last_mut().unwrap();
        scope.funcs.insert(decl.name.clone(), sig);
        scope.places.insert(decl.name.clone(), place);
        scope.pending.insert(decl.name.clone());
        scope.depth = depth;
        Ok(())
    }

    fn scoped_stmt(&mut self, stmt: &Stmt, facts: &Facts) -> Check<()> {
        self.scoped(facts, |c| match &stmt.kind {
            StmtKind::Block(stmts) => c.block(stmts),
            _ => c.stmt(stmt),
        })
    }

    fn stmt(&mut self, stmt: &Stmt) -> Check<()> {
        let span = stmt.span;
        self.guard(span)?;
        if self.around_cursor(span) {
            self.snapshot();
        }
        self.stmt_inner(stmt)?;
        // After a statement before the place, the scope has what it declared
        if self.cursor().is_some_and(|c| span.end <= c) {
            self.snapshot();
        }
        Ok(())
    }

    fn stmt_inner(&mut self, stmt: &Stmt) -> Check<()> {
        let span = stmt.span;
        match &stmt.kind {
            StmtKind::Class(decl) => self.class(decl, span)?,
            StmtKind::Let { constant, name, ty, value } => {
                let declared = match ty {
                    Some(t) => Some(self.ty(t)?),
                    None => None,
                };
                let value_ty = self.expr(value, declared.as_ref())?;
                let var_ty = match declared {
                    Some(d) => {
                        if !self.assignable(&value_ty, &d) {
                            let found = self.resolve(&value_ty);
                            return self.fail(format!("`${name}` is {}, but its value is {}", a(&d), a(&found)), value.span);
                        }
                        d
                    }
                    // `let $x = null;` can hold anything
                    None if self.shallow(&value_ty) == Ty::Null => Ty::Any,
                    None => value_ty,
                };
                self.declare(name, var_ty, *constant, span)?;
            }
            StmtKind::LetList { constant, names, rest, value } => {
                let value_ty = self.expr(value, None)?;
                let elem = match self.shallow(&value_ty) {
                    Ty::List(e) => *e,
                    Ty::Any | Ty::Var(_) => Ty::Any,
                    other => return self.fail(format!("only lists can be destructured, and this is {}", a(&other)), value.span),
                };
                for n in names {
                    self.declare(n, elem.clone(), *constant, span)?;
                }
                if let Some(r) = rest {
                    self.declare(r, Ty::List(Box::new(elem)), *constant, span)?;
                }
            }
            StmtKind::Function(decl) => self.function(decl, span)?,
            StmtKind::If { cond, then, otherwise } => {
                let (yes, no) = self.condition(cond)?;
                self.scoped_stmt(then, &yes)?;
                if let Some(o) = otherwise {
                    self.scoped_stmt(o, &no)?;
                }
                // After `if (...) return;`, what's left only runs when the condition was false
                match (terminates(then), otherwise.as_ref().map(|o| terminates(o))) {
                    (true, None) | (true, Some(false)) => self.narrow_here(&no),
                    (false, Some(true)) => self.narrow_here(&yes),
                    _ => {}
                }
            }
            StmtKind::While { cond, body } => {
                let (yes, _) = self.condition(cond)?;
                self.loop_depth += 1;
                let result = self.scoped_stmt(body, &yes);
                self.loop_depth -= 1;
                result?;
            }
            StmtKind::For { pattern, iterable, body } => {
                let it = self.expr(iterable, None)?;
                let it = self.shallow(&it);
                let mut vars: Vec<(String, Ty)> = Vec::new();
                let elem = |c: &mut Self, it: &Ty| -> Check<Ty> {
                    Ok(match it {
                        Ty::List(e) => (**e).clone(),
                        Ty::Range => Ty::Number,
                        Ty::Str => Ty::Str,
                        Ty::Map(k, _) => (**k).clone(),
                        Ty::Any | Ty::Var(_) => Ty::Any,
                        other => return c.fail(format!("can't loop over {}", a(other)), iterable.span),
                    })
                };
                match pattern {
                    ForPattern::Value(v) => vars.push((v.clone(), elem(self, &it)?)),
                    ForPattern::Indexed { index, value } => {
                        if matches!(it, Ty::Map(..)) {
                            return self.fail("use `for ($key => $value in $map)` to loop over a map with its keys", iterable.span);
                        }
                        vars.push((index.clone(), Ty::Number));
                        vars.push((value.clone(), elem(self, &it)?));
                    }
                    ForPattern::KeyValue { key, value } => match &it {
                        Ty::Map(k, v) => {
                            vars.push((key.clone(), (**k).clone()));
                            vars.push((value.clone(), (**v).clone()));
                        }
                        Ty::Any | Ty::Var(_) => {
                            vars.push((key.clone(), Ty::Any));
                            vars.push((value.clone(), Ty::Any));
                        }
                        other => return self.fail(format!("`=>` only works with maps, and this is {}", a(other)), iterable.span),
                    },
                    ForPattern::List { names, rest } => {
                        let element = elem(self, &it)?;
                        let inner = match self.shallow(&element) {
                            Ty::List(e) => *e,
                            Ty::Any | Ty::Var(_) => Ty::Any,
                            other => return self.fail(format!("`for ([...] in ...)` needs a list of lists, but its elements are {}", a(&other)), iterable.span),
                        };
                        for n in names {
                            vars.push((n.clone(), inner.clone()));
                        }
                        if let Some(r) = rest {
                            vars.push((r.clone(), Ty::List(Box::new(inner))));
                        }
                    }
                }
                self.loop_depth += 1;
                let result = self.scoped(&Vec::new(), |c| {
                    for (name, ty) in vars {
                        c.declare_at(&name, ty, false, iterable.span, span)?;
                    }
                    match &body.kind {
                        StmtKind::Block(stmts) => c.block(stmts),
                        _ => c.stmt(body),
                    }
                });
                self.loop_depth -= 1;
                result?;
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Return(value) => {
                let Some(ret) = self.returns.last() else {
                    return self.fail("`return` outside a function", span);
                };
                match (ret, value) {
                    (Return::Declared(Ty::Void), Some(v)) => {
                        return self.fail("a `void` function can't return a value", v.span);
                    }
                    (Return::Declared(expected), Some(v)) => {
                        let expected = expected.clone();
                        let found = self.expr(v, Some(&expected))?;
                        if !self.assignable(&found, &expected) {
                            let found = self.resolve(&found);
                            return self.fail(format!("this function returns {}, not {}", a(&expected), a(&found)), v.span);
                        }
                    }
                    (Return::Declared(expected), None) if *expected != Ty::Void && *expected != Ty::Any => {
                        let expected = expected.clone();
                        return self.fail(format!("this function must return {}", a(&expected)), span);
                    }
                    (Return::Declared(_), None) => {}
                    (Return::Inferred(var), v) => {
                        let var = var.clone();
                        let found = match v {
                            Some(v) => self.expr(v, None)?,
                            None => Ty::Void,
                        };
                        let joined = self.join(&var, &found);
                        if let Some(Return::Inferred(slot)) = self.returns.last_mut() {
                            *slot = joined;
                        }
                    }
                }
            }
            StmtKind::Yield(value) => {
                self.expr(value, None)?;
            }
            StmtKind::Switch(switch) => {
                self.switch(switch, false)?;
            }
            StmtKind::Try { body, catches, finally } => {
                self.scoped(&Vec::new(), |c| c.block(body))?;
                for catch in catches {
                    let ty = match &catch.ty {
                        Some(t) => {
                            let t = self.ty(t)?;
                            let custom = matches!(&t, Ty::Object(c, _) if c.0.body.borrow().exception.is_some());
                            if !custom && !matches!(t, Ty::Exception(_)) {
                                let span = catch.ty.as_ref().unwrap().span;
                                return self.fail(format!("`catch` needs an exception type, not {}", a(&t)), span);
                            }
                            t
                        }
                        None => Ty::Exception(Rc::from("Exception")),
                    };
                    self.scoped(&Vec::new(), |c| {
                        c.declare(&catch.name, ty, false, span)?;
                        c.block(&catch.body)
                    })?;
                }
                if let Some(f) = finally {
                    self.scoped(&Vec::new(), |c| c.block(f))?;
                }
            }
            StmtKind::Throw(value) => self.thrown(value)?,
            StmtKind::Use(decl) => self.use_decl(decl, span)?,
            StmtKind::Pub(inner) => self.stmt(inner)?,
            StmtKind::Block(stmts) => self.scoped(&Vec::new(), |c| c.block(stmts))?,
            StmtKind::Expr(e) => {
                self.expr(e, None)?;
            }
        }
        Ok(())
    }

    fn function(&mut self, decl: &FunctionDecl, span: Span) -> Check<()> {
        // Usually declared when the block started *(see `block`)*
        if !self.scopes.last_mut().unwrap().pending.remove(&decl.name) {
            self.hoist(decl, span)?;
            self.scopes.last_mut().unwrap().pending.remove(&decl.name);
        }
        let sig = self.scopes.last().unwrap().funcs[&decl.name].clone();
        let ret = sig.ret.clone();
        self.push();
        self.scopes.last_mut().unwrap().generics.extend(decl.generics.iter().cloned());
        let result = self.body(decl, &sig, Return::Declared(ret.clone()), span);
        self.pop();
        result?;
        if !matches!(ret, Ty::Void | Ty::Any) {
            if let FunctionBody::Block(stmts) = &decl.body {
                if !block_terminates(stmts) {
                    return self.fail(format!("`{}` must return {} at the end", decl.name, a(&ret)), span);
                }
            }
        }
        Ok(())
    }

    fn signature(&mut self, decl: &FunctionDecl, span: Span) -> Check<FnTy> {
        let mut params = Vec::new();
        let mut names = Vec::new();
        let mut required = 0;
        let mut variadic = false;
        for (i, p) in decl.params.iter().enumerate() {
            let t = match &p.ty {
                Some(t) => self.ty(t)?,
                None => Ty::Any,
            };
            params.push(t);
            names.push(p.name.clone());
            if p.variadic {
                variadic = true;
            } else if p.default.is_none() && required == i {
                required = i + 1;
            }
        }
        let ret = match &decl.ret {
            Some(t) => self.ty(t)?,
            None => Ty::Any,
        };
        let _ = span;
        Ok(FnTy { params, names, required, variadic, ret, generics: decl.generics.iter().map(|g| Rc::from(g.as_str())).collect() })
    }

    /// Checks the parameters' defaults and the body of a function.
    fn body(&mut self, decl: &FunctionDecl, sig: &FnTy, ret: Return, span: Span) -> Check<Ty> {
        for (i, p) in decl.params.iter().enumerate() {
            let declared = sig.params[i].clone();
            if let Some(default) = &p.default {
                let found = self.expr(default, Some(&declared))?;
                if !self.assignable(&found, &declared) {
                    let found = self.resolve(&found);
                    return self.fail(format!("the default of `${}` is {}, but the parameter is {}", p.name, a(&found), a(&declared)), default.span);
                }
            }
            let ty = if p.variadic { Ty::List(Box::new(declared)) } else { declared };
            if self.declared_var(&p.name).is_some() {
                return self.fail(format!("the parameter `${}` of `{}` has the name of a variable that already exists", p.name, decl.name), span);
            }
            let place = self.recording().then(|| self.here(span, format!("${}", p.name), false));
            let about = if decl.name.is_empty() || decl.name == "anonymous" { "A parameter.".to_string() } else { format!("A parameter of `{}`.", decl.name) };
            if let Some(place) = &place {
                let show = names::Show::Var { constant: false, name: p.name.clone(), ty: ty.clone() };
                self.note(span, span.start, "", &place.needle.clone(), show, Some(about.clone()), Some(place.clone()));
            }
            self.scopes.last_mut().unwrap().vars.insert(p.name.clone(), VarInfo { ty, constant: false, place, about: Some(about) });
        }
        if self.around_cursor(span) {
            self.snapshot();
        }
        self.returns.push(ret);
        let loops = std::mem::take(&mut self.loop_depth);
        let result = match &decl.body {
            FunctionBody::Expr(e) => {
                let expected = match self.returns.last() {
                    Some(Return::Declared(t)) => Some(t.clone()),
                    _ => None,
                };
                self.expr(e, expected.as_ref()).and_then(|found| {
                    if let Some(expected) = expected {
                        if !self.assignable(&found, &expected) {
                            let found = self.resolve(&found);
                            return self.fail(format!("`{}` returns {}, not {}", decl.name, a(&expected), a(&found)), e.span);
                        }
                    }
                    Ok(found)
                })
            }
            FunctionBody::Block(stmts) => self.block(stmts).map(|_| Ty::Void),
        };
        self.loop_depth = loops;
        let ret = self.returns.pop();
        let body_ty = result?;
        Ok(match (ret, &decl.body) {
            (Some(Return::Inferred(t)), FunctionBody::Block(_)) => t,
            (_, FunctionBody::Expr(_)) => body_ty,
            (Some(Return::Declared(t)), _) => t,
            _ => Ty::Any,
        })
    }

    /// The value after `throw`, which must be an exception.
    fn thrown(&mut self, value: &Expr) -> Check<()> {
        let t = self.expr(value, None)?;
        let t = self.shallow(&t);
        let custom = matches!(&t, Ty::Object(c, _) if c.0.body.borrow().exception.is_some());
        if !custom && !matches!(t, Ty::Exception(_) | Ty::Any | Ty::Var(_)) {
            return self.fail(format!("only exceptions can be thrown, not {} *(i.e. `throw ValueError('...')`)*", a(&t)), value.span);
        }
        Ok(())
    }

    fn use_decl(&mut self, decl: &UseDecl, span: Span) -> Check<()> {
        let module = self.module(&decl.path, span)?;
        if self.recording() {
            self.note_use(decl, &module, span);
        }
        if decl.type_module {
            // A module named like a type holds that type, and only shares it
            let name = decl.path.last().unwrap();
            let file = format!("{}.lks", decl.path.join("/"));
            if !module.types.contains_key(name) {
                return self.fail(format!("`{file}` is named like a type, so it must declare `pub class {name}`, `pub interface {name}` or `pub enum {name}`"), span);
            }
            let others: Vec<String> = module
                .types
                .keys()
                .chain(module.functions.keys())
                .filter(|n| *n != name)
                .cloned()
                .chain(module.constants.keys().map(|c| format!("${c}")))
                .collect();
            if let Some(other) = others.first() {
                return self.fail(
                    format!("`{file}` holds the type `{name}`, so that's the only thing it can make `pub`, and `{other}` is `pub` too. Move it to another module, or make it private"),
                    span,
                );
            }
        }
        match &decl.items {
            UseItems::Module { alias } => {
                let name = alias.clone().unwrap_or_else(|| decl.path.last().cloned().unwrap_or_default());
                self.scopes.last_mut().unwrap().modules.insert(name, module);
            }
            UseItems::Items(items) => {
                for item in items {
                    let local = item.alias.clone().unwrap_or_else(|| item.name.clone());
                    if item.variable {
                        match module.constants.get(&item.name) {
                            Some(t) => {
                                self.declare(&local, t.clone(), true, span)?;
                                if let (Some(place), Some(info)) = (module.places.get(&format!("${}", item.name)), self.scopes.last_mut().unwrap().vars.get_mut(&local)) {
                                    info.place = Some(place.clone());
                                }
                            }
                            None => return self.fail(format!("`{}` has no `pub` constant named `${}`", module.path, item.name), span),
                        }
                    } else {
                        let function = module.functions.get(&item.name).cloned();
                        let ty = module.types.get(&item.name).cloned();
                        if function.is_none() && ty.is_none() {
                            return self.fail(format!("`{}` has no `pub` function or type named `{}`", module.path, item.name), span);
                        }
                        // A class is both: a type, and a function that creates its objects
                        if let Some(f) = function {
                            self.scopes.last_mut().unwrap().funcs.insert(local.clone(), f);
                            if let Some(place) = module.places.get(&item.name) {
                                self.scopes.last_mut().unwrap().places.insert(local.clone(), place.clone());
                            }
                        }
                        if let Some(t) = ty {
                            if self.scopes.last().unwrap().types.contains_key(&local) {
                                return self.fail(format!("`{local}` is already imported: give one of them another name with `as`"), span);
                            }
                            self.scopes.last_mut().unwrap().types.insert(local, t);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// The signatures of a module: from the prelude for `std/`, or by checking its file.
    fn module(&mut self, path: &[String], span: Span) -> Check<ModuleSig> {
        let source = match self.modules.loader.resolve(path) {
            Ok(s) => s,
            Err(message) => return self.fail(message, span),
        };
        match source {
            crate::modules::ModuleSource::Std(name) => match self.prelude.modules.get(&name) {
                Some(m) => Ok(m.clone()),
                None => self.fail(format!("there's no module `{name}`"), span),
            },
            crate::modules::ModuleSource::File(file) => {
                if let Some(cached) = self.modules.cache.get(&file) {
                    return cached.clone();
                }
                // Modules that import each other: the signatures come from the declarations
                if self.declaring || self.modules.checking.contains(&file) {
                    return self.declarations(&file);
                }
                let result = self.check_module(&file);
                self.modules.cache.insert(file, result.clone());
                result
            }
        }
    }

    /// The signature of a module read from its declarations, without checking
    /// its code: for modules that import each other. Everything `pub` has its
    /// type written, so nothing else is needed.
    fn declarations(&mut self, file: &std::path::Path) -> Check<ModuleSig> {
        if let Some(sig) = self.modules.declared.get(file) {
            return Ok(sig.clone());
        }
        let shown = file.display().to_string();
        // An error of a module it uses already names that module
        let located = |mut d: Diagnostic| {
            d.file.get_or_insert_with(|| shown.clone());
            d
        };
        let source = self.modules.loader.read(file).map_err(|e| Diagnostic::new(format!("can't read the module '{shown}': {e}"), Span::default()))?;
        let program = crate::parse_file(&source, u32::MAX).map_err(located)?;
        let mut sig = ModuleSig { path: shown.clone(), ..Default::default() };
        let public_classes = program.iter().filter_map(|s| match &s.kind {
            StmtKind::Pub(inner) => match &inner.kind {
                StmtKind::Class(d) => Some(d.clone()),
                _ => None,
            },
            _ => None,
        });
        if self.modules.declaring.iter().any(|p| p == file) {
            // Reached again while its declarations are read: only its types,
            // whose fields and methods are filled in before any code uses them
            for decl in public_classes {
                let class = self.modules.classes.entry((file.to_path_buf(), decl.name.clone())).or_insert_with(|| {
                    ClassRef(Rc::new(types::ClassSig {
                        name: Rc::from(decl.name.as_str()),
                        interface: decl.kind == ClassKind::Interface,
                        is_enum: decl.kind == ClassKind::Enum,
                        command: decl.kind == ClassKind::Command,
                        generics: decl.generics.iter().map(|g| Rc::from(g.as_str())).collect(),
                        body: std::cell::RefCell::new(types::ClassBody::default()),
                        places: Default::default(),
                    }))
                });
                sig.types.insert(decl.name.clone(), Ty::Object(class.clone(), Vec::new()));
            }
            return Ok(sig);
        }
        let mut state = State::default();
        let modules = Modules {
            loader: self.modules.loader,
            cache: &mut *self.modules.cache,
            checking: self.modules.checking.clone(),
            classes: &mut *self.modules.classes,
            declared: &mut *self.modules.declared,
            declaring: {
                let mut d = self.modules.declaring.clone();
                d.push(file.to_path_buf());
                d
            },
            names: None,
            complete: None,
            recover: self.modules.recover,
        };
        let host_functions = self.host_functions.clone();
        let mut checker = Checker::new(self.prelude, modules, &mut state, &host_functions);
        checker.stack_budget = self.stack_budget;
        checker.file = Some(shown.clone());
        checker.path = Some(file.to_path_buf());
        checker.declaring = true;
        checker.declare_block(&program).map_err(located)?;
        let globals = checker.scopes[1].clone();
        for stmt in &program {
            let StmtKind::Pub(inner) = &stmt.kind else { continue };
            match &inner.kind {
                StmtKind::Function(d) => {
                    if let Some(f) = globals.funcs.get(&d.name) {
                        sig.functions.insert(d.name.clone(), f.clone());
                    }
                }
                StmtKind::Class(d) => {
                    if let Some(t) = globals.types.get(&d.name) {
                        sig.types.insert(d.name.clone(), t.clone());
                    }
                    if let Some(f) = globals.funcs.get(&d.name) {
                        sig.functions.insert(d.name.clone(), f.clone());
                    }
                }
                StmtKind::Let { name, ty: Some(ty), .. } => {
                    let t = checker.ty(ty).map_err(located)?;
                    sig.constants.insert(name.clone(), t);
                }
                _ => {}
            }
        }
        self.modules.declared.insert(file.to_path_buf(), sig.clone());
        Ok(sig)
    }

    /// The part of `block` that declares: `use`, classes and the signatures of functions.
    fn declare_block(&mut self, stmts: &[Stmt]) -> Check<()> {
        for stmt in stmts {
            if let StmtKind::Use(_) = &stmt.kind {
                self.stmt(stmt)?;
            }
        }
        let classes = self.declare_classes(stmts)?;
        let _ = classes;
        for stmt in stmts {
            let inner = match &stmt.kind {
                StmtKind::Pub(inner) => &**inner,
                _ => stmt,
            };
            if let StmtKind::Function(decl) = &inner.kind {
                self.hoist(decl, inner.span)?;
            }
        }
        Ok(())
    }

    /// The statements of a module, and its first syntax error when going on after them.
    fn parse_module(&self, source: &str) -> Check<(Vec<Stmt>, Option<Diagnostic>)> {
        if !self.modules.recover {
            return Ok((crate::parse_file(source, u32::MAX)?, None));
        }
        let tokens = crate::lexer::tokenize_file(source, u32::MAX)?;
        let (program, errors) = crate::parser::parse_recovering(tokens, self.stack_budget);
        Ok((program, errors.into_iter().next()))
    }

    fn check_module(&mut self, file: &std::path::Path) -> Check<ModuleSig> {
        let shown = file.display().to_string();
        // An error of a module it uses already names that module
        let located = |mut d: Diagnostic| {
            d.file.get_or_insert_with(|| shown.clone());
            d
        };
        let source = self.modules.loader.read(file).map_err(|e| Diagnostic::new(format!("can't read the module '{shown}': {e}"), Span::default()))?;
        let (program, syntax) = self.parse_module(&source).map_err(located)?;
        let mut state = State::default();
        let modules = Modules {
            loader: self.modules.loader,
            cache: &mut *self.modules.cache,
            checking: {
                let mut c = self.modules.checking.clone();
                c.push(file.to_path_buf());
                c
            },
            classes: &mut *self.modules.classes,
            declared: &mut *self.modules.declared,
            declaring: self.modules.declaring.clone(),
            names: self.modules.names.as_deref_mut(),
            complete: self.modules.complete.as_deref_mut(),
            recover: self.modules.recover,
        };
        let host_functions = self.host_functions.clone();
        let mut checker = Checker::new(self.prelude, modules, &mut state, &host_functions);
        checker.stack_budget = self.stack_budget;
        checker.file = Some(shown.clone());
        checker.path = Some(file.to_path_buf());
        let mut public_fns = Vec::new();
        let mut public_consts = Vec::new();
        let mut public_types = Vec::new();
        for stmt in &program {
            if let StmtKind::Pub(inner) = &stmt.kind {
                match &inner.kind {
                    StmtKind::Function(d) => public_fns.push(d.name.clone()),
                    StmtKind::Let { name, .. } => public_consts.push(name.clone()),
                    StmtKind::Class(d) => public_types.push(d.name.clone()),
                    _ => {}
                }
            }
        }
        for stmt in &program {
            let inner = match &stmt.kind {
                StmtKind::Pub(inner) => &**inner,
                _ => stmt,
            };
            let declaration = matches!(
                inner.kind,
                StmtKind::Use(_) | StmtKind::Function(_) | StmtKind::Let { .. } | StmtKind::Class(_)
            );
            if !declaration {
                return Err(located(Diagnostic::new(
                    "an imported module can only have declarations at its top level: functions, `const`, `let`, classes, interfaces and enums",
                    stmt.span,
                )));
            }
        }
        let result = checker.block(&program);
        checker.finish_names();
        checker.finish_completion();
        // A syntax error comes first, when the code is checked while it's written
        if let Some(error) = syntax {
            return Err(located(error));
        }
        result.map_err(located)?;
        let globals = &checker.scopes[1];
        let mut sig = ModuleSig { path: shown, ..Default::default() };
        for name in public_fns {
            if let Some(f) = globals.funcs.get(&name) {
                sig.functions.insert(name.clone(), f.clone());
            }
            if let Some(p) = globals.places.get(&name) {
                sig.places.insert(name, p.clone());
            }
        }
        for name in public_types {
            if let Some(t) = globals.types.get(&name) {
                sig.types.insert(name.clone(), t.clone());
            }
            if let Some(f) = globals.funcs.get(&name) {
                sig.functions.insert(name, f.clone());
            }
        }
        for name in public_consts {
            if let Some(v) = globals.vars.get(&name) {
                // Unknown types can't leave the module: they become `any`
                let ty = checker.resolve(&v.ty);
                if let Some(p) = &v.place {
                    sig.places.insert(format!("${name}"), p.clone());
                }
                sig.constants.insert(name, forget_vars(&ty));
            }
        }
        Ok(sig)
    }

    // ---- Conditions and narrowing ----

    /// Checks a condition, and returns what it proves when it's true and when it's false.
    fn condition(&mut self, cond: &Expr) -> Check<(Facts, Facts)> {
        let facts = self.facts(cond)?;
        Ok(facts)
    }

    fn facts(&mut self, cond: &Expr) -> Check<(Facts, Facts)> {
        match &cond.kind {
            ExprKind::Unary { op: UnaryOp::Not, operand } => {
                let (yes, no) = self.facts(operand)?;
                Ok((no, yes))
            }
            ExprKind::Binary { op: BinaryOp::And, left, right } => {
                let (ly, _) = self.facts(left)?;
                let (ry, _) = self.scoped(&ly, |c| c.facts(right))?;
                Ok(([ly, ry].concat(), Vec::new()))
            }
            ExprKind::Binary { op: BinaryOp::Or, left, right } => {
                let (_, ln) = self.facts(left)?;
                let (_, rn) = self.scoped(&ln, |c| c.facts(right))?;
                Ok((Vec::new(), [ln, rn].concat()))
            }
            ExprKind::Binary { op: op @ (BinaryOp::Eq | BinaryOp::NotEq), left, right } => {
                self.bool_expr(cond)?;
                let var = match (&left.kind, &right.kind) {
                    (ExprKind::Variable(v), ExprKind::Null) | (ExprKind::Null, ExprKind::Variable(v)) => v,
                    _ => return Ok((Vec::new(), Vec::new())),
                };
                let Some(declared) = self.lookup_var(var) else { return Ok((Vec::new(), Vec::new())) };
                let not_null = vec![(var.clone(), self.non_null(&declared))];
                let null = vec![(var.clone(), Ty::Null)];
                Ok(if *op == BinaryOp::Eq { (null, not_null) } else { (not_null, null) })
            }
            ExprKind::TypeCheck { receiver, ty, check: TypeCheckKind::Is, nullable: false } => {
                self.bool_expr(cond)?;
                if let ExprKind::Variable(v) = &receiver.kind {
                    let t = self.ty(ty)?;
                    return Ok((vec![(v.clone(), t)], Vec::new()));
                }
                Ok((Vec::new(), Vec::new()))
            }
            _ => {
                let t = self.expr(cond, Some(&Ty::Bool))?;
                if !self.assignable(&t, &Ty::Bool) {
                    let t = self.resolve(&t);
                    return self.fail(format!("this must be a bool, but it's {}", a(&t)), cond.span);
                }
                Ok((Vec::new(), Vec::new()))
            }
        }
    }

    fn bool_expr(&mut self, e: &Expr) -> Check<()> {
        let t = self.expr(e, Some(&Ty::Bool))?;
        if !self.assignable(&t, &Ty::Bool) {
            let t = self.resolve(&t);
            return self.fail(format!("the condition must be a bool, but this is {}", a(&t)), e.span);
        }
        Ok(())
    }

    // ---- Expressions ----

    fn guard(&mut self, span: Span) -> Check<()> {
        if crate::stack::position().abs_diff(self.stack_base) > self.stack_budget {
            return self.fail("the code is nested too deeply", span);
        }
        Ok(())
    }

    fn expr(&mut self, e: &Expr, expected: Option<&Ty>) -> Check<Ty> {
        let span = e.span;
        self.guard(span)?;
        if self.around_cursor(span) {
            self.snapshot();
        }
        Ok(match &e.kind {
            ExprKind::TypeArgs { name, args } => {
                let Some(class) = self.class_named(name) else {
                    return self.fail(format!("there's no class named `{name}`"), span);
                };
                self.note_class(&class, span, span.start, name);
                let ctor = class.0.body.borrow().constructor.clone();
                let Some(ctor) = ctor.filter(|_| !class.0.interface && !class.0.is_enum) else {
                    return self.fail(format!("`{name}` doesn't create objects"), span);
                };
                if args.len() != class.0.generics.len() {
                    let n = class.0.generics.len();
                    return self.fail(format!("`{name}` takes {n} type parameter{}", if n == 1 { "" } else { "s" }), span);
                }
                let given = args.iter().map(|a| self.ty(a)).collect::<Check<Vec<_>>>()?;
                Ty::Func(match Ty::Func(ctor).substitute(&class.map(&given)) {
                    Ty::Func(f) => f,
                    _ => unreachable!(),
                })
            }
            ExprKind::This => match self.this.last() {
                Some(this) if !this.is_static => this.class.this_type(),
                Some(_) => return self.fail("a static method has no `@`: it belongs to the class, not to an object", span),
                None => return self.fail("`@` can only be used in the methods and the constructor of a class", span),
            },
            ExprKind::Field { target, name } => self.field(target, name, span)?,
            ExprKind::Static { class, name, variable } => self.static_member(class, name, *variable, span)?,
            ExprKind::SuperNew(args) => {
                let Some(this) = self.this.last().cloned().filter(|t| t.constructor) else {
                    return self.fail("`@super(...)` can only be used in a constructor", span);
                };
                let parent = this.class.0.body.borrow().parent.clone();
                let exception = this.class.0.body.borrow().exception.clone();
                let Some(parent) = parent else {
                    if let Some(kind) = exception {
                        // The built-in exception's constructor takes the message
                        let sig = FnTy { params: vec![Ty::Str], names: vec!["message".into()], required: 0, variadic: false, ret: Ty::Void, generics: Vec::new() };
                        self.call(&sig, &HashMap::new(), args, &kind, span)?;
                        return Ok(Ty::Void);
                    }
                    return self.fail(format!("`{}` has no superclass for `@super(...)`", this.class.0.name), span);
                };
                let ctor = parent.0.body.borrow().constructor.clone();
                let parent_args = this.class.0.body.borrow().parent_args.clone();
                match ctor {
                    Some(sig) => {
                        let Ty::Func(sig) = Ty::Func(sig).substitute(&parent.map(&parent_args)) else { unreachable!() };
                        self.call(&sig, &HashMap::new(), args, &parent.0.name, span)?;
                    }
                    None => self.args_loosely(args)?,
                }
                Ty::Void
            }
            ExprKind::SuperMethod { method, args } => {
                let Some(this) = self.this.last().cloned().filter(|t| !t.is_static) else {
                    return self.fail("`@super` can only be used in the methods and the constructor of a class", span);
                };
                let parent = this.class.0.body.borrow().parent.clone();
                let Some(parent) = parent else {
                    return self.fail(format!("`{}` has no superclass for `@super`", this.class.0.name), span);
                };
                let Some((m, owner)) = parent.find_method(method) else {
                    return self.fail(format!("`{}` has no method `{method}`", parent.0.name), span);
                };
                let parent_args = this.class.0.body.borrow().parent_args.clone();
                let mut map = parent.member_map(&parent_args, &owner);
                map.insert(Rc::from("Self"), this.class.this_type());
                if self.recording() {
                    let show = names::Show::Func { name: method.clone(), ty: Ty::Func(m.ty.clone()).substitute(&map) };
                    let place = names::class_place(&owner, Some(method), method);
                    self.note(span, span.start, "", method, show, Some(format!("A method of `{}`.", owner.0.name)), place);
                }
                self.call(&m.ty, &map, args, method, span)?
            }
            ExprKind::Int(_) | ExprKind::Float(_) => Ty::Number,
            ExprKind::Str(_) => Ty::Str,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Null => Ty::Null,
            ExprKind::Symbol(_) => Ty::Symbol,
            ExprKind::Regex { .. } => Ty::Regex,
            ExprKind::Interpolated(parts) => {
                for p in parts {
                    if let StringPart::Expr(x) = p {
                        self.expr(x, None)?;
                    }
                }
                Ty::Str
            }
            ExprKind::Command(parts) => {
                for p in parts {
                    if let StringPart::Expr(x) = p {
                        let t = self.expr(x, None)?;
                        let t = self.resolve(&t);
                        let ok = match &t {
                            Ty::List(inner) => matches!(**inner, Ty::Str | Ty::Number | Ty::Bool | Ty::Symbol | Ty::Any | Ty::Var(_)),
                            other => matches!(other, Ty::Str | Ty::Number | Ty::Bool | Ty::Symbol | Ty::Any | Ty::Var(_)),
                        };
                        if !ok {
                            return self.fail(format!("{} can't be used in a command", a(&t)), x.span);
                        }
                    }
                }
                Ty::Command
            }
            ExprKind::List { elem_type, items } => {
                let mut elem = match elem_type {
                    Some(t) => self.ty(t)?,
                    None => match expected.map(|t| self.shallow(t)) {
                        Some(Ty::List(e)) => *e,
                        _ => self.fresh(),
                    },
                };
                let fixed = elem_type.is_some();
                for item in items {
                    let t = self.expr(item, Some(&elem))?;
                    if fixed {
                        if !self.assignable(&t, &elem) {
                            let t = self.resolve(&t);
                            return self.fail(format!("this list holds {}, not {}", plural(&elem), a(&t)), item.span);
                        }
                    } else {
                        elem = self.join(&elem, &t);
                    }
                }
                Ty::List(Box::new(elem))
            }
            ExprKind::Map { types, entries } => {
                let (mut k, mut v) = match types {
                    Some((kt, vt)) => (self.ty(kt)?, self.ty(vt)?),
                    None => match expected.map(|t| self.shallow(t)) {
                        Some(Ty::Map(k, v)) => (*k, *v),
                        _ => (self.fresh(), self.fresh()),
                    },
                };
                for (key, value) in entries {
                    let kt = self.expr(key, Some(&k))?;
                    let vt = self.expr(value, Some(&v))?;
                    if types.is_some() {
                        if !self.assignable(&kt, &k) || !self.assignable(&vt, &v) {
                            return self.fail(format!("this map holds {} keys and {} values", k, v), key.span);
                        }
                    } else {
                        k = self.join(&k, &kt);
                        v = self.join(&v, &vt);
                    }
                }
                Ty::Map(Box::new(k), Box::new(v))
            }
            ExprKind::Variable(name) => match self.lookup_var(name) {
                Some(t) => {
                    if self.recording() {
                        let info = self.declared_var(name);
                        let constant = info.as_ref().is_some_and(|i| i.constant);
                        let show = names::Show::Var { constant, name: name.clone(), ty: t.clone() };
                        let about = info.as_ref().and_then(|i| i.about.clone());
                        self.note(span, span.start, "", &format!("${name}"), show, about, info.and_then(|i| i.place));
                    }
                    t
                }
                None => return self.fail(format!("`${name}` isn't declared"), span),
            },
            // A class's constructor, read when it's used: in modules that use each
            // other, it's only known once the class's signature is complete
            ExprKind::Function(name)
                if !self.not_declared_yet(name)
                    && self.class_named(name).is_some_and(|c| !c.0.interface && !c.0.is_enum && !c.0.command && c.0.body.borrow().constructor.is_some()) =>
            {
                let class = self.class_named(name).unwrap();
                self.note_class(&class, span, span.start, name);
                Ty::Func(class.0.body.borrow().constructor.clone().unwrap())
            }
            ExprKind::Function(name) => match self.lookup_func(name) {
                Some(_) if self.not_declared_yet(name) => {
                    return self.fail(
                        format!("`{name}` is declared further down, so it can't be used here yet. Other functions can call it from anywhere"),
                        span,
                    );
                }
                Some(f) => {
                    if self.recording() {
                        let place = self.function_place(name);
                        let show = names::Show::Func { name: name.clone(), ty: Ty::Func(f.clone()) };
                        let about = place.is_none().then(|| "A built-in function.".to_string());
                        self.note(span, span.start, "", name, show, about, place);
                    }
                    Ty::Func(f)
                }
                None if self.class_named(name).is_some_and(|c| c.0.interface) => {
                    return self.fail(format!("`{name}` is an interface: it doesn't create objects, classes implement it"), span);
                }
                None if self.class_named(name).is_some_and(|c| c.0.command) => {
                    return self.fail(format!("`{name}` is a command: its object comes from `{name}::parse()`"), span);
                }
                None if self.class_named(name).is_some_and(|c| c.0.is_enum) => {
                    return self.fail(format!("`{name}` is an enum: its values are created where it's declared *(i.e. `{name}::...`)*"), span);
                }
                None => return self.fail(format!("there's no function named `{name}`. Functions must be defined before they're used"), span),
            },
            // `fs::` while it's being written
            ExprKind::ModuleFunction { name, .. } if name.is_empty() => Ty::Any,
            ExprKind::ModuleFunction { module, name } => {
                let Some(m) = self.lookup_module(module) else {
                    return self.fail(format!("there's no module named `{module}`. Import it with `use`"), span);
                };
                self.note_module(module, &m, span);
                match m.functions.get(name) {
                    Some(f) => {
                        if self.recording() {
                            let show = names::Show::Func { name: name.clone(), ty: Ty::Func(f.clone()) };
                            let place = m.places.get(name).cloned();
                            self.note(span, span.start, &format!("{module}::"), name, show, Some(format!("From `{}`.", names::module_name(&m.path))), place);
                        }
                        Ty::Func(f.clone())
                    }
                    None => return self.fail(format!("`{}` has no `pub` function named `{name}`", m.path), span),
                }
            }
            ExprKind::ModuleConstant { module, name } => {
                let Some(m) = self.lookup_module(module) else {
                    return self.fail(format!("there's no module named `{module}`. Import it with `use`"), span);
                };
                self.note_module(module, &m, span);
                match m.constants.get(name) {
                    Some(t) => {
                        if self.recording() {
                            let show = names::Show::Var { constant: true, name: name.clone(), ty: t.clone() };
                            let place = m.places.get(&format!("${name}")).cloned();
                            self.note(span, span.start, &format!("{module}::"), &format!("${name}"), show, Some(format!("From `{}`.", names::module_name(&m.path))), place);
                        }
                        t.clone()
                    }
                    None => return self.fail(format!("`{}` has no `pub` constant named `${name}`", m.path), span),
                }
            }
            ExprKind::Unary { op, operand } => {
                let t = self.expr(operand, None)?;
                let t = self.shallow(&t);
                if let (Ty::Object(..), UnaryOp::Neg) = (&t, op) {
                    return self.object_operator(&t, "INeg", "neg", "-", None, span);
                }
                let (want, ok) = match op {
                    UnaryOp::Not => ("a bool", matches!(t, Ty::Bool | Ty::Any | Ty::Var(_))),
                    _ => ("a number", matches!(t, Ty::Number | Ty::Any | Ty::Var(_))),
                };
                if !ok {
                    return self.fail(format!("`{}` needs {want}, and this is {}", op.symbol(), a(&t)), operand.span);
                }
                if *op == UnaryOp::Not { Ty::Bool } else { Ty::Number }
            }
            ExprKind::Binary { op, left, right } => self.binary(*op, left, right, span)?,
            ExprKind::PathTest { op, path } => {
                let t = self.expr(path, Some(&Ty::Str))?;
                if !self.assignable(&t, &Ty::Str) {
                    let t = self.resolve(&t);
                    return self.fail(format!("`{}` needs a path string, not {}", op.symbol(), a(&t)), path.span);
                }
                Ty::Bool
            }
            ExprKind::Range { start, end, .. } => {
                for part in std::iter::once(&**start).chain(end.as_deref()) {
                    let t = self.expr(part, Some(&Ty::Number))?;
                    if !self.assignable(&t, &Ty::Number) {
                        let t = self.resolve(&t);
                        return self.fail(format!("ranges need numbers, and this is {}", a(&t)), part.span);
                    }
                }
                Ty::Range
            }
            ExprKind::Ternary { cond, then, otherwise } => {
                let (yes, no) = self.condition(cond)?;
                let a_ty = self.scoped(&yes, |c| c.expr(then, expected))?;
                let b_ty = self.scoped(&no, |c| c.expr(otherwise, expected))?;
                self.join(&a_ty, &b_ty)
            }
            ExprKind::Assign { op, target, value } => self.assign(*op, target, value, span)?,
            ExprKind::Increment { target, .. } => {
                let t = self.expr(target, None)?;
                if !self.assignable(&t, &Ty::Number) {
                    let t = self.resolve(&t);
                    return self.fail(format!("`++` and `--` only work on numbers, and this is {}", a(&t)), span);
                }
                self.check_assignable_target(target)?;
                Ty::Number
            }
            ExprKind::Call { callee, args } => {
                let callee_ty = self.expr(callee, None)?;
                match self.shallow(&callee_ty) {
                    Ty::Func(f) => {
                        let name = match &callee.kind {
                            ExprKind::Function(n)
                            | ExprKind::ModuleFunction { name: n, .. }
                            | ExprKind::Static { name: n, .. }
                            | ExprKind::TypeArgs { name: n, .. } => n.clone(),
                            _ => "the function".into(),
                        };
                        self.call(&f, &HashMap::new(), args, &name, span)?
                    }
                    Ty::Any | Ty::Var(_) | Ty::Param(_) => {
                        self.args_loosely(args)?;
                        Ty::Any
                    }
                    other => return self.fail(format!("{} can't be called", a(&other)), callee.span),
                }
            }
            ExprKind::MethodCall { receiver, method, args, nullable } => self.method(receiver, method, args, *nullable, span)?,
            ExprKind::TypeCheck { receiver, ty, check, nullable } => {
                let r = self.expr(receiver, None)?;
                if !*nullable {
                    if let Ty::Nullable(_) = self.shallow(&r) {
                        // `.is()` and `.as()` work on `null` too: `.as(?string)` accepts it
                    }
                }
                let t = self.ty(ty)?;
                if matches!(t, Ty::Func(_)) {
                    return self.fail("function types can't be checked while the script runs, because types are removed before it runs", ty.span);
                }
                match check {
                    TypeCheckKind::Is => Ty::Bool,
                    TypeCheckKind::As if *nullable => t.nullable(),
                    TypeCheckKind::As => t,
                }
            }
            ExprKind::Index { target, index, nullable } => self.index(target, index, *nullable)?,
            ExprKind::Lambda(decl) => self.lambda(decl, expected, span)?,
            ExprKind::If { cond, then, otherwise } => {
                let (yes, no) = self.condition(cond)?;
                let a_ty = self.scoped(&yes, |c| c.branch(then, expected))?;
                let b_ty = self.scoped(&no, |c| c.branch(otherwise, expected))?;
                self.join(&a_ty, &b_ty)
            }
            ExprKind::Switch(switch) => self.switch(switch, true)?,
            ExprKind::Try(inner) => {
                let t = self.expr(inner, None)?;
                Ty::Result(Box::new(t), Box::new(Ty::Exception(Rc::from("Exception"))))
            }
            ExprKind::Throw(value) => {
                self.thrown(value)?;
                // It never gives a value, so it takes whatever type is expected there
                match expected {
                    Some(t) => t.clone(),
                    None => self.fresh(),
                }
            }
        })
    }

    fn branch(&mut self, branch: &Branch, expected: Option<&Ty>) -> Check<Ty> {
        match branch {
            Branch::Expr(e) => self.expr(e, expected),
            Branch::Block(stmts, _) => {
                self.block(stmts)?;
                // The value is the `yield`, checked as part of the block
                Ok(yield_type(self, stmts))
            }
        }
    }

    /// A `switch` used as a value without a `default` must have a case for
    /// every value: both bools, or every value of an enum.
    fn covers_every_value(&mut self, switch: &Switch, subject: &Ty) -> Check<()> {
        let bools = |b: bool| switch.cases.iter().any(|c| c.patterns.iter().any(|p| matches!(p, Pattern::Value(Expr { kind: ExprKind::Bool(v), .. }) if *v == b)));
        if bools(true) && bools(false) {
            return Ok(());
        }
        let Ty::Object(class, _) = self.non_null(subject) else {
            return self.fail("a `switch` used as a value must match every value: add a `default`", switch.subject.span);
        };
        let subcommands = class.0.body.borrow().subcommands.clone();
        if !subcommands.is_empty() {
            // A case for each subcommand
            let mut missing: Vec<ClassRef> = subcommands;
            for case in &switch.cases {
                for pattern in &case.patterns {
                    if let Pattern::Type(t) = pattern {
                        if let Ok(Ty::Object(c, _)) = self.ty(t) {
                            missing.retain(|m| *m != c);
                        }
                    }
                }
            }
            if missing.is_empty() {
                return Ok(());
            }
            let names: Vec<String> = missing.iter().map(|m| format!("`{}`", m.0.name)).collect();
            return self.fail(format!("this `switch` has no case for {}: add them, or a `default`", names.join(", ")), switch.subject.span);
        }
        if !class.0.is_enum {
            return self.fail("a `switch` used as a value must match every value: add a `default`", switch.subject.span);
        }
        let mut missing: Vec<String> = class.0.body.borrow().values.clone();
        for case in &switch.cases {
            for pattern in &case.patterns {
                if let Pattern::Value(Expr { kind: ExprKind::Static { class: c, name, variable: false }, .. }) = pattern {
                    if self.class_named(c).is_some_and(|c| c == class) {
                        missing.retain(|m| m != name);
                    }
                }
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        let names: Vec<String> = missing.iter().map(|m| format!("`{}::{m}`", class.0.name)).collect();
        self.fail(format!("this `switch` has no case for {}: add them, or a `default`", names.join(", ")), switch.subject.span)
    }

    fn switch(&mut self, switch: &Switch, as_value: bool) -> Check<Ty> {
        let subject = self.expr(&switch.subject, None)?;
        let subject_var = match &switch.subject.kind {
            ExprKind::Variable(v) => Some(v.clone()),
            // `switch ($args.subcommand)`: the field of a variable is narrowed too
            ExprKind::Field { target, name } => match &target.kind {
                ExprKind::Variable(v) => Some(format!("{v}.{name}")),
                _ => None,
            },
            _ => None,
        };
        let mut result: Option<Ty> = None;
        let mut bodies: Vec<(Facts, &CaseBody)> = Vec::new();
        for case in &switch.cases {
            let mut facts = Vec::new();
            for pattern in &case.patterns {
                match pattern {
                    Pattern::Value(v) => {
                        let t = self.expr(v, None)?;
                        let t = self.resolve(&t);
                        let s = self.resolve(&subject);
                        if t != Ty::Range && !could_be_equal(&s, &t) {
                            return self.fail(format!("this case can never match: the value is {}, and the case is {}", a(&s), a(&t)), v.span);
                        }
                    }
                    Pattern::Type(ty) => {
                        let t = self.ty(ty)?;
                        if let (Some(var), 1) = (&subject_var, case.patterns.len()) {
                            facts.push((var.clone(), t));
                        }
                    }
                }
            }
            bodies.push((facts, &case.body));
        }
        if let Some(d) = &switch.default {
            bodies.push((Vec::new(), d));
        } else if as_value {
            self.covers_every_value(switch, &subject)?;
        }
        for (facts, body) in bodies {
            let t = self.scoped(&facts, |c| match body {
                CaseBody::Value(b) => c.branch(b, None),
                CaseBody::Stmt(s) => match &s.kind {
                    StmtKind::Block(stmts) => c.block(stmts).map(|_| Ty::Void),
                    _ => c.stmt(s).map(|_| Ty::Void),
                },
            })?;
            if as_value {
                result = Some(match result {
                    None => t,
                    Some(prev) => self.join(&prev, &t),
                });
            }
        }
        Ok(result.unwrap_or(Ty::Void))
    }

    fn lambda(&mut self, decl: &Rc<FunctionDecl>, expected: Option<&Ty>, span: Span) -> Check<Ty> {
        let expected_fn = match expected.map(|t| self.shallow(t)) {
            Some(Ty::Func(f)) => Some(f),
            _ => None,
        };
        let mut params = Vec::new();
        let mut names = Vec::new();
        let mut required = 0;
        let mut variadic = false;
        for (i, p) in decl.params.iter().enumerate() {
            let t = match &p.ty {
                Some(t) => self.ty(t)?,
                // An untyped parameter takes its type from where the function goes
                None => expected_fn.as_ref().and_then(|f| f.params.get(i).cloned()).unwrap_or(Ty::Any),
            };
            params.push(t);
            names.push(p.name.clone());
            if p.variadic {
                variadic = true;
            } else if p.default.is_none() && required == i {
                required = i + 1;
            }
        }
        let ret_var = self.fresh();
        let sig = FnTy { params: params.clone(), names: names.clone(), required, variadic, ret: ret_var.clone(), generics: Vec::new() };
        self.push();
        let ret = self.body(decl, &sig, Return::Inferred(ret_var), span);
        self.pop();
        let ret = ret?;
        Ok(Ty::Func(Rc::new(FnTy { params, names, required, variadic, ret, generics: Vec::new() })))
    }

    fn args_loosely(&mut self, args: &[Arg]) -> Check<()> {
        for arg in args {
            self.expr(&arg.value, None)?;
        }
        Ok(())
    }

    /// Checks the arguments of a call against a signature, and returns its result type.
    fn call(&mut self, sig: &FnTy, receiver: &HashMap<Rc<str>, Ty>, args: &[Arg], name: &str, span: Span) -> Check<Ty> {
        let mut map = receiver.clone();
        for g in &sig.generics {
            if !map.contains_key(g) {
                let v = self.fresh();
                map.insert(g.clone(), v);
            }
        }
        let params: Vec<Ty> = sig.params.iter().map(|p| p.substitute(&map)).collect();
        // The constructor that takes every field names them without `$`
        let fields = matches!(&sig.ret, Ty::Object(c, _) if (*c.0.name == *name || c.0.body.borrow().values.iter().any(|v| v == name)) && !c.0.body.borrow().own_constructor);
        let show = |n: &String| if fields { format!("`{n}`") } else { format!("`${n}`") };
        let fixed = if sig.variadic { params.len() - 1 } else { params.len() };
        let mut given = vec![false; params.len()];
        let mut position = 0;
        let mut spread = false;
        for arg in args {
            if arg.spread {
                let t = self.expr(&arg.value, None)?;
                match self.shallow(&t) {
                    Ty::List(e) => {
                        if sig.variadic {
                            let elem = params[fixed].clone();
                            if !self.assignable(&e, &elem) {
                                let e = self.resolve(&e);
                                return self.fail(format!("`{name}` takes {} here, but the list holds {}", plural(&elem), plural(&e)), arg.value.span);
                            }
                        }
                    }
                    Ty::Any | Ty::Var(_) => {}
                    other => return self.fail(format!("only lists can be spread with `...`, not {}", a(&other)), arg.value.span),
                }
                spread = true;
                continue;
            }
            let index = match &arg.name {
                Some(n) => match sig.names.iter().position(|p| p == n) {
                    Some(i) if !(sig.variadic && i == fixed) => i,
                    _ if sig.names.is_empty() => return self.fail(format!("`{name}` doesn't take named arguments"), arg.value.span),
                    _ => return self.fail(format!("`{name}` has no parameter named `{n}`"), arg.value.span),
                },
                None => {
                    let i = position.min(params.len().saturating_sub(1));
                    if position >= fixed && !sig.variadic {
                        if !spread {
                            return self.fail(format!("`{name}` takes {} arguments, but got more", params.len()), arg.value.span);
                        }
                    }
                    position += 1;
                    if position > fixed && sig.variadic { fixed } else { i }
                }
            };
            if index < given.len() {
                if given[index] && !(sig.variadic && index == fixed) {
                    return self.fail(format!("`{}` was given twice", sig.names.get(index).cloned().unwrap_or_default()), arg.value.span);
                }
                given[index] = true;
            }
            let expected = params.get(index).cloned().unwrap_or(Ty::Any);
            let found = self.expr(&arg.value, Some(&expected))?;
            if !self.assignable(&found, &expected) {
                let (found, expected) = (self.resolve(&found), self.resolve(&expected));
                let param = sig.names.get(index).map(show).unwrap_or_else(|| format!("argument {}", index + 1));
                return self.fail(format!("{param} of `{name}` must be {}, but this is {}", a(&expected), a(&found)), arg.value.span);
            }
        }
        if !spread {
            for i in 0..sig.required.min(given.len()) {
                if !given[i] {
                    let param = sig.names.get(i).map(show).unwrap_or_else(|| format!("argument {}", i + 1));
                    return self.fail(format!("`{name}` needs a value for {param}"), span);
                }
            }
        }
        Ok(sig.ret.substitute(&map))
    }

    fn method(&mut self, receiver: &Expr, method: &str, args: &[Arg], nullable: bool, span: Span) -> Check<Ty> {
        let receiver_expr = receiver;
        let r = self.expr(receiver, None)?;
        self.note_receiver(receiver.span.end, span, &r);
        let mut r = self.shallow(&r);
        if let Ty::Nullable(inner) = &r {
            if !nullable {
                return self.fail(
                    format!("this can be `null`, so `.{method}()` can't always be called. Use `?.{method}()`, or check for `null` first"),
                    receiver.span,
                );
            }
            r = self.shallow(inner);
        }
        if let Ty::Object(class, class_args) = &r {
            if let Some((m, owner)) = class.find_method(method) {
                self.check_access(m.public, &owner, &format!("`.{method}()`"), span)?;
                // The class's type parameters, and in an interface's methods, `Self` is the class of the object
                let mut receiver = class.member_map(class_args, &owner);
                receiver.insert(Rc::from("Self"), r.clone());
                let overloads = owner.0.body.borrow().overloads.get(method).cloned();
                let chosen = match overloads {
                    Some(all) => self.pick_overload(&all, &receiver, args)?,
                    None => m,
                };
                if self.recording() {
                    let show = names::Show::Func { name: method.to_string(), ty: Ty::Func(chosen.ty.clone()).substitute(&receiver) };
                    let place = names::class_place(&owner, Some(method), method);
                    self.note(span, receiver_expr.span.end, "", method, show, Some(format!("A method of `{}`.", owner.0.name)), place);
                }
                return self.call(&chosen.ty, &receiver, args, method, span);
            }
            let exception = class.0.body.borrow().exception.clone();
            match exception {
                // The methods every exception has
                Some(kind) => r = Ty::Exception(kind),
                None if !matches!(method, "toString" | "typeName" | "equals" | "same?") => {
                    return self.fail(format!("`{}` has no method `.{method}()`", class.0.name), span);
                }
                None => {}
            }
        }
        let (key, params): (&str, Vec<Ty>) = match &r {
            Ty::Str => ("string", vec![]),
            Ty::Number => ("number", vec![]),
            Ty::Bool => ("bool", vec![]),
            Ty::Symbol => ("symbol", vec![]),
            Ty::Regex => ("regex", vec![]),
            Ty::Range => ("range", vec![]),
            Ty::Command => ("command", vec![]),
            Ty::List(e) => ("list", vec![(**e).clone()]),
            Ty::Map(k, v) => ("map", vec![(**k).clone(), (**v).clone()]),
            Ty::Result(t, e) => ("result", vec![(**t).clone(), (**e).clone()]),
            Ty::Exception(_) => ("exception", vec![]),
            _ => {
                // Not known: anything goes
                self.args_loosely(args)?;
                return Ok(Ty::Any);
            }
        };
        let found = self.prelude.types.get(key).and_then(|t| t.methods.get(method).cloned().map(|m| (m, t.params.clone())));
        let found = found.or_else(|| {
            let any = self.prelude.types.get("any")?;
            any.methods.get(method).cloned().map(|m| (m, Vec::new()))
        });
        let Some((sig, names)) = found else {
            return self.fail(format!("{} has no method `.{method}()`", a(&self.resolve(&r))), span);
        };
        // The string methods have a version for strings and one for regexes
        if key == "string" {
            if let Some(first) = args.first() {
                let regex_version = match method {
                    "replace" => Some("replaceMatches"),
                    "replaceFirst" => Some("replaceFirstMatch"),
                    "replaceLast" => Some("replaceLastMatch"),
                    "remove" => Some("removeMatches"),
                    "split" => Some("splitMatches"),
                    _ => None,
                };
                if let (Some(other), ExprKind::Regex { .. }) = (regex_version, &first.value.kind) {
                    return self.fail(format!("`.{method}()` takes a string. For a regex, use `.{other}()`"), first.value.span);
                }
                let string_version = match method {
                    "replaceMatches" => Some("replace"),
                    "replaceFirstMatch" => Some("replaceFirst"),
                    "replaceLastMatch" => Some("replaceLast"),
                    "removeMatches" => Some("remove"),
                    "splitMatches" => Some("split"),
                    _ => None,
                };
                if let (Some(other), ExprKind::Str(_) | ExprKind::Interpolated(_)) = (string_version, &first.value.kind) {
                    return self.fail(format!("`.{method}()` takes a regex. For a string, use `.{other}()`"), first.value.span);
                }
            }
        }
        let map: HashMap<Rc<str>, Ty> = names.into_iter().zip(params).collect();
        if self.recording() {
            let show = names::Show::Func { name: method.to_string(), ty: Ty::Func(sig.clone()).substitute(&map) };
            self.note(span, receiver_expr.span.end, "", method, show, Some(format!("A method of `{key}`.")), None);
        }
        let ret = self.call(&sig, &map, args, &format!(".{method}()"), span)?;
        Ok(if nullable { ret.nullable() } else { ret })
    }

    fn index(&mut self, target: &Expr, index: &Expr, nullable: bool) -> Check<Ty> {
        let t = self.expr(target, None)?;
        let mut t = self.shallow(&t);
        if let Ty::Nullable(inner) = &t {
            if !nullable {
                return self.fail("this can be `null`, so it can't always be indexed. Use `?[...]`, or check for `null` first", target.span);
            }
            t = self.shallow(inner);
        }
        if let Ty::Object(..) = &t {
            let i = self.expr(index, None)?;
            return self.object_operator(&t, "IIndex", "get", "[]", Some((&i, index.span)), target.span);
        }
        let result = match &t {
            Ty::List(e) | Ty::Nullable(e) if matches!(t, Ty::List(_)) => {
                let i = self.expr(index, None)?;
                match self.shallow(&i) {
                    Ty::Range => t.clone(),
                    // Past the end it's `null`, like a missing key
                    Ty::Number | Ty::Any | Ty::Var(_) => (**e).clone().nullable(),
                    other => return self.fail(format!("list indexes are numbers, not {}", a(&other)), index.span),
                }
            }
            Ty::Str => {
                let i = self.expr(index, None)?;
                match self.shallow(&i) {
                    Ty::Range => Ty::Str,
                    Ty::Number | Ty::Any | Ty::Var(_) => Ty::Str.nullable(),
                    other => return self.fail(format!("string indexes are numbers, not {}", a(&other)), index.span),
                }
            }
            Ty::Map(k, v) => {
                let kt = self.expr(index, Some(k))?;
                if !self.assignable(&kt, k) {
                    let (kt, k) = (self.resolve(&kt), self.resolve(k));
                    return self.fail(format!("the keys of this map are {}, not {}", plural(&k), plural(&kt)), index.span);
                }
                // A missing key gives `null`
                (**v).clone().nullable()
            }
            Ty::Any | Ty::Var(_) | Ty::Param(_) => {
                self.expr(index, None)?;
                Ty::Any
            }
            other => return self.fail(format!("{} can't be indexed", a(other)), target.span),
        };
        Ok(if nullable { result.nullable() } else { result })
    }

    fn binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, span: Span) -> Check<Ty> {
        use BinaryOp::*;
        if matches!(op, And | Or) {
            let both_bools = |c: &mut Self, side: &Expr, t: Ty| -> Check<()> {
                if !c.assignable(&t, &Ty::Bool) {
                    let t = c.resolve(&t);
                    return c.fail(format!("both sides of `{}` must be bools, but this is {}", op.symbol(), a(&t)), side.span);
                }
                Ok(())
            };
            let l = self.expr(left, Some(&Ty::Bool))?;
            both_bools(self, left, l)?;
            // Narrowing: the right side only runs when the left one is true (`&&`) or false (`||`)
            let (yes, no) = self.facts(left)?;
            let facts = if op == And { yes } else { no };
            let r = self.scoped(&facts, |c| c.expr(right, Some(&Ty::Bool)))?;
            both_bools(self, right, r)?;
            return Ok(Ty::Bool);
        }
        let l = self.expr(left, None)?;
        let expected = if matches!(op, Coalesce) { Some(self.non_null(&l)) } else { None };
        let r = self.expr(right, expected.as_ref())?;
        let (ls, rs) = (self.shallow(&l), self.shallow(&r));
        // Objects get their operators from interfaces *(i.e. `IAdd`)*
        if matches!(op, Eq | NotEq) && (matches!(ls, Ty::Object(..)) || matches!(rs, Ty::Object(..))) {
            return Ok(Ty::Bool);
        }
        if let (Ty::Object(..), Some((interface, method))) = (&ls, crate::operators::for_binary(op)) {
            if !matches!(op, In | NotIn) {
                let result = self.object_operator(&ls, interface, method, op.symbol(), Some((&r, right.span)), span)?;
                return Ok(if interface == "IOrdered" { Ty::Bool } else { result });
            }
        }
        if let (Ty::Object(..), In | NotIn) = (&rs, op) {
            self.object_operator(&rs, "IContains", "contains", "-in", Some((&l, left.span)), span)?;
            return Ok(Ty::Bool);
        }
        let unknown = |t: &Ty| matches!(t, Ty::Any | Ty::Var(_) | Ty::Param(_));
        let both = |c: &mut Self, t: &Ty| c.assignable(&l, t) && c.assignable(&r, t);
        Ok(match op {
            Add => {
                if matches!(ls, Ty::Str) || matches!(rs, Ty::Str) {
                    if !both(self, &Ty::Str) {
                        let (l, r) = (self.resolve(&l), self.resolve(&r));
                        return self.fail(
                            format!("`+` joins two strings, but got {} and {}. Use an f-string instead *(i.e. f\"Age: {{$age}}\")*", a(&l), a(&r)),
                            span,
                        );
                    }
                    Ty::Str
                } else if unknown(&ls) && unknown(&rs) {
                    Ty::Any
                } else if both(self, &Ty::Number) {
                    Ty::Number
                } else {
                    let (l, r) = (self.resolve(&l), self.resolve(&r));
                    return self.fail(format!("`+` adds two numbers or joins two strings, but got {} and {}", a(&l), a(&r)), span);
                }
            }
            Sub | Mul | Div | Rem | Pow => {
                if !both(self, &Ty::Number) {
                    let (l, r) = (self.resolve(&l), self.resolve(&r));
                    return self.fail(format!("`{}` needs numbers, but got {} and {}", op.symbol(), a(&l), a(&r)), span);
                }
                Ty::Number
            }
            BitAnd | BitOr | BitXor if matches!(ls, Ty::Bool) || matches!(rs, Ty::Bool) => {
                if !both(self, &Ty::Bool) {
                    let (l, r) = (self.resolve(&l), self.resolve(&r));
                    return self.fail(format!("`{}` needs two bools or two numbers, but got {} and {}", op.symbol(), a(&l), a(&r)), span);
                }
                Ty::Bool
            }
            Shl if matches!(ls, Ty::List(_)) => {
                let Ty::List(e) = ls else { unreachable!() };
                if !self.assignable(&r, &e) {
                    let (e, r) = (self.resolve(&e), self.resolve(&r));
                    return self.fail(format!("this list holds {}, so {} can't be added to it", plural(&e), a(&r)), right.span);
                }
                l
            }
            BitAnd | BitOr | BitXor | Shl | Shr | UShr => {
                if !both(self, &Ty::Number) {
                    let (l, r) = (self.resolve(&l), self.resolve(&r));
                    return self.fail(format!("`{}` needs numbers, but got {} and {}", op.symbol(), a(&l), a(&r)), span);
                }
                Ty::Number
            }
            Eq | NotEq => {
                let (lr, rr) = (self.resolve(&l), self.resolve(&r));
                if !could_be_equal(&lr, &rr) {
                    return self.fail(
                        format!("{} and {} can never be equal. Convert one first *(i.e. `$text.toNumber() == 1`)*", a(&lr), a(&rr)),
                        span,
                    );
                }
                Ty::Bool
            }
            Lt | Gt | LtEq | GtEq => {
                let ok = (unknown(&ls) || unknown(&rs))
                    || (matches!(ls, Ty::Number) && matches!(rs, Ty::Number))
                    || (matches!(ls, Ty::Str) && matches!(rs, Ty::Str));
                if !ok {
                    let (l, r) = (self.resolve(&l), self.resolve(&r));
                    return self.fail(format!("`{}` compares two numbers or two strings, but got {} and {}", op.symbol(), a(&l), a(&r)), span);
                }
                Ty::Bool
            }
            Coalesce => {
                let left_inner = self.non_null(&l);
                match self.shallow(&r) {
                    Ty::Null => left_inner.nullable(),
                    Ty::Nullable(inner) => self.join(&left_inner, &inner).nullable(),
                    other => self.join(&left_inner, &other),
                }
            }
            In | NotIn => {
                match &rs {
                    Ty::List(_) | Ty::Map(..) | Ty::Range => {}
                    t if unknown(t) => {}
                    other => return self.fail(format!("`{}` needs a list, a map or a range on its right, not {}", op.symbol(), a(other)), right.span),
                }
                Ty::Bool
            }
            Like | NotLike => {
                if !both(self, &Ty::Str) {
                    let (l, r) = (self.resolve(&l), self.resolve(&r));
                    return self.fail(format!("`{}` needs two strings, but got {} and {}", op.symbol(), a(&l), a(&r)), span);
                }
                Ty::Bool
            }
            Match | NotMatch => {
                if !self.assignable(&l, &Ty::Str) {
                    let l = self.resolve(&l);
                    return self.fail(format!("`{}` needs a string on its left, not {}", op.symbol(), a(&l)), left.span);
                }
                if !matches!(rs, Ty::Str | Ty::Regex) && !unknown(&rs) {
                    let r = self.resolve(&r);
                    return self.fail(format!("`{}` needs a regex or a string on its right, not {}", op.symbol(), a(&r)), right.span);
                }
                Ty::Bool
            }
            And | Or => unreachable!(),
        })
    }

    fn assign(&mut self, op: Option<BinaryOp>, target: &Expr, value: &Expr, span: Span) -> Check<Ty> {
        let target_ty = match &target.kind {
            ExprKind::Variable(name) => {
                let Some(info) = self.declared_var(name) else {
                    return self.fail(format!("`${name}` isn't declared. Declare it with `let ${name} = ...;`"), target.span);
                };
                if self.recording() {
                    let show = names::Show::Var { constant: info.constant, name: name.clone(), ty: info.ty.clone() };
                    self.note(target.span, target.span.start, "", &format!("${name}"), show, info.about.clone(), info.place.clone());
                }
                if info.constant {
                    return self.fail(format!("`${name}` is a constant, it can't be changed"), target.span);
                }
                info.ty
            }
            ExprKind::Index { target: container, index, .. } => {
                let c = self.expr(container, None)?;
                match self.shallow(&c) {
                    // `$x[k] = v` on an object: `V` of the `ISetIndex<K, V>` it implements for `K`
                    object @ Ty::Object(..) => {
                        let k = self.expr(index, None)?;
                        self.set_index_type(&object, &k, index.span, target.span)?
                    }
                    Ty::List(e) => {
                        self.expr(index, Some(&Ty::Number))?;
                        *e
                    }
                    Ty::Map(k, v) => {
                        let kt = self.expr(index, Some(&k))?;
                        if !self.assignable(&kt, &k) {
                            let (kt, k) = (self.resolve(&kt), self.resolve(&k));
                            return self.fail(format!("the keys of this map are {}, not {}", plural(&k), plural(&kt)), index.span);
                        }
                        *v
                    }
                    Ty::Str => return self.fail("strings can't be changed. Build a new one instead", target.span),
                    Ty::Any | Ty::Var(_) => {
                        self.expr(index, None)?;
                        Ty::Any
                    }
                    other => return self.fail(format!("{} can't be indexed", a(&other)), target.span),
                }
            }
            ExprKind::Field { target: object, name } => {
                let ty = self.field(object, name, target.span)?;
                self.check_assignable_target(target)?;
                ty
            }
            ExprKind::Static { class, name, variable: true } => {
                let ty = self.static_member(class, name, true, target.span)?;
                self.check_assignable_target(target)?;
                ty
            }
            _ => return self.fail("only variables, indexes and fields can be assigned to", target.span),
        };
        let value_ty = match op {
            None => self.expr(value, Some(&target_ty))?,
            Some(op) => self.binary(op, target, value, span)?,
        };
        if !self.assignable(&value_ty, &target_ty) {
            let (v, t) = (self.resolve(&value_ty), self.resolve(&target_ty));
            let what = match &target.kind {
                ExprKind::Variable(name) => format!("`${name}`"),
                _ => "this".into(),
            };
            return self.fail(format!("{what} is {}, so it can't hold {}", a(&t), a(&v)), value.span);
        }
        // A new value replaces what a condition proved about the variable
        if let ExprKind::Variable(name) = &target.kind {
            for scope in &mut self.scopes {
                scope.narrowed.remove(name);
            }
            let declared = self.resolve(&target_ty);
            let assigned = self.resolve(&value_ty);
            if matches!(declared, Ty::Nullable(_)) && !matches!(assigned, Ty::Nullable(_) | Ty::Null | Ty::Any) {
                let narrowed = self.non_null(&declared);
                self.scopes.last_mut().unwrap().narrowed.insert(name.clone(), narrowed);
            }
        }
        Ok(value_ty)
    }

    fn check_assignable_target(&mut self, target: &Expr) -> Check<()> {
        if let ExprKind::Field { target: object, name } = &target.kind {
            let t = self.expr(object, None)?;
            if let Ty::Object(class, _) = self.shallow(&t) {
                if let Some((field, owner)) = class.find(|b| b.fields.iter().find(|f| f.name == *name).cloned()) {
                    let building = self.this.last().is_some_and(|t| t.constructor && t.class == owner) && matches!(object.kind, ExprKind::This);
                    if field.constant && !building {
                        return self.fail(format!("`{name}` is a `const` field of `{}`: it can only be set in its constructor", owner.0.name), target.span);
                    }
                }
            }
        }
        if let ExprKind::Static { class, name, variable: true } = &target.kind {
            if let Some(class) = self.class_named(class) {
                if class.0.body.borrow().statics.get(name).is_some_and(|f| f.constant) {
                    return self.fail(format!("`{}::${name}` is a constant, it can't be changed", class.0.name), target.span);
                }
            }
        }
        if let ExprKind::Variable(name) = &target.kind {
            if let Some(info) = self.declared_var(name) {
                if info.constant {
                    return self.fail(format!("`${name}` is a constant, it can't be changed"), target.span);
                }
            }
        }
        Ok(())
    }
}

/// The type of the `yield`s of a block, after it's been checked.
fn yield_type(checker: &mut Checker, stmts: &[Stmt]) -> Ty {
    let mut found: Option<Ty> = None;
    for stmt in stmts {
        if let StmtKind::Yield(e) = &stmt.kind {
            let t = expr_type_again(checker, e);
            found = Some(match found {
                None => t,
                Some(prev) => checker.join(&prev, &t),
            });
        }
    }
    found.unwrap_or(Ty::Any)
}

/// The type of an expression that was already checked, without reporting errors twice.
fn expr_type_again(checker: &mut Checker, e: &Expr) -> Ty {
    checker.push();
    let t = checker.expr(e, None).unwrap_or(Ty::Any);
    checker.pop();
    t
}

fn plural(ty: &Ty) -> String {
    match ty {
        Ty::Any | Ty::Var(_) => "any values".into(),
        other => format!("{other} values"),
    }
}

/// Removes unknown type variables, which only mean something inside one checker.
fn forget_vars(ty: &Ty) -> Ty {
    match ty {
        Ty::Var(_) => Ty::Any,
        Ty::List(e) => Ty::List(Box::new(forget_vars(e))),
        Ty::Map(k, v) => Ty::Map(Box::new(forget_vars(k)), Box::new(forget_vars(v))),
        Ty::Nullable(t) => Ty::Nullable(Box::new(forget_vars(t))),
        Ty::Result(t, e) => Ty::Result(Box::new(forget_vars(t)), Box::new(forget_vars(e))),
        Ty::Object(c, args) => Ty::Object(c.clone(), args.iter().map(forget_vars).collect()),
        other => other.clone(),
    }
}

/// `false` when values of these two types can never be equal *(i.e. a number and a string)*.
fn could_be_equal(a: &Ty, b: &Ty) -> bool {
    let strip = |t: &Ty| match t {
        Ty::Nullable(inner) => (**inner).clone(),
        other => other.clone(),
    };
    let nullable = |t: &Ty| matches!(t, Ty::Nullable(_) | Ty::Null);
    if nullable(a) && nullable(b) {
        return true;
    }
    if matches!(a, Ty::Null) || matches!(b, Ty::Null) {
        return nullable(a) || nullable(b) || matches!(a, Ty::Any | Ty::Var(_) | Ty::Param(_)) || matches!(b, Ty::Any | Ty::Var(_) | Ty::Param(_));
    }
    let (a, b) = (strip(a), strip(b));
    match (&a, &b) {
        (Ty::Any | Ty::Var(_) | Ty::Param(_), _) | (_, Ty::Any | Ty::Var(_) | Ty::Param(_)) => true,
        (Ty::Exception(_), Ty::Exception(_)) => true,
        _ => std::mem::discriminant(&a) == std::mem::discriminant(&b),
    }
}

/// `true` if running the statement never continues to the next one
/// *(`return`, `throw`, `break`, `continue`, `exit()`, or all branches doing it)*.
fn terminates(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Break | StmtKind::Continue | StmtKind::Yield(_) => true,
        StmtKind::Expr(Expr { kind: ExprKind::Call { callee, .. }, .. }) => {
            matches!(&callee.kind, ExprKind::Function(name) if name == "exit")
        }
        StmtKind::Block(stmts) => block_terminates(stmts),
        StmtKind::If { then, otherwise: Some(o), .. } => terminates(then) && terminates(o),
        StmtKind::While { cond: Expr { kind: ExprKind::Bool(true), .. }, body } => !contains_break(body),
        StmtKind::Switch(s) => {
            s.default.as_ref().is_some_and(case_terminates) && s.cases.iter().all(|c| case_terminates(&c.body))
        }
        StmtKind::Try { body, catches, finally } => {
            finally.as_ref().is_some_and(|f| block_terminates(f))
                || (block_terminates(body) && catches.iter().all(|c| block_terminates(&c.body)))
        }
        _ => false,
    }
}

fn case_terminates(body: &CaseBody) -> bool {
    match body {
        CaseBody::Stmt(s) => terminates(s),
        CaseBody::Value(_) => false,
    }
}

fn block_terminates(stmts: &[Stmt]) -> bool {
    stmts.iter().any(terminates)
}

fn contains_break(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Break => true,
        StmtKind::Block(stmts) => stmts.iter().any(contains_break),
        StmtKind::If { then, otherwise, .. } => contains_break(then) || otherwise.as_ref().is_some_and(|o| contains_break(o)),
        StmtKind::Switch(s) => s.cases.iter().any(|c| matches!(&c.body, CaseBody::Stmt(b) if contains_break(b))),
        StmtKind::Try { body, catches, finally } => {
            body.iter().any(contains_break) || catches.iter().any(|c| c.body.iter().any(contains_break)) || finally.as_ref().is_some_and(|f| f.iter().any(contains_break))
        }
        // A `break` inside an inner loop belongs to that loop
        _ => false,
    }
}
