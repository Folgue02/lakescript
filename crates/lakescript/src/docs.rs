//! The documentation of a module, read from its source: its `//!` text and
//! its `pub` items with their `///` comments, for `ferry doc` *(see
//! `ideas/tooling/docs.md`)*. Nothing is run: the source is only parsed.

use crate::ast::{ClassDecl, ClassKind, Expr, FunctionDecl, MemberKind, Origin, Param, Stmt, StmtKind};
use crate::bytecode::ArgType;
use crate::diagnostic::Diagnostic;
use crate::token::{Keyword, Token, TokenKind};

/// A module and what it makes public.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDoc {
    /// Its `//!` text, in Markdown.
    pub text: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Function,
    Constant,
    Class,
    Interface,
    Enum,
    Field,
    Method,
    /// A value of an enum.
    Value,
    /// A built-in type, in the docs of the standard library *(i.e. `string`)*.
    Type,
}

impl ItemKind {
    pub fn word(self) -> &'static str {
        match self {
            ItemKind::Function => "function",
            ItemKind::Constant => "const",
            ItemKind::Class => "class",
            ItemKind::Interface => "interface",
            ItemKind::Enum => "enum",
            ItemKind::Field => "field",
            ItemKind::Method => "method",
            ItemKind::Value => "value",
            ItemKind::Type => "type",
        }
    }
}

/// Something a module declares, or a member of a class.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub name: String,
    /// Its declaration as Lakescript, without its body *(i.e. `pub number bytes(string $text)`)*.
    pub code: String,
    /// The line of the source where it's declared.
    pub line: u32,
    pub doc: Doc,
    /// The fields, methods and values of a class, an interface or an enum.
    pub members: Vec<Item>,
    /// A class's superclass, or the interfaces an interface extends.
    pub extends: Vec<String>,
    /// The interfaces of a class's `implement` blocks.
    pub implements: Vec<String>,
    /// A method of an interface with a body: a default.
    pub default: bool,
    pub is_static: bool,
    /// The values given to a value of an enum, as written *(i.e. `0.39`)*.
    pub args: Vec<String>,
    /// For a method of an `implement` block: its interface.
    pub from: Option<String>,
    /// The group it's in, in the docs of the standard library *(i.e. `Reading`)*.
    pub group: Option<String>,
    /// The groups of its members, with their text, in order.
    pub groups: Vec<(String, String)>,
}

/// A `///` comment, split in its parts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Doc {
    /// What's before the lines of the parameters, in Markdown.
    pub text: String,
    /// `$name: ...` lines: the parameter, without `$`, and its description.
    pub params: Vec<(String, String)>,
    /// The `returns: ...` line.
    pub returns: Option<String>,
    /// `throws ValueError: ...` lines: the exception and when.
    pub throws: Vec<(String, String)>,
}

impl Item {
    /// An item with only its kind, its name and its declaration.
    pub fn new(kind: ItemKind, name: &str, code: &str) -> Item {
        Item { code: code.to_string(), ..blank(kind, name, 0, Doc::default()) }
    }
}

impl Doc {
    /// Its first sentence, or line, for lists and search *(i.e. on a project's front page)*.
    pub fn summary(&self) -> String {
        let first = self.text.split("\n\n").next().unwrap_or("").replace('\n', " ");
        first.trim().to_string()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.params.is_empty() && self.returns.is_none() && self.throws.is_empty()
    }
}

