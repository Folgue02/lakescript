//! `std/template`: templates in the style of Ruby's ERB *(see
//! `ideas/stdlib/template.md`)*.
//!
//! A template becomes Lakescript code: its text is written with `__text(n)`,
//! `{{ $x }}` with `__write($x)`, and its blocks become the blocks of the
//! language. That code runs as a script whose variables are the values given,
//! and its errors are moved back to where they are in the template.

use std::cell::RefCell;
use std::rc::Rc;

use super::{Args, Exports, io_error};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::interpreter::{Exec, Flow, Interpreter, flow_error, flow_error_as, native_error};
use crate::modules::Globals;
use crate::token::Span;
use crate::value::{ExceptionValue, Function, Key, NativeError, NativeFunction, Value};

pub fn register(m: &mut Exports) {
    m.vm_function("render", &["text", "values", "html"], render);
    m.vm_function("renderFile", &["path", "values", "html"], render_file);
}

fn render(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("template::render", values, 1, 3).map_err(|e| native_error(e, span))?;
    let text = a.str(0).map_err(|e| native_error(e, span))?;
    let html = flag(&a, 2, false).map_err(|e| native_error(e, span))?;
    run(vm, &text, "<template>", &a.value(1), html, span)
}

fn render_file(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("template::renderFile", values, 1, 3).map_err(|e| native_error(e, span))?;
    let path = a.str(0).map_err(|e| native_error(e, span))?;
    let file = vm.host.state.resolve(&path);
    let text = std::fs::read_to_string(&file).map_err(|e| native_error(io_error("read", &path, e), span))?;
    let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let html = flag(&a, 2, extension == "html" || extension == "htm").map_err(|e| native_error(e, span))?;
    run(vm, &text, &file.display().to_string(), &a.value(1), html, span)
}

/// A `bool` argument that can be left out.
fn flag(a: &Args, i: usize, default: bool) -> Result<bool, NativeError> {
    match a.value(i) {
        Value::Null => Ok(default),
        Value::Bool(b) => Ok(b),
        other => Err(NativeError::new(TYPE_ERROR, format!("`html` of `{}` must be a bool, not a {}", a.name, other.type_name()))),
    }
}

fn run(vm: &mut Interpreter, text: &str, name: &str, values: &Value, html: bool, span: Span) -> Exec<Value> {
    let file = vm.file_number(name);
    let located = |e: TemplateError| flow_error(e.message, place(text, e.at, e.at, file));
    let template = compile(text).map_err(located)?;

    let globals = Rc::new(Globals::default());
    match values {
        Value::Null => {}
        Value::Map(map) => {
            for (key, value) in map.borrow().iter() {
                let Key::Str(key) = key else {
                    return Err(flow_error_as(TYPE_ERROR, "the keys of the values of a template must be strings", span));
                };
                if !crate::lexer::is_valid_name(key) {
                    return Err(flow_error_as(
                        VALUE_ERROR,
                        format!("'{key}' can't be the name of a variable of a template: names have letters, digits and `_`, and don't start with a digit"),
                        span,
                    ));
                }
                globals.define(&format!("${key}"), value.clone(), true);
            }
        }
        other => return Err(flow_error_as(TYPE_ERROR, format!("the values of a template must be a map, not a {}", other.type_name()), span)),
    }

    // What the template writes
    let out = Rc::new(RefCell::new(String::new()));
    let texts = Rc::new(template.texts);
    let writer = |name: &str, f: Box<crate::value::NativeFn>| {
        Value::Function(Rc::new(Function::Native(NativeFunction { name: name.to_string(), params: Vec::new(), func: f })))
    };
    let (o, t) = (out.clone(), texts.clone());
    globals.define(
        "__text",
        writer(
            "__text",
            Box::new(move |_, args| {
                if let Some(Value::Int(i)) = args.first() {
                    o.borrow_mut().push_str(&t[*i as usize]);
                }
                Ok(Value::Null)
            }),
        ),
        true,
    );
    for (key, escape) in [("__write", html), ("__raw", false)] {
        let o = out.clone();
        let write = move |_: &mut crate::value::NativeContext, args: Vec<Value>| {
            match args.first() {
                None | Some(Value::Null) => {}
                Some(value) => {
                    let shown = value.to_string();
                    let mut o = o.borrow_mut();
                    if escape { escape_html(&shown, &mut o) } else { o.push_str(&shown) }
                }
            }
            Ok(Value::Null)
        };
        globals.define(key, writer(key, Box::new(write)), true);
    }

    let map = &template.map;
    let function = vm.compile_generated(&template.code, file, &globals).map_err(|d| {
        let (message, start, end) = moved(&d.message, d.span.start, d.span.end, &template.code, map);
        flow_error(message, place(text, start, end, file))
    })?;
    match vm.call(&function, Vec::new(), Vec::new(), span) {
        Ok(_) => {}
        // An error inside the template points at the template
        Err(Flow::Throw(e)) if e.span.file == file => {
            let (message, start, end) = moved(&e.message, e.span.start, e.span.end, &template.code, map);
            let e = ExceptionValue { kind: e.kind.clone(), base: e.base.clone(), object: e.object.clone(), message, span: place(text, start, end, file) };
            return Err(Flow::Throw(Rc::new(e)));
        }
        Err(other) => return Err(other),
    }
    let written = out.borrow().clone();
    Ok(Value::string(&written))
}

