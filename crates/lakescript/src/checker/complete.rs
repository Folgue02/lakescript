//! What can be written at a place of a file, for the completion of editors
//! *(see `ideas/tooling/lsp.md`)*: the names in scope there, and the members
//! of the value before a `.`.
//!
//! The checker keeps the last scope it was in before the place, while it
//! checks the file: the statement or expression around it, or the last
//! statement before it.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::names::{class_code, function_code, module_name};
use super::types::{ClassRef, Ty};
use super::{Checker, Scope, This};
use crate::token::Span;

/// What can be written at a place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Completion {
    /// The variables, functions, classes and modules in scope.
    pub scope: Vec<Item>,
    /// After `.`: the fields and methods of the value before it, if its type is known.
    pub members: Option<Vec<Item>>,
    /// After `@`, in a class: its fields and methods, also the private ones.
    pub own: Vec<Item>,
    /// After `name::`: what a module or a class here has, by that name.
    pub paths: HashMap<String, Vec<Item>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// What's written: `$count`, `greet`, `origin`, `RED`...
    pub label: String,
    pub kind: ItemKind,
    /// Its declaration, as Lakescript code *(i.e. `number $count`)*.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Variable,
    Constant,
    Function,
    Method,
    Field,
    Class,
    Interface,
    Enum,
    EnumValue,
    Module,
}

/// Where completion is wanted, and what was found there.
pub struct Request {
    /// `None` for the script being checked.
    pub file: Option<String>,
    /// In characters.
    pub offset: usize,
    pub result: Option<Completion>,
}

/// The scope at the place, as it was when the checker was there.
pub(super) struct Snapshot {
    scopes: Vec<Scope>,
    this: Vec<This>,
}

