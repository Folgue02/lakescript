//! The methods of the built-in types. Phase 1 has only the few that simple
//! scripts need. The full list is in `ideas/stdlib/types/`.

use crate::interpreter::{Exec, Interpreter, error};
use crate::token::Span;
use crate::value::{Key, Value};

pub fn call(_interp: &mut Interpreter, receiver: Value, method: &str, args: Vec<Value>, span: Span) -> Exec<Value> {
    let arity = |n: usize| -> Exec<()> {
        if args.len() == n {
            Ok(())
        } else {
            let s = if n == 1 { "" } else { "s" };
            error(format!("`.{method}()` takes {n} argument{s}, but got {}", args.len()), span)
        }
    };

    // Methods every value has
    match method {
        "toString" => {
            arity(0)?;
            return Ok(Value::string(&receiver.to_string()));
        }
        "equals" => {
            arity(1)?;
            return Ok(Value::Bool(receiver.equals(&args[0])));
        }
        "typeName" => {
            arity(0)?;
            return Ok(Value::string(receiver.type_name()));
        }
        _ => {}
    }

    match (&receiver, method) {
        (Value::Str(s), "length") => {
            arity(0)?;
            Ok(Value::Int(s.chars().count() as i64))
        }
        (Value::Str(s), "empty?") => {
            arity(0)?;
            Ok(Value::Bool(s.is_empty()))
        }
        (Value::Str(s), "upper") => {
            arity(0)?;
            Ok(Value::string(&s.to_uppercase()))
        }
        (Value::Str(s), "lower") => {
            arity(0)?;
            Ok(Value::string(&s.to_lowercase()))
        }
        (Value::Str(s), "contains") => {
            arity(1)?;
            match &args[0] {
                Value::Str(part) => Ok(Value::Bool(s.contains(&**part))),
                other => error(format!("`.contains()` on a string needs a string, not a {}", other.type_name()), span),
            }
        }
        (Value::List(list), "length") => {
            arity(0)?;
            Ok(Value::Int(list.borrow().len() as i64))
        }
        (Value::List(list), "empty?") => {
            arity(0)?;
            Ok(Value::Bool(list.borrow().is_empty()))
        }
        (Value::List(list), "contains") => {
            arity(1)?;
            Ok(Value::Bool(list.borrow().iter().any(|v| v.equals(&args[0]))))
        }
        (Value::List(list), "push") => {
            list.borrow_mut().extend(args);
            Ok(receiver.clone())
        }
        (Value::List(list), "join") => {
            let separator = match args.as_slice() {
                [] => String::new(),
                [Value::Str(s)] => s.to_string(),
                _ => return error("`.join()` takes an optional string separator", span),
            };
            let parts: Vec<String> = list.borrow().iter().map(|v| v.to_string()).collect();
            Ok(Value::string(&parts.join(&separator)))
        }
        (Value::Map(map), "length") => {
            arity(0)?;
            Ok(Value::Int(map.borrow().len() as i64))
        }
        (Value::Map(map), "empty?") => {
            arity(0)?;
            Ok(Value::Bool(map.borrow().is_empty()))
        }
        (Value::Map(map), "has") => {
            arity(1)?;
            Ok(Value::Bool(Key::from_value(&args[0]).is_some_and(|k| map.borrow().contains_key(&k))))
        }
        (Value::Map(map), "keys") => {
            arity(0)?;
            Ok(Value::list(map.borrow().keys().map(Key::to_value).collect()))
        }
        (Value::Map(map), "values") => {
            arity(0)?;
            Ok(Value::list(map.borrow().values().cloned().collect()))
        }
        (Value::Range(range), "step") => {
            arity(1)?;
            match args[0].as_whole() {
                Some(step) if step > 0 => {
                    let mut range = *range;
                    range.step = step;
                    Ok(Value::Range(range))
                }
                _ => error("the step must be a whole number bigger than 0", span),
            }
        }
        (Value::Range(range), "toList") => {
            arity(0)?;
            Ok(Value::list(range.iter().map(Value::Int).collect()))
        }
        (Value::Int(_) | Value::Float(_), "even?" | "odd?") => {
            arity(0)?;
            let even = receiver.as_whole().is_some_and(|n| n % 2 == 0);
            let odd = receiver.as_whole().is_some_and(|n| n % 2 != 0);
            Ok(Value::Bool(if method == "even?" { even } else { odd }))
        }
        _ => error(format!("a {} has no method `.{method}()`", receiver.type_name()), span),
    }
}