fn escape_html(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}

/// The span of the bytes `start..end` of a template.
/// Spans count characters, like the lexer does.
fn place(text: &str, start: usize, end: usize, file: u32) -> Span {
    let boundary = |mut i: usize| {
        i = i.min(text.len());
        while !text.is_char_boundary(i) {
            i -= 1;
        }
        i
    };
    let (start, end) = (boundary(start), boundary(end));
    let before = &text[..start];
    let line = before.matches('\n').count() as u32 + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() as u32 + 1;
    let chars = before.chars().count();
    Span { start: chars, end: chars + text[start..end.max(start)].chars().count(), line, column, file }
}

/// The byte where the character number `n` of `code` starts.
fn byte_of(code: &str, n: usize) -> usize {
    code.char_indices().nth(n).map(|(b, _)| b).unwrap_or(code.len())
}

/// Where a byte of the code made from a template comes from in the template,
/// and whether it was copied from it.
fn back(map: &[Copied], at: usize) -> (usize, bool) {
    match map.iter().rev().find(|c| c.code <= at) {
        Some(c) if at < c.code + c.len => (c.template + (at - c.code), true),
        // Code that was added after a tag's code: the `}}` of the tag
        Some(c) => (c.template + c.len, false),
        None => (0, false),
    }
}

/// The error of code made from a template, where it is in the template.
fn moved(message: &str, start: usize, end: usize, code: &str, map: &[Copied]) -> (String, usize, usize) {
    let ((start, copied), (end, _)) = (back(map, byte_of(code, start)), back(map, byte_of(code, end)));
    let mut message = message.to_string();
    if !copied {
        // What it found was added around the tag's code
        for added in ["`)`", "`;`", "`{`"] {
            message = message.replace(&format!("found {added}"), "found the `}}` of the tag");
        }
    }
    (message, start, end.max(start))
}

// ---- Turning a template into code ----

/// A part of the template copied into the code as it is.
struct Copied {
    /// Where it is in the code, and in the template.
    code: usize,
    template: usize,
    len: usize,
}

struct Template {
    code: String,
    texts: Vec<String>,
    map: Vec<Copied>,
}

struct TemplateError {
    message: String,
    at: usize,
}

fn fail<T>(message: impl Into<String>, at: usize) -> Result<T, TemplateError> {
    Err(TemplateError { message: message.into(), at })
}

#[derive(Clone, Copy, PartialEq)]
enum Block {
    For,
    If,
    While,
}

impl Block {
    fn name(self) -> &'static str {
        match self {
            Block::For => "for",
            Block::If => "if",
            Block::While => "while",
        }
    }
}

enum Kind {
    /// `{{ $x }}`, or `{{ raw($x) }}` when `raw`: the code to write.
    Write { raw: bool, from: usize, to: usize },
    /// `{{ for (...) }}`, `{{ if (...) }}`, `{{ while (...) }}`.
    Open(Block),
    /// `{{ else if (...) }}`, or `{{ else }}` without a condition.
    Else { condition: bool },
    End(Block),
    /// `{{ const ... }}`, `{{ let ... }}`, `{{ break }}`, `{{ continue }}`.
    Statement,
    Comment,
}

enum Piece {
    Text(usize, usize),
    /// A tag: where `{{` starts, and where its inside starts and ends.
    Tag { start: usize, inner: (usize, usize), kind: Kind },
}

