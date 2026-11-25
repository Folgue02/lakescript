//! Bundles: a program in one file, `todo.lkb` *(see ideas/tooling/bundles.md)*.
//! A bundle is a zip after a `#!/usr/bin/env lks` line:
//!
//! - `bundle.toml`: the command, its project and version, and the oldest
//!   `lks` that can run it.
//! - `bin/`: the command's script, and the other scripts of `bin/`, which it
//!   can `use` like any script uses the files next to it.
//! - `src/`: every module of the project and of its dependencies. Each
//!   package's modules are under its own `author/name/`, so they never clash.
//! - `resources/`: the project's resources, for `std/resources`.
//!
//! `ferry bundle` writes it, and `lks todo.lkb` runs it, reading the modules
//! from the zip instead of from folders.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::rc::Rc;

/// What a bundle says about itself, in its `bundle.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleInfo {
    /// The command it runs, its script in `bin/` *(i.e. `todo`)*.
    pub command: String,
    /// The ID of its project *(i.e. `folgue02/todo`)*.
    pub project: String,
    /// The version of its project.
    pub version: String,
    /// The oldest `lks` that can run it: the one of the `ferry` that made it.
    pub lks: String,
}

/// The first line of every bundle, so with `chmod +x` it runs as `./todo.lkb`.
/// A zip is read from its end, so it can have it.
const SHEBANG: &str = "#!/usr/bin/env lks\n";

const MANIFEST: &str = "bundle.toml";

/// The version of `lks` of this library: the oldest that can run what it makes.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Writes a bundle at `out`, with `files` by their path inside it *(i.e.
/// `src/folgue02/todo/list.lks`, `resources/help.txt`)*. The manifest is
/// written from `info`, with this library's version as the oldest `lks`.
pub fn write_bundle(out: &Path, command: &str, project: &str, version: &str, files: &[(String, Vec<u8>)]) -> std::io::Result<()> {
    let mut buffer = Cursor::new(Vec::new());
    buffer.write_all(SHEBANG.as_bytes())?;
    {
        // The offsets in the zip count from the start of the file, after the first line
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let manifest = format!(
            "# Made by ferry bundle. Run it with `lks {command}.lkb`, or as `./{command}.lkb`.\ncommand = {}\nproject = {}\nversion = {}\nlks = {}\n",
            quoted(command),
            quoted(project),
            quoted(version),
            quoted(VERSION)
        );
        zip.start_file(MANIFEST, options).map_err(std::io::Error::other)?;
        zip.write_all(manifest.as_bytes())?;
        for (name, bytes) in files {
            zip.start_file(name.as_str(), options).map_err(std::io::Error::other)?;
            zip.write_all(bytes)?;
        }
        zip.finish().map_err(std::io::Error::other)?;
    }
    std::fs::write(out, buffer.into_inner())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Whether the file is a bundle: its name ends in `.lkb`, or it's a zip,
/// maybe after a first line `#!...`. By what's inside too, so a bundle
/// reached through a link or copied under another name *(i.e.
/// `/usr/local/bin/todo`)* is still one.
pub fn is_bundle(path: &Path) -> bool {
    if path.extension().is_some_and(|e| e == "lkb") {
        return true;
    }
    let mut start = [0u8; 256];
    let read = std::fs::File::open(path).and_then(|mut f| f.read(&mut start)).unwrap_or(0);
    let start = &start[..read];
    let zip = |bytes: &[u8]| bytes.starts_with(b"PK\x03\x04");
    if zip(start) {
        return true;
    }
    // After the first line, when it's `#!...`
    start.starts_with(b"#!") && start.iter().position(|b| *b == b'\n').is_some_and(|n| zip(&start[n + 1..]))
}

/// A bundle read into memory.
#[derive(Debug)]
pub(crate) struct Opened {
    pub info: BundleInfo,
    /// Its code, by its path inside it *(i.e. `src/folgue02/todo/list.lks`)*.
    pub files: HashMap<String, Rc<[u8]>>,
    pub resources: BundleResources,
}

/// The resources of a bundle, decompressed only when they're read, so a big
/// one costs nothing to a run that doesn't use it.
pub(crate) struct BundleResources {
    zip: RefCell<zip::ZipArchive<Cursor<Vec<u8>>>>,
    names: Vec<String>,
}

impl std::fmt::Debug for BundleResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BundleResources({:?})", self.names)
    }
}

impl crate::host::ResourceFiles for BundleResources {
    fn names(&self) -> &[String] {
        &self.names
    }

    fn read(&self, name: &str) -> Option<Rc<[u8]>> {
        let mut zip = self.zip.borrow_mut();
        let mut entry = zip.by_name(&format!("resources/{name}")).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        Some(Rc::from(bytes))
    }
}

