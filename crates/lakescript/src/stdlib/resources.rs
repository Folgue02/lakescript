//! `std/resources` *(see `ideas/stdlib/resources.md`)*: the files that aren't
//! code, in `resources/`. They're read from that folder while a project is
//! developed, and from inside the `.lkb` when it's shipped, with the same
//! names, so the code doesn't change. Only read, since a bundle can't change.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::{Args, Exports};
use crate::exceptions::VALUE_ERROR;
use crate::host::{ResourceFiles, Resources};
use crate::value::{NativeContext, NativeError, Value};

const NOT_FOUND: &str = "FileNotFound";

pub fn register(m: &mut Exports) {
    m.function("read", |ctx, values| {
        let a = Args::new("resources::read", values, 1, 1)?;
        let name = a.str(0)?;
        let bytes = read(ctx, &name)?;
        match String::from_utf8(bytes.to_vec()) {
            Ok(text) => Ok(Value::string(&text)),
            Err(_) => Err(NativeError::new(VALUE_ERROR, format!("the resource '{name}' isn't text: `resources::readBytes()` reads any file"))),
        }
    });
    m.function("readBytes", |ctx, values| {
        let a = Args::new("resources::readBytes", values, 1, 1)?;
        Ok(Value::Bytes(read(ctx, &a.str(0)?)?))
    });
    m.function("exists?", |ctx, values| {
        let a = Args::new("resources::exists?", values, 1, 1)?;
        let name = a.str(0)?;
        let name = checked(&name)?;
        // `''` is `resources/` itself, not a resource, and hidden files aren't ones
        if name.is_empty() || hidden(&name) {
            return Ok(Value::Bool(false));
        }
        Ok(Value::Bool(match where_they_are(ctx)? {
            Place::Folder(folder) => folder.join(&name).exists(),
            Place::Files(files) => files.names().iter().any(|k| *k == name || k.starts_with(&format!("{name}/"))),
        }))
    });
    m.function_named("list", &["folder"], |ctx, values| {
        let a = Args::new("resources::list", values, 0, 1)?;
        let folder = a.str_or(0, "")?;
        Ok(Value::list(list(ctx, &folder)?.into_iter().map(|n| Value::string(&n)).collect()))
    });
}

/// Where they are for this script.
enum Place {
    Folder(PathBuf),
    Files(Rc<dyn ResourceFiles>),
}

fn where_they_are(ctx: &NativeContext) -> Result<Place, NativeError> {
    match &ctx.state.resources {
        Resources::Folder(folder) => Ok(Place::Folder(folder.clone())),
        Resources::Files(files) => Ok(Place::Files(files.clone())),
        Resources::NextToScript => match ctx.state.script_path.as_deref().and_then(Path::parent) {
            Some(folder) => Ok(Place::Folder(folder.join("resources"))),
            None => Err(NativeError::new(NOT_FOUND, "there are no resources: the script isn't a file, so there's no resources/ next to it")),
        },
    }
}

/// A name as a path inside `resources/`, with `/`, without `.` or `..`.
fn checked(name: &str) -> Result<String, NativeError> {
    let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    if name.starts_with('/') || name.contains('\\') || name.contains(':') || parts.contains(&"..") {
        return Err(NativeError::new(VALUE_ERROR, format!("'{name}' isn't the name of a resource: it's a path inside resources/, with `/` (i.e. 'templates/page.html')")));
    }
    Ok(parts.join("/"))
}

/// Whether a name has a part that starts with `.` *(i.e. `.DS_Store`)*:
/// bundles leave those out, so they're never resources, also in a folder.
fn hidden(clean: &str) -> bool {
    clean.split('/').any(|part| part.starts_with('.'))
}

fn read(ctx: &NativeContext, name: &str) -> Result<Rc<[u8]>, NativeError> {
    let clean = checked(name)?;
    let missing = |place: &str| NativeError::new(NOT_FOUND, format!("there's no resource '{name}' in {place}"));
    if hidden(&clean) {
        return Err(missing("resources/: hidden files aren't resources"));
    }
    match where_they_are(ctx)? {
        Place::Folder(folder) => {
            let path = folder.join(&clean);
            if !path.is_file() {
                return Err(missing(&folder.display().to_string()));
            }
            std::fs::read(&path).map(Rc::from).map_err(|e| NativeError::new("IOError", format!("can't read the resource '{name}': {e}")))
        }
        Place::Files(files) => files.read(&clean).ok_or_else(|| missing("the bundle")),
    }
}

/// The names of what's directly inside a folder of `resources/`, sorted.
fn list(ctx: &NativeContext, folder: &str) -> Result<Vec<String>, NativeError> {
    let clean = checked(folder)?;
    if hidden(&clean) {
        return Err(NativeError::new(NOT_FOUND, format!("there's no folder '{folder}' in resources/: hidden folders aren't resources")));
    }
    let mut names = match where_they_are(ctx)? {
        Place::Folder(root) => {
            let dir = root.join(&clean);
            // Without resources/, there are none, like in a bundle without them
            if clean.is_empty() && !dir.exists() {
                return Ok(Vec::new());
            }
            let entries = std::fs::read_dir(&dir).map_err(|_| NativeError::new(NOT_FOUND, format!("there's no folder '{folder}' in {}", root.display())))?;
            entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>()
        }
        Place::Files(files) => {
            let prefix = if clean.is_empty() { String::new() } else { format!("{clean}/") };
            let mut names: Vec<String> = files
                .names()
                .iter()
                .filter_map(|k| k.strip_prefix(&prefix))
                .map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
                .collect();
            names.sort();
            names.dedup();
            if names.is_empty() && !clean.is_empty() {
                return Err(NativeError::new(NOT_FOUND, format!("there's no folder '{folder}' in the bundle")));
            }
            names
        }
    };
    names.retain(|n| !n.starts_with('.'));
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::checked;

    #[test]
    fn names() {
        assert_eq!(checked("templates/page.html").unwrap(), "templates/page.html");
        assert_eq!(checked("./a//b").unwrap(), "a/b");
        assert_eq!(checked("").unwrap(), "");
        assert!(super::hidden(".DS_Store") && super::hidden("a/.git/config") && !super::hidden("a/b.txt"));
        for bad in ["../x", "a/../../b", "/etc/passwd", "a\\b", "C:/x"] {
            assert!(checked(bad).is_err(), "{bad}");
        }
    }
}
