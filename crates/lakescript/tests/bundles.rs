//! Bundles and `std/resources` *(see `ideas/tooling/bundles.md` and
//! `ideas/stdlib/resources.md`)*.

#![cfg(feature = "bundle")]

use std::path::{Path, PathBuf};

use lakescript::{CapturedOutput, Engine, write_bundle};

/// A folder that's removed at the end of the test.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lks-bundles-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path.canonicalize().unwrap())
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_file(path: &Path, set: impl FnOnce(&mut Engine)) -> Result<String, String> {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    set(&mut engine);
    engine.run_file(path).map(|_| output.text()).map_err(|e| e.message)
}

fn run_bundle(path: &Path) -> Result<String, String> {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_type_check(false);
    engine.run_bundle(path).map(|_| output.text()).map_err(|e| e.message)
}

const READER: &str = "use std/resources;\noutln([resources::read('a.txt'), resources::readBytes('img/b.bin').length(), resources::exists?('img'), resources::exists?('c.txt'), resources::list(), resources::list('img')]);\n";

#[test]
fn resources_next_to_the_script() {
    let dir = TempDir::new("next");
    let script = dir.file("tool.lks", READER);
    dir.file("resources/a.txt", "text");
    dir.file("resources/img/b.bin", "12345");
    assert_eq!(run_file(&script, |_| {}).unwrap(), "['text', 5, true, false, ['a.txt', 'img'], ['b.bin']]\n");
    // Or in a folder given by the program that runs it *(i.e. `lks --resources`)*
    dir.file("other/a.txt", "other");
    dir.file("other/img/b.bin", "1");
    let other = dir.0.join("other");
    assert_eq!(run_file(&script, |e| e.set_resources(&other)).unwrap(), "['other', 1, true, false, ['a.txt', 'img'], ['b.bin']]\n");
}

#[test]
fn what_resources_say_is_wrong() {
    let dir = TempDir::new("wrong");
    dir.file("resources/bin.dat", "");
    std::fs::write(dir.0.join("resources/bin.dat"), [0xff, 0xfe, 0x00]).unwrap();
    let error = |code: &str| run_file(&dir.file("t.lks", &format!("use std/resources;\n{code}")), |_| {}).unwrap_err();
    assert!(error("resources::read('nope.txt');").contains("FileNotFound: there's no resource 'nope.txt'"));
    assert!(error("resources::read('bin.dat');").contains("isn't text: `resources::readBytes()` reads any file"));
    assert!(error("resources::read('../t.lks');").contains("isn't the name of a resource"));
    assert!(error("resources::read('/etc/passwd');").contains("isn't the name of a resource"));
    assert!(error("resources::list('nope');").contains("FileNotFound: there's no folder 'nope'"));
}

#[test]
fn a_bundle_runs_with_its_modules_and_resources() {
    let dir = TempDir::new("bundle");
    let files = vec![
        ("bin/tool.lks".to_string(), format!("use me/app/text;\nuse me/app/Shape;\nuse std/process;\n{READER}outln(text::shout('hi'));\noutln(Shape(2).area());\noutln(process::scriptPath()!.endsWith('tool.lkb'));").into_bytes()),
        ("src/me/app/text.lks".to_string(), b"pub string shout(string $s) = $s.upper();".to_vec()),
        // Types in the same folder, without `use`, also inside a bundle
        ("src/me/app/Shape.lks".to_string(), b"pub class Shape {\n    pub number @side;\n    pub number area() = Unit(@side).squared();\n}\n".to_vec()),
        ("src/me/app/Unit.lks".to_string(), b"pub class Unit {\n    pub number @n;\n    pub number squared() = @n * @n;\n}\n".to_vec()),
        ("resources/a.txt".to_string(), b"inside".to_vec()),
        ("resources/img/b.bin".to_string(), b"123".to_vec()),
    ];
    let out = dir.0.join("tool.lkb");
    write_bundle(&out, "tool", "me/app", "1.0.0", &files).unwrap();
    assert_eq!(run_bundle(&out).unwrap(), "['inside', 3, true, false, ['a.txt', 'img'], ['b.bin']]\nHI\n4\ntrue\n");
}

#[test]
fn errors_inside_a_bundle_say_where() {
    let dir = TempDir::new("errors");
    let files = vec![
        ("bin/tool.lks".to_string(), b"use me/app/broken;\nbroken::go();".to_vec()),
        ("src/me/app/broken.lks".to_string(), b"pub void go() {\n    throw ValueError('inside');\n}\n".to_vec()),
    ];
    let out = dir.0.join("tool.lkb");
    write_bundle(&out, "tool", "me/app", "1.0.0", &files).unwrap();
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_type_check(false);
    let e = engine.run_bundle(&out).unwrap_err();
    assert_eq!(e.message, "ValueError: inside");
    let shown = e.render("tool.lkb");
    assert!(shown.contains("tool.lkb/src/me/app/broken.lks:2:"), "{shown}");
    // A file that isn't one
    std::fs::write(dir.0.join("fake.lkb"), "outln(1);").unwrap();
    assert!(run_bundle(&dir.0.join("fake.lkb")).unwrap_err().contains("isn't a bundle made by `ferry bundle`"));
}

#[test]
fn a_folder_and_a_bundle_say_the_same() {
    // A project without resources/ has none, in a folder and in a bundle
    let dir = TempDir::new("same");
    let code = "use std/resources;\noutln([resources::list(), resources::exists?(''), resources::exists?('x')]);";
    let script = dir.file("t.lks", code);
    let in_folder = run_file(&script, |_| {}).unwrap();
    let out = dir.0.join("t.lkb");
    write_bundle(&out, "t", "a/b", "1.0.0", &[("bin/t.lks".to_string(), code.as_bytes().to_vec())]).unwrap();
    assert_eq!(in_folder, "[[], false, false]\n");
    assert_eq!(run_bundle(&out).unwrap(), in_folder);
    // And with some
    dir.file("resources/a/b.txt", "x");
    let files = vec![("bin/t.lks".to_string(), code.as_bytes().to_vec()), ("resources/a/b.txt".to_string(), b"x".to_vec())];
    write_bundle(&out, "t", "a/b", "1.0.0", &files).unwrap();
    assert_eq!(run_file(&script, |_| {}).unwrap(), "[['a'], false, false]\n");
    assert_eq!(run_bundle(&out).unwrap(), "[['a'], false, false]\n");
}

#[test]
fn a_bundle_under_another_name() {
    let dir = TempDir::new("renamed");
    let out = dir.0.join("tool.lkb");
    write_bundle(&out, "tool", "a/b", "1.0.0", &[("bin/tool.lks".to_string(), b"outln('ran');".to_vec())]).unwrap();
    let copy = dir.0.join("tool");
    std::fs::copy(&out, &copy).unwrap();
    assert!(lakescript::is_bundle(&copy));
    assert_eq!(run_bundle(&copy).unwrap(), "ran\n");
}
