//! `std/tooling/ferry/native`, the Rust part of `std/tooling/ferry` *(see
//! ideas/stdlib/tooling/ferry.md)*: it reads a project with the crate
//! `ferry-project`, the same code ferry reads it with, so a script sees a
//! project exactly as ferry does, and gets ferry's errors. Only `ferry.lks`
//! uses it.

use std::path::Path;

use ferry_project::{Kind, Lock, Project, find_dir, not_found};

use crate::exceptions::VALUE_ERROR;
use crate::stdlib::{Args, Exports};
use crate::value::{NativeError, Value};

pub fn register(m: &mut Exports) {
    // The project of a folder or the folders above it, as
    // `[id, author, name, version, kind, lks, dir, [[package, wanted, version], ...]]`
    m.function("project", |ctx, values| {
        let a = Args::new("ferry::project", values, 1, 1)?;
        let start = ctx.state.resolve(&a.str(0)?);
        let dir = find_dir(&start).ok_or_else(|| NativeError::new("FileNotFound", not_found(&start)))?;
        let project = Project::read(&dir).map_err(|e| NativeError::new(VALUE_ERROR, e))?;
        let lock = Lock::read(&dir).map_err(|e| NativeError::new(VALUE_ERROR, e))?;
        let info = &project.manifest.project;
        let dependencies = project
            .dependencies
            .iter()
            .map(|(package, wanted)| {
                let version = lock.version(package).map(Value::string).unwrap_or(Value::Null);
                Value::list(vec![Value::string(&package.to_string()), Value::string(wanted), version])
            })
            .collect();
        let kind = match info.kind {
            Kind::Library => "library",
            Kind::Application => "application",
        };
        Ok(Value::list(vec![
            Value::string(&project.id),
            Value::string(&info.author),
            Value::string(&info.name),
            Value::string(&info.version),
            Value::string(kind),
            info.lks.as_deref().map(Value::string).unwrap_or(Value::Null),
            Value::string(&shown(&dir)),
            Value::list(dependencies),
        ]))
    });
}

/// A path with `/`, like every path in Lakescript.
fn shown(dir: &Path) -> String {
    let text = dir.to_string_lossy();
    if cfg!(windows) { text.replace('\\', "/") } else { text.into_owned() }
}
