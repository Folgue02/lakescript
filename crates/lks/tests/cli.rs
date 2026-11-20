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
