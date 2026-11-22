//! `std/path` *(see `ideas/stdlib/path.md`)*. Paths are plain strings that
//! use `/`. Nothing here touches the disk.

use super::{Args, Exports, strings};
use crate::value::{NativeError, Value};

fn clean(path: &str) -> String {
    if cfg!(windows) { path.replace('\\', "/") } else { path.to_string() }
}

pub fn absolute_q(path: &str) -> bool {
    let b = path.as_bytes();
    path.starts_with('/') || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/')
}

/// The path without `/` at the end, except for the root.
fn trimmed(path: &str) -> &str {
    let t = path.trim_end_matches('/');
    if t.is_empty() && path.starts_with('/') { "/" } else { t }
}

pub fn join(parts: &[String]) -> String {
    let mut result = String::new();
    for part in parts {
        let part = clean(part);
        if part.is_empty() {
            continue;
        }
        if absolute_q(&part) || result.is_empty() {
            result = part;
        } else if result.ends_with('/') {
            result.push_str(part.trim_start_matches('/'));
        } else {
            result.push('/');
            result.push_str(&part);
        }
    }
    result
}

fn parent(path: &str) -> Option<String> {
    let p = trimmed(path);
    if p == "/" {
        return None;
    }
    match p.rfind('/') {
        None => None,
        Some(0) => Some("/".into()),
        Some(i) => Some(p[..i].to_string()),
    }
}

fn name(path: &str) -> String {
    let p = trimmed(path);
    if p == "/" {
        return String::new();
    }
    p.rsplit('/').next().unwrap_or(p).to_string()
}

/// The stem and the extension of a name. `.bashrc` has no extension.
fn split_name(name: &str) -> (String, Option<String>) {
    match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), Some(name[i + 1..].to_string())),
        _ => (name.to_string(), None),
    }
}

/// The path without its last part, keeping the `/`.
fn folder_part(path: &str) -> String {
    let p = trimmed(path);
    let n = name(p);
    p[..p.len() - n.len()].to_string()
}

pub fn normalize(path: &str) -> String {
    let path = clean(path);
    let absolute = absolute_q(&path);
    let mut prefix = String::new();
    let mut rest = path.as_str();
    if absolute {
        if path.starts_with('/') {
            prefix.push('/');
            rest = &path[1..];
        } else {
            prefix = path[..3].to_string();
            rest = &path[3..];
        }
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => match parts.last() {
                Some(&last) if last != ".." => {
                    parts.pop();
                }
                _ if absolute => {}
                _ => parts.push(".."),
            },
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("{prefix}{joined}"),
        (false, true) => ".".into(),
        (false, false) => joined,
    }
}

fn relative(path: &str, from: &str) -> String {
    let parts = |p: &str| -> Vec<String> { p.split('/').filter(|s| !s.is_empty()).map(String::from).collect() };
    let (to, base) = (parts(path), parts(from));
    let common = to.iter().zip(base.iter()).take_while(|(a, b)| a == b).count();
    let mut out: Vec<String> = std::iter::repeat_n("..".to_string(), base.len() - common).collect();
    out.extend(to[common..].iter().cloned());
    if out.is_empty() { ".".into() } else { out.join("/") }
}

pub fn home(ctx_home: Option<String>) -> Result<String, NativeError> {
    ctx_home.ok_or_else(|| NativeError::new("IOError", "can't find the home folder: `HOME` isn't set"))
}

pub fn register(m: &mut Exports) {
    m.constant("separator", Value::string(if cfg!(windows) { "\\" } else { "/" }));

    m.function("join", |_, values| {
        let a = Args::new("path::join", values, 0, usize::MAX)?;
        Ok(Value::string(&join(&a.rest_strings(0)?)))
    });
    m.function("withExtension", |_, values| {
        let a = Args::new("path::withExtension", values, 2, 2)?;
        let path = clean(&a.str(0)?);
        let ext = a.str(1)?;
        let ext = ext.trim_start_matches('.');
        let (stem, _) = split_name(&name(&path));
        let new_name = if ext.is_empty() { stem } else { format!("{stem}.{ext}") };
        Ok(Value::string(&format!("{}{new_name}", folder_part(&path))))
    });
    m.function("withName", |_, values| {
        let a = Args::new("path::withName", values, 2, 2)?;
        let path = clean(&a.str(0)?);
        Ok(Value::string(&format!("{}{}", folder_part(&path), a.str(1)?)))
    });
    m.function("parent", |_, values| {
        let a = Args::new("path::parent", values, 1, 1)?;
        Ok(parent(&clean(&a.str(0)?)).map(|p| Value::string(&p)).unwrap_or(Value::Null))
    });
    m.function("name", |_, values| {
        let a = Args::new("path::name", values, 1, 1)?;
        Ok(Value::string(&name(&clean(&a.str(0)?))))
    });
    m.function("stem", |_, values| {
        let a = Args::new("path::stem", values, 1, 1)?;
        Ok(Value::string(&split_name(&name(&clean(&a.str(0)?))).0))
    });
    m.function("extension", |_, values| {
        let a = Args::new("path::extension", values, 1, 1)?;
        Ok(split_name(&name(&clean(&a.str(0)?))).1.map(|e| Value::string(&e)).unwrap_or(Value::Null))
    });
    m.function("parts", |_, values| {
        let a = Args::new("path::parts", values, 1, 1)?;
        let path = clean(&a.str(0)?);
        let mut parts = Vec::new();
        if path.starts_with('/') {
            parts.push("/".to_string());
        }
        parts.extend(path.split('/').filter(|p| !p.is_empty()).map(String::from));
        Ok(strings(parts))
    });
    m.function("absolute?", |_, values| {
        let a = Args::new("path::absolute?", values, 1, 1)?;
        Ok(Value::Bool(absolute_q(&clean(&a.str(0)?))))
    });
    m.function("absolute", |ctx, values| {
        let a = Args::new("path::absolute", values, 1, 1)?;
        let path = clean(&a.str(0)?);
        let full = if absolute_q(&path) { path } else { join(&[ctx.state.cwd.display().to_string(), path]) };
        Ok(Value::string(&normalize(&full)))
    });
    m.function("normalize", |_, values| {
        let a = Args::new("path::normalize", values, 1, 1)?;
        Ok(Value::string(&normalize(&a.str(0)?)))
    });
    m.function("relative", |ctx, values| {
        let a = Args::new("path::relative", values, 1, 2)?;
        let cwd = ctx.state.cwd.display().to_string();
        let full = |p: std::rc::Rc<str>| {
            let p = clean(&p);
            normalize(&if absolute_q(&p) { p } else { join(&[cwd.clone(), p]) })
        };
        let path = full(a.str(0)?);
        let from = full(a.str_or(1, ".")?);
        Ok(Value::string(&relative(&path, &from)))
    });
    m.function("expandHome", |ctx, values| {
        let a = Args::new("path::expandHome", values, 1, 1)?;
        let path = a.str(0)?;
        if &*path == "~" || path.starts_with("~/") {
            let home = home(ctx.state.env_get("HOME").or_else(|| ctx.state.env_get("USERPROFILE")))?;
            return Ok(Value::string(&format!("{home}{}", &path[1..])));
        }
        Ok(Value::string(&path))
    });
    m.function("home", |ctx, values| {
        Args::new("path::home", values, 0, 0)?;
        Ok(Value::string(&home(ctx.state.env_get("HOME").or_else(|| ctx.state.env_get("USERPROFILE")))?))
    });
}
