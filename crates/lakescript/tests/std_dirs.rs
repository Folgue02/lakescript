//! `std/dirs` *(see `ideas/stdlib/dirs.md`)*. The paths of each system are
//! tested in `stdlib/dirs.rs`; these run the module as scripts see it.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(&format!("use std/dirs;\nuse std/env;\nuse std/fs;\n{source}")) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

fn error(source: &str) -> String {
    match Engine::with_output(CapturedOutput::new()).run(&format!("use std/dirs;\n{source}")) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.message,
    }
}

#[cfg(not(windows))]
#[test]
fn folders_follow_the_variables_of_the_script() {
    let code = "env::set('HOME', '/home/ana');\nenv::set('XDG_CONFIG_HOME', '/etc/ana');\nenv::set('XDG_CACHE_HOME', 'not/absolute');\nenv::set('TMPDIR', '/var/tmp/');\nenv::remove('XDG_DATA_HOME');\nenv::remove('XDG_RUNTIME_DIR');\n\
        const $app = dirs::App('tool');\n\
        outln([$app.name, $app.config(), $app.data(), $app.cache()]);\n\
        outln([dirs::config(), dirs::temp(), dirs::runtime()]);";
    assert_eq!(
        run(code),
        "['tool', '/etc/ana/tool', '/home/ana/.local/share/tool', '/home/ana/.cache/tool']\n['/etc/ana', '/var/tmp', null]\n"
    );
}

#[test]
fn create_makes_the_folder_only_when_asked() {
    let code = "const $base = fs::tempDir();\nenv::set('XDG_STATE_HOME', $base);\nenv::set('LOCALAPPDATA', $base);\n\
        const $app = dirs::App('tool');\n\
        outln(fs::dir?($app.state()));\n\
        const $made = $app.state(create: true);\n\
        outln([$made == $app.state(), fs::dir?($made)]);\n\
        fs::removeAll($base);";
    assert_eq!(run(code), "false\n[true, true]\n");
}

#[test]
fn names_that_would_be_other_folders_throw() {
    assert!(error("dirs::App('a/b');").contains("can't have `/`"), "{}", error("dirs::App('a/b');"));
    assert!(error("dirs::App('a\\\\b');").contains("can't have `/`"));
    assert!(error("dirs::App('  ');").contains("empty"));
    assert!(error("dirs::App('');").contains("empty"));
    // There's no function for it, only the class
    assert!(!error("dirs::app('tool');").is_empty());
}

#[test]
fn types() {
    // The user's folders and the runtime one may be missing
    assert!(error("const string $d = dirs::downloads();").contains("?string"), "{}", error("const string $d = dirs::downloads();"));
    assert_eq!(run("const string $c = dirs::App('x').config();\nconst ?string $r = dirs::runtime();\noutln($c.length() > 0);"), "true\n");
    assert!(error("dirs::App('x').config(create: 'yes');").contains("bool"));
}
