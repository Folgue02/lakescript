//! `std/archive/native`: the Rust part of `std/archive` *(see `archive.lks`
//! and `ideas/stdlib/archive.md`)*, on the crates `zip`, `tar` and `flate2`.
//! Every function opens the archive again: an `Archive` of Lakescript only
//! keeps its path, and what's added to a new one until it's closed.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

use super::{Args, Exports, io_error};
use crate::exceptions::VALUE_ERROR;
use crate::value::{NativeError, Value};

pub fn register(m: &mut Exports) {
    m.function("entries", |ctx, values| {
        let a = Args::new("archive::entries", values, 1, 1)?;
        let (shown, file) = (a.str(0)?, ctx.state.resolve(&a.str(0)?));
        let entries = entries(&shown, &file)?;
        Ok(Value::list(
            entries
                .into_iter()
                .map(|e| Value::list(vec![Value::string(&e.name), Value::Int(e.size as i64), Value::Bool(e.dir), e.modified.map(Value::Int).unwrap_or(Value::Null)]))
                .collect(),
        ))
    });
    m.function("read", |ctx, values| {
        let a = Args::new("archive::read", values, 2, 2)?;
        let (shown, name) = (a.str(0)?, a.str(1)?);
        let bytes = read(&shown, &ctx.state.resolve(&shown), &name)?;
        String::from_utf8(bytes)
            .map(|text| Value::string(&text))
            .map_err(|_| NativeError::new(VALUE_ERROR, format!("'{name}' in '{shown}' isn't text: `extract()` writes it as a file")))
    });
    m.function("readBytes", |ctx, values| {
        let a = Args::new("archive::readBytes", values, 2, 2)?;
        let (shown, name) = (a.str(0)?, a.str(1)?);
        Ok(Value::Bytes(std::rc::Rc::from(read(&shown, &ctx.state.resolve(&shown), &name)?)))
    });
    m.function("extract", |ctx, values| {
        let a = Args::new("archive::extract", values, 2, 2)?;
        let (shown, dir) = (a.str(0)?, a.str(1)?);
        extract(&shown, &ctx.state.resolve(&shown), &dir, &ctx.state.resolve(&dir))?;
        Ok(Value::Null)
    });
    m.function("format", |_, values| {
        let a = Args::new("archive::format", values, 1, 1)?;
        format(&a.str(0)?)?;
        Ok(Value::Null)
    });
    m.function("check", |ctx, values| {
        let a = Args::new("archive::check", values, 1, 1)?;
        let path = a.str(0)?;
        std::fs::metadata(ctx.state.resolve(&path)).map_err(|e| io_error("add", &path, e))?;
        Ok(Value::Null)
    });
    m.function("write", |ctx, values| {
        let a = Args::new("archive::write", values, 2, 2)?;
        let shown = a.str(0)?;
        let Value::List(items) = a.value(1) else { return Err(NativeError::new(crate::exceptions::TYPE_ERROR, "`archive::write` needs a list")) };
        let mut adds = Vec::new();
        for item in items.borrow().iter() {
            let Value::List(parts) = item else { continue };
            let parts = parts.borrow();
            let [kind, source, name] = parts.as_slice() else { continue };
            let name = name.to_string();
            adds.push(match (kind.to_string().as_str(), source) {
                ("text", text) => Add::Data { name, data: text.to_string().into_bytes() },
                ("bytes", Value::Bytes(bytes)) => Add::Data { name, data: bytes.to_vec() },
                (_, source) => Add::Path { shown: source.to_string(), path: ctx.state.resolve(&source.to_string()), name },
            });
        }
        write(&shown, &ctx.state.resolve(&shown), adds)?;
        Ok(Value::Null)
    });
}

enum Format {
    Zip,
    Tar,
    TarGz,
}

/// The format of an archive, from its name.
fn format(shown: &str) -> Result<Format, NativeError> {
    let lower = shown.to_lowercase();
    if lower.ends_with(".zip") {
        Ok(Format::Zip)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Ok(Format::TarGz)
    } else if lower.ends_with(".tar") {
        Ok(Format::Tar)
    } else {
        Err(NativeError::new(VALUE_ERROR, format!("can't tell the format of '{shown}': an archive's name ends in .zip, .tar, .tar.gz or .tgz")))
    }
}

