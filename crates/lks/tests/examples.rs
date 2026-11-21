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
        ("examples/classes.lks", "home is 5 away"),
        ("examples/classes.lks", "#1 write the docs (1 notes)"),
        ("examples/classes.lks", "rex says woof!"),
        ("examples/interfaces.lks", "Total area: 31.78"),
        ("examples/enums.lks", "['. DEBUG', 'i INFO', '! WARNING', 'x ERROR']"),
        ("examples/enums.lks", "EARTH: a year is 365 days, mass 1"),
        ("examples/generics.lks", "FIX THE BUG"),
        ("examples/generics.lks", "8081 PORT"),
        ("examples/generics.lks", "Vector(x: 3, y: 6)"),
        ("examples/shop/main.lks", "book for Ana (paid)"),
        ("examples/shop/main.lks", "Ana spent 19.99"),
        ("examples/operators.lks", "16.49 EUR"),
        ("examples/operators.lks", "['1 EUR', '3 EUR', '5 EUR']"),
        ("examples/operators.lks", "[true, true]"),
        ("examples/interfaces.lks", "a square with sides of 3"),
        ("examples/control_flow.lks", "['child', 'teenager', 'adult']"),
        ("examples/strings.lks", "['userAccountName', 'user_account_name', 'user-account-name', 'User Account Name']"),
        ("examples/collections.lks", "{ 'a' => 3, 'b' => 1, 'c' => 1 }"),
        ("examples/collections.lks", "[[1, 'fig'], [2, 'pear'], [2, 'apple']]"),
        ("examples/patterns.lks", "Year 2026, month 11, day 19"),
        ("examples/errors.lks", "app.json: `port` is missing (the key was port)"),
        ("examples/errors.lks", "ConfigError: the port must be a number"),
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
    let commands = run_ok(&["examples/commands.lks"]);
    assert!(commands.contains("<a file; echo hacked>"), "{commands}");
    assert!(commands.contains("[a|b][$HOME][*]"), "{commands}");
    assert!(commands.contains("['and more', 'saved']"), "{commands}");
    assert!(commands.contains("HAPPY"), "{commands}");
    assert!(run_ok(&["examples/files.lks"]).contains("Cleaned up: true"));
    let json = run_ok(&["examples/json.lks"]);
    assert!(json.contains("lakescript by folgue02, indent 4"));
    assert!(json.contains("['start', 'stop']"));
    let csv = run_ok(&["examples/csv.lks"]);
    assert!(csv.contains("Smith, John | 17 | Boston"), "{csv}");
    assert!(csv.contains("{ 'Lisbon' => 2, 'Boston' => 1 }"), "{csv}");
    assert!(csv.contains("name,age\n\"Smith, John\",17\nAna,34\nBo,51\n"), "{csv}");
    assert!(csv.contains("name;score\nAna;9.5\n"), "{csv}");
    assert_eq!(run_ok(&["examples/time.lks"]), "4999950000\ntimed it\n");
    let modules = run_ok(&["examples/modules/main.lks"]);
    assert!(modules.contains("The Module System"));
    assert!(modules.contains("8 4"));
    assert!(modules.contains("math::average ran 2 times"));
}

/// A server that answers every request with the same JSON, for `examples/http.lks`.
fn json_server() -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line.trim() != "" {
                line.clear();
            }
            let body = r#"{"name": "lake", "stars": 3}"#;
            let answer = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    address
}

#[test]
fn http() {
    let url = format!("{}/repo", json_server());
    let out = run_ok(&["examples/http.lks", &url]);
    assert_eq!(out, format!("200 from {url}\nType: application/json\nKeys: name, stars\nDownloaded 28 bytes\n"));
    let output = lks(&["examples/http.lks", "http://127.0.0.1:1/"], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("No answer: can't reach"));
}

