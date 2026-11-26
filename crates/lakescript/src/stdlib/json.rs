//! `std/format/json` *(see `ideas/stdlib/format/json.md`)*: strict JSON, without a schema.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use std::rc::Weak;

use super::{Args, Exports, io_error};
use crate::ast::{TypeExpr, TypeKind};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::interpreter::{Exec, Interpreter, error_as, native_error};
use crate::modules::Globals;
use crate::token::Span;
use crate::value::{Class, Key, NativeError, Value, format_number};

/// How deep arrays and objects can be nested, so a huge or circular value
/// can't overflow the stack.
const MAX_DEPTH: usize = 512;

// ---- Reading ----

/// The value read into a type, when one is given *(i.e. `json::parse($text, User)`)*.
pub(super) fn typed(vm: &mut Interpreter, value: Value, target: &Value, span: Span) -> Exec<Value> {
    if matches!(target, Value::Null) { Ok(value) } else { convert_to(vm, value, target, "$", span) }
}

/// Into a class or a type given as a value. `path` says where the value is,
/// for errors *(i.e. `$[2].age`)*.
fn convert_to(vm: &mut Interpreter, value: Value, target: &Value, path: &str, span: Span) -> Exec<Value> {
    match target {
        Value::Class(class) => into_class(vm, value, class, path, span),
        Value::Type(t) => convert(vm, value, &t.0, &t.1, path, span),
        other => error_as(TYPE_ERROR, format!("the type to read data into must be a type (i.e. `User` or `list<User>`), not a {}", other.type_name()), span),
    }
}

fn wrong<T>(value: &Value, path: &str, expected: &str, span: Span) -> Exec<T> {
    let got = match value {
        Value::Null => "`null`".to_string(),
        Value::Map(_) => "an object".to_string(),
        Value::List(_) => "an array".to_string(),
        other => format!("a {}", other.type_name()),
    };
    error_as(TYPE_ERROR, format!("`{path}` is {got}, not {expected}"), span)
}

fn convert(vm: &mut Interpreter, value: Value, ty: &TypeExpr, home: &Weak<Globals>, path: &str, span: Span) -> Exec<Value> {
    let TypeKind::Named { name, args } = &ty.kind else {
        if let TypeKind::Nullable(inner) = &ty.kind {
            return if matches!(value, Value::Null) { Ok(Value::Null) } else { convert(vm, value, inner, home, path, span) };
        }
        return error_as(TYPE_ERROR, format!("`{ty}` can't be read from JSON"), span);
    };
    let ok = |good: bool, expected: &str| if good { Ok(value.clone()) } else { wrong(&value, path, expected, span) };
    match name.as_str() {
        "any" => Ok(value),
        "string" => ok(matches!(value, Value::Str(_)), "a string"),
        "number" => ok(matches!(value, Value::Int(_) | Value::Float(_)), "a number"),
        "bool" => ok(matches!(value, Value::Bool(_)), "a bool"),
        "list" => {
            let Value::List(items) = &value else { return wrong(&value, path, &format!("an array for `{ty}`"), span) };
            let items: Vec<Value> = items.borrow().clone();
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                out.push(match args.first() {
                    Some(t) => convert(vm, item, t, home, &format!("{path}[{i}]"), span)?,
                    None => item,
                });
            }
            Ok(Value::list(out))
        }
        "map" => {
            let Value::Map(entries) = &value else { return wrong(&value, path, &format!("an object for `{ty}`"), span) };
            if let Some(TypeExpr { kind: TypeKind::Named { name: key, .. }, .. }) = args.first()
                && key != "string"
                && key != "any"
            {
                return error_as(TYPE_ERROR, format!("the keys of JSON objects are strings, so `{ty}` can't be read: use `map<string, ...>`"), span);
            }
            let entries: Vec<(Key, Value)> = entries.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let mut out = IndexMap::new();
            for (key, item) in entries {
                let at = format!("{path}['{}']", key.to_value());
                let item = match args.get(1) {
                    Some(t) => convert(vm, item, t, home, &at, span)?,
                    None => item,
                };
                out.insert(key, item);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }
        class => match find_class(vm, class, home, span)? {
            Some(c) => into_class(vm, value, &c, path, span),
            None => error_as(TYPE_ERROR, format!("`{class}` can't be read from JSON: it isn't a class, an enum, or a type JSON has"), span),
        },
    }
}