/// Reads a bundle: its manifest and its code, and the names of its
/// resources. The error says what's wrong with it, for the person who runs it.
pub(crate) fn open(path: &Path) -> Result<Opened, String> {
    let shown = path.display();
    let bytes = std::fs::read(path).map_err(|e| format!("can't read {shown}: {e}"))?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("{shown} isn't a bundle made by `ferry bundle`: {e}"))?;
    let mut files = HashMap::new();
    let mut names = Vec::new();
    let mut manifest = None;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("{shown} is damaged: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().map_err(|e| format!("{shown} is damaged: {e}"))?.to_string();
        if let Some(resource) = name.strip_prefix("resources/") {
            names.push(resource.to_string());
            continue;
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content).map_err(|e| format!("{shown} is damaged: {e}"))?;
        if name == MANIFEST {
            manifest = Some(String::from_utf8_lossy(&content).into_owned());
        } else {
            files.insert(name, Rc::from(content));
        }
    }
    let manifest = manifest.ok_or_else(|| format!("{shown} isn't a bundle made by `ferry bundle`: it has no {MANIFEST}"))?;
    let info = parse_manifest(&manifest).map_err(|e| format!("the {MANIFEST} of {shown} is wrong: {e}"))?;
    if newer(&info.lks, VERSION) {
        return Err(format!("{shown} needs lks {} or newer, and this is lks {VERSION}", info.lks));
    }
    if !files.contains_key(&format!("bin/{}.lks", info.command)) {
        return Err(format!("{shown} is damaged: it has no bin/{}.lks for its command", info.command));
    }
    names.sort();
    Ok(Opened { info, files, resources: BundleResources { zip: RefCell::new(zip), names } })
}

/// The lines `key = "value"` of a `bundle.toml`.
fn parse_manifest(text: &str) -> Result<BundleInfo, String> {
    let mut values = HashMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (key, value) = line.split_once('=').ok_or_else(|| format!("`{line}` isn't `key = \"value\"`"))?;
        let value = value.trim();
        let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).ok_or_else(|| format!("the value of `{}` isn't a string", key.trim()))?;
        values.insert(key.trim().to_string(), value.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    let mut take = |key: &str| values.remove(key).ok_or_else(|| format!("it has no `{key}`"));
    Ok(BundleInfo { command: take("command")?, project: take("project")?, version: take("version")?, lks: take("lks")? })
}

/// Whether the version `a` is newer than `b` *(i.e. `0.12.1` than `0.12.0`)*.
fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) > parts(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lks-bundle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn written_and_opened_with_its_first_line() {
        let dir = temp("roundtrip");
        let out = dir.join("todo.lkb");
        let files = vec![("bin/todo.lks".to_string(), b"outln('hi');".to_vec()), ("resources/a/b.txt".to_string(), b"text".to_vec())];
        write_bundle(&out, "todo", "folgue02/todo", "1.2.0", &files).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(SHEBANG.as_bytes()));
        let opened = open(&out).unwrap();
        assert_eq!(opened.info, BundleInfo { command: "todo".into(), project: "folgue02/todo".into(), version: "1.2.0".into(), lks: VERSION.into() });
        assert_eq!(&*opened.files["bin/todo.lks"], b"outln('hi');");
        use crate::host::ResourceFiles;
        assert_eq!(opened.resources.names(), ["a/b.txt"]);
        assert_eq!(&*opened.resources.read("a/b.txt").unwrap(), b"text");
        assert!(opened.resources.read("nope").is_none() && !opened.files.contains_key("resources/a/b.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn found_by_what_is_inside() {
        let dir = temp("found");
        let out = dir.join("todo.lkb");
        write_bundle(&out, "todo", "a/b", "1.0.0", &[("bin/todo.lks".to_string(), b"outln(1);".to_vec())]).unwrap();
        let renamed = dir.join("todo");
        std::fs::copy(&out, &renamed).unwrap();
        assert!(is_bundle(&out) && is_bundle(&renamed));
        // A plain zip too, and not a script or a file that isn't there
        let script = dir.join("x.lks");
        std::fs::write(&script, "#!/usr/bin/env lks\noutln(1);").unwrap();
        std::fs::write(dir.join("plain"), b"PK\x03\x04rest").unwrap();
        assert!(is_bundle(&dir.join("plain")));
        assert!(!is_bundle(&script) && !is_bundle(&dir.join("missing")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_isnt_a_bundle() {
        let dir = temp("broken");
        let out = dir.join("x.lkb");
        std::fs::write(&out, "outln(1);").unwrap();
        assert!(open(&out).unwrap_err().contains("isn't a bundle made by `ferry bundle`"));
        assert!(open(&dir.join("missing.lkb")).unwrap_err().contains("can't read"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifests_and_versions() {
        let info = parse_manifest("# a comment\ncommand = \"todo\"\nproject = \"a/b\"\nversion = \"1.0.0\"\nlks = \"0.12.0\"\n").unwrap();
        assert_eq!(info.command, "todo");
        assert!(parse_manifest("command = todo").unwrap_err().contains("isn't a string"));
        assert!(parse_manifest("command = \"x\"").unwrap_err().contains("no `project`"));
        assert!(newer("0.12.1", "0.12.0") && newer("1.0.0", "0.99.9") && !newer("0.11.2", "0.12.0") && !newer("0.12.0", "0.12.0"));
    }
}