impl Checker<'_> {
    /// The place completion is wanted at, in the file being checked.
    pub(super) fn cursor(&self) -> Option<usize> {
        let request = self.modules.complete.as_ref()?;
        (request.file == self.file && !self.declaring).then_some(request.offset)
    }

    pub(super) fn around_cursor(&self, span: Span) -> bool {
        self.cursor().is_some_and(|c| span.start <= c && c <= span.end)
    }

    /// Keeps the scope as it is now, if the checker is around the place or
    /// before it. The last one kept is the scope at the place.
    pub(super) fn snapshot(&mut self) {
        self.snapshot = Some(Snapshot { scopes: self.scopes[1..].to_vec(), this: self.this.clone() });
    }

    /// Before a `.` *(i.e. `$point.` or `$point.di`)*: the members of the value.
    pub(super) fn note_receiver(&mut self, receiver_end: usize, span: Span, ty: &Ty) {
        // The `.` is after the receiver: a chain that goes on after the place isn't it
        if self.cursor().is_some_and(|c| receiver_end < c && c <= span.end) {
            let ty = self.resolve(ty);
            let ty = match ty {
                Ty::Nullable(inner) => *inner,
                other => other,
            };
            let accessible: Vec<ClassRef> = self.this.iter().map(|t| t.class.clone()).collect();
            self.members = self.members_of(&ty, &accessible);
        }
    }

    /// Writes down what was found at the place, once the check is done.
    pub(super) fn finish_completion(&mut self) {
        let Some(snapshot) = self.snapshot.take() else {
            if let (Some(members), Some(request)) = (self.members.take(), self.modules.complete.as_deref_mut()) {
                request.result.get_or_insert_with(Default::default).members = Some(members);
            }
            return;
        };
        if self.cursor().is_none() {
            return;
        }
        let mut scopes: Vec<&Scope> = snapshot.scopes.iter().rev().collect();
        scopes.push(&self.scopes[0]);
        let accessible: Vec<ClassRef> = snapshot.this.iter().map(|t| t.class.clone()).collect();
        let mut completion = Completion { members: self.members.take(), ..Default::default() };
        let mut seen = HashSet::new();
        let types: HashSet<&String> = scopes.iter().flat_map(|s| s.types.keys()).collect();
        for scope in &scopes {
            let mut vars: Vec<_> = scope.vars.iter().collect();
            vars.sort_by(|a, b| a.0.cmp(b.0));
            for (name, info) in vars {
                let kind = if info.constant { ItemKind::Constant } else { ItemKind::Variable };
                let detail = format!("{}{} ${name}", if info.constant { "const " } else { "" }, self.resolve(&info.ty));
                push(&mut completion.scope, &mut seen, format!("${name}"), kind, detail);
            }
            for (name, f) in &scope.funcs {
                // A class's constructor is the class
                if !types.contains(name) {
                    push(&mut completion.scope, &mut seen, name.clone(), ItemKind::Function, function_code(name, f));
                }
            }
            for (name, ty) in &scope.types {
                if let Ty::Object(class, _) = ty {
                    push(&mut completion.scope, &mut seen, name.clone(), class_kind(class), class_code(class));
                    let statics = self.statics_of(class, &accessible);
                    completion.paths.entry(name.clone()).or_insert(statics);
                }
            }
            for (name, module) in &scope.modules {
                push(&mut completion.scope, &mut seen, name.clone(), ItemKind::Module, format!("module {}", module_name(&module.path)));
                let mut items = Vec::new();
                let mut seen_here = HashSet::new();
                for (f, sig) in &module.functions {
                    let kind = if module.types.contains_key(f) { ItemKind::Class } else { ItemKind::Function };
                    push(&mut items, &mut seen_here, f.clone(), kind, function_code(f, sig));
                }
                for (c, ty) in &module.constants {
                    push(&mut items, &mut seen_here, format!("${c}"), ItemKind::Constant, format!("const {ty} ${c}"));
                }
                for (t, ty) in &module.types {
                    if let Ty::Object(class, _) = ty {
                        push(&mut items, &mut seen_here, t.clone(), class_kind(class), class_code(class));
                    }
                }
                items.sort_by(|a, b| a.label.cmp(&b.label));
                completion.paths.entry(name.clone()).or_insert(items);
            }
        }
        if let Some(this) = snapshot.this.last() {
            let statics = self.statics_of(&this.class, &accessible);
            completion.paths.insert("Self".into(), statics);
            if !this.is_static {
                completion.own = self.members_of(&this.class.this_type(), &accessible).unwrap_or_default();
            }
        }
        if let Some(request) = self.modules.complete.as_deref_mut() {
            request.result = Some(completion);
        }
    }

    /// The fields and methods of a value of this type. `None` when the type isn't known.
    fn members_of(&self, ty: &Ty, accessible: &[ClassRef]) -> Option<Vec<Item>> {
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let builtin = |items: &mut Vec<Item>, seen: &mut HashSet<String>, key: &str, args: Vec<Ty>| {
            if let Some(sigs) = self.prelude.type_sigs(key) {
                let map: HashMap<Rc<str>, Ty> = sigs.params.iter().cloned().zip(args).collect();
                let mut methods: Vec<_> = sigs.methods.iter().collect();
                methods.sort_by(|a, b| a.0.cmp(b.0));
                for (name, sig) in methods {
                    let detail = match Ty::Func(sig.clone()).substitute(&map) {
                        Ty::Func(f) => function_code(name, &f),
                        _ => unreachable!(),
                    };
                    push(items, seen, name.clone(), ItemKind::Method, detail);
                }
            }
        };
        match ty {
            Ty::Object(class, args) => {
                let mut current = Some((class.clone(), args.clone()));
                while let Some((c, a)) = current {
                    let map = c.map(&a);
                    let body = c.0.body.borrow();
                    let open = accessible.contains(&c);
                    for field in body.fields.iter().filter(|f| f.public || open) {
                        let detail = format!("{}{} @{}", if field.constant { "const " } else { "" }, field.ty.substitute(&map), field.name);
                        push(&mut items, &mut seen, field.name.clone(), ItemKind::Field, detail);
                    }
                    let mut methods: Vec<_> = body.methods.iter().filter(|(_, m)| m.public || open).collect();
                    methods.sort_by(|a, b| a.0.cmp(b.0));
                    for (name, m) in methods {
                        let detail = match Ty::Func(m.ty.clone()).substitute(&map) {
                            Ty::Func(f) => function_code(name, &f),
                            _ => unreachable!(),
                        };
                        push(&mut items, &mut seen, name.clone(), ItemKind::Method, detail);
                    }
                    current = body.parent.clone().map(|p| (p, body.parent_args.iter().map(|t| t.substitute(&map)).collect()));
                }
                for interface in class.all_interfaces() {
                    let body = interface.0.body.borrow();
                    for (name, m) in &body.methods {
                        push(&mut items, &mut seen, name.clone(), ItemKind::Method, function_code(name, &m.ty));
                    }
                }
                if class.0.body.borrow().exception.is_some() {
                    builtin(&mut items, &mut seen, "exception", Vec::new());
                }
            }
            Ty::Str => builtin(&mut items, &mut seen, "string", Vec::new()),
            Ty::Number => builtin(&mut items, &mut seen, "number", Vec::new()),
            Ty::Bool => builtin(&mut items, &mut seen, "bool", Vec::new()),
            Ty::Regex => builtin(&mut items, &mut seen, "regex", Vec::new()),
            Ty::Bytes => builtin(&mut items, &mut seen, "bytes", Vec::new()),
            Ty::Range => builtin(&mut items, &mut seen, "range", Vec::new()),
            Ty::Command => builtin(&mut items, &mut seen, "command", Vec::new()),
            Ty::List(e) => builtin(&mut items, &mut seen, "list", vec![(**e).clone()]),
            Ty::Map(k, v) => builtin(&mut items, &mut seen, "map", vec![(**k).clone(), (**v).clone()]),
            Ty::Result(t, e) => builtin(&mut items, &mut seen, "result", vec![(**t).clone(), (**e).clone()]),
            Ty::Exception(_) => builtin(&mut items, &mut seen, "exception", Vec::new()),
            // Not known: what every value has
            Ty::Any | Ty::Var(_) | Ty::Param(_) => {}
            _ => return None,
        }
        // What every value has
        builtin(&mut items, &mut seen, "any", Vec::new());
        Some(items)
    }

    /// What `Class::` reaches: static methods and fields, and the values of an enum.
    fn statics_of(&self, class: &ClassRef, accessible: &[ClassRef]) -> Vec<Item> {
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let body = class.0.body.borrow();
        let open = accessible.contains(class);
        for value in &body.values {
            push(&mut items, &mut seen, value.clone(), ItemKind::EnumValue, format!("{}::{value}", class.0.name));
        }
        let mut methods: Vec<_> = body.static_methods.iter().filter(|(_, m)| m.public || open).collect();
        methods.sort_by(|a, b| a.0.cmp(b.0));
        for (name, m) in methods {
            push(&mut items, &mut seen, name.clone(), ItemKind::Method, function_code(&format!("{}::{name}", class.0.name), &m.ty));
        }
        let mut statics: Vec<_> = body.statics.iter().filter(|(_, f)| f.public || open).collect();
        statics.sort_by(|a, b| a.0.cmp(b.0));
        for (name, f) in statics {
            push(&mut items, &mut seen, format!("${name}"), ItemKind::Field, format!("{} {}::${name}", f.ty, class.0.name));
        }
        items
    }
}

fn push(items: &mut Vec<Item>, seen: &mut HashSet<String>, label: String, kind: ItemKind, detail: String) {
    if seen.insert(label.clone()) {
        items.push(Item { label, kind, detail });
    }
}

fn class_kind(class: &ClassRef) -> ItemKind {
    if class.0.interface {
        ItemKind::Interface
    } else if class.0.is_enum {
        ItemKind::Enum
    } else {
        ItemKind::Class
    }
}
