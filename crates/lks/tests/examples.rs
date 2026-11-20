//! Runs every script in `examples/` with the real `lks`, so the examples
//! always work with the current interpreter.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn lks(args: &[&str], input: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lks"));
    command.args(args).current_dir(root()).env_remove("GREETING");
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    if let Some(text) = input {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn run_ok(args: &[&str]) -> String {
    let output = lks(args, None);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "{args:?} failed with {:?}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

#[test]
fn every_example_is_listed_and_tested() {
    let readme = std::fs::read_to_string(root().join("examples/README.md")).unwrap();
    let mut scripts = Vec::new();
    for entry in std::fs::read_dir(root().join("examples")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "lks") {
            scripts.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    let tests = std::fs::read_to_string(root().join(file!())).unwrap();
    for script in scripts {
        assert!(readme.contains(&format!("]({script})")), "{script} isn't listed in examples/README.md");
        assert!(tests.contains(&format!("examples/{script}")), "{script} isn't run by this test");
    }
}

#[test]
fn language_examples() {
    for (script, expected) in [
        ("examples/hello.lks", "Sum of squares: 14"),
        ("examples/features.lks", "First 10 primes: [2, 3, 5, 7, 11, 13, 17, 19, 23, 29]"),
        ("examples/closures.lks", "The counter is at 3"),
        ("examples/closures.lks", "7 is odd: true"),
        ("examples/control_flow.lks", "['child', 'teenager', 'adult']"),
        ("examples/strings.lks", "['userAccountName', 'user_account_name', 'user-account-name', 'User Account Name']"),
        ("examples/collections.lks", "{ 'a' => 3, 'b' => 1, 'c' => 1 }"),
        ("examples/collections.lks", "[[1, 'fig'], [2, 'pear'], [2, 'apple']]"),
        ("examples/patterns.lks", "Year 2026, month 11, day 19"),
        ("examples/errors.lks", "Bad port '99999': 99999 isn't a valid port"),
        ("examples/errors.lks", "https\ntrue"),
        ("examples/types.lks", "the string 'hi'"),
    ] {
        let out = run_ok(&[script]);
        assert!(out.contains(expected), "{script} didn't print {expected:?}:\n{out}");
    }
}

#[test]
fn system_examples() {
    assert!(run_ok(&["examples/commands.lks"]).contains("<a file; echo hacked>"));
    assert!(run_ok(&["examples/files.lks"]).contains("Cleaned up: true"));
    let json = run_ok(&["examples/json.lks"]);
    assert!(json.contains("lakescript by folgue02, indent 4"));
    assert!(json.contains("['start', 'stop']"));
    assert_eq!(run_ok(&["examples/time.lks"]), "4999950000\ntimed it\n");
    let modules = run_ok(&["examples/modules/main.lks"]);
    assert!(modules.contains("The Module System"));
    assert!(modules.contains("8 4"));
    assert!(modules.contains("math::average ran 2 times"));
}

#[test]
fn greet() {
    assert_eq!(run_ok(&["examples/greet.lks", "ana", "bo"]), "Hello, Ana!\nHello, Bo!\n");
    let output = lks(&["examples/greet.lks"], None);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "usage: greet.lks <name>...\n");
}

#[test]
fn wordcount() {
    let output = lks(&["examples/wordcount.lks", "2"], Some("the cat, the dog. The end"));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "6 words, 4 different\n    3  the\n    1  cat\n");
}

#[test]
fn backup() {
    let base = std::env::temp_dir().join(format!("lks-example-backup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (project, backups) = (base.join("project"), base.join("backups"));
    std::fs::create_dir_all(project.join("cache")).unwrap();
    std::fs::create_dir_all(&backups).unwrap();
    std::fs::write(project.join("notes file.txt"), "hi").unwrap();
    std::fs::write(project.join("junk.tmp"), "x").unwrap();
    std::fs::write(backups.join("backup.json"), r#"{"keep": 1, "exclude": ["*.tmp"]}"#).unwrap();

    let (p, b) = (project.display().to_string(), backups.display().to_string());
    let out = run_ok(&["examples/backup/backup.lks", &p, &b]);
    assert!(out.contains("Backing up 1 file"), "{out}");
    let archives: Vec<String> = std::fs::read_dir(&backups)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tar.gz"))
        .collect();
    assert_eq!(archives.len(), 1);
    let listing = Command::new("tar").arg("-tzf").arg(backups.join(&archives[0])).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&listing.stdout), "notes file.txt\n");

    let output = lks(&["examples/backup/backup.lks"], None);
    assert_eq!(output.status.code(), Some(2));
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn type_errors_stop_the_script_before_it_starts() {
    let dir = std::env::temp_dir().join(format!("lks-example-types-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("broken.lks");
    std::fs::write(&script, "outln('started');\nlet $total = 0;\n$total = 'none';\n").unwrap();
    let path = script.display().to_string();
    let output = lks(&[&path], None);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "", "nothing ran");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.contains("error: `$total` is a number, so it can't hold a string"), "{stderr}");
    assert!(stderr.contains("3 | $total = 'none';"), "{stderr}");
    assert!(stderr.contains("  |          ^^^^^^"), "{stderr}");
    // Without the check it starts, and fails only when the line runs
    let output = lks(&["--no-type-check", &path], None);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "started\n");
    std::fs::remove_dir_all(&dir).unwrap();
}
