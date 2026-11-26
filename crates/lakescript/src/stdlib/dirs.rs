//! `std/dirs/native`, the Rust part of `std/dirs` *(see ideas/stdlib/dirs.md)*:
//! where the folders of a tool and of the user are on each system, from the
//! variables of the script. Only `dirs.lks` uses it.

use super::{Args, Exports};
use crate::exceptions::VALUE_ERROR;
use crate::value::{NativeError, Value};

const IO_ERROR: &str = "IOError";

#[derive(Clone, Copy, PartialEq, Debug)]
enum System {
    Linux,
    Macos,
    Windows,
}

const SYSTEM: System = if cfg!(windows) {
    System::Windows
} else if cfg!(target_os = "macos") {
    System::Macos
} else {
    System::Linux
};

/// What the folders are found from: the system, the variables of the script,
/// and the disk, which tests replace.
struct Place<'a> {
    system: System,
    var: &'a dyn Fn(&str) -> Option<String>,
    /// `true` if the folder exists
    exists: &'a dyn Fn(&str) -> bool,
    /// The text of a file, or `None`
    read: &'a dyn Fn(&str) -> Option<String>,
}

impl Place<'_> {
    /// A path with `/`, and without one at the end.
    fn plain(&self, path: &str) -> String {
        let path = if self.system == System::Windows { path.replace('\\', "/") } else { path.to_string() };
        let trimmed = path.trim_end_matches('/');
        match trimmed {
            "" => "/".to_string(),
            // `C:` is the current folder of the drive, `C:/` its root
            t if t.len() == 2 && t.ends_with(':') => format!("{t}/"),
            t => t.to_string(),
        }
    }

    fn absolute(&self, path: &str) -> bool {
        let b = path.as_bytes();
        let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\');
        path.starts_with('/') || (self.system == System::Windows && (drive || path.starts_with('\\')))
    }

    /// A variable that is set and not empty.
    fn set(&self, name: &str) -> Option<String> {
        (self.var)(name).filter(|v| !v.is_empty())
    }

    fn join(&self, base: &str, rest: &str) -> String {
        let base = self.plain(base);
        if base.ends_with('/') { format!("{base}{rest}") } else { format!("{base}/{rest}") }
    }

    fn home(&self) -> Result<String, NativeError> {
        let home = if self.system == System::Windows {
            self.set("USERPROFILE").or_else(|| self.set("HOME"))
        } else {
            self.set("HOME").or_else(|| self.set("USERPROFILE"))
        };
        let missing = if self.system == System::Windows { "`USERPROFILE`" } else { "`HOME`" };
        home.map(|h| self.plain(&h))
            .ok_or_else(|| NativeError::new(IO_ERROR, format!("can't find the home folder: {missing} isn't set")))
    }

    /// An `XDG_` variable, which only counts if it's an absolute path, or the
    /// folder in the home folder the spec gives when it isn't.
    fn xdg(&self, name: &str, fallback: &str) -> Result<String, NativeError> {
        match self.set(name).filter(|v| self.absolute(v)) {
            Some(path) => Ok(self.plain(&path)),
            None => Ok(self.join(&self.home()?, fallback)),
        }
    }

    /// A variable of Windows, or the folder in the home folder it usually is.
    fn windows(&self, name: &str, fallback: &str) -> Result<String, NativeError> {
        match self.set(name) {
            Some(path) => Ok(self.plain(&path)),
            None => Ok(self.join(&self.home()?, fallback)),
        }
    }

    /// The folder of a kind, for every tool: `config`, `data`, `cache`,
    /// `state`, `temp` or `runtime`.
    fn base(&self, kind: &str) -> Result<Option<String>, NativeError> {
        let windows = self.system == System::Windows;
        let path = match kind {
            "config" if windows => self.windows("APPDATA", "AppData/Roaming")?,
            "data" | "cache" | "state" if windows => self.windows("LOCALAPPDATA", "AppData/Local")?,
            "config" => self.xdg("XDG_CONFIG_HOME", ".config")?,
            "data" => self.xdg("XDG_DATA_HOME", ".local/share")?,
            "cache" => self.xdg("XDG_CACHE_HOME", ".cache")?,
            "state" => self.xdg("XDG_STATE_HOME", ".local/state")?,
            "temp" if windows => match self.set("TEMP").or_else(|| self.set("TMP")) {
                Some(path) => self.plain(&path),
                None => self.plain(&std::env::temp_dir().to_string_lossy()),
            },
            "temp" => self.plain(&self.set("TMPDIR").unwrap_or_else(|| "/tmp".to_string())),
            "runtime" => return Ok(self.set("XDG_RUNTIME_DIR").filter(|v| self.absolute(v)).map(|v| self.plain(&v))),
            _ => return Err(NativeError::new(VALUE_ERROR, format!("'{kind}' isn't a kind of folder"))),
        };
        Ok(Some(path))
    }

    /// The folder of a kind for one tool. On Windows, the configuration is the
    /// tool's folder in `%APPDATA%`, and the rest share its folder in
    /// `%LOCALAPPDATA%`, each in its own.
    fn app(&self, kind: &str, name: &str) -> Result<String, NativeError> {
        if !matches!(kind, "config" | "data" | "cache" | "state") {
            return Err(NativeError::new(VALUE_ERROR, format!("'{kind}' isn't a kind of folder of a tool")));
        }
        let base = self.base(kind)?.unwrap_or_default();
        if self.system == System::Windows && kind != "config" {
            return Ok(self.join(&base, &format!("{name}/{kind}")));
        }
        Ok(self.join(&base, name))
    }

    /// A folder of the user: `desktop`, `documents`, `downloads`, `music`,
    /// `pictures` or `videos`.
    fn user(&self, kind: &str) -> Result<Option<String>, NativeError> {
        let (english, key) = match kind {
            "desktop" => ("Desktop", "DESKTOP"),
            "documents" => ("Documents", "DOCUMENTS"),
            "downloads" => ("Downloads", "DOWNLOAD"),
            "music" => ("Music", "MUSIC"),
            "pictures" => ("Pictures", "PICTURES"),
            "videos" if self.system == System::Macos => ("Movies", "VIDEOS"),
            "videos" => ("Videos", "VIDEOS"),
            _ => return Err(NativeError::new(VALUE_ERROR, format!("'{kind}' isn't a folder of the user"))),
        };
        if self.system == System::Windows {
            return Ok(known_folder(kind).map(|path| self.plain(&path)));
        }
        let home = self.home()?;
        if self.system == System::Linux {
            let file = self.join(&self.base("config")?.unwrap_or_default(), "user-dirs.dirs");
            if let Some(found) = (self.read)(&file).and_then(|text| user_dirs(&text, key, &home)) {
                // One set to the home folder itself is turned off
                return Ok((found != home).then(|| self.plain(&found)));
            }
        }
        let path = self.join(&home, english);
        Ok((self.exists)(&path).then_some(path))
    }
}

