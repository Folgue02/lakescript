//! Importing types from completion *(see Importing types from completion in
//! `ideas/tooling/lsp.md`)*: the `pub` types every module that can be reached
//! declares, and the edit of the `use` lines that imports one.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use lakescript::docs::{ItemKind, module_doc};

/// A type that can be imported.
#[derive(Debug, Clone, PartialEq)]
pub struct Importable {
    pub name: String,
    pub kind: ItemKind,
    /// The `use` path of its module *(i.e. `std/time`, `folgue02/geo/shapes`)*.
    pub module: String,
    /// A type module, imported as itself *(i.e. `use people/Person;`)*.
    pub type_module: bool,
}

/// The `pub` types of a file, by name.
type Types = Vec<(String, ItemKind)>;

/// The types of the files read so far, by the time each one changed.
#[derive(Default)]
pub struct Index {
    files: RefCell<HashMap<PathBuf, (SystemTime, Types)>>,
}

impl Index {
    /// The types of the standard library and of the modules under `roots`.
    /// `open` has the text of the files the editor has, saved or not, and
    /// `skip` is the file being completed.
    pub fn types(&self, roots: &[PathBuf], open: &HashMap<PathBuf, String>, skip: &Path) -> Vec<Importable> {
        let mut found = Vec::new();
        for module in lakescript::std_modules() {
            let Some(source) = lakescript::std_source(module) else { continue };
            let type_module = last_part(module).starts_with(|c: char| c.is_ascii_uppercase());
            for (name, kind) in types_in(source) {
                found.push(Importable { name, kind, module: module.to_string(), type_module });
            }
        }
        for root in roots {
            let mut files = Vec::new();
            walk(root, &mut files);
            for file in files {
                if file == skip {
                    continue;
                }
                let Ok(relative) = file.strip_prefix(root) else { continue };
                let module: Vec<String> = relative.with_extension("").iter().map(|p| p.to_string_lossy().into_owned()).collect();
                let module = module.join("/");
                let types = match open.get(&file) {
                    Some(text) => types_in(text),
                    None => self.cached(&file),
                };
                let type_module = last_part(&module).starts_with(|c: char| c.is_ascii_uppercase());
                for (name, kind) in types {
                    let type_module = type_module && name == last_part(&module);
                    found.push(Importable { name, kind, module: module.clone(), type_module });
                }
            }
        }
        found
    }

    /// The types of a file on disk, read again only when it changed.
    fn cached(&self, file: &Path) -> Types {
        let Ok(changed) = std::fs::metadata(file).and_then(|m| m.modified()) else { return Vec::new() };
        if let Some((when, types)) = self.files.borrow().get(file)
            && *when == changed
        {
            return types.clone();
        }
        let types = std::fs::read_to_string(file).map(|t| types_in(&t)).unwrap_or_default();
        self.files.borrow_mut().insert(file.to_path_buf(), (changed, types.clone()));
        types
    }
}

fn last_part(module: &str) -> &str {
    module.rsplit('/').next().unwrap_or(module)
}

/// The `pub` classes, interfaces and enums of a module, read without running it.
fn types_in(source: &str) -> Types {
    let Ok(doc) = module_doc(source) else { return Vec::new() };
    doc.items.into_iter().filter(|i| matches!(i.kind, ItemKind::Class | ItemKind::Interface | ItemKind::Enum)).map(|i| (i.name, i.kind)).collect()
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() {
            walk(&path, files);
        } else if path.extension().is_some_and(|e| e == "lks") {
            files.push(path);
        }
    }
}

/// A change to the text: the characters from `start` to `end` replaced by `text`.
#[derive(Debug, PartialEq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// A `use` line of the file: where it is, and what it imports.
struct UseLine {
    /// Its first character, and the one after its new line.
    start: usize,
    end: usize,
    path: String,
    /// The names in `::{...}` or after `::`, and `None` for a whole module.
    items: Option<Vec<String>>,
}

/// The edit that imports a type into a file, or `None` when it's already
/// imported. Lines that are there aren't moved: a new one goes in its place,
/// the standard library first, an empty line, then the rest, each in
/// alphabetical order.
pub fn use_edit(text: &str, wanted: &Importable) -> Option<Edit> {
    let lines = use_lines(text);
    let std = |path: &str| path == "std" || path.starts_with("std/");
    if wanted.type_module {
        let path = wanted.module.clone();
        if lines.iter().any(|l| l.path == path) {
            return None;
        }
        return Some(insert(text, &lines, &path, format!("use {path};\n"), std(&path)));
    }
    // A line that already imports from that module gets the name too
    if let Some(line) = lines.iter().find(|l| l.path == wanted.module && l.items.is_some()) {
        let mut items = line.items.clone().unwrap_or_default();
        if items.contains(&wanted.name) {
            return None;
        }
        items.push(wanted.name.clone());
        items.sort();
        let new = format!("use {}::{{{}}};\n", wanted.module, items.join(", "));
        return Some(Edit { start: line.start, end: line.end, text: new });
    }
    let line = format!("use {}::{};\n", wanted.module, wanted.name);
    Some(insert(text, &lines, &wanted.module, line, std(&wanted.module)))
}