/// A class by the name written in a type, in the top level it was written in.
fn find_class(vm: &mut Interpreter, name: &str, home: &Weak<Globals>, span: Span) -> Exec<Option<Rc<Class>>> {
    let Some(globals) = home.upgrade() else { return Ok(None) };
    let found = match name.split_once("::") {
        // A module is loaded when it's first used, which may be now
        Some((module, member)) => match globals.find_module(module) {
            Some(slot) => Some(vm.module_member(&globals, slot as usize, member, false, span)?),
            None => None,
        },
        None => globals.get(name),
    };
    Ok(match found {
        Some(Value::Class(c)) => Some(c),
        _ => None,
    })
}

fn into_class(vm: &mut Interpreter, value: Value, class: &Rc<Class>, path: &str, span: Span) -> Exec<Value> {
    if class.is_enum {
        let Value::Str(name) = &value else { return wrong(&value, path, &format!("the name of a value of `{}`", class.name), span) };
        let values = class.values.borrow();
        if let Some(found) = values.iter().find(|v| matches!(v, Value::Object(o) if o.enum_name.as_deref() == Some(&**name))) {
            return Ok(found.clone());
        }
        let names: Vec<String> = values.iter().filter_map(|v| match v { Value::Object(o) => o.enum_name.as_ref().map(|n| n.to_string()), _ => None }).collect();
        return error_as(TYPE_ERROR, format!("`{path}` is '{name}', which isn't a value of `{}`: {}", class.name, names.join(", ")), span);
    }
    let Value::Map(entries) = &value else { return wrong(&value, path, &format!("an object for `{}`", class.name), span) };
    let entries = entries.borrow().clone();
    let mut named = Vec::new();
    for field in &class.fields {
        match entries.get(&Key::Str(field.name.clone())) {
            Some(item) => {
                let at = format!("{path}.{}", field.name);
                let item = match &field.ty {
                    Some(ty) => convert(vm, item.clone(), ty, &field.home, &at, span)?,
                    None => item.clone(),
                };
                named.push((field.name.to_string(), item, span));
            }
            // Only the constructor that takes every field says which ones it needs
            None if class.constructor.is_none() && !field.has_value && !field.nullable => {
                return error_as(TYPE_ERROR, format!("`{path}` has no `{}`, which `{}` needs", field.name, class.name), span);
            }
            None => {}
        }
    }
    vm.construct_object(class, named, span)
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn error(&self, message: &str) -> NativeError {
        let before = &self.text[..self.pos.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
        NativeError::new(VALUE_ERROR, format!("invalid JSON at line {line}, column {column}: {message}"))
    }

    fn skip_space(&mut self) {
        while self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r') {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), NativeError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", byte as char)))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, NativeError> {
        if depth > MAX_DEPTH {
            return Err(self.error("it's nested too deeply"));
        }
        self.skip_space();
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Value::string(&self.string()?)),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("expected a value")),
            None => Err(self.error("it ends too early")),
        }
    }

    fn word(&mut self, word: &str, value: Value) -> Result<Value, NativeError> {
        if self.text[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("expected a value"))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, NativeError> {
        self.pos += 1;
        let mut map = IndexMap::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Map(Rc::new(RefCell::new(map))));
        }
        loop {
            self.skip_space();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a key in double quotes"));
            }
            let key = self.string()?;
            self.skip_space();
            self.expect(b':')?;
            let value = self.value(depth + 1)?;
            map.insert(Key::Str(Rc::from(key.as_str())), value);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Map(Rc::new(RefCell::new(map))));
                }
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, NativeError> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::list(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::list(items));
                }
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn string(&mut self) -> Result<String, NativeError> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(c) = self.text[self.pos..].chars().next() else {
                return Err(self.error("a string is never closed"));
            };
            self.pos += c.len_utf8();
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let Some(e) = self.peek() else { return Err(self.error("a string is never closed")) };
                    self.pos += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let first = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&first) {
                                // A surrogate pair, for characters outside the first plane
                                if !self.text[self.pos..].starts_with("\\u") {
                                    return Err(self.error("a \\u surrogate isn't followed by its pair"));
                                }
                                self.pos += 2;
                                let second = self.hex4()?;
                                0x10000 + ((first - 0xD800) << 10) + (second.wrapping_sub(0xDC00) & 0x3FF)
                            } else {
                                first
                            };
                            out.push(char::from_u32(code).ok_or_else(|| self.error("invalid \\u escape"))?);
                        }
                        _ => return Err(self.error("unknown escape in a string")),
                    }
                }
                c if (c as u32) < 0x20 => return Err(self.error("a string can't contain control characters like new lines")),
                c => out.push(c),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, NativeError> {
        let digits = self.text.get(self.pos..self.pos + 4).ok_or_else(|| self.error("invalid \\u escape"))?;
        let code = u32::from_str_radix(digits, 16).map_err(|_| self.error("invalid \\u escape"))?;
        self.pos += 4;
        Ok(code)
    }

    fn number(&mut self) -> Result<Value, NativeError> {
        let start = self.pos;
        let digits = |r: &mut Self| {
            let s = r.pos;
            while r.peek().is_some_and(|b| b.is_ascii_digit()) {
                r.pos += 1;
            }
            r.pos > s
        };
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        if self.peek() == Some(b'0') {
            self.pos += 1;
        } else if !digits(self) {
            return Err(self.error("invalid number"));
        }
        let mut float = false;
        if self.peek() == Some(b'.') {
            self.pos += 1;
            float = true;
            if !digits(self) {
                return Err(self.error("invalid number"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            float = true;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !digits(self) {
                return Err(self.error("invalid number"));
            }
        }
        let text = &self.text[start..self.pos];
        if !float {
            if let Ok(n) = text.parse::<i64>() {
                return Ok(Value::Int(n));
            }
        }
        text.parse::<f64>().map(Value::Float).map_err(|_| self.error("invalid number"))
    }
}

pub fn parse(text: &str) -> Result<Value, NativeError> {
    let mut reader = Reader { text, bytes: text.as_bytes(), pos: 0 };
    let value = reader.value(0)?;
    reader.skip_space();
    if reader.pos < text.len() {
        return Err(reader.error("there's more after the value"));
    }
    Ok(value)
}

/// Every value in a text with several of them, separated by whitespace:
/// streams like `{"a":1} {"a":2}` and JSON Lines. An empty text gives `[]`.
pub fn parse_all(text: &str) -> Result<Value, NativeError> {
    let mut reader = Reader { text, bytes: text.as_bytes(), pos: 0 };
    let mut values = Vec::new();
    loop {
        reader.skip_space();
        if reader.pos >= text.len() {
            return Ok(Value::list(values));
        }
        values.push(reader.value(0)?);
    }
}

// ---- Writing ----

fn quote(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn newline(indent: Option<usize>, level: usize, out: &mut String) {
    if let Some(n) = indent {
        out.push('\n');
        out.push_str(&" ".repeat(n * level));
    }
}

/// What can't be written, and where *(i.e. `` `$.onKey` is a function ``)*.
fn unwritable(what: &str, path: &str) -> NativeError {
    if path == "$" {
        NativeError::new(TYPE_ERROR, format!("{what} can't be written as JSON"))
    } else {
        NativeError::new(TYPE_ERROR, format!("`{path}` is {what}, which JSON can't have"))
    }
}

/// A key or a field after a path *(i.e. `$.name`, or `$["a b"]` for one that isn't a name)*.
pub(super) fn member_path(path: &str, key: &str) -> String {
    if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') && !key.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{path}.{key}")
    } else {
        let mut quoted = String::new();
        quote(key, &mut quoted);
        format!("{path}[{quoted}]")
    }
}

/// The opening, members and closing of an array or an object.
fn members(entries: Vec<(Option<String>, Value)>, open: char, close: char, indent: Option<usize>, level: usize, path: &str, out: &mut String) -> Result<(), NativeError> {
    if entries.is_empty() {
        out.push(open);
        out.push(close);
        return Ok(());
    }
    out.push(open);
    for (i, (key, item)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        newline(indent, level + 1, out);
        let at = match key {
            Some(key) => {
                // JSON keys can only be strings
                quote(key, out);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                member_path(path, key)
            }
            None => format!("{path}[{i}]"),
        };
        write(item, indent, level + 1, &at, out)?;
    }
    newline(indent, level, out);
    out.push(close);
    Ok(())
}

fn write(value: &Value, indent: Option<usize>, level: usize, path: &str, out: &mut String) -> Result<(), NativeError> {
    if level > MAX_DEPTH {
        return Err(NativeError::new(TYPE_ERROR, "the value is nested too deeply, or contains itself"));
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(f) if !f.is_finite() && path == "$" => {
            return Err(NativeError::new(TYPE_ERROR, format!("JSON can't hold {}", format_number(*f))));
        }
        Value::Float(f) if !f.is_finite() => return Err(unwritable(&format_number(*f), path)),
        Value::Float(f) => out.push_str(&format_number(*f)),
        Value::Str(s) => quote(s, out),
        Value::List(l) => {
            let items = l.borrow().iter().map(|v| (None, v.clone())).collect();
            members(items, '[', ']', indent, level, path, out)?;
        }
        Value::Range(r) => members(r.iter().map(|n| (None, Value::Int(n))).collect(), '[', ']', indent, level, path, out)?,
        Value::Map(map) => {
            let entries = map.borrow().iter().map(|(k, v)| (Some(k.to_value().to_string()), v.clone())).collect();
            members(entries, '{', '}', indent, level, path, out)?;
        }
        // A value of an enum: its name, the way it's read
        Value::Object(o) if o.enum_name.is_some() => quote(o.enum_name.as_deref().unwrap_or_default(), out),
        // Every field, by its name, so reading it back into its class gives the same object
        Value::Object(o) => {
            let fields = o.fields.borrow().clone();
            let entries = o.class.fields.iter().zip(fields).map(|(slot, v)| (Some(slot.name.to_string()), v)).collect();
            members(entries, '{', '}', indent, level, path, out)?;
        }
        Value::Handle(_) => return Err(unwritable("Rust data of the standard library", path)),
        other => return Err(unwritable(&format!("a {}", other.type_name()), path)),
    }
    Ok(())
}

pub fn stringify(value: &Value, indent: Option<usize>) -> Result<String, NativeError> {
    let mut out = String::new();
    write(value, indent, 0, "$", &mut out)?;
    Ok(out)
}

fn indent_of(a: &Args, i: usize, default: Option<usize>) -> Result<Option<usize>, NativeError> {
    if a.values.len() <= i {
        return Ok(default);
    }
    match a.opt_whole(i)? {
        None => Ok(None),
        Some(n) if (0..=16).contains(&n) => Ok(Some(n as usize)),
        Some(_) => Err(NativeError::new(VALUE_ERROR, "the indent must be between 0 and 16")),
    }
}

pub fn register(m: &mut Exports) {
    m.vm_function("parse", &["text", "type"], |vm, values, span| {
        let a = Args::new("json::parse", values, 1, 2).map_err(|e| native_error(e, span))?;
        let value = parse(&a.str(0).map_err(|e| native_error(e, span))?).map_err(|e| native_error(e, span))?;
        typed(vm, value, &a.value(1), span)
    });
    m.vm_function("parseAll", &["text", "type"], |vm, values, span| {
        let a = Args::new("json::parseAll", values, 1, 2).map_err(|e| native_error(e, span))?;
        let all = parse_all(&a.str(0).map_err(|e| native_error(e, span))?).map_err(|e| native_error(e, span))?;
        let Value::List(items) = &all else { return Ok(all) };
        if matches!(a.value(1), Value::Null) {
            return Ok(all);
        }
        let items: Vec<Value> = items.borrow().clone();
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.into_iter().enumerate() {
            out.push(convert_to(vm, item, &a.value(1), &format!("$[{i}]"), span)?);
        }
        Ok(Value::list(out))
    });
    m.function_named("stringify", &["value", "indent"], |_, values| {
        let a = Args::new("json::stringify", values, 1, 2)?;
        Ok(Value::string(&stringify(&a.value(0), indent_of(&a, 1, None)?)?))
    });
    m.vm_function("read", &["path", "type"], |vm, values, span| {
        let a = Args::new("json::read", values, 1, 2).map_err(|e| native_error(e, span))?;
        let p = a.str(0).map_err(|e| native_error(e, span))?;
        let text = std::fs::read_to_string(vm.host.state.resolve(&p)).map_err(|e| native_error(io_error("read", &p, e), span))?;
        let value = parse(&text).map_err(|e| native_error(NativeError::new(&e.kind, format!("{} (in '{p}')", e.message)), span))?;
        typed(vm, value, &a.value(1), span)
    });
    m.function_named("write", &["path", "value", "indent"], |ctx, values| {
        let a = Args::new("json::write", values, 2, 3)?;
        let p = a.str(0)?;
        let mut text = stringify(&a.value(1), indent_of(&a, 2, Some(2))?)?;
        text.push('\n');
        std::fs::write(ctx.state.resolve(&p), text).map_err(|e| io_error("write", &p, e))?;
        Ok(Value::Null)
    });
}
