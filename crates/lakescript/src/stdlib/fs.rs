//! `std/fs` *(see `ideas/stdlib/fs.md`)*. Relative paths start at the
//! script's current folder.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Args, Exports, io_error, strings};
use crate::exceptions::VALUE_ERROR;
use crate::value::{NativeError, Value};

fn read_text(path: &Path, shown: &str) -> Result<String, NativeError> {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| NativeError::new(VALUE_ERROR, format!("'{shown}' isn't valid UTF-8 text"))),
        Err(e) => Err(io_error("read", shown, e)),
    }
}

fn sorted_names(dir: &Path, shown: &str) -> Result<Vec<String>, NativeError> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| io_error("list", shown, e))?
        .filter_map(|entry| entry.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    Ok(names)
}

fn join_shown(base: &str, name: &str) -> String {
    if base.is_empty() || base == "." {
        name.to_string()
    } else if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

fn walk(dir: &Path, shown: &str, out: &mut Vec<String>) -> Result<(), NativeError> {
    for name in sorted_names(dir, shown)? {
        let path = dir.join(&name);
        let child = join_shown(shown, &name);
        if path.is_dir() && !path.is_symlink() {
            walk(&path, &child, out)?;
        } else {
            out.push(child);
        }
    }
    Ok(())
}

fn has_wildcards(part: &str) -> bool {
    part.contains(['*', '?', '['])
}

fn glob(base: &Path, shown: &str, parts: &[&str], out: &mut Vec<String>) -> Result<(), NativeError> {
    let Some((&first, rest)) = parts.split_first() else {
        if base.exists() {
            out.push(shown.to_string());
        }
        return Ok(());
    };
    if first == "**" {
        // Zero folders, or one more and still `**`
        glob(base, shown, rest, out)?;
        if base.is_dir() {
            for name in sorted_names(base, shown).unwrap_or_default() {
                let path = base.join(&name);
                if path.is_dir() && !name.starts_with('.') {
                    glob(&path, &join_shown(shown, &name), parts, out)?;
                }
            }
        }
        return Ok(());
    }
    if !has_wildcards(first) {
        let path = base.join(first);
        if path.exists() || path.is_symlink() {
            glob(&path, &join_shown(shown, first), rest, out)?;
        }
        return Ok(());
    }
    if !base.is_dir() {
        return Ok(());
    }
    for name in sorted_names(base, shown).unwrap_or_default() {
        // Like the shell, `*` doesn't match hidden files unless the pattern starts with `.`
        if name.starts_with('.') && !first.starts_with('.') {
            continue;
        }
        if crate::patterns::like(&name, first).map_err(|m| NativeError::new(VALUE_ERROR, m))? {
            glob(&base.join(&name), &join_shown(shown, &name), rest, out)?;
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path, shown: &str) -> Result<(), NativeError> {
    fs::create_dir_all(to).map_err(|e| io_error("copy to", shown, e))?;
    for entry in fs::read_dir(from).map_err(|e| io_error("copy", shown, e))? {
        let entry = entry.map_err(|e| io_error("copy", shown, e))?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst, shown)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| io_error("copy", &src.display().to_string(), e))?;
        }
    }
    Ok(())
}

pub(super) fn temp_name(extension: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ext = if extension.is_empty() || extension.starts_with('.') { extension.to_string() } else { format!(".{extension}") };
    format!("lks-{}-{nanos}-{n}{ext}", std::process::id())
}