fn compile(text: &str) -> Result<Template, TemplateError> {
    let mut pieces = Vec::new();
    let mut i = 0;
    while let Some(found) = text[i..].find("{{") {
        let start = i + found;
        if start > i {
            pieces.push(Piece::Text(i, start));
        }
        let Some(close) = closing(text, start + 2) else {
            return fail("this `{{` has no `}}` that closes it", start);
        };
        let kind = kind(text, start + 2, close)?;
        pieces.push(Piece::Tag { start, inner: (start + 2, close), kind });
        i = close + 2;
    }
    if i < text.len() {
        pieces.push(Piece::Text(i, text.len()));
    }
    let texts = trim_lines(text, &pieces);

    let mut code = String::new();
    let mut map = Vec::new();
    let mut texts_out = Vec::new();
    let mut open: Vec<(Block, usize, bool)> = Vec::new();
    let line = |at: usize| text[..at].matches('\n').count() + 1;
    let mut copy = |code: &mut String, from: usize, to: usize| {
        map.push(Copied { code: code.len(), template: from, len: to - from });
        code.push_str(&text[from..to]);
    };
    for (n, piece) in pieces.iter().enumerate() {
        match piece {
            Piece::Text(..) => {
                let (from, to) = texts[n];
                if from < to {
                    code.push_str(&format!("__text({});\n", texts_out.len()));
                    texts_out.push(text[from..to].to_string());
                }
            }
            Piece::Tag { start, inner, kind, .. } => {
                let start = *start;
                match kind {
                    Kind::Comment => {}
                    Kind::Write { raw, from, to } => {
                        code.push_str(if *raw { "__raw(" } else { "__write(" });
                        copy(&mut code, *from, *to);
                        code.push_str("\n);\n");
                    }
                    Kind::Statement => {
                        copy(&mut code, inner.0, inner.1);
                        code.push_str(if text[inner.0..inner.1].trim_end().ends_with(';') { "\n" } else { "\n;\n" });
                    }
                    Kind::Open(block) => {
                        open.push((*block, start, false));
                        copy(&mut code, inner.0, inner.1);
                        code.push_str("\n{\n");
                    }
                    Kind::Else { condition } => {
                        let word = if *condition { "{{ else if }}" } else { "{{ else }}" };
                        match open.last_mut() {
                            Some((Block::If, _, false)) => {}
                            Some((Block::If, at, true)) => {
                                return fail(format!("`{word}` after the `{{{{ else }}}}` of the `{{{{ if }}}}` of line {}: `{{{{ else }}}}` is the last part of an `{{{{ if }}}}`", line(*at)), start);
                            }
                            _ => return fail(format!("`{word}` can only be inside an `{{{{ if }}}}`"), start),
                        }
                        if !condition {
                            open.last_mut().expect("there's an if").2 = true;
                        }
                        code.push_str("} ");
                        copy(&mut code, inner.0, inner.1);
                        code.push_str("\n{\n");
                    }
                    Kind::End(block) => match open.pop() {
                        Some((b, _, _)) if b == *block => code.push_str("}\n"),
                        Some((b, at, _)) => {
                            return fail(
                                format!(
                                    "this `{{{{ end{} }}}}` should be `{{{{ end{} }}}}`, to close the `{{{{ {} }}}}` of line {}",
                                    block.name(),
                                    b.name(),
                                    b.name(),
                                    line(at)
                                ),
                                start,
                            );
                        }
                        None => return fail(format!("there's no `{{{{ {} }}}}` for this `{{{{ end{} }}}}` to close", block.name(), block.name()), start),
                    },
                }
            }
        }
    }
    if let Some((block, at, _)) = open.pop() {
        return fail(format!("the `{{{{ {} }}}}` of line {} has no `{{{{ end{} }}}}`", block.name(), line(at), block.name()), at);
    }
    Ok(Template { code, texts: texts_out, map })
}

