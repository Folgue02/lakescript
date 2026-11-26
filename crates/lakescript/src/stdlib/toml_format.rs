//! `std/format/toml` *(see `ideas/stdlib/format/toml.md`)*: TOML read into
//! maps and lists, or into classes like `std/format/json`, and written from
//! them.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use super::json::{member_path, typed};
use super::{Args, Exports, io_error};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::interpreter::native_error;
use crate::value::{Key, NativeError, Value, format_number};

/// How deep tables and arrays can be nested when written.
const MAX_DEPTH: usize = 512;

// ---- Reading ----

pub fn parse(text: &str) -> Result<Value, NativeError> {
    match toml::from_str::<toml::Table>(text) {
        Ok(table) => Ok(table_value(table)),
        Err(e) => {
            let place = match e.span() {
                Some(span) => {
                    let before = &text[..span.start.min(text.len())];
                    let line = before.matches('\n').count() + 1;
                    let column = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
                    format!(" at line {line}, column {column}")
                }
                None => String::new(),
            };
            Err(NativeError::new(VALUE_ERROR, format!("invalid TOML{place}: {}", e.message().trim())))
        }
    }
}

fn table_value(table: toml::Table) -> Value {
    let map: IndexMap<Key, Value> = table.into_iter().map(|(k, v)| (Key::Str(Rc::from(k.as_str())), value(v))).collect();
    Value::Map(Rc::new(RefCell::new(map)))
}

fn value(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::string(&s),
        toml::Value::Integer(n) => Value::Int(n),
        toml::Value::Float(f) => Value::Float(f),
        toml::Value::Boolean(b) => Value::Bool(b),
        // Its text, as TOML writes it *(i.e. `1979-05-27T07:32:00Z`)*, which `std/time` reads
        toml::Value::Datetime(d) => Value::string(&d.to_string()),
        toml::Value::Array(items) => Value::list(items.into_iter().map(value).collect()),
        toml::Value::Table(table) => table_value(table),
    }
}

// ---- Writing ----

fn unwritable(what: &str, path: &str) -> NativeError {
    NativeError::new(TYPE_ERROR, format!("`{path}` is {what}, which TOML can't have"))
}

fn to_toml(v: &Value, level: usize, path: &str) -> Result<toml::Value, NativeError> {
    if level > MAX_DEPTH {
        return Err(NativeError::new(TYPE_ERROR, "the value is nested too deeply, or contains itself"));
    }
    Ok(match v {
        Value::Null => return Err(unwritable("`null`", path)),
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Int(n) => toml::Value::Integer(*n),
        Value::Float(f) if f.is_nan() => return Err(unwritable(&format_number(*f), path)),
        Value::Float(f) => toml::Value::Float(*f),
        Value::Str(s) => toml::Value::String(s.to_string()),
        Value::List(l) => {
            let items = l.borrow().clone();
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                out.push(to_toml(item, level + 1, &format!("{path}[{i}]"))?);
            }
            toml::Value::Array(out.into_iter().collect())
        }
        Value::Range(r) => toml::Value::Array(r.iter().map(toml::Value::Integer).collect()),
        Value::Map(map) => {
            let entries: Vec<(String, Value)> = map.borrow().iter().map(|(k, v)| (k.to_value().to_string(), v.clone())).collect();
            table(entries, level, path)?
        }
        // A value of an enum: its name, the way it's read
        Value::Object(o) if o.enum_name.is_some() => toml::Value::String(o.enum_name.as_deref().unwrap_or_default().to_string()),
        // Every field, by its name, so reading it back into its class gives the same object
        Value::Object(o) => {
            let fields = o.fields.borrow().clone();
            let entries = o.class.fields.iter().zip(fields).map(|(slot, v)| (slot.name.to_string(), v)).collect();
            table(entries, level, path)?
        }
        Value::Handle(_) => return Err(unwritable("Rust data of the standard library", path)),
        other => return Err(unwritable(&format!("a {}", other.type_name()), path)),
    })
}

fn table(entries: Vec<(String, Value)>, level: usize, path: &str) -> Result<toml::Value, NativeError> {
    let mut out = toml::Table::new();
    for (key, item) in entries {
        let at = member_path(path, &key);
        out.insert(key, to_toml(&item, level + 1, &at)?);
    }
    Ok(toml::Value::Table(out))
}

pub fn stringify(v: &Value) -> Result<String, NativeError> {
    let toml::Value::Table(table) = to_toml(v, 0, "$")? else {
        let what = match v {
            Value::List(_) | Value::Range(_) => "a list".to_string(),
            other => format!("a {}", other.type_name()),
        };
        return Err(NativeError::new(TYPE_ERROR, format!("a TOML document is a table: a map or an object, not {what}")));
    };
    toml::to_string(&table).map_err(|e| NativeError::new(TYPE_ERROR, format!("can't be written as TOML: {e}")))
}

pub fn register(m: &mut Exports) {
    m.vm_function("parse", &["text", "type"], |vm, values, span| {
        let a = Args::new("toml::parse", values, 1, 2).map_err(|e| native_error(e, span))?;
        let value = parse(&a.str(0).map_err(|e| native_error(e, span))?).map_err(|e| native_error(e, span))?;
        typed(vm, value, &a.value(1), span)
    });
    m.function_named("stringify", &["value"], |_, values| {
        let a = Args::new("toml::stringify", values, 1, 1)?;
        Ok(Value::string(&stringify(&a.value(0))?))
    });
    m.vm_function("read", &["path", "type"], |vm, values, span| {
        let a = Args::new("toml::read", values, 1, 2).map_err(|e| native_error(e, span))?;
        let p = a.str(0).map_err(|e| native_error(e, span))?;
        let text = std::fs::read_to_string(vm.host.state.resolve(&p)).map_err(|e| native_error(io_error("read", &p, e), span))?;
        let value = parse(&text).map_err(|e| native_error(NativeError::new(&e.kind, format!("{} (in '{p}')", e.message)), span))?;
        typed(vm, value, &a.value(1), span)
    });
    m.function_named("write", &["path", "value"], |ctx, values| {
        let a = Args::new("toml::write", values, 2, 2)?;
        let p = a.str(0)?;
        let text = stringify(&a.value(1))?;
        std::fs::write(ctx.state.resolve(&p), text).map_err(|e| io_error("write", &p, e))?;
        Ok(Value::Null)
    });
}