/// A new `use` line in its place.
fn insert(text: &str, lines: &[UseLine], path: &str, line: String, is_std: bool) -> Edit {
    let std = |l: &&UseLine| l.path == "std" || l.path.starts_with("std/");
    let (group, other): (Vec<&UseLine>, Vec<&UseLine>) = lines.iter().partition(|l| std(l) == is_std);
    // Before the first of its group that goes after it, or after the last of its group
    if let Some(next) = group.iter().find(|l| l.path.as_str() > path) {
        return Edit { start: next.start, end: next.start, text: line };
    }
    if let Some(last) = group.last() {
        return Edit { start: last.end, end: last.end, text: line };
    }
    // The first of its group: the standard library goes before the rest, with an empty line between
    match (is_std, other.first(), other.last()) {
        (true, Some(first), _) => Edit { start: first.start, end: first.start, text: format!("{line}\n") },
        (false, _, Some(last)) => Edit { start: last.end, end: last.end, text: format!("\n{line}") },
        _ => {
            let at = first_code(text);
            let after: String = text.chars().skip(at).take(1).collect();
            let gap = if after == "\n" || after.is_empty() { "" } else { "\n" };
            Edit { start: at, end: at, text: format!("{line}{gap}") }
        }
    }
}

/// Where the code of a file starts: after its `#!` line, its `//!` comment
/// and the empty lines after them.
fn first_code(text: &str) -> usize {
    let mut offset = 0;
    let mut seen_doc = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let header = (offset == 0 && trimmed.starts_with("#!")) || trimmed.starts_with("//!") || (trimmed.is_empty() && (seen_doc || offset == 0));
        if !header {
            break;
        }
        seen_doc |= trimmed.starts_with("//!");
        offset += line.chars().count();
    }
    offset
}

/// The `use` lines of a file, which go before anything else in it.
fn use_lines(text: &str) -> Vec<UseLine> {
    let mut found = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let length = line.chars().count();
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("use ").and_then(|r| r.strip_suffix(';')) {
            let rest = rest.trim();
            let (path, items) = match rest.split_once("::") {
                Some((path, list)) => {
                    let list = list.trim().trim_start_matches('{').trim_end_matches('}');
                    (path.trim().to_string(), Some(list.split(',').map(|i| i.trim().to_string()).filter(|i| !i.is_empty()).collect()))
                }
                None => (rest.split(" as ").next().unwrap_or(rest).trim().to_string(), None),
            };
            found.push(UseLine { start: offset, end: offset + length, path, items });
        } else if !(trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("#!")) {
            break;
        }
        offset += length;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted(module: &str, name: &str) -> Importable {
        Importable { name: name.into(), kind: ItemKind::Class, module: module.into(), type_module: false }
    }

    fn apply(text: &str, wanted: &Importable) -> String {
        let Some(edit) = use_edit(text, wanted) else { return text.to_string() };
        let chars: Vec<char> = text.chars().collect();
        let before: String = chars[..edit.start].iter().collect();
        let after: String = chars[edit.end..].iter().collect();
        format!("{before}{}{after}", edit.text)
    }

    #[test]
    fn lines_go_in_their_place() {
        // The first one, after the comment of the module
        assert_eq!(apply("//! A tool.\n\noutln(1);\n", &wanted("std/time", "Date")), "//! A tool.\n\nuse std/time::Date;\n\noutln(1);\n");
        assert_eq!(apply("outln(1);\n", &wanted("shapes", "Circle")), "use shapes::Circle;\n\noutln(1);\n");
        // In alphabetical order inside its group
        let text = "use std/fs;\nuse std/time;\n\nuse acme/web;\n\noutln(1);\n";
        assert_eq!(apply(text, &wanted("std/process", "X")), "use std/fs;\nuse std/process::X;\nuse std/time;\n\nuse acme/web;\n\noutln(1);\n");
        assert_eq!(apply(text, &wanted("zed/geo", "Circle")), "use std/fs;\nuse std/time;\n\nuse acme/web;\nuse zed/geo::Circle;\n\noutln(1);\n");
        // A group that isn't there yet: the standard library first, then an empty line
        assert_eq!(apply("use acme/web;\n\nx();\n", &wanted("std/time", "Date")), "use std/time::Date;\n\nuse acme/web;\n\nx();\n");
        assert_eq!(apply("use std/fs;\n\nx();\n", &wanted("acme/geo", "Circle")), "use std/fs;\n\nuse acme/geo::Circle;\n\nx();\n");
    }

    #[test]
    fn a_second_name_of_a_module_goes_in_braces() {
        assert_eq!(apply("use shapes::Circle;\nx();\n", &wanted("shapes", "Square")), "use shapes::{Circle, Square};\nx();\n");
        assert_eq!(apply("use shapes::{Square, Line};\n", &wanted("shapes", "Circle")), "use shapes::{Circle, Line, Square};\n");
        // Already imported: nothing to do
        assert_eq!(use_edit("use shapes::Circle;\n", &wanted("shapes", "Circle")), None);
        // A type module is imported as itself
        let person = Importable { name: "Person".into(), kind: ItemKind::Class, module: "people/Person".into(), type_module: true };
        assert_eq!(apply("use std/fs;\n\nx();\n", &person), "use std/fs;\n\nuse people/Person;\n\nx();\n");
        assert_eq!(use_edit("use people/Person;\n", &person), None);
    }
}
