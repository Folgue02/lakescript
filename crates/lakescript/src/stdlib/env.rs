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
