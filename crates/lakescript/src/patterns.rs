//! Wildcards for `-like`, and the path tests `-f`, `-d`, `-x`, `-r` and `-w`.

use std::path::Path;

use crate::ast::PathTestOp;

/// `-like`: `*` matches any number of characters, `?` one character, and
/// `[abc]` one of the characters inside. `[a-z]` is a range, and `[!abc]`
/// matches any character that isn't inside.
pub fn like(text: &str, pattern: &str) -> Result<bool, String> {
    let text: Vec<char> = text.chars().collect();
    let pattern: Vec<char> = pattern.chars().collect();
    // Iterative matching with backtracking to the last `*`, so it never
    // takes more than text × pattern steps.
    let (mut t, mut p) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() {
            match pattern[p] {
                '*' => {
                    star = Some((p, t));
                    p += 1;
                    continue;
                }
                '?' => {
                    t += 1;
                    p += 1;
                    continue;
                }
                '[' => {
                    let (matched, next) = class(&pattern, p, text[t])?;
                    if matched {
                        t += 1;
                        p = next;
                        continue;
                    }
                }
                c if c == text[t] => {
                    t += 1;
                    p += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some((sp, st)) => {
                p = sp + 1;
                t = st + 1;
                star = Some((sp, st + 1));
            }
            None => return Ok(false),
        }
    }
    Ok(pattern[p..].iter().all(|&c| c == '*'))
}

/// Matches `c` against the `[...]` that starts at `start`. Returns whether it
/// matched and where the pattern continues.
fn class(pattern: &[char], start: usize, c: char) -> Result<(bool, usize), String> {
    let mut i = start + 1;
    let negated = pattern.get(i) == Some(&'!');
    if negated {
        i += 1;
    }
    let mut matched = false;
    let mut first = true;
    while i < pattern.len() && (pattern[i] != ']' || first) {
        first = false;
        let low = pattern[i];
        if pattern.get(i + 1) == Some(&'-') && pattern.get(i + 2).is_some_and(|&h| h != ']') {
            let high = pattern[i + 2];
            matched |= low <= c && c <= high;
            i += 3;
        } else {
            matched |= low == c;
            i += 1;
        }
    }
    if i >= pattern.len() {
        return Err("a `[` in the pattern of `-like` is never closed with `]`".into());
    }
    Ok((matched != negated, i + 1))
}

pub fn path_test(op: PathTestOp, path: &str) -> bool {
    let path = Path::new(path);
    match op {
        PathTestOp::File => path.is_file(),
        PathTestOp::Dir => path.is_dir(),
        PathTestOp::Exec => access(path, Access::Execute),
        PathTestOp::Read => access(path, Access::Read),
        PathTestOp::Write => access(path, Access::Write),
    }
}

enum Access {
    Read,
    Write,
    Execute,
}

#[cfg(unix)]
fn access(path: &Path, mode: Access) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else { return false };
    let flag = match mode {
        Access::Read => libc::R_OK,
        Access::Write => libc::W_OK,
        Access::Execute => libc::X_OK,
    };
    // SAFETY: `c_path` is a valid C string that lives until the call returns.
    let ok = unsafe { libc::access(c_path.as_ptr(), flag) } == 0;
    // `access` says directories are "executable" when they can be entered
    ok && !(matches!(mode, Access::Execute) && path.is_dir())
}

#[cfg(not(unix))]
fn access(path: &Path, mode: Access) -> bool {
    match std::fs::metadata(path) {
        Err(_) => false,
        Ok(meta) => match mode {
            Access::Read => true,
            Access::Write => !meta.permissions().readonly(),
            Access::Execute => path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["exe", "bat", "cmd", "com"].contains(&e.to_ascii_lowercase().as_str())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::like;

    #[test]
    fn wildcards() {
        assert!(like("notes.txt", "*.txt").unwrap());
        assert!(!like("notes.txt.bak", "*.txt").unwrap());
        assert!(like("a1", "a?").unwrap());
        assert!(!like("a", "a?").unwrap());
        assert!(like("cat", "[bc]at").unwrap());
        assert!(!like("rat", "[bc]at").unwrap());
        assert!(like("x7", "x[0-9]").unwrap());
        assert!(like("xa", "x[!0-9]").unwrap());
        assert!(like("", "*").unwrap());
        assert!(like("abcabc", "*b*c").unwrap());
        assert!(like("ñandú", "ñ*ú").unwrap());
        assert!(like("a]", "a[]]").unwrap());
        assert!(like("aXbYc", "a*b*c").unwrap());
        assert!(!like("aXbY", "a*b*c").unwrap());
        assert!(like("x", "[x").is_err());
    }
}
