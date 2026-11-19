//! Checking values against types while the script runs: `.is()`, `.as()` and
//! `case string ->`.

use crate::ast::{TypeExpr, TypeKind};
use crate::value::Value;

/// `true` if the value has the type. Generic types check everything inside
/// *(i.e. `list<string>` checks every element)*.
pub fn matches(value: &Value, ty: &TypeExpr) -> Result<bool, String> {
    match &ty.kind {
        TypeKind::Nullable(inner) => Ok(matches!(value, Value::Null) || matches(value, inner)?),
        TypeKind::Function { .. } => Err(
            "function types can't be checked while the script runs, because types are removed before it runs".into(),
        ),
        TypeKind::Named { name, args } => {
            let no_args = |ok: bool| -> Result<bool, String> {
                if args.is_empty() { Ok(ok) } else { Err(format!("`{name}` doesn't take type parameters")) }
            };
            match name.as_str() {
                "any" => no_args(true),
                "string" => no_args(matches!(value, Value::Str(_))),
                "number" => no_args(matches!(value, Value::Int(_) | Value::Float(_))),
                "bool" => no_args(matches!(value, Value::Bool(_))),
                "symbol" => no_args(matches!(value, Value::Symbol(_))),
                "range" => no_args(matches!(value, Value::Range(_))),
                "regex" => no_args(matches!(value, Value::Regex(_))),
                "result" => Ok(matches!(value, Value::Result(_))),
                "command" => no_args(matches!(value, Value::Command(_))),
                name if crate::exceptions::is_exception_type(name) => {
                    no_args(matches!(value, Value::Exception(e) if crate::exceptions::is_a(&e.kind, name)))
                }
                "list" => {
                    let Value::List(items) = value else { return Ok(false) };
                    match args.as_slice() {
                        [] => Ok(true),
                        [elem] => {
                            for item in items.borrow().iter() {
                                if !matches(item, elem)? {
                                    return Ok(false);
                                }
                            }
                            Ok(true)
                        }
                        _ => Err("`list` takes one type parameter *(i.e. `list<string>`)*".into()),
                    }
                }
                "map" => {
                    let Value::Map(entries) = value else { return Ok(false) };
                    match args.as_slice() {
                        [] => Ok(true),
                        [k, v] => {
                            for (key, val) in entries.borrow().iter() {
                                if !matches(&key.to_value(), k)? || !matches(val, v)? {
                                    return Ok(false);
                                }
                            }
                            Ok(true)
                        }
                        _ => Err("`map` takes two type parameters *(i.e. `map<string, number>`)*".into()),
                    }
                }
                "void" => Err("`void` isn't the type of any value".into()),
                other => Err(format!("unknown type `{other}`")),
            }
        }
    }
}
