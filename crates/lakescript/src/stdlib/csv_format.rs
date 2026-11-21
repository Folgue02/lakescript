//! `std/format/csv` *(see `ideas/stdlib/format/csv.md`)*: rows as lists of
//! strings, or as maps by the names of the columns.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use super::{Args, Exports, io_error};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::value::{Key, NativeError, Value};

/// The separator, which must be one character of ASCII.
fn separator(a: &Args, i: usize) -> Result<u8, NativeError> {
    let s = a.str_or(i, ",")?;
    match s.as_bytes() {
        [b] if b.is_ascii() && *b != b'"' && *b != b'\n' && *b != b'\r' => Ok(*b),
        _ => Err(NativeError::new(VALUE_ERROR, format!("the separator must be one character, like ',' or '\\t', not '{s}'"))),
    }
}

fn invalid(e: csv::Error, file: Option<&str>) -> NativeError {
    let place = match e.position() {
        Some(p) => format!(" at line {}", p.line()),
        None => String::new(),
    };
    let reason = match e.kind() {
        csv::ErrorKind::Utf8 { .. } => "it isn't valid UTF-8".to_string(),
        _ => e.to_string(),
    };
    let file = file.map(|f| format!(" (in '{f}')")).unwrap_or_default();
    NativeError::new(VALUE_ERROR, format!("invalid CSV{place}: {reason}{file}"))
}

/// The rows of a text, every value a string.
fn rows(text: &str, sep: u8, file: Option<&str>) -> Result<Vec<Vec<String>>, NativeError> {
    let mut reader = csv::ReaderBuilder::new().has_headers(false).flexible(true).delimiter(sep).from_reader(text.as_bytes());
    let mut out = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| invalid(e, file))?;
        out.push(record.iter().map(String::from).collect());
    }
    Ok(out)
}

fn parse(text: &str, sep: u8, file: Option<&str>) -> Result<Value, NativeError> {
    let rows = rows(text, sep, file)?;
    Ok(Value::list(rows.into_iter().map(super::strings).collect()))
}

/// The rows after the first as maps, by the names in the first.
fn parse_maps(text: &str, sep: u8, file: Option<&str>) -> Result<Value, NativeError> {
    let mut rows = rows(text, sep, file)?.into_iter();
    let Some(header) = rows.next() else { return Ok(Value::list(Vec::new())) };
    for (i, name) in header.iter().enumerate() {
        if header[..i].contains(name) {
            return Err(NativeError::new(VALUE_ERROR, format!("the column '{name}' is twice in the first line")));
        }
    }
    let mut out = Vec::new();
    for (n, row) in rows.enumerate() {
        if row.len() > header.len() {
            return Err(NativeError::new(
                VALUE_ERROR,
                format!("line {} has {} values, but the first line only names {} columns", n + 2, row.len(), header.len()),
            ));
        }
        let mut map = IndexMap::new();
        for (i, name) in header.iter().enumerate() {
            let value = row.get(i).map(String::as_str).unwrap_or("");
            map.insert(Key::Str(Rc::from(name.as_str())), Value::string(value));
        }
        out.push(Value::Map(Rc::new(RefCell::new(map))));
    }
    Ok(Value::list(out))
}

/// A value of a cell as text.
fn cell(value: &Value) -> Result<String, NativeError> {
    match value {
        Value::Null => Ok(String::new()),
        Value::Str(_) | Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Symbol(_) => Ok(value.to_string()),
        other => Err(NativeError::new(TYPE_ERROR, format!("a value of CSV can be a string, a number, a bool or null, not a {}", other.type_name()))),
    }
}

/// Lists of values, or maps that all have the same keys, whose names go in the first line.
fn stringify(value: &Value, sep: u8) -> Result<String, NativeError> {
    let Value::List(items) = value else {
        return Err(NativeError::new(TYPE_ERROR, format!("CSV is written from a list of rows, not a {}", value.type_name())));
    };
    let mut writer = csv::WriterBuilder::new().flexible(true).delimiter(sep).from_writer(Vec::new());
    let items = items.borrow();
    let mut header: Option<Vec<Key>> = None;
    for item in items.iter() {
        let cells: Vec<String> = match item {
            Value::List(values) => values.borrow().iter().map(cell).collect::<Result<_, _>>()?,
            Value::Map(map) => {
                let map = map.borrow();
                let keys = match &header {
                    Some(keys) => keys,
                    None => {
                        let keys: Vec<Key> = map.keys().cloned().collect();
                        let names: Vec<String> = keys.iter().map(|k| k.to_value().to_string()).collect();
                        writer.write_record(&names).map_err(|e| invalid(e, None))?;
                        header.insert(keys)
                    }
                };
                if let Some(extra) = map.keys().find(|k| !keys.contains(k)) {
                    return Err(NativeError::new(
                        VALUE_ERROR,
                        format!("a row has the key '{}', which the first one doesn't have", extra.to_value()),
                    ));
                }
                keys.iter().map(|k| map.get(k).map(cell).unwrap_or(Ok(String::new()))).collect::<Result<_, _>>()?
            }
            other => return Err(NativeError::new(TYPE_ERROR, format!("a row of CSV is a list or a map, not a {}", other.type_name()))),
        };
        writer.write_record(&cells).map_err(|e| invalid(e, None))?;
    }
    let bytes = writer.into_inner().map_err(|e| NativeError::new(VALUE_ERROR, e.to_string()))?;
    Ok(String::from_utf8(bytes).unwrap_or_default())
}

pub fn register(m: &mut Exports) {
    m.function_named("parse", &["text", "separator"], |_, values| {
        let a = Args::new("csv::parse", values, 1, 2)?;
        parse(&a.str(0)?, separator(&a, 1)?, None)
    });
    m.function_named("parseMaps", &["text", "separator"], |_, values| {
        let a = Args::new("csv::parseMaps", values, 1, 2)?;
        parse_maps(&a.str(0)?, separator(&a, 1)?, None)
    });
    m.function_named("stringify", &["rows", "separator"], |_, values| {
        let a = Args::new("csv::stringify", values, 1, 2)?;
        Ok(Value::string(&stringify(&a.value(0), separator(&a, 1)?)?))
    });
    m.function_named("read", &["path", "separator"], |ctx, values| {
        let a = Args::new("csv::read", values, 1, 2)?;
        let p = a.str(0)?;
        let text = std::fs::read_to_string(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        parse(&text, separator(&a, 1)?, Some(&p))
    });
    m.function_named("readMaps", &["path", "separator"], |ctx, values| {
        let a = Args::new("csv::readMaps", values, 1, 2)?;
        let p = a.str(0)?;
        let text = std::fs::read_to_string(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        parse_maps(&text, separator(&a, 1)?, Some(&p))
    });
    m.function_named("write", &["path", "rows", "separator"], |ctx, values| {
        let a = Args::new("csv::write", values, 2, 3)?;
        let p = a.str(0)?;
        let text = stringify(&a.value(1), separator(&a, 2)?)?;
        std::fs::write(ctx.state.resolve(&p), text).map_err(|e| io_error("write", &p, e))?;
        Ok(Value::Null)
    });
}