/// The folder of `XDG_<key>_DIR` in the text of `user-dirs.dirs`, whose lines
/// are like `XDG_DOWNLOAD_DIR="$HOME/Downloads"`.
fn user_dirs(text: &str, key: &str, home: &str) -> Option<String> {
    let name = format!("XDG_{key}_DIR");
    let value = text.lines().map(str::trim).filter(|l| !l.starts_with('#')).find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == name).then(|| v.trim().trim_matches('"').to_string())
    })?;
    let path = if value == "$HOME" {
        home.to_string()
    } else if let Some(rest) = value.strip_prefix("$HOME/") {
        format!("{home}/{rest}")
    } else if value.starts_with('/') {
        value
    } else {
        return None;
    };
    Some(path.trim_end_matches('/').to_string()).map(|p| if p.is_empty() { "/".to_string() } else { p })
}

/// The Known Folder of Windows for a folder of the user, which may have been
/// moved *(i.e. documents in OneDrive)*.
#[cfg(windows)]
fn known_folder(kind: &str) -> Option<String> {
    let path = match kind {
        "desktop" => dirs::desktop_dir(),
        "documents" => dirs::document_dir(),
        "downloads" => dirs::download_dir(),
        "music" => dirs::audio_dir(),
        "pictures" => dirs::picture_dir(),
        "videos" => dirs::video_dir(),
        _ => None,
    }?;
    Some(crate::paths::without_verbatim(&path.to_string_lossy()).unwrap_or_else(|| path.to_string_lossy().into_owned()))
}

#[cfg(not(windows))]
fn known_folder(_: &str) -> Option<String> {
    None
}

fn found(path: Option<String>) -> Value {
    path.map(|p| Value::string(&p)).unwrap_or(Value::Null)
}

