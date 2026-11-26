//! The command line of `lks`: its own options never mix with the script's.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn lks(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lks")).args(args).current_dir(root()).env_remove("GREETING").output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn runs_a_script_with_its_arguments() {
    let out = lks(&["examples/greet.lks", "ana"]);
    assert_eq!(stdout(&out), "Hello, Ana!\n");
}

#[test]
fn options_after_the_script_belong_to_the_script() {
    let out = lks(&["--no-type-check", "examples/greet.lks", "--no-type-check", "-h", "--version"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "Hello, --no-type-check!\nHello, -h!\nHello, --version!\n");
}

#[test]
fn double_dash_ends_the_options() {
    let dir = std::env::temp_dir().join(format!("lks-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("-odd.lks"), "outln('odd name');").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lks")).args(["--", "-odd.lks"]).current_dir(&dir).output().unwrap();
    assert_eq!(stdout(&out), "odd name\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn help_and_version() {
    let out = lks(&["--help"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: lks [options] <script> [arguments for the script...]"));
    assert!(stdout(&out).contains("--no-type-check"));
    assert_eq!(stdout(&lks(&["-V"])), format!("lks {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn wrong_command_lines() {
    let out = lks(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("lks: no script given"));
    let out = lks(&["--nope", "examples/hello.lks"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("unknown option `--nope`"));
    let out = lks(&["examples/missing.lks"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("can't read examples/missing.lks"));
}

#[test]
fn the_exit_code_is_the_scripts() {
    assert_eq!(lks(&["examples/greet.lks"]).status.code(), Some(2));
    assert_eq!(lks(&["examples/greet.lks", ""]).status.code(), Some(1));
}

#[test]
fn module_path_adds_folders_for_use() {
    let dir = std::env::temp_dir().join(format!("lks-module-path-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("one/a")).unwrap();
    std::fs::create_dir_all(dir.join("two")).unwrap();
    std::fs::write(dir.join("one/a/x.lks"), "pub number n() = 1;").unwrap();
    std::fs::write(dir.join("two/y.lks"), "pub number n() = 2;").unwrap();
    std::fs::write(dir.join("main.lks"), "use a/x;\nuse y;\noutln(x::n() + y::n());").unwrap();
    let folders = format!("{}:{}", dir.join("one").display(), dir.join("two").display());
    let out = lks(&["--module-path", &folders, dir.join("main.lks").to_str().unwrap()]);
    assert_eq!(stdout(&out), "3\n", "{}", stderr(&out));
    assert_eq!(lks(&["--module-path"]).status.code(), Some(2));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn errors_have_colors_when_asked() {
    let dir = std::env::temp_dir().join(format!("lks-colors-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("bad.lks");
    std::fs::write(&script, "number $a = 1;\n$a = 'x';").unwrap();
    let script = script.to_str().unwrap();
    let run = |args: &[&str], env: &[(&str, &str)]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lks"));
        command.args(args).env_remove("NO_COLOR").env_remove("CLICOLOR_FORCE");
        for (k, v) in env {
            command.env(k, v);
        }
        stderr(&command.output().unwrap())
    };
    // stderr isn't a terminal here, so there are none unless asked for
    assert!(!run(&[script], &[]).contains('\x1b'));
    assert!(run(&["--color", "always", script], &[]).contains("\x1b[1;31merror:"));
    assert!(!run(&["--color", "never", script], &[("CLICOLOR_FORCE", "1")]).contains('\x1b'));
    assert!(run(&[script], &[("CLICOLOR_FORCE", "1")]).contains('\x1b'));
    assert!(!run(&[script], &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")]).contains('\x1b'));
    assert!(run(&["--color", "sometimes", script], &[]).contains("`--color` needs `auto`, `always` or `never`"));
    assert!(stdout(&lks(&["--color", "always", "--help"])).contains("\x1b[1;32mOptions:"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_usage_line_says_what_was_typed() {
    let dir = std::env::temp_dir().join(format!("lks-usage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("cinder.lks");
    std::fs::write(&script, "command Cinder {\n    /// The seed\n    flag number @seed = 1;\n}\nCinder::parse();\n").unwrap();
    let usage = |args: &[&str]| stdout(&lks(args)).lines().next().unwrap_or_default().to_string();
    let path = script.display().to_string();
    assert!(usage(&[&path, "--help"]).starts_with("Usage: cinder.lks"));
    assert!(usage(&["--name", "cinder", &path, "--help"]).starts_with("Usage: cinder "), "{}", usage(&["--name", "cinder", &path, "--help"]));
    // Through a link, its name, not the real file's
    #[cfg(unix)]
    {
        let link = dir.join("deploy");
        std::os::unix::fs::symlink(&script, &link).unwrap();
        assert!(usage(&[&link.display().to_string(), "--help"]).starts_with("Usage: deploy "));
    }
    assert!(stderr(&lks(&["--name"])).contains("`--name` needs the name"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bundles_and_resources() {
    let dir = std::env::temp_dir().join(format!("lks-cli-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("res")).unwrap();
    let script = dir.join("show.lks");
    std::fs::write(&script, "use std/resources;\nuse std/process;\noutln(resources::read('a.txt'));\noutln(process::args());\n").unwrap();
    std::fs::write(dir.join("res/a.txt"), "from --resources").unwrap();
    let path = script.display().to_string();
    let res = dir.join("res").display().to_string();
    assert_eq!(stdout(&lks(&["--resources", &res, &path, "x"])), "from --resources\n['x']\n");
    assert!(stderr(&lks(&["--resources"])).contains("`--resources` needs the folder"));
    // A bundle, with its arguments after it
    let bundle = dir.join("show.lkb");
    let files = vec![
        ("bin/show.lks".to_string(), std::fs::read(&script).unwrap()),
        ("resources/a.txt".to_string(), b"from the bundle".to_vec()),
    ];
    lakescript::write_bundle(&bundle, "show", "me/show", "1.0.0", &files).unwrap();
    let bundle = bundle.display().to_string();
    let out = lks(&[&bundle, "y", "--z"]);
    assert_eq!(stdout(&out), "from the bundle\n['y', '--z']\n", "{}", stderr(&out));
    let out = lks(&["--test", &bundle]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("is a bundle"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bundles_through_links_and_their_errors() {
    let dir = std::env::temp_dir().join(format!("lks-cli-links-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bundle = dir.join("tool.lkb");
    lakescript::write_bundle(&bundle, "tool", "me/tool", "1.0.0", &[("bin/tool.lks".to_string(), b"outln('ran');".to_vec())]).unwrap();
    // Through a link with another name, like an installed command
    #[cfg(unix)]
    {
        let link = dir.join("tool");
        std::os::unix::fs::symlink(&bundle, &link).unwrap();
        assert_eq!(stdout(&lks(&[&link.display().to_string()])), "ran\n");
    }
    // What can't be run says why, without a place in the code
    std::fs::write(dir.join("broken.lkb"), "not a zip").unwrap();
    let out = lks(&[&dir.join("broken.lkb").display().to_string()]);
    assert!(stderr(&out).starts_with("error: ") && stderr(&out).contains("isn't a bundle made by `ferry bundle`"), "{}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_without_running() {
    let dir = std::env::temp_dir().join(format!("lks-check-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_lks")).args(args).current_dir(&dir).output().unwrap();
    // Nothing runs, and nothing is said when there's no error
    std::fs::write(dir.join("writes.lks"), "use std/fs;\nfs::write('ran.txt', 'yes');\noutln('ran');\n").unwrap();
    let out = run(&["--check", "writes.lks"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!((stdout(&out), stderr(&out)), (String::new(), String::new()));
    assert!(!dir.join("ran.txt").exists());
    // A type error after code that would delete files
    std::fs::write(dir.join("wrong.lks"), "use std/fs;\nfs::write('ran.txt', 'yes');\nconst number $a = 'old';\n").unwrap();
    let out = run(&["--check", "--color", "never", "wrong.lks"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains(" --> wrong.lks:3:"), "{}", stderr(&out));
    assert!(!dir.join("ran.txt").exists());
    // And in a module it uses
    std::fs::write(dir.join("helper.lks"), "pub number twice(number $n) = $n * 'two';\n").unwrap();
    std::fs::write(dir.join("uses.lks"), "use helper;\noutln(helper::twice(2));\n").unwrap();
    let out = run(&["--check", "--color", "never", "uses.lks"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("helper.lks:1:"), "{}", stderr(&out));
    // It only checks
    for (args, said) in [
        (&["--check", "writes.lks", "x"][..], "`lks --check` takes only the script"),
        (&["--check", "--test", "writes.lks"][..], "`--check` checks a script without running it, and `--test` runs its tests"),
        (&["--check", "--no-type-check", "writes.lks"][..], "`--check` checks the types of the script, and `--no-type-check` doesn't"),
    ] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(stderr(&out).contains(said), "{args:?}: {}", stderr(&out));
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn errors_as_json() {
    let dir = std::env::temp_dir().join(format!("lks-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_lks")).arg("--message-format").arg("json").args(args).current_dir(&dir).output().unwrap();
        let said = stderr(&out);
        // One object on each line, and nothing else
        assert!(said.lines().all(|l| l.starts_with("{\"type\":") && l.ends_with('}')), "{said}");
        (out.status.code(), said)
    };
    std::fs::write(dir.join("types.lks"), "outln(1);\nconst number $age = 'old';\n").unwrap();
    assert_eq!(
        run(&["--check", "types.lks"]),
        (Some(1), "{\"type\":\"error\",\"message\":\"`$age` is a number, but its value is a string\",\"exception\":null,\"file\":\"types.lks\",\"line\":2,\"column\":21,\"length\":5,\"trace\":[]}\n".to_string())
    );
    // An uncaught exception, its type and its calls, and a message that needs escaping
    std::fs::write(dir.join("throws.lks"), "void f() {\n    throw ValueError(\"a \\\"b\\\"\\n\\tc\");\n}\nf();\n").unwrap();
    assert_eq!(
        run(&["throws.lks"]),
        (
            Some(1),
            "{\"type\":\"error\",\"message\":\"ValueError: a \\\"b\\\"\\n\\tc\",\"exception\":\"ValueError\",\"file\":\"throws.lks\",\"line\":2,\"column\":5,\"length\":33,\"trace\":[{\"function\":\"the script\",\"file\":\"throws.lks\",\"line\":4,\"column\":1,\"times\":1}]}\n".to_string()
        )
    );
    // What isn't at a place in the code
    let (code, said) = run(&["missing.lks"]);
    assert_eq!(code, Some(1));
    assert!(said.starts_with("{\"type\":\"error\",\"message\":\"can't read missing.lks") && said.contains("\"line\":null,\"column\":null,\"length\":null,\"trace\":[]}"), "{said}");
    let (code, said) = run(&["--nope", "types.lks"]);
    assert_eq!(code, Some(2));
    assert!(said.contains("\"message\":\"unknown option `--nope`. Usage: lks [options] <script> [arguments for the script...]\""), "{said}");
    // Wrong arguments for the script itself
    std::fs::write(dir.join("cli.lks"), "command Cinder {\n    /// The seed\n    flag number @seed = 1;\n}\nCinder::parse();\n").unwrap();
    let (code, said) = run(&["cli.lks", "--nope"]);
    assert_eq!(code, Some(2));
    assert!(said.contains("\"exception\":\"UsageError\""), "{said}");
    // The script's own output is its own
    std::fs::write(dir.join("fine.lks"), "outln('out');\nerrln('err');\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lks")).args(["--message-format", "json", "fine.lks"]).current_dir(&dir).output().unwrap();
    assert_eq!((stdout(&out), stderr(&out)), ("out\n".to_string(), "err\n".to_string()));
    // And `text` is the default
    let out = Command::new(env!("CARGO_BIN_EXE_lks")).args(["--message-format", "text", "--color", "never", "types.lks"]).current_dir(&dir).output().unwrap();
    assert!(stderr(&out).starts_with("error: `$age` is a number"), "{}", stderr(&out));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn every_type_error_at_once() {
    let dir = std::env::temp_dir().join(format!("lks-every-error-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("two.lks"), "outln('ran');\nconst number $a = 'one';\nconst string $s = 2;\n").unwrap();
    let run = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_lks")).args(args).current_dir(&dir).output().unwrap();
    for args in [&["--color", "never", "--check", "two.lks"][..], &["--color", "never", "two.lks"][..]] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(1));
        let said = stderr(&out);
        assert!(said.contains(" --> two.lks:2:19") && said.contains(" --> two.lks:3:19"), "{args:?}: {said}");
        assert!(said.contains("^^^^^\n\nerror: `$s`"), "a blank line between them: {said}");
        assert_eq!(stdout(&out), "", "nothing ran");
    }
    let said = stderr(&run(&["--message-format", "json", "--check", "two.lks"]));
    assert_eq!(said.lines().count(), 2, "{said}");
    // Only the first syntax error, outside an editor
    std::fs::write(dir.join("syntax.lks"), "outln(1\nconst string $s = 2;\nlet $x = ;\n").unwrap();
    let said = stderr(&run(&["--color", "never", "--check", "syntax.lks"]));
    assert_eq!(said.matches("error:").count(), 1, "{said}");
    std::fs::remove_dir_all(&dir).unwrap();
}
