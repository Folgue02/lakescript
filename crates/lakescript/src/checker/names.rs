//! What the checker found about each name, for editors *(see
//! `ideas/tooling/lsp.md`)*: its type, where it's declared and its `///`
//! comment, for hover and go to definition.
//!
//! The syntax tree only has the place of whole expressions, so the checker
//! notes the text of each name and where to look for it, and `locate` finds
//! it in the source once the check is done.

use std::collections::HashMap;
use std::rc::Rc;

use super::Checker;
use super::types::{ClassRef, FnTy, Ty};
use crate::token::{Span, TokenKind};

/// A name in a file, and what's known about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Name {
    /// The file it's in: `None` for the script that was checked.
    pub file: Option<String>,
    /// Where it is, in characters.
    pub start: usize,
    pub end: usize,
    /// Its declaration, as Lakescript code *(i.e. `number $count`)*.
    pub code: String,
    /// What it is, in a sentence *(i.e. "A method of `Person`.")*.
    pub about: Option<String>,
    /// The `///` comment of its declaration.
    pub doc: Option<String>,
    pub definition: Option<Definition>,
}

/// Where a name is declared.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    /// `None` for the script that was checked.
    pub file: Option<String>,
    pub start: usize,
    pub end: usize,
}

/// A declaration as the checker notes it: the statement or member it's in,
/// and the name to find in it.
#[derive(Clone, Debug)]
pub struct Place {
    pub file: Option<String>,
    pub span: Span,
    pub needle: String,
    /// Whether the `///` comment before the statement is about this name. A
    /// parameter is declared in its function's statement, but the comment is the function's.
    pub doc: bool,
}

/// How a name is shown. Types are written out once the check is done, when
/// the types found out later are known *(i.e. the elements of `[]`)*.
pub(super) enum Show {
    Var { constant: bool, name: String, ty: Ty },
    Func { name: String, ty: Ty },
    /// A field: `prefix` is `@` or `Class::$`.
    Field { constant: bool, prefix: String, name: String, ty: Ty },
    Class(ClassRef),
    Text(String),
}

pub(super) struct Raw {
    file: Option<String>,
    span: Span,
    from: usize,
    lead: String,
    needle: String,
    show: Show,
    about: Option<String>,
    definition: Option<Place>,
}

/// A name noted by a checker, with its types written out.
pub struct Found {
    file: Option<String>,
    span: Span,
    from: usize,
    lead: String,
    needle: String,
    code: String,
    about: Option<String>,
    definition: Option<Place>,
}

impl Checker<'_> {
    pub(super) fn recording(&self) -> bool {
        self.names.is_some()
    }

    /// The place of a declaration in the file being checked.
    pub(super) fn here(&self, span: Span, needle: impl Into<String>, doc: bool) -> Place {
        Place { file: self.file.clone(), span, needle: needle.into(), doc }
    }

    /// Notes a name: `lead` and then `needle`, looked for in `span` from
    /// `from` *(i.e. `Point::` and `origin`)*. Only `needle` is the name.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn note(&mut self, span: Span, from: usize, lead: &str, needle: &str, show: Show, about: Option<String>, definition: Option<Place>) {
        let file = self.file.clone();
        if let Some(names) = &mut self.names {
            names.push(Raw { file, span, from, lead: lead.into(), needle: needle.into(), show, about, definition });
        }
    }

    /// Writes out the names noted while checking, now that every type is known.
    pub(super) fn finish_names(&mut self) {
        let Some(raw) = self.names.take() else { return };
        let found: Vec<Found> = raw
            .into_iter()
            .map(|r| {
                let code = match &r.show {
                    Show::Var { constant, name, ty } => format!("{}{} ${name}", if *constant { "const " } else { "" }, self.resolve(ty)),
                    Show::Func { name, ty } => match self.resolve(ty) {
                        Ty::Func(f) => function_code(name, &f),
                        other => format!("{other} {name}"),
                    },
                    Show::Field { constant, prefix, name, ty } => format!("{}{} {prefix}{name}", if *constant { "const " } else { "" }, self.resolve(ty)),
                    Show::Class(class) => class_code(class),
                    Show::Text(text) => text.clone(),
                };
                Found { file: r.file, span: r.span, from: r.from, lead: r.lead, needle: r.needle, code, about: r.about, definition: r.definition }
            })
            .collect();
        if let Some(sink) = self.modules.names.as_deref_mut() {
            sink.extend(found);
        }
    }

    /// Where a function of this scope is declared.
    pub(super) fn function_place(&self, name: &str) -> Option<Place> {
        self.scopes.iter().rev().find(|s| s.funcs.contains_key(name)).and_then(|s| s.places.get(name).cloned())
    }

    /// Notes a class's name.
    pub(super) fn note_class(&mut self, class: &ClassRef, span: Span, from: usize, needle: &str) {
        if !self.recording() {
            return;
        }
        let definition = class_place(class, None, needle);
        self.note(span, from, "", needle, Show::Class(class.clone()), None, definition);
    }

    /// Records where a class and its members are declared.
    pub(super) fn set_places(&self, class: &ClassRef, decl: &crate::ast::ClassDecl) {
        use crate::ast::MemberKind;
        let mut places = super::types::Places { file: self.file.clone(), class: Some(decl.span), members: HashMap::new() };
        for member in &decl.members {
            let key = match &member.kind {
                MemberKind::Field { name, .. } if member.is_static => format!("${name}"),
                MemberKind::Field { name, .. } => name.clone(),
                MemberKind::Method { decl, .. } if member.is_static => format!("::{}", decl.name),
                MemberKind::Method { decl, .. } => decl.name.clone(),
                MemberKind::Constructor(_) => "new".into(),
            };
            places.members.entry(key).or_insert(member.span);
        }
        for value in &decl.values {
            places.members.insert(format!("::{}", value.name), value.span);
        }
        *class.0.places.borrow_mut() = places;
    }
}

