//! Messages found while writing two packages, `folgue02/text` and
//! `folgue02/table`, with ferry: what they said, and what they say now.

use lakescript::{CapturedOutput, Engine};

fn error(source: &str) -> String {
    match Engine::with_output(CapturedOutput::new()).run(source) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.message,
    }
}

#[test]
fn messages_have_no_markdown() {
    for code in ["enum Align { Left, Right }", "class point {}", "const $x = 1..=;"] {
        let message = error(code);
        assert!(!message.contains("*(") && !message.contains(")*"), "{code}: {message}");
        assert!(message.contains("(i.e. `"), "{code}: {message}");
    }
}

#[test]
fn enum_values_are_reached_with_colons() {
    assert_eq!(error("enum E { A, B }\nE f() = E.A;"), "the values of an enum are reached with `::`: `E::A`, not `E.A`");
    // The casing, and the `;` only where it's missing before a member
    assert_eq!(error("enum Align { Left, Right }"), "the values of an enum are written in SCREAMING_CASE (i.e. `LEFT`)");
    assert!(error("enum A { X, Y, number f() = 1; }").contains("If it starts a field or a method, the values end with `;` before it"));
    // One line needs no `;`
    let output = CapturedOutput::new();
    Engine::with_output(output.clone()).run("enum Align { LEFT, RIGHT, CENTER }\noutln(Align::RIGHT);").unwrap();
    assert_eq!(output.text(), "Align::RIGHT\n");
}

#[test]
fn a_return_of_another_type_says_what_it_is() {
    assert_eq!(error("pub string x() = 1;"), "`x` returns a string, but this is a number");
    assert_eq!(error("string x() { return 1; }"), "this function returns a string, but this is a number");
}

#[test]
fn lists_are_joined_with_concat() {
    let message = error("const $a = [1];\noutln($a + [2]);");
    assert!(message.ends_with("Two lists are joined with `$a.concat($b)` or `[...$a, ...$b]`"), "{message}");
    assert!(!error("outln(1 + 'a');").contains("concat"));
}

#[test]
fn failed_asserts_line_up_values_of_several_lines() {
    let message = error("assert 'a\\nb' == 'a\\nc';");
    assert!(message.ends_with("  left:  'a\n          b'\n  right: 'a\n          c'"), "{message}");
}

#[test]
fn a_module_named_like_a_reserved_word_needs_an_alias() {
    let dir = std::env::temp_dir().join(format!("lks-reserved-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("text")).unwrap();
    std::fs::write(dir.join("text/case.lks"), "pub string x() = 'x';\n").unwrap();
    let run = |code: &str| {
        let script = dir.join("main.lks");
        std::fs::write(&script, code).unwrap();
        let output = CapturedOutput::new();
        Engine::with_output(output.clone()).run_file(&script).map(|_| output.text()).map_err(|e| (e.message, e.span.column))
    };
    // Said at the `use`, not where it's used
    let (message, column) = run("use text/case;\noutln(case::x());\n").unwrap_err();
    assert_eq!(message, "`case` is a reserved word, so the module needs another name to be used: `use text/case as c;`");
    assert_eq!(column, 10);
    assert_eq!(run("use text/case as c;\nuse text/case::x;\noutln(c::x() + x());\n").unwrap(), "xx\n");
    let _ = std::fs::remove_dir_all(&dir);
}