struct Entry {
    name: String,
    size: u64,
    dir: bool,
    /// When it was last changed, in milliseconds since 1970.
    modified: Option<i64>,
}

fn open(shown: &str, file: &Path) -> Result<File, NativeError> {
    File::open(file).map_err(|e| io_error("open", shown, e))
}

fn broken(shown: &str, e: impl std::fmt::Display) -> NativeError {
    NativeError::new("IOError", format!("can't read '{shown}': {e}"))
}

fn zip_archive(shown: &str, file: &Path) -> Result<zip::ZipArchive<BufReader<File>>, NativeError> {
    zip::ZipArchive::new(BufReader::new(open(shown, file)?)).map_err(|e| broken(shown, e))
}

fn tar_archive(shown: &str, file: &Path, gz: bool) -> Result<tar::Archive<Box<dyn Read>>, NativeError> {
    let reader = BufReader::new(open(shown, file)?);
    let reader: Box<dyn Read> = if gz { Box::new(flate2::read::GzDecoder::new(reader)) } else { Box::new(reader) };
    Ok(tar::Archive::new(reader))
}

/// The name of an entry as scripts see it: with `/`, and without the one at the end of a folder.
fn clean(name: &str) -> String {
    name.replace('\\', "/").trim_end_matches('/').to_string()
}

fn entries(shown: &str, file: &Path) -> Result<Vec<Entry>, NativeError> {
    let mut found = Vec::new();
    match format(shown)? {
        Format::Zip => {
            let mut zip = zip_archive(shown, file)?;
            for i in 0..zip.len() {
                let entry = zip.by_index(i).map_err(|e| broken(shown, e))?;
                let name = entry.name().map_err(|e| broken(shown, e))?.to_string();
                let modified = entry.last_modified().and_then(local_millis);
                found.push(Entry { name: clean(&name), size: entry.size(), dir: entry.is_dir(), modified });
            }
        }
        format => {
            let mut tar = tar_archive(shown, file, matches!(format, Format::TarGz))?;
            for entry in tar.entries().map_err(|e| broken(shown, e))? {
                let entry = entry.map_err(|e| broken(shown, e))?;
                let kind = entry.header().entry_type();
                if !kind.is_file() && !kind.is_dir() {
                    continue;
                }
                let name = entry.path().map_err(|e| broken(shown, e))?.to_string_lossy().into_owned();
                let modified = entry.header().mtime().ok().map(|s| s as i64 * 1000);
                found.push(Entry { name: clean(&name), size: if kind.is_dir() { 0 } else { entry.size() }, dir: kind.is_dir(), modified });
            }
        }
    }
    Ok(found)
}

fn read(shown: &str, file: &Path, name: &str) -> Result<Vec<u8>, NativeError> {
    let missing = || NativeError::new("FileNotFound", format!("there's no '{name}' in '{shown}'"));
    let mut bytes = Vec::new();
    match format(shown)? {
        Format::Zip => {
            let mut zip = zip_archive(shown, file)?;
            let mut entry = zip.by_name(name).map_err(|_| missing())?;
            entry.read_to_end(&mut bytes).map_err(|e| broken(shown, e))?;
        }
        format => {
            let mut tar = tar_archive(shown, file, matches!(format, Format::TarGz))?;
            for entry in tar.entries().map_err(|e| broken(shown, e))? {
                let mut entry = entry.map_err(|e| broken(shown, e))?;
                let path = entry.path().map_err(|e| broken(shown, e))?.to_string_lossy().into_owned();
                if clean(&path) == name && entry.header().entry_type().is_file() {
                    entry.read_to_end(&mut bytes).map_err(|e| broken(shown, e))?;
                    return Ok(bytes);
                }
            }
            return Err(missing());
        }
    }
    Ok(bytes)
}