/// Where the `}}` that closes a tag is, skipping strings and the braces of
/// maps and blocks inside it. A comment ends at the first `}}`.
fn closing(text: &str, from: usize) -> Option<usize> {
    if text[from..].trim_start().starts_with("//") {
        return text[from..].find("}}").map(|i| from + i);
    }
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'{' => depth += 1,
            b'}' if depth > 0 => depth -= 1,
            b'}' if bytes.get(i + 1) == Some(&b'}') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// What a tag does, from what's inside it.
fn kind(text: &str, from: usize, to: usize) -> Result<Kind, TemplateError> {
    let inner = &text[from..to];
    let trimmed = inner.trim();
    let at = from + (inner.len() - inner.trim_start().len());
    if trimmed.is_empty() {
        return fail("there's nothing inside this `{{ }}`", from - 2);
    }
    if trimmed.starts_with("//") {
        return Ok(Kind::Comment);
    }
    let word: String = trimmed.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let rest = trimmed[word.len()..].trim_start();
    let block = match word.as_str() {
        "for" => Some(Block::For),
        "if" => Some(Block::If),
        "while" => Some(Block::While),
        _ => None,
    };
    // `{{ if ($a) 'x' else 'y' }}` is an `if` that gives a value, not a block
    if let Some(block) = block
        && only_parentheses(rest)
    {
        return Ok(Kind::Open(block));
    }
    match word.as_str() {
        "else" if rest.is_empty() => return Ok(Kind::Else { condition: false }),
        "else" => {
            if rest.strip_prefix("if").map(str::trim_start).is_some_and(only_parentheses) {
                return Ok(Kind::Else { condition: true });
            }
            return fail("`{{ else }}` is alone, or `{{ else if (...) }}` with a condition", from - 2);
        }
        "endfor" | "endif" | "endwhile" if rest.is_empty() => {
            let block = match word.as_str() {
                "endfor" => Block::For,
                "endif" => Block::If,
                _ => Block::While,
            };
            return Ok(Kind::End(block));
        }
        "end" if rest.is_empty() => return fail("blocks close with their own end: `{{ endfor }}`, `{{ endif }}` or `{{ endwhile }}`", from - 2),
        "const" | "let" | "break" | "continue" => return Ok(Kind::Statement),
        "raw" if only_parentheses(rest) => {
            let open = at + trimmed.find('(').expect("there are parentheses");
            let close = at + trimmed.len() - 1;
            return Ok(Kind::Write { raw: true, from: open + 1, to: close });
        }
        _ => {}
    }
    if assigns(trimmed) {
        return Ok(Kind::Statement);
    }
    Ok(Kind::Write { raw: false, from: at, to: at + trimmed.len() })
}

/// `true` for code that changes a variable *(i.e. `$i += 1`, `$list[0] = 'a'`,
/// `$n++`)*, which writes nothing like a definition.
fn assigns(code: &str) -> bool {
    use crate::token::TokenKind::*;
    let Ok(tokens) = crate::lexer::tokenize(code) else { return false };
    let mut depth = 0usize;
    for token in tokens {
        match token.kind {
            LParen | LBracket | LBrace | QuestionBracket => depth += 1,
            RParen | RBracket | RBrace => depth = depth.saturating_sub(1),
            Eq | PlusEq | MinusEq | StarEq | SlashEq | PercentEq | StarStarEq | AmpEq | PipeEq | CaretEq | ShlEq | ShrEq | UShrEq | PlusPlus
            | MinusMinus
                if depth == 0 =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// `true` for text that is one group in parentheses and nothing else *(i.e.
/// `($a in $b)`, but not `($a) 'x'`)*.
fn only_parentheses(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'(') {
        return false;
    }
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return text[i + 1..].trim().is_empty();
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// The part of each text that's written: a line with only a tag that writes
/// nothing *(a block, a definition or a comment)* disappears, with its new line.
fn trim_lines(text: &str, pieces: &[Piece]) -> Vec<(usize, usize)> {
    let mut kept: Vec<(usize, usize)> = pieces
        .iter()
        .map(|p| match p {
            Piece::Text(a, b) => (*a, *b),
            Piece::Tag { .. } => (0, 0),
        })
        .collect();
    let silent = |p: &Piece| matches!(p, Piece::Tag { kind, .. } if !matches!(kind, Kind::Write { .. }));
    for (n, piece) in pieces.iter().enumerate() {
        if !silent(piece) {
            continue;
        }
        // What's before it on its line
        let before = match n.checked_sub(1).map(|p| &pieces[p]) {
            None => Some(None),
            Some(Piece::Text(a, b)) => match text[*a..*b].rfind('\n') {
                Some(nl) if text[a + nl + 1..*b].trim().is_empty() => Some(Some(a + nl + 1)),
                None if n == 1 && text[*a..*b].trim().is_empty() => Some(Some(*a)),
                _ => None,
            },
            Some(_) => None,
        };
        // What's after it on its line, with the new line
        let after = match pieces.get(n + 1) {
            None => Some(None),
            Some(Piece::Text(a, b)) => match text[*a..*b].find('\n') {
                Some(nl) if text[*a..a + nl].trim().is_empty() => Some(Some(a + nl + 1)),
                None if n + 2 == pieces.len() && text[*a..*b].trim().is_empty() => Some(Some(*b)),
                _ => None,
            },
            Some(_) => None,
        };
        if let (Some(before), Some(after)) = (before, after) {
            if let Some(cut) = before {
                kept[n - 1].1 = kept[n - 1].1.min(cut);
            }
            if let Some(cut) = after {
                kept[n + 1].0 = kept[n + 1].0.max(cut);
            }
        }
    }
    for k in &mut kept {
        if k.0 > k.1 {
            k.0 = k.1;
        }
    }
    kept
}