/// Splits a `///` comment: its text, then lines that start with `$name:`,
/// `returns:` or `throws Exception:`. A line after one of those, before an
/// empty line or the next of them, goes on with it.
pub fn parse_doc(comment: &str) -> Doc {
    enum Part {
        Text,
        Param(usize),
        Returns,
        Throws(usize),
    }
    let mut doc = Doc::default();
    let mut text: Vec<&str> = Vec::new();
    let mut part = Part::Text;
    for line in comment.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('$')
            && let Some((name, what)) = rest.split_once(':')
            && is_name(name)
        {
            doc.params.push((name.to_string(), what.trim().to_string()));
            part = Part::Param(doc.params.len() - 1);
            continue;
        }
        if let Some(what) = trimmed.strip_prefix("returns:") {
            doc.returns = Some(what.trim().to_string());
            part = Part::Returns;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("throws ")
            && let Some((exception, what)) = rest.split_once(':')
            && is_name(exception.trim())
        {
            doc.throws.push((exception.trim().to_string(), what.trim().to_string()));
            part = Part::Throws(doc.throws.len() - 1);
            continue;
        }
        let more = |s: &mut String| {
            s.push(' ');
            s.push_str(trimmed);
        };
        match part {
            _ if trimmed.is_empty() => {
                part = Part::Text;
                text.push("");
            }
            Part::Text => text.push(line),
            Part::Param(i) => more(&mut doc.params[i].1),
            Part::Returns => more(doc.returns.as_mut().expect("there's a returns")),
            Part::Throws(i) => more(&mut doc.throws[i].1),
        }
    }
    doc.text = text.join("\n").trim().to_string();
    doc
}

fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The `//!` lines at the top of a file, after its shebang and empty lines.
pub fn module_text(source: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if lines.is_empty() && (trimmed.is_empty() || trimmed.starts_with("#!")) {
            continue;
        }
        match trimmed.strip_prefix("//!") {
            Some(rest) => lines.push(rest.strip_prefix(' ').unwrap_or(rest).trim_end()),
            None => break,
        }
    }
    let text = lines.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The docs of a module: its `//!` text and its `pub` items.
pub fn module_doc(source: &str) -> Result<ModuleDoc, Diagnostic> {
    let tokens = crate::lexer::tokenize(source)?;
    let program = crate::parser::parse(tokens.clone())?;
    let reader = Reader { source: source.chars().collect(), tokens };
    let mut items = Vec::new();
    for stmt in &program {
        let StmtKind::Pub(inner) = &stmt.kind else { continue };
        if let Some(item) = reader.item(inner, stmt) {
            items.push(item);
        }
    }
    Ok(ModuleDoc { text: module_text(source), items })
}

/// Something in a comment that doesn't match the code, but doesn't stop
/// anything from running: the language has no warnings of its own, so only
/// editors, `ferry check` and `ferry doc` show these.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub message: String,
    /// Where it is: the line of the comment, from 1, and its columns, from 1,
    /// with `start` and `end` in characters like spans.
    pub line: u32,
    pub column: u32,
    pub start: usize,
    pub end: usize,
}