pub fn register(m: &mut Exports) {
    fn place<T>(ctx: &crate::value::NativeContext, f: impl FnOnce(&Place) -> T) -> T {
        let var = |name: &str| ctx.state.env_get(name);
        let exists = |path: &str| std::path::Path::new(path).is_dir();
        let read = |path: &str| std::fs::read_to_string(path).ok();
        f(&Place { system: SYSTEM, var: &var, exists: &exists, read: &read })
    }
    m.function("base", |ctx, values| {
        let a = Args::new("dirs::base", values, 1, 1)?;
        let kind = a.str(0)?;
        Ok(found(place(ctx, |p| p.base(&kind))?))
    });
    m.function("app", |ctx, values| {
        let a = Args::new("dirs::App", values, 2, 2)?;
        let (kind, name) = (a.str(0)?, a.str(1)?);
        Ok(Value::string(&place(ctx, |p| p.app(&kind, &name))?))
    });
    m.function("user", |ctx, values| {
        let a = Args::new("dirs::user", values, 1, 1)?;
        let kind = a.str(0)?;
        Ok(found(place(ctx, |p| p.user(&kind))?))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn with<T>(system: System, vars: &[(&str, &str)], dirs: &[&str], files: &[(&str, &str)], f: impl FnOnce(&Place) -> T) -> T {
        let vars: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let files: HashMap<String, String> = files.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let var = |name: &str| vars.get(name).cloned();
        let exists = |path: &str| dirs.contains(&path);
        let read = |path: &str| files.get(path).cloned();
        f(&Place { system, var: &var, exists: &exists, read: &read })
    }

    #[test]
    fn linux_and_macos_follow_xdg() {
        for system in [System::Linux, System::Macos] {
            with(system, &[("HOME", "/home/ana/")], &[], &[], |p| {
                assert_eq!(p.app("config", "tool").unwrap(), "/home/ana/.config/tool");
                assert_eq!(p.app("data", "tool").unwrap(), "/home/ana/.local/share/tool");
                assert_eq!(p.app("cache", "tool").unwrap(), "/home/ana/.cache/tool");
                assert_eq!(p.app("state", "tool").unwrap(), "/home/ana/.local/state/tool");
                assert_eq!(p.base("temp").unwrap().unwrap(), "/tmp");
                assert_eq!(p.base("runtime").unwrap(), None);
            });
        }
        let vars = [("HOME", "/home/ana"), ("XDG_CONFIG_HOME", "/etc/ana/"), ("XDG_CACHE_HOME", "relative"), ("XDG_RUNTIME_DIR", "/run/user/1000"), ("TMPDIR", "/var/tmp/")];
        with(System::Linux, &vars, &[], &[], |p| {
            assert_eq!(p.base("config").unwrap().unwrap(), "/etc/ana");
            // Not absolute, so ignored
            assert_eq!(p.base("cache").unwrap().unwrap(), "/home/ana/.cache");
            assert_eq!(p.base("runtime").unwrap().unwrap(), "/run/user/1000");
            assert_eq!(p.base("temp").unwrap().unwrap(), "/var/tmp");
        });
    }

    #[test]
    fn windows_roams_only_the_configuration() {
        let vars = [("USERPROFILE", r"C:\Users\ana"), ("APPDATA", r"C:\Users\ana\AppData\Roaming"), ("LOCALAPPDATA", r"C:\Users\ana\AppData\Local"), ("TEMP", r"C:\Temp\")];
        with(System::Windows, &vars, &[], &[], |p| {
            assert_eq!(p.app("config", "tool").unwrap(), "C:/Users/ana/AppData/Roaming/tool");
            assert_eq!(p.app("data", "tool").unwrap(), "C:/Users/ana/AppData/Local/tool/data");
            assert_eq!(p.app("cache", "tool").unwrap(), "C:/Users/ana/AppData/Local/tool/cache");
            assert_eq!(p.app("state", "tool").unwrap(), "C:/Users/ana/AppData/Local/tool/state");
            assert_eq!(p.base("config").unwrap().unwrap(), "C:/Users/ana/AppData/Roaming");
            assert_eq!(p.base("temp").unwrap().unwrap(), "C:/Temp");
            assert_eq!(p.base("runtime").unwrap(), None);
        });
        // Without the variables, the folders they usually are
        with(System::Windows, &[("USERPROFILE", r"C:\Users\ana")], &[], &[], |p| {
            assert_eq!(p.base("config").unwrap().unwrap(), "C:/Users/ana/AppData/Roaming");
            assert_eq!(p.base("cache").unwrap().unwrap(), "C:/Users/ana/AppData/Local");
        });
    }

    #[test]
    fn user_folders() {
        let dirs = "# Written by xdg-user-dirs-update\nXDG_DOWNLOAD_DIR=\"$HOME/Descargas\"\nXDG_MUSIC_DIR=\"$HOME/\"\nXDG_PICTURES_DIR=\"/data/fotos\"\n";
        let files = [("/home/ana/.config/user-dirs.dirs", dirs)];
        let vars = [("HOME", "/home/ana")];
        with(System::Linux, &vars, &["/home/ana/Desktop"], &files, |p| {
            assert_eq!(p.user("downloads").unwrap().as_deref(), Some("/home/ana/Descargas"));
            assert_eq!(p.user("pictures").unwrap().as_deref(), Some("/data/fotos"));
            // Set to the home folder: turned off
            assert_eq!(p.user("music").unwrap(), None);
            // Not in the file: the English name, if it exists
            assert_eq!(p.user("desktop").unwrap().as_deref(), Some("/home/ana/Desktop"));
            assert_eq!(p.user("videos").unwrap(), None);
            assert!(p.user("trash").is_err());
        });
        with(System::Macos, &[("HOME", "/Users/ana")], &["/Users/ana/Movies", "/Users/ana/Downloads"], &[], |p| {
            assert_eq!(p.user("videos").unwrap().as_deref(), Some("/Users/ana/Movies"));
            assert_eq!(p.user("downloads").unwrap().as_deref(), Some("/Users/ana/Downloads"));
            assert_eq!(p.user("music").unwrap(), None);
        });
    }

    #[test]
    fn no_home() {
        with(System::Linux, &[], &[], &[], |p| {
            let e = p.base("config").unwrap_err();
            assert_eq!(e.kind, IO_ERROR);
        });
    }
}