/// A name of an entry as a path inside `dir`, or `None` when it would be
/// outside of it *(i.e. `../x` or `/etc/x`)*, so an archive can't write
/// anywhere else.
fn inside(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut path = dir.to_path_buf();
    for part in Path::new(&name.replace('\\', "/")).components() {
        match part {
            Component::Normal(p) => path.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(path)
}

fn extract(shown: &str, file: &Path, dir_shown: &str, dir: &Path) -> Result<(), NativeError> {
    std::fs::create_dir_all(dir).map_err(|e| io_error("create", dir_shown, e))?;
    let outside = |name: &str| NativeError::new(VALUE_ERROR, format!("'{name}' in '{shown}' would be written outside '{dir_shown}', so nothing after it was extracted"));
    let write_error = |name: &str, e: std::io::Error| io_error("write", &format!("{dir_shown}/{name}"), e);
    match format(shown)? {
        Format::Zip => {
            let mut zip = zip_archive(shown, file)?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| broken(shown, e))?;
                let name = entry.name().map_err(|e| broken(shown, e))?.to_string();
                let target = inside(dir, &name).ok_or_else(|| outside(&name))?;
                if entry.is_dir() {
                    std::fs::create_dir_all(&target).map_err(|e| write_error(&name, e))?;
                    continue;
                }
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| write_error(&name, e))?;
                }
                let mut out = File::create(&target).map_err(|e| write_error(&name, e))?;
                std::io::copy(&mut entry, &mut out).map_err(|e| write_error(&name, e))?;
                #[cfg(unix)]
                if let Some(mode) = entry.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode & 0o777));
                }
            }
        }
        format => {
            let mut tar = tar_archive(shown, file, matches!(format, Format::TarGz))?;
            for entry in tar.entries().map_err(|e| broken(shown, e))? {
                let mut entry = entry.map_err(|e| broken(shown, e))?;
                let name = entry.path().map_err(|e| broken(shown, e))?.to_string_lossy().into_owned();
                if inside(dir, &name).is_none() {
                    return Err(outside(&name));
                }
                entry.unpack_in(dir).map_err(|e| write_error(&name, e))?;
            }
        }
    }
    Ok(())
}

/// Something added to a new archive.
enum Add {
    /// A file or a folder, with everything in it, under `name`.
    Path { shown: String, path: PathBuf, name: String },
    /// A file with these contents.
    Data { name: String, data: Vec<u8> },
}

/// A file or folder to write in an archive, and its name there.
struct Item {
    path: PathBuf,
    name: String,
    dir: bool,
}

/// A path and, for a folder, everything in it, sorted, so archives come out the same every time.
fn walk(shown: &str, path: &Path, name: &str, items: &mut Vec<Item>) -> Result<(), NativeError> {
    let meta = std::fs::metadata(path).map_err(|e| io_error("add", shown, e))?;
    if !meta.is_dir() {
        items.push(Item { path: path.to_path_buf(), name: name.to_string(), dir: false });
        return Ok(());
    }
    if !name.is_empty() {
        items.push(Item { path: path.to_path_buf(), name: name.to_string(), dir: true });
    }
    let mut children: Vec<PathBuf> = std::fs::read_dir(path).map_err(|e| io_error("read", shown, e))?.flatten().map(|e| e.path()).collect();
    children.sort();
    for child in children {
        let Some(file_name) = child.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        let child_name = if name.is_empty() { file_name } else { format!("{name}/{file_name}") };
        walk(shown, &child, &child_name, items)?;
    }
    Ok(())
}

