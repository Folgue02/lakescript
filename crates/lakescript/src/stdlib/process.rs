//! `std/process` *(see `ideas/stdlib/process.md`)*. `chdir` only changes the
//! script's own current folder, never the real process's.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::{Args, Exports, strings};
use crate::command::{CommandKind, CommandValue};
use crate::value::{NativeError, Value};

pub fn register(m: &mut Exports) {
    m.function("args", |ctx, values| {
        Args::new("process::args", values, 0, 0)?;
        Ok(strings(ctx.state.args.clone()))
    });
    m.function("scriptPath", |ctx, values| {
        Args::new("process::scriptPath", values, 0, 0)?;
        Ok(ctx.state.script_path.as_ref().map(|p| Value::string(&p.display().to_string())).unwrap_or(Value::Null))
    });
    m.function("cwd", |ctx, values| {
        Args::new("process::cwd", values, 0, 0)?;
        Ok(Value::string(&ctx.state.cwd.display().to_string()))
    });
    m.function("chdir", |ctx, values| {
        let a = Args::new("process::chdir", values, 1, 1)?;
        let path = a.str(0)?;
        let target = ctx.state.resolve(&path);
        if !target.is_dir() {
            let kind = if target.exists() { "IOError" } else { "FileNotFound" };
            return Err(NativeError::new(kind, format!("can't change to '{path}': it isn't a folder")));
        }
        // Normalized but not resolved, so paths built from the name the script used still match
        ctx.state.cwd = PathBuf::from(super::path::normalize(&target.display().to_string()));
        Ok(Value::Null)
    });
    m.function("pid", |_, values| {
        Args::new("process::pid", values, 0, 0)?;
        Ok(Value::Int(std::process::id() as i64))
    });
    m.function("platform", |_, values| {
        Args::new("process::platform", values, 0, 0)?;
        let name = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(windows) {
            "windows"
        } else {
            "linux"
        };
        Ok(Value::Symbol(Rc::from(name)))
    });
    m.function("sleep", |_, values| {
        let a = Args::new("process::sleep", values, 1, 1)?;
        let seconds = a.num(0)?;
        if seconds.is_finite() && seconds > 0.0 {
            std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
        }
        Ok(Value::Null)
    });
    m.function("which", |ctx, values| {
        let a = Args::new("process::which", values, 1, 1)?;
        Ok(which(&a.str(0)?, ctx.state.env_get("PATH")).map(|p| Value::string(&p.display().to_string())).unwrap_or(Value::Null))
    });
    m.function("shell", |_, values| {
        // The one way to use a shell: `sh -c` on Linux and macOS, `cmd /C` on Windows
        let a = Args::new("process::shell", values, 1, 1)?;
        Ok(Value::Command(Rc::new(RefCell::new(CommandValue::new(CommandKind::Shell(a.str(0)?.to_string()))))))
    });
    m.function("command", |_, values| {
        let a = Args::new("process::command", values, 1, usize::MAX)?;
        let program = a.str(0)?.to_string();
        let args = a.rest_strings(1)?;
        Ok(Value::Command(Rc::new(RefCell::new(CommandValue::new(CommandKind::Direct { program, args })))))
    });
}

/// Looks for a program in `PATH`, like the `which` command.
fn which(name: &str, path: Option<String>) -> Option<PathBuf> {
    if name.contains('/') {
        let p = Path::new(name);
        return is_executable(p).then(|| p.to_path_buf());
    }
    let separator = if cfg!(windows) { ';' } else { ':' };
    for dir in path?.split(separator).filter(|d| !d.is_empty()) {
        let candidate = Path::new(dir).join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        if cfg!(windows) {
            for ext in ["exe", "bat", "cmd", "com"] {
                let with_ext = candidate.with_extension(ext);
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    path.is_file() && crate::patterns::path_test(crate::ast::PathTestOp::Exec, &path.display().to_string())
}