/// The `$name:` lines of `///` comments for a parameter the function or
/// method doesn't have *(i.e. after a rename)*, so comments don't go stale
/// *(see Doc comments in ideas/language/functions.md)*.
pub fn stale_params(source: &str) -> Result<Vec<Warning>, Diagnostic> {
    let tokens = crate::lexer::tokenize(source)?;
    let program = crate::parser::parse(tokens.clone())?;
    let reader = Reader { source: source.chars().collect(), tokens };
    let mut warnings = Vec::new();
    let mut check = |decl: &FunctionDecl, start: usize, what: &str| {
        let doc = reader.doc_at(start);
        for (name, _) in &doc.params {
            if decl.params.iter().any(|p| p.name == *name) {
                continue;
            }
            let have: Vec<String> = decl.params.iter().map(|p| format!("`${}`", p.name)).collect();
            let has = match have.as_slice() {
                [] => "it has no parameters".to_string(),
                [one] => format!("its only parameter is {one}"),
                _ => format!("its parameters are {}", have.join(", ")),
            };
            let message = format!("the comment describes `${name}`, but {what} has no parameter `${name}`: {has}");
            warnings.push(reader.param_line(start, name, message));
        }
    };
    for stmt in &program {
        let inner = match &stmt.kind {
            StmtKind::Pub(inner) => inner,
            _ => stmt,
        };
        match &inner.kind {
            StmtKind::Function(decl) => check(decl, stmt.span.start, &format!("`{}`", decl.name)),
            StmtKind::Class(class) => {
                for member in &class.members {
                    match &member.kind {
                        MemberKind::Method { decl, .. } => check(decl, member.span.start, &format!("`{}.{}`", class.name, decl.name)),
                        MemberKind::Constructor(decl) => check(decl, member.span.start, &format!("the constructor of `{}`", class.name)),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(warnings)
}

struct Reader {
    source: Vec<char>,
    tokens: Vec<Token>,
}

impl Reader {
    /// The source between two places, counted in characters like spans.
    fn text(&self, start: usize, end: usize) -> String {
        self.source[start.min(self.source.len())..end.min(self.source.len())].iter().collect()
    }

    fn expr_text(&self, expr: &Expr) -> String {
        self.text(expr.span.start, expr.span.end).trim().to_string()
    }

    /// The `///` comment before the declaration that starts at `start`, or
    /// before its `pub`.
    fn doc_at(&self, start: usize) -> Doc {
        let i = self.tokens.partition_point(|t| t.span.start < start);
        let Some(token) = self.tokens.get(i) else { return Doc::default() };
        let comment = token.doc.clone().or_else(|| {
            let before = i.checked_sub(1).map(|j| &self.tokens[j])?;
            matches!(before.kind, TokenKind::Keyword(Keyword::Pub)).then(|| before.doc.clone()).flatten()
        });
        comment.map(|c| parse_doc(&c)).unwrap_or_default()
    }

    /// A warning on the `/// $name:` line of the comment before the
    /// declaration at `start`, or on the declaration when it can't be found.
    fn param_line(&self, start: usize, name: &str, message: String) -> Warning {
        let needle: Vec<char> = format!("${name}:").chars().collect();
        // The comment's lines are right before the declaration
        let from = start.saturating_sub(4000);
        let found = (from..start.saturating_sub(needle.len()))
            .rev()
            .find(|&i| self.source[i..i + needle.len()] == needle[..] && self.source[..i].iter().rev().take_while(|c| **c != '\n').collect::<String>().contains("///"));
        let at = found.unwrap_or(start);
        let len = if found.is_some() { needle.len() - 1 } else { 1 };
        let line = self.source[..at].iter().filter(|c| **c == '\n').count() as u32 + 1;
        let column = self.source[..at].iter().rev().take_while(|c| **c != '\n').count() as u32 + 1;
        Warning { message, line, column, start: at, end: at + len }
    }

    fn item(&self, stmt: &Stmt, outer: &Stmt) -> Option<Item> {
        let doc = self.doc_at(outer.span.start);
        let line = outer.span.line;
        match &stmt.kind {
            StmtKind::Function(decl) => Some(Item { code: format!("pub {}", self.signature(decl)), ..blank(ItemKind::Function, &decl.name, line, doc) }),
            StmtKind::Let { constant: true, name, ty, value } => {
                let ty = ty.as_ref().map(|t| format!("{t} ")).unwrap_or_default();
                let code = format!("pub const {ty}${name} = {}", self.expr_text(value));
                Some(Item { code, ..blank(ItemKind::Constant, &format!("${name}"), line, doc) })
            }
            StmtKind::Class(decl) if decl.kind != ClassKind::Command => Some(self.class(decl, doc, line)),
            _ => None,
        }
    }

    fn signature(&self, decl: &FunctionDecl) -> String {
        let ret = decl.ret.as_ref().map(|t| format!("{t} ")).unwrap_or_default();
        let generics = if decl.generics.is_empty() { String::new() } else { format!("<{}>", decl.generics.join(", ")) };
        let params: Vec<String> = decl.params.iter().map(|p| self.param(p)).collect();
        format!("{ret}{}{generics}({})", decl.name, params.join(", "))
    }

    fn param(&self, p: &Param) -> String {
        let ty = p.ty.as_ref().map(|t| format!("{t} ")).unwrap_or_default();
        let dots = if p.variadic { "..." } else { "" };
        let sigil = if p.field { "@" } else { "$" };
        let default = p.default.as_ref().map(|d| format!(" = {}", self.expr_text(d))).unwrap_or_default();
        format!("{dots}{ty}{sigil}{}{default}", p.name)
    }

    fn class(&self, decl: &ClassDecl, doc: Doc, line: u32) -> Item {
        let kind = match decl.kind {
            ClassKind::Interface => ItemKind::Interface,
            ClassKind::Enum => ItemKind::Enum,
            _ => ItemKind::Class,
        };
        let generics = if decl.generics.is_empty() { String::new() } else { format!("<{}>", decl.generics.join(", ")) };
        let extends: Vec<String> = decl.extends.iter().map(|t| t.to_string()).collect();
        let implements: Vec<String> = decl.implements.iter().map(|t| t.to_string()).collect();
        let mut code = format!("pub {} {}{generics}", decl.kind.word(), decl.name);
        if !extends.is_empty() {
            code.push_str(&format!(" extends {}", extends.join(", ")));
        }
        let mut members = Vec::new();
        for value in &decl.values {
            let args: Vec<String> = value.args.iter().map(|a| self.expr_text(&a.value)).collect();
            let code = if args.is_empty() { value.name.clone() } else { format!("{}({})", value.name, args.join(", ")) };
            members.push(Item { code, args, ..blank(ItemKind::Value, &value.name, value.span.line, self.doc_at(value.span.start)) });
        }
        let interface = decl.kind == ClassKind::Interface;
        for member in &decl.members {
            // An interface's members are all public, and so are the methods that implement one
            let from = match &member.origin {
                Origin::Implement(ty) => Some(ty.to_string()),
                _ => None,
            };
            if !member.public && !interface && from.is_none() {
                continue;
            }
            let doc = self.doc_at(member.span.start);
            let line = member.span.line;
            let statics = if member.is_static { "static " } else { "" };
            match &member.kind {
                MemberKind::Field { constant, ty, name, .. } => {
                    let constant = if *constant { "const " } else { "" };
                    let sigil = if member.is_static { "$" } else { "@" };
                    let code = format!("pub {statics}{constant}{ty} {sigil}{name}");
                    members.push(Item { code, is_static: member.is_static, ..blank(ItemKind::Field, name, line, doc) });
                }
                MemberKind::Method { decl: method, body } => {
                    let public = if interface || from.is_some() { "" } else { "pub " };
                    let code = format!("{public}{statics}{}", self.signature(method));
                    let default = interface && *body;
                    members.push(Item { code, default, is_static: member.is_static, from: from.clone(), ..blank(ItemKind::Method, &method.name, line, doc) });
                }
                _ => {}
            }
        }
        Item { code, members, extends, implements, ..blank(kind, &decl.name, line, doc) }
    }
}

fn blank(kind: ItemKind, name: &str, line: u32, doc: Doc) -> Item {
    Item {
        kind,
        name: name.to_string(),
        code: String::new(),
        line,
        doc,
        members: Vec::new(),
        extends: Vec::new(),
        implements: Vec::new(),
        default: false,
        is_static: false,
        args: Vec::new(),
        from: None,
        group: None,
        groups: Vec::new(),
    }
}

// ---- Commands ----

/// The arguments a script reads with its `command`: what its `--help` shows.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandDoc {
    /// Its name as it's run *(i.e. `todo`, or `todo add` for a subcommand)*.
    pub name: String,
    /// The `///` comment of the command, in Markdown.
    pub text: Option<String>,
    /// `todo [options] <command>`.
    pub usage: String,
    pub args: Vec<ArgDoc>,
    pub subcommands: Vec<CommandDoc>,
}

/// An argument or an option of a command.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgDoc {
    /// `<task>`, or `-p, --priority` for an option.
    pub name: String,
    pub option: bool,
    pub text: Option<String>,
    /// The values an enum allows, as they're written *(i.e. `low`, `normal`, `high`)*.
    pub choices: Vec<String>,
    pub default: Option<String>,
    /// It can be given several times *(i.e. `...string`, or `-vv`)*.
    pub repeats: bool,
    /// An argument without a default that must be given.
    pub required: bool,
}

/// The command a script parses: the `command` declared in it whose `::parse()`
/// it calls. `None` when there's no such command, or more than one. `name`
/// is how the script is run *(i.e. `todo`)*.
pub fn command_doc(source: &str, name: &str) -> Result<Option<CommandDoc>, Diagnostic> {
    let tokens = crate::lexer::tokenize(source)?;
    let program = crate::parser::parse(tokens.clone())?;
    let mut commands = Vec::new();
    let mut enums = Vec::new();
    for stmt in &program {
        let inner = match &stmt.kind {
            StmtKind::Pub(inner) => &**inner,
            _ => stmt,
        };
        if let StmtKind::Class(decl) = &inner.kind {
            match decl.kind {
                ClassKind::Command => commands.push(decl.clone()),
                ClassKind::Enum => enums.push(decl.clone()),
                _ => {}
            }
        }
    }
    // The ones it parses: `Todo::parse(`
    let parsed: Vec<&std::rc::Rc<ClassDecl>> = commands
        .iter()
        .filter(|c| {
            tokens.windows(3).any(|w| {
                matches!(&w[0].kind, TokenKind::Ident(n) if *n == c.name)
                    && w[1].kind == TokenKind::ColonColon
                    && matches!(&w[2].kind, TokenKind::Ident(n) if n == "parse")
            })
        })
        .collect();
    let [command] = parsed.as_slice() else { return Ok(None) };
    Ok(Some(describe(command, name, &enums)))
}

fn describe(decl: &ClassDecl, name: &str, enums: &[std::rc::Rc<ClassDecl>]) -> CommandDoc {
    let (info, types) = crate::compiler::command_info(decl);
    let choices = |i: u32| -> Vec<String> {
        let wanted = types.get(i as usize).map(|t| t.to_string()).unwrap_or_default();
        enums
            .iter()
            .find(|e| e.name == wanted)
            .map(|e| e.values.iter().map(|v| v.name.to_ascii_lowercase().replace('_', "-")).collect())
            .unwrap_or_default()
    };
    let mut args = Vec::new();
    for arg in info.args.iter().filter(|a| !a.flag) {
        let required = !arg.variadic && !arg.nullable && !arg.has_value;
        args.push(ArgDoc {
            name: format!("<{}>", crate::cli::kebab(&arg.name)),
            option: false,
            text: arg.doc.clone(),
            choices: if let ArgType::Enum(i) = arg.kind { choices(i) } else { Vec::new() },
            default: arg.default.clone(),
            repeats: arg.variadic,
            required,
        });
    }
    for arg in info.args.iter().filter(|a| a.flag) {
        let short = arg.short.map(|c| format!("-{c}, ")).unwrap_or_default();
        args.push(ArgDoc {
            name: format!("{short}--{}", crate::cli::kebab(&arg.name)),
            option: true,
            text: arg.doc.clone(),
            choices: if let ArgType::Enum(i) = arg.kind { choices(i) } else { Vec::new() },
            default: arg.default.clone(),
            repeats: arg.variadic || arg.count,
            required: false,
        });
    }
    let usage = crate::cli::usage(&info, name).trim_start_matches("Usage: ").to_string();
    let subcommands = decl
        .subcommands
        .iter()
        .map(|sub| {
            let short = crate::cli::kebab(sub.name.rsplit("::").next().unwrap_or(&sub.name));
            describe(sub, &format!("{name} {short}"), enums)
        })
        .collect();
    CommandDoc { name: name.to_string(), text: decl.doc.clone(), usage, args, subcommands }
}