fn write(shown: &str, file: &Path, adds: Vec<Add>) -> Result<(), NativeError> {
    let format = format(shown)?;
    let mut items = Vec::new();
    let mut texts = Vec::new();
    for add in adds {
        match add {
            Add::Path { shown, path, name } => {
                // Without a name, its own: `photos` for `/home/ana/photos/`, and what's inside for `.`
                let name = match name.trim_matches('/') {
                    "" => Path::new(&shown).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    given => given.to_string(),
                };
                walk(&shown, &path, &name, &mut items)?
            }
            Add::Data { name, data } => texts.push((name.trim_matches('/').to_string(), data)),
        }
    }
    // Names that would be extracted outside the folder they're extracted in
    for name in items.iter().map(|i| &i.name).chain(texts.iter().map(|(n, _)| n)) {
        if inside(Path::new(""), name).is_none() || name.starts_with('/') {
            return Err(NativeError::new(VALUE_ERROR, format!("'{name}' can't be a name in an archive: it would be extracted outside the folder, with `..`")));
        }
    }
    let failed = |e: &dyn std::fmt::Display| NativeError::new("IOError", format!("can't write '{shown}': {e}"));
    let out = BufWriter::new(File::create(file).map_err(|e| io_error("write", shown, e))?);
    let now = std::time::SystemTime::now();
    match format {
        Format::Zip => {
            use zip::write::SimpleFileOptions;
            let mut zip = zip::ZipWriter::new(out);
            let options = |modified: std::time::SystemTime| {
                let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
                match zip_time(modified) {
                    Some(t) => options.last_modified_time(t),
                    None => options,
                }
            };
            for item in &items {
                let meta = std::fs::metadata(&item.path).map_err(|e| io_error("add", &item.path.display().to_string(), e))?;
                let mut options = options(meta.modified().unwrap_or(now));
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    options = options.unix_permissions(meta.permissions().mode() & 0o777);
                }
                if item.dir {
                    zip.add_directory(format!("{}/", item.name), options).map_err(|e| failed(&e))?;
                } else {
                    zip.start_file(&item.name, options).map_err(|e| failed(&e))?;
                    let mut source = File::open(&item.path).map_err(|e| io_error("add", &item.path.display().to_string(), e))?;
                    std::io::copy(&mut source, &mut zip).map_err(|e| failed(&e))?;
                }
            }
            for (name, text) in &texts {
                zip.start_file(name, options(now)).map_err(|e| failed(&e))?;
                zip.write_all(text).map_err(|e| failed(&e))?;
            }
            zip.finish().map_err(|e| failed(&e))?.flush().map_err(|e| failed(&e))?;
        }
        format => {
            let writer: Box<dyn Write> = if matches!(format, Format::TarGz) { Box::new(flate2::write::GzEncoder::new(out, flate2::Compression::default())) } else { Box::new(out) };
            let mut tar = tar::Builder::new(writer);
            tar.follow_symlinks(true);
            for item in &items {
                if item.dir {
                    let mut header = tar::Header::new_gnu();
                    let meta = std::fs::metadata(&item.path).map_err(|e| failed(&e))?;
                    header.set_metadata(&meta);
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    tar.append_data(&mut header, format!("{}/", item.name), std::io::empty()).map_err(|e| failed(&e))?;
                } else {
                    tar.append_path_with_name(&item.path, &item.name).map_err(|e| failed(&e))?;
                }
            }
            let seconds = now.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            for (name, text) in &texts {
                let mut header = tar::Header::new_gnu();
                header.set_size(text.len() as u64);
                header.set_mode(0o644);
                header.set_mtime(seconds);
                header.set_entry_type(tar::EntryType::Regular);
                tar.append_data(&mut header, name, &text[..]).map_err(|e| failed(&e))?;
            }
            let mut writer = tar.into_inner().map_err(|e| failed(&e))?;
            writer.flush().map_err(|e| failed(&e))?;
            // A gzip file needs its end written, which dropping it does
            drop(writer);
        }
    }
    Ok(())
}

/// A time of a zip archive, which has no zone: the local one, like `zip` and `unzip` do.
fn local_millis(t: zip::DateTime) -> Option<i64> {
    let civil = jiff::civil::date(t.year() as i16, t.month() as i8, t.day() as i8).at(t.hour() as i8, t.minute() as i8, t.second() as i8, 0);
    civil.to_zoned(jiff::tz::TimeZone::system()).ok().map(|z| z.timestamp().as_millisecond())
}

fn zip_time(t: std::time::SystemTime) -> Option<zip::DateTime> {
    let zoned = jiff::Timestamp::try_from(t).ok()?.to_zoned(jiff::tz::TimeZone::system());
    zip::DateTime::from_date_and_time(zoned.year() as u16, zoned.month() as u8, zoned.day() as u8, zoned.hour() as u8, zoned.minute() as u8, zoned.second() as u8).ok()
}
