//! `std/env` *(see `ideas/stdlib/env.md`)*. Changes stay in the script and
//! the commands it runs: the real environment is never touched.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use super::{Args, Exports};
use crate::exceptions::VALUE_ERROR;
use crate::value::{Key, NativeError, Value};

pub fn register(m: &mut Exports) {
    m.function("get", |ctx, values| {
        let a = Args::new("env::get", values, 1, 1)?;
        Ok(ctx.state.env_get(&a.str(0)?).map(|v| Value::string(&v)).unwrap_or(Value::Null))
    });
    m.function("require", |ctx, values| {
        let a = Args::new("env::require", values, 1, 1)?;
        let name = a.str(0)?;
        match ctx.state.env_get(&name) {
            Some(v) if !v.is_empty() => Ok(Value::string(&v)),
            Some(_) => Err(NativeError::new(VALUE_ERROR, format!("the environment variable `{name}` is empty"))),
            None => Err(NativeError::new(VALUE_ERROR, format!("the environment variable `{name}` isn't set"))),
        }
    });
    m.function("has", |ctx, values| {
        let a = Args::new("env::has", values, 1, 1)?;
        Ok(Value::Bool(ctx.state.env_get(&a.str(0)?).is_some()))
    });
    m.function("set", |ctx, values| {
        let a = Args::new("env::set", values, 2, 2)?;
        let name = a.str(0)?;
        if name.is_empty() || name.contains('=') || name.contains('\0') {
            return Err(NativeError::new(VALUE_ERROR, format!("'{name}' isn't a valid environment variable name")));
        }
        ctx.state.env.insert(name.to_string(), Some(a.str(1)?.to_string()));
        Ok(Value::Null)
    });
    m.function("remove", |ctx, values| {
        let a = Args::new("env::remove", values, 1, 1)?;
        ctx.state.env.insert(a.str(0)?.to_string(), None);
        Ok(Value::Null)
    });
    m.function("parse", |_, values| {
        let a = Args::new("env::parse", values, 1, 1)?;
        let pairs = dotenv(&a.str(0)?).map_err(|e| NativeError::new(VALUE_ERROR, e))?;
        let map: IndexMap<Key, Value> = pairs.into_iter().map(|(k, v)| (Key::Str(Rc::from(k.as_str())), Value::string(&v))).collect();
        Ok(Value::Map(Rc::new(RefCell::new(map))))
    });
    m.function("load", |ctx, values| {
        let a = Args::new("env::load", values, 0, 1)?;
        let shown = a.str_or(0, ".env")?.to_string();
        let path = ctx.state.resolve(&shown);
        let text = std::fs::read_to_string(&path).map_err(|e| super::io_error("read", &shown, e))?;
        let pairs = dotenv(&text).map_err(|e| NativeError::new(VALUE_ERROR, format!("'{shown}': {e}")))?;
        // A variable that's already set wins, like dotenv: the file only gives defaults
        for (name, value) in pairs {
            if ctx.state.env_get(&name).is_none() {
                ctx.state.env.insert(name, Some(value));
            }
        }
        Ok(Value::Null)
    });
    m.function("all", |ctx, values| {
        Args::new("env::all", values, 0, 0)?;
        let map: IndexMap<Key, Value> = ctx
            .state
            .env_all()
            .into_iter()
            .map(|(k, v)| (Key::Str(Rc::from(k.as_str())), Value::string(&v)))
            .collect();
        Ok(Value::Map(Rc::new(RefCell::new(map))))
    });
}

/// The variables of a `.env` file, in order: `NAME=value` lines, with `#`
/// comments, an `export` before them, and values in quotes. A value in
/// `"..."` takes `\n`, `\t`, `\"` and `\\`; one in `'...'` is as it's
/// written; one without quotes ends at a ` #`.
fn dotenv(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map(str::trim_start).unwrap_or(line);
        let wrong = |why: &str| format!("line {} isn't `NAME=value`: {why}", i + 1);
        let Some((name, value)) = line.split_once('=') else { return Err(wrong("it has no `=`")) };
        let name = name.trim();
        let valid = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
        if !valid {
            return Err(wrong(&format!("'{name}' isn't the name of a variable")));
        }
        let value = value.trim();
        let value = match value.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let rest = &value[1..];
                let Some(end) = closing(rest, quote) else { return Err(wrong(&format!("its value has no closing {quote}"))) };
                let after = rest[end + 1..].trim();
                if !after.is_empty() && !after.starts_with('#') {
                    return Err(wrong("there's something after the quotes"));
                }
                let inner = &rest[..end];
                if quote == '"' { unescape(inner) } else { inner.to_string() }
            }
            _ => match value.find(" #") {
                Some(at) => value[..at].trim_end().to_string(),
                None => value.to_string(),
            },
        };
        pairs.push((name.to_string(), value));
    }
    Ok(pairs)
}

/// Where the `quote` that closes a value is, skipping `\"` inside `"..."`.
fn closing(text: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' && quote == '"' {
            escaped = true;
        } else if c == quote {
            return Some(i);
        }
    }
    None
}

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::dotenv;

    #[test]
    fn env_files() {
        let text = "# a comment\n\nexport A=1\nB = two words # a comment\nC=\"x\\ny \\\"q\\\"\"\nD='$no \\n'\nE=\nF=a#b\n";
        let found = dotenv(text).unwrap();
        let expected = [("A", "1"), ("B", "two words"), ("C", "x\ny \"q\""), ("D", "$no \\n"), ("E", ""), ("F", "a#b")];
        assert_eq!(found, expected.map(|(k, v)| (k.to_string(), v.to_string())));
        assert_eq!(dotenv("A=1\nnope\n").unwrap_err(), "line 2 isn't `NAME=value`: it has no `=`");
        assert!(dotenv("1A=x").unwrap_err().contains("isn't the name of a variable"));
        assert!(dotenv("A=\"open").unwrap_err().contains("no closing \""));
    }
}