pub fn register(m: &mut Exports) {
    // Reading
    m.function("read", |ctx, values| {
        let a = Args::new("fs::read", values, 1, 1)?;
        let p = a.str(0)?;
        Ok(Value::string(&read_text(&ctx.state.resolve(&p), &p)?))
    });
    m.function("readLines", |ctx, values| {
        let a = Args::new("fs::readLines", values, 1, 1)?;
        let p = a.str(0)?;
        Ok(strings(read_text(&ctx.state.resolve(&p), &p)?.lines().map(String::from)))
    });

    m.function("readBytes", |ctx, values| {
        let a = Args::new("fs::readBytes", values, 1, 1)?;
        let p = a.str(0)?;
        let bytes = fs::read(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        Ok(Value::Bytes(std::rc::Rc::from(bytes)))
    });

    // Writing
    m.function("writeBytes", |ctx, values| {
        let a = Args::new("fs::writeBytes", values, 2, 2)?;
        let p = a.str(0)?;
        let Value::Bytes(bytes) = a.value(1) else {
            return Err(crate::value::NativeError::new(crate::exceptions::TYPE_ERROR, format!("argument 2 of `fs::writeBytes` must be bytes, not a {}", a.value(1).type_name())));
        };
        fs::write(ctx.state.resolve(&p), &bytes[..]).map_err(|e| io_error("write", &p, e))?;
        Ok(Value::Null)
    });
    m.function("write", |ctx, values| {
        let a = Args::new("fs::write", values, 2, 2)?;
        let p = a.str(0)?;
        fs::write(ctx.state.resolve(&p), a.str(1)?.as_bytes()).map_err(|e| io_error("write", &p, e))?;
        Ok(Value::Null)
    });
    m.function("append", |ctx, values| {
        let a = Args::new("fs::append", values, 2, 2)?;
        let p = a.str(0)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ctx.state.resolve(&p))
            .map_err(|e| io_error("write", &p, e))?;
        file.write_all(a.str(1)?.as_bytes()).map_err(|e| io_error("write", &p, e))?;
        Ok(Value::Null)
    });
    m.function("writeLines", |ctx, values| {
        let a = Args::new("fs::writeLines", values, 2, 2)?;
        let p = a.str(0)?;
        let text: String = a.strings(1)?.iter().map(|l| format!("{l}\n")).collect();
        fs::write(ctx.state.resolve(&p), text).map_err(|e| io_error("write", &p, e))?;
        Ok(Value::Null)
    });

    // Information
    m.function("exists?", |ctx, values| {
        let a = Args::new("fs::exists?", values, 1, 1)?;
        let path = ctx.state.resolve(&a.str(0)?);
        Ok(Value::Bool(path.exists() || path.is_symlink()))
    });
    m.function("file?", |ctx, values| {
        let a = Args::new("fs::file?", values, 1, 1)?;
        Ok(Value::Bool(ctx.state.resolve(&a.str(0)?).is_file()))
    });
    m.function("dir?", |ctx, values| {
        let a = Args::new("fs::dir?", values, 1, 1)?;
        Ok(Value::Bool(ctx.state.resolve(&a.str(0)?).is_dir()))
    });
    m.function("link?", |ctx, values| {
        let a = Args::new("fs::link?", values, 1, 1)?;
        Ok(Value::Bool(ctx.state.resolve(&a.str(0)?).is_symlink()))
    });
    m.function("size", |ctx, values| {
        let a = Args::new("fs::size", values, 1, 1)?;
        let p = a.str(0)?;
        let meta = fs::metadata(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        Ok(Value::Int(meta.len() as i64))
    });
    m.function("modified", |ctx, values| {
        let a = Args::new("fs::modified", values, 1, 1)?;
        let p = a.str(0)?;
        let meta = fs::metadata(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        let secs = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Ok(Value::Int(secs))
    });
    // For `fs::modified()` of Lakescript, which makes a date of it
    m.function("modifiedMillis", |ctx, values| {
        let a = Args::new("fs::modified", values, 1, 1)?;
        let p = a.str(0)?;
        let meta = fs::metadata(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        let millis = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
        Ok(Value::Int(millis))
    });
    m.function("mode", |ctx, values| {
        let a = Args::new("fs::mode", values, 1, 1)?;
        let p = a.str(0)?;
        let meta = fs::metadata(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            (meta.permissions().mode() & 0o7777) as i64
        };
        #[cfg(not(unix))]
        let mode = if meta.permissions().readonly() { 0o444 } else { 0o666 };
        Ok(Value::Int(mode))
    });

    // Folders
    m.function("list", |ctx, values| {
        let a = Args::new("fs::list", values, 0, 1)?;
        let p = a.str_or(0, ".")?;
        Ok(strings(sorted_names(&ctx.state.resolve(&p), &p)?))
    });
    m.function("walk", |ctx, values| {
        let a = Args::new("fs::walk", values, 0, 1)?;
        let p = a.str_or(0, ".")?;
        let mut out = Vec::new();
        walk(&ctx.state.resolve(&p), if &*p == "." { "" } else { &p }, &mut out)?;
        Ok(strings(out))
    });
    m.function("glob", |ctx, values| {
        let a = Args::new("fs::glob", values, 1, 1)?;
        let pattern = a.str(0)?;
        let (base, shown, rest) = match pattern.strip_prefix('/') {
            Some(rest) => (std::path::PathBuf::from("/"), "/".to_string(), rest.to_string()),
            None => (ctx.state.cwd.clone(), String::new(), pattern.to_string()),
        };
        let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
        let mut out = Vec::new();
        glob(&base, &shown, &parts, &mut out)?;
        out.sort();
        out.dedup();
        Ok(strings(out))
    });
    m.function("makeDir", |ctx, values| {
        let a = Args::new("fs::makeDir", values, 1, 1)?;
        let p = a.str(0)?;
        fs::create_dir_all(ctx.state.resolve(&p)).map_err(|e| io_error("create", &p, e))?;
        Ok(Value::Null)
    });
    m.function("tempDir", |_, values| {
        Args::new("fs::tempDir", values, 0, 0)?;
        let path = std::env::temp_dir().join(temp_name(""));
        fs::create_dir(&path).map_err(|e| io_error("create", &path.display().to_string(), e))?;
        Ok(Value::string(&path.display().to_string()))
    });
    m.function("tempFile", |_, values| {
        let a = Args::new("fs::tempFile", values, 0, 1)?;
        let path = std::env::temp_dir().join(temp_name(&a.str_or(0, "")?));
        fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| io_error("create", &path.display().to_string(), e))?;
        Ok(Value::string(&path.display().to_string()))
    });

    // Copying, moving and removing
    m.function("copy", |ctx, values| {
        let a = Args::new("fs::copy", values, 2, 2)?;
        let (from, to) = (a.str(0)?, a.str(1)?);
        let (src, dst) = (ctx.state.resolve(&from), ctx.state.resolve(&to));
        if src.is_dir() {
            copy_dir(&src, &dst, &to)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| io_error("copy", &from, e))?;
        }
        Ok(Value::Null)
    });
    m.function("move", |ctx, values| {
        let a = Args::new("fs::move", values, 2, 2)?;
        let (from, to) = (a.str(0)?, a.str(1)?);
        let (src, dst) = (ctx.state.resolve(&from), ctx.state.resolve(&to));
        if fs::rename(&src, &dst).is_err() {
            // Another disk: copy, then remove
            if src.is_dir() {
                copy_dir(&src, &dst, &to)?;
                fs::remove_dir_all(&src).map_err(|e| io_error("move", &from, e))?;
            } else {
                fs::copy(&src, &dst).map_err(|e| io_error("move", &from, e))?;
                fs::remove_file(&src).map_err(|e| io_error("move", &from, e))?;
            }
        }
        Ok(Value::Null)
    });
    m.function("remove", |ctx, values| {
        let a = Args::new("fs::remove", values, 1, 1)?;
        let p = a.str(0)?;
        let path = ctx.state.resolve(&p);
        let Ok(meta) = fs::symlink_metadata(&path) else { return Ok(Value::Null) };
        let result = if meta.is_dir() { fs::remove_dir(&path) } else { fs::remove_file(&path) };
        result.map_err(|e| {
            if meta.is_dir() && fs::read_dir(&path).map(|mut d| d.next().is_some()).unwrap_or(false) {
                NativeError::new("IOError", format!("can't remove '{p}': the folder isn't empty. Use fs::removeAll() to remove it with everything inside"))
            } else {
                io_error("remove", &p, e)
            }
        })?;
        Ok(Value::Null)
    });
    m.function("removeAll", |ctx, values| {
        let a = Args::new("fs::removeAll", values, 1, 1)?;
        let p = a.str(0)?;
        let path = ctx.state.resolve(&p);
        let Ok(meta) = fs::symlink_metadata(&path) else { return Ok(Value::Null) };
        let result = if meta.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
        result.map_err(|e| io_error("remove", &p, e))?;
        Ok(Value::Null)
    });
    m.function("setMode", |ctx, values| {
        let a = Args::new("fs::setMode", values, 2, 2)?;
        let p = a.str(0)?;
        let mode = a.whole(1)?;
        let path = ctx.state.resolve(&p);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(mode as u32)).map_err(|e| io_error("change", &p, e))?;
        }
        #[cfg(not(unix))]
        {
            let mut perms = fs::metadata(&path).map_err(|e| io_error("change", &p, e))?.permissions();
            perms.set_readonly(mode & 0o222 == 0);
            fs::set_permissions(&path, perms).map_err(|e| io_error("change", &p, e))?;
        }
        Ok(Value::Null)
    });
    m.function("link", |ctx, values| {
        let a = Args::new("fs::link", values, 2, 2)?;
        let (target, p) = (a.str(0)?, a.str(1)?);
        let path = ctx.state.resolve(&p);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&*target, &path).map_err(|e| io_error("create the link", &p, e))?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&*target, &path).map_err(|e| io_error("create the link", &p, e))?;
        Ok(Value::Null)
    });
}