/// Where a class, or one of its members, is declared. `member` is a key of `Places::members`.
pub(super) fn class_place(class: &ClassRef, member: Option<&str>, needle: &str) -> Option<Place> {
    let places = class.0.places.borrow();
    let span = match member {
        Some(m) => *places.members.get(m)?,
        None => places.class?,
    };
    Some(Place { file: places.file.clone(), span, needle: needle.into(), doc: true })
}

/// How a module is named in messages: `std/fs`, or the name of its file.
pub(super) fn module_name(path: &str) -> String {
    if path.starts_with("std/") {
        return path.to_string();
    }
    std::path::Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// `string greet(string $name, number $times = …)`
pub(super) fn function_code(name: &str, f: &FnTy) -> String {
    let mut params = Vec::new();
    for (i, ty) in f.params.iter().enumerate() {
        let variadic = f.variadic && i + 1 == f.params.len();
        let mut p = if variadic { format!("...{ty}") } else { ty.to_string() };
        if let Some(n) = f.names.get(i) {
            p.push_str(&format!(" ${n}"));
        }
        if i >= f.required && !variadic {
            p.push_str(" = …");
        }
        params.push(p);
    }
    let generics = if f.generics.is_empty() { String::new() } else { format!("<{}>", f.generics.iter().map(|g| g.to_string()).collect::<Vec<_>>().join(", ")) };
    format!("{} {name}{generics}({})", f.ret, params.join(", "))
}

/// `class Stack<T> extends List`
pub(super) fn class_code(class: &ClassRef) -> String {
    let sig = &class.0;
    let word = if sig.interface {
        "interface"
    } else if sig.is_enum {
        "enum"
    } else if sig.command {
        "command"
    } else {
        "class"
    };
    let mut code = format!("{word} {}", sig.name);
    if !sig.generics.is_empty() {
        code.push_str(&format!("<{}>", sig.generics.iter().map(|g| g.to_string()).collect::<Vec<_>>().join(", ")));
    }
    let body = sig.body.borrow();
    if let Some(parent) = &body.parent {
        code.push_str(&format!(" extends {}", parent.0.name));
    } else if let Some(exception) = &body.exception {
        code.push_str(&format!(" extends {exception}"));
    }
    code
}

/// Finds the noted names in their files' text. `read` gives the text of a
/// file, `None` being the script that was checked.
pub fn locate(found: Vec<Found>, read: impl Fn(Option<&str>) -> Option<String>) -> Vec<Name> {
    let mut files: HashMap<Option<String>, Option<Rc<Source>>> = HashMap::new();
    let mut source = |file: &Option<String>| -> Option<Rc<Source>> {
        files.entry(file.clone()).or_insert_with(|| read(file.as_deref()).map(|text| Rc::new(Source::new(&text)))).clone()
    };
    let mut names = Vec::new();
    for f in found {
        let Some(text) = source(&f.file) else { continue };
        let Some(start) = text.find(f.from.max(f.span.start), f.span.end, &format!("{}{}", f.lead, f.needle)) else { continue };
        let start = start + f.lead.chars().count();
        let end = start + f.needle.chars().count();
        let (definition, doc) = match &f.definition {
            Some(place) => match source(&place.file) {
                Some(def_text) => {
                    let at = def_text.find(place.span.start, place.span.end, &place.needle).unwrap_or(place.span.start);
                    let doc = if place.doc { def_text.doc_at(place.span.start) } else { None };
                    (Some(Definition { file: place.file.clone(), start: at, end: at + place.needle.chars().count() }), doc)
                }
                None => (None, None),
            },
            None => (None, None),
        };
        let name = Name { file: f.file, start, end, code: f.code, about: f.about, doc, definition };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// A file's text, as characters like spans count them, and its tokens for the `///` comments.
struct Source {
    chars: Vec<char>,
    tokens: Vec<crate::token::Token>,
}

impl Source {
    fn new(text: &str) -> Source {
        Source { chars: text.chars().collect(), tokens: crate::lexer::tokenize(text).unwrap_or_default() }
    }

    /// Where `needle` is between `from` and `to`, as a whole word.
    fn find(&self, from: usize, to: usize, needle: &str) -> Option<usize> {
        let needle: Vec<char> = needle.chars().collect();
        let to = to.min(self.chars.len());
        if needle.is_empty() || to < needle.len() {
            return None;
        }
        let word = |c: char| c.is_alphanumeric() || c == '_';
        (from..=to - needle.len()).find(|&i| {
            self.chars[i..i + needle.len()] == needle[..]
                && (i == 0 || !word(needle[0]) || !word(self.chars[i - 1]))
                && self.chars.get(i + needle.len()).is_none_or(|&c| !word(c) || !word(*needle.last().unwrap()))
        })
    }

    /// The `///` comment of the declaration that starts here, or before its `pub`.
    fn doc_at(&self, start: usize) -> Option<String> {
        let i = self.tokens.iter().position(|t| t.span.start == start)?;
        if let Some(doc) = &self.tokens[i].doc {
            return Some(doc.clone());
        }
        let before = self.tokens.get(i.checked_sub(1)?)?;
        match before.kind {
            TokenKind::Keyword(crate::token::Keyword::Pub) => before.doc.clone(),
            _ => None,
        }
    }
}