#[test]
fn html() {
    let out = run_ok(&["examples/html.lks"]);
    assert_eq!(
        out,
        "The Lake Shop\nToday's specials\nGreen tea: 3.50 -> /tea\nCake & cream: 4.20 -> /cake\nApple pie: 5.00 (sold out) -> /pie\n\
         Available: Green tea, Cake & cream\n{\"name\": \"The Lake Shop\", \"rating\": 4.8}\nOpen every day\n&lt;b&gt;Tom &amp; Jerry&lt;/b&gt;\n"
    );
}

#[test]
fn testing() {
    let out = run_ok(&["--test", "examples/testing.lks"]);
    assert!(out.starts_with("          ok testSlug\n"), "{out}");
    assert!(out.contains("          ok testAssertNarrows\n"), "{out}");
    assert!(out.contains("Finished 6 passed, 0 failed in "), "{out}");
    let out = run_ok(&["--test", "examples/testing.lks", "Average"]);
    assert!(out.contains("ok testAverage\n") && out.contains("ok testAverageOfNothing\n") && out.contains("2 passed, 0 failed"), "{out}");

    // A test that fails
    let dir = std::env::temp_dir().join(format!("lks-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("fails.lks");
    std::fs::write(&file, "void testOk() {}\nvoid testBad() {\n    outln('looking');\n    assert 'a'.upper() == 'b';\n}\n").unwrap();
    let output = lks(&["--color", "never", "--test", &file.display().to_string()], None);
    assert_eq!(output.status.code(), Some(1));
    let out = String::from_utf8_lossy(&output.stdout);
    let expected = format!(
        "          ok testOk\n        FAIL testBad\n             {}:4:5: assert failed: 'a'.upper() == 'b'\n               left:  'A'\n               right: 'b'\n             It printed:\n               looking\n",
        file.display()
    );
    assert!(out.starts_with(&expected), "{out}");
    assert!(out.contains("Finished 1 passed, 1 failed in "), "{out}");
    // A file that can't run, and too many arguments
    std::fs::write(&file, "void testA() {\n    number $n = 'x';\n}\n").unwrap();
    let output = lks(&["--test", &file.display().to_string()], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("`$n` is a number"));
    assert_eq!(lks(&["--test", &file.display().to_string(), "a", "b"], None).status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn templates() {
    let out = run_ok(&["examples/templates/templates.lks"]);
    assert!(out.starts_with("# PRICES\n1. Tea\n2. Coffee <decaf> (expensive)\n3. Cake\nTotal: 14.5 €, 3 products\n"), "{out}");
    assert!(out.contains("<ul>\n  <li>Tea: 3 € <em>cheap</em></li>\n  <li>Coffee &lt;decaf&gt;: 7.5 €</li>\n"), "{out}");
    assert!(out.contains("<footer><a href=\"/about\">About</a></footer>\n"), "{out}");
    assert!(out.ends_with("line 2: `$missing` isn't declared\n"), "{out}");
}

#[test]
fn greet() {
    assert_eq!(run_ok(&["examples/greet.lks", "ana", "bo"]), "Hello, Ana!\nHello, Bo!\n");
    let output = lks(&["examples/greet.lks"], None);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "usage: greet.lks <name>...\n");
}

#[test]
fn todo() {
    assert_eq!(
        run_ok(&["examples/todo.lks", "add", "write the docs", "--priority", "high", "-t", "docs"]),
        "Added 'write the docs' with high priority [docs]\n"
    );
    assert_eq!(run_ok(&["examples/todo.lks", "list", "--done", "-n", "3"]), "Showing up to 3 done tasks\n");
    let help = run_ok(&["examples/todo.lks", "add", "--help"]);
    assert!(help.contains("-p, --priority <value>  How soon it must be done (low, normal, high) (default: normal)"), "{help}");
    let output = lks(&["examples/todo.lks", "add", "x", "-p", "soon"], None);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`--priority` must be one of low, normal, high, not `soon`"), "{stderr}");
    assert!(stderr.contains("Usage: todo.lks add [options] <task>"), "{stderr}");
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
