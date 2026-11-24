//! `std/archive`: zip, tar and tar.gz *(see `ideas/stdlib/archive.md`)*.

// Only with the features of the crate these modules need
#![cfg(feature = "archive")]

use std::path::PathBuf;

use lakescript::{CapturedOutput, Engine};

/// A folder that's removed at the end of the test.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lks-archive-{name}-{}", std::process::id()));
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

/// Runs a script in the folder, type checked like `lks` does.
fn run_in(dir: &TempDir, source: &str) -> String {
    let script = dir.file("main.lks", &format!("use std/archive;\nuse std/process;\nprocess::chdir('{}');\n{source}", dir.0.display()));
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run_file(&script) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("main.lks"), output.text());
    }
    output.text()
}

fn error_in(dir: &TempDir, source: &str) -> String {
    let script = dir.file("main.lks", &format!("use std/archive;\nuse std/process;\nprocess::chdir('{}');\n{source}", dir.0.display()));
    match Engine::with_output(CapturedOutput::new()).run_file(&script) {
        Ok(_) => panic!("expected an error:\n{source}"),
        Err(e) => e.message,
    }
}

const WRITE_AND_READ: &str = "
const $new = archive::create($name);
$new.add('photos/', as: 'pictures');
$new.add('photos/a.txt');
$new.addText('notes/readme.md', '# Hello');
$new.close();
const $read = archive::open($name);
for ($e in $read.entries())
    outln(f\"{$e.name} {$e.size} {$e.dir?()} {$e.modified != null}\");
outln($read.read('notes/readme.md'));
$read.extract(f\"out-{$name}\");
";

#[test]
fn every_format_is_written_read_and_extracted() {
    let dir = TempDir::new("formats");
    dir.file("photos/a.txt", "hi\n");
    dir.file("photos/2026/b.txt", "deep\n");
    let expected = "pictures 0 true true\npictures/2026 0 true true\npictures/2026/b.txt 5 false true\npictures/a.txt 3 false true\na.txt 3 false true\nnotes/readme.md 7 false true\n# Hello\n";
    for name in ["x.zip", "x.tar", "x.tar.gz", "x.tgz"] {
        assert_eq!(run_in(&dir, &format!("const $name = '{name}';\n{WRITE_AND_READ}")), expected, "{name}");
        let out = dir.0.join(format!("out-{name}"));
        assert_eq!(std::fs::read_to_string(out.join("pictures/2026/b.txt")).unwrap(), "deep\n", "{name}");
        assert_eq!(std::fs::read_to_string(out.join("notes/readme.md")).unwrap(), "# Hello", "{name}");
    }
    // The modified date is a Date, in the local zone
    assert_eq!(run_in(&dir, "outln(archive::open('x.zip').entries()[0]!.modified!.year >= 2026);"), "true\n");
    // Without `as`, a folder keeps its own name; `.` adds what's inside
    let code = "const $a = archive::create('y.zip');\n$a.add('photos');\n$a.close();\noutln(archive::open('y.zip').entries().map(($e) -> $e.name));";
    assert_eq!(run_in(&dir, code), "['photos', 'photos/2026', 'photos/2026/b.txt', 'photos/a.txt']\n");
}

#[test]
fn an_archive_cant_write_outside_the_folder() {
    let dir = TempDir::new("outside");
    let error = error_in(&dir, "const $a = archive::create('bad.tar');\n$a.addText('../up.txt', 'x');\n$a.close();");
    assert!(error.contains("'../up.txt' can't be a name in an archive"), "{error}");
    // One written by another program is refused when it's extracted
    let bad = dir.0.join("bad.zip");
    {
        let mut zip = zip_writer(&bad);
        use std::io::Write;
        zip.start_file("../up.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
    }
    let error = error_in(&dir, "archive::open('bad.zip').extract('in');");
    assert!(error.contains("'../up.txt' in 'bad.zip' would be written outside 'in'"), "{error}");
    assert!(!dir.0.join("up.txt").exists());
}

fn zip_writer(path: &std::path::Path) -> zip::ZipWriter<std::fs::File> {
    zip::ZipWriter::new(std::fs::File::create(path).unwrap())
}

#[test]
fn mistakes_say_what_to_do() {
    let dir = TempDir::new("mistakes");
    dir.file("a.txt", "a");
    run_in(&dir, "const $a = archive::create('ok.zip');\n$a.add('a.txt');\n$a.close();");
    assert!(error_in(&dir, "archive::open('nope.zip');").contains("can't open 'nope.zip': it doesn't exist"));
    assert!(error_in(&dir, "archive::create('x.rar');").contains("an archive's name ends in .zip, .tar, .tar.gz or .tgz"));
    assert!(error_in(&dir, "archive::open('ok.zip').read('b.txt');").contains("there's no 'b.txt' in 'ok.zip'"));
    assert!(error_in(&dir, "archive::open('ok.zip').add('a.txt');").contains("`archive::create()` gives one to write"));
    assert!(error_in(&dir, "archive::create('n.zip').entries();").contains("`archive::open()` gives one to read"));
    assert!(error_in(&dir, "archive::create('n.zip').add('missing.txt');").contains("can't add 'missing.txt': it doesn't exist"));
    let error = error_in(&dir, "const $a = archive::create('n.zip');\n$a.close();\n$a.addText('x', 'y');");
    assert!(error.contains("'n.zip' is already closed"), "{error}");
}
