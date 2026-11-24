//! `std/format/json` *(see `ideas/stdlib/format/json.md`)*: strict JSON, without a schema.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use super::{Args, Exports, io_error};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::value::{Key, NativeError, Value, format_number};

/// How deep arrays and objects can be nested, so a huge or circular value
/// can't overflow the stack.
const MAX_DEPTH: usize = 512;

// ---- Reading ----

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

fn write(value: &Value, indent: Option<usize>, level: usize, out: &mut String) -> Result<(), NativeError> {
    if level > MAX_DEPTH {
        return Err(NativeError::new(TYPE_ERROR, "the value is nested too deeply, or contains itself"));
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(f) if !f.is_finite() => {
            return Err(NativeError::new(TYPE_ERROR, format!("JSON can't hold {}", format_number(*f))));
        }
        Value::Float(f) => out.push_str(&format_number(*f)),
        Value::Str(s) => quote(s, out),
        Value::List(_) | Value::Range(_) => {
            let items: Vec<Value> = match value {
                Value::List(l) => l.borrow().clone(),
                Value::Range(r) => r.iter().map(Value::Int).collect(),
                _ => unreachable!(),
            };
            if items.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(indent, level + 1, out);
                write(item, indent, level + 1, out)?;
            }
            newline(indent, level, out);
            out.push(']');
        }
        Value::Map(map) => {
            let map = map.borrow();
            if map.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(indent, level + 1, out);
                // JSON keys can only be strings
                quote(&key.to_value().to_string(), out);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write(item, indent, level + 1, out)?;
            }
            newline(indent, level, out);
            out.push('}');
        }
        other => return Err(NativeError::new(TYPE_ERROR, format!("a {} can't be written as JSON", other.type_name()))),
    }
    Ok(())
}

pub fn stringify(value: &Value, indent: Option<usize>) -> Result<String, NativeError> {
    let mut out = String::new();
    write(value, indent, 0, &mut out)?;
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
    m.function("parse", |_, values| {
        let a = Args::new("json::parse", values, 1, 1)?;
        parse(&a.str(0)?)
    });
    m.function("parseAll", |_, values| {
        let a = Args::new("json::parseAll", values, 1, 1)?;
        parse_all(&a.str(0)?)
    });
    m.function_named("stringify", &["value", "indent"], |_, values| {
        let a = Args::new("json::stringify", values, 1, 2)?;
        Ok(Value::string(&stringify(&a.value(0), indent_of(&a, 1, None)?)?))
    });
    m.function("read", |ctx, values| {
        let a = Args::new("json::read", values, 1, 1)?;
        let p = a.str(0)?;
        let text = std::fs::read_to_string(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        parse(&text).map_err(|e| NativeError::new(&e.kind, format!("{} (in '{p}')", e.message)))
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
