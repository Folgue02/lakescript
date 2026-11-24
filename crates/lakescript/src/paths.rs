//! Paths as people and other programs write them.

use std::path::{Path, PathBuf};

/// The real path of a file, or the path itself if it can't be found.
///
/// Like `Path::canonicalize`, but without Windows' verbatim prefix: there it
/// gives `\\?\C:\…` *(and `\\?\UNC\server\share\…`)*, which errors would
/// show and other programs read badly *(i.e. a `file://` URI for an
/// editor)*. A path too long for the plain form keeps its prefix.
pub fn canonical(path: &Path) -> PathBuf {
    let real = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if cfg!(windows)
        && let Some(plain) = real.to_str().and_then(without_verbatim)
    {
        return PathBuf::from(plain);
    }
    real
}

/// `\\?\C:\x` → `C:\x`, `\\?\UNC\server\x` → `\\server\x`. `None` when
/// there's nothing to take off, or when the path would pass Windows' limit of
/// 260 characters without it.
pub fn without_verbatim(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    let plain = if let Some(unc) = rest.strip_prefix(r"UNC\") {
        format!(r"\\{unc}")
    } else {
        let b = rest.as_bytes();
        if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' {
            return None;
        }
        rest.to_string()
    };
    (plain.len() < 260).then_some(plain)
}

#[cfg(test)]
mod tests {
    use super::without_verbatim as plain;

    #[test]
    fn the_verbatim_prefix_is_taken_off() {
        assert_eq!(plain(r"\\?\C:\Users\ana\day1.lks").as_deref(), Some(r"C:\Users\ana\day1.lks"));
        assert_eq!(plain(r"\\?\UNC\server\share\x.lks").as_deref(), Some(r"\\server\share\x.lks"));
        assert_eq!(plain(r"C:\plain"), None);
        assert_eq!(plain("/tmp/x.lks"), None);
        assert_eq!(plain(r"\\?\Volume{abc}\x"), None);
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(plain(&long), None, "too long for the plain form");
    }
}
