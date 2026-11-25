//! `std/terminal` without a person at a terminal: prompts read stdin, a line
//! each *(see `ideas/stdlib/terminal.md`)*. The prompts on a terminal are drawn
//! by `inquire`, and were tried by hand in a pseudo-terminal.

// Only with the features of the crate these modules need
#![cfg(feature = "terminal")]

use lakescript::{CapturedOutput, Engine};

fn engine(input: &str) -> (Engine, CapturedOutput) {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_input(std::io::Cursor::new(input.to_string()), true);
    (engine, output)
}

fn run(input: &str, code: &str) -> String {
    let source = format!("use std/terminal;\nuse std/terminal/Style;\n{code}");
    let (mut engine, output) = engine(input);
    if let Err(e) = engine.run(&source) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("test.lks"), output.text());
    }
    output.text()
}

fn error(input: &str, code: &str) -> String {
    let (mut engine, _) = engine(input);
    match engine.run(&format!("use std/terminal;\nuse std/terminal/Style;\n{code}")) {
        Ok(_) => panic!("it should have failed:\n{code}"),
        Err(e) => e.render("test.lks"),
    }
}

#[test]
fn styles_without_a_terminal_are_plain_text() {
    assert_eq!(run("", "outln(terminal::style('hi', Style::RED, Style::ON_BLUE, Style::BOLD));"), "hi\n");
    assert_eq!(run("", "outln(terminal::strip('\\u{1b}[1;31mhi\\u{1b}[0m!'));"), "hi!\n");
    assert_eq!(run("", "outln(terminal::visibleWidth('\\u{1b}[31mñu😀\\u{1b}[0m'));"), "4\n");
    assert_eq!(run("", "outln(Style::ON_BRIGHT_CYAN.name());"), "ON_BRIGHT_CYAN\n");
    assert!(error("", "terminal::style('x', 'RED');").contains("error"));
}

#[test]
fn the_size_without_a_terminal() {
    assert_eq!(run("", "outln(f\"{terminal::width()} {terminal::height()} {terminal::interactive?()}\");"), "80 24 false\n");
}

#[test]
fn ask_reads_lines() {
    assert_eq!(run("Ann\n\nrest\n", "outln(terminal::ask('Name?'));\noutln(terminal::ask('City?', 'Lima'));\noutln(terminal::ask('More?'));"), "Ann\nLima\nrest\n");
    // At the end of stdin, the default
    assert_eq!(run("", "outln(terminal::ask('City?', 'Lima'));"), "Lima\n");
    assert!(error("", "terminal::ask('Name?');").contains("ValueError: nothing answers 'Name?'"));
}

#[test]
fn a_check_that_fails_without_a_terminal_throws() {
    let check = "($a) -> $a.empty?() ? 'write something' : null";
    assert_eq!(run("Bo\n", &format!("outln(terminal::ask('Name?', null, {check}));")), "Bo\n");
    assert!(error("\n", &format!("terminal::ask('Name?', null, {check});")).contains("ValueError: write something"));
    let caught = format!("try {{ terminal::ask('Name?', null, {check}); }} catch (ValueError $e) {{ outln($e.message()); }}");
    assert_eq!(run("\n", &caught), "write something\n");
}

#[test]
fn confirm_choose_and_choose_many() {
    assert_eq!(run("yes\nN\n\n", "outln([terminal::confirm('A?'), terminal::confirm('B?'), terminal::confirm('C?', true)]);"), "[true, false, true]\n");
    assert!(error("maybe\n", "terminal::confirm('A?');").contains("'maybe' doesn't answer 'A?'"));
    assert_eq!(run("b\n", "outln(terminal::choose('Pick', ['a', 'b']));"), "b\n");
    assert!(error("z\n", "terminal::choose('Pick', ['a', 'b']);").contains("'z' isn't one of the options of 'Pick': a, b"));
    assert_eq!(run("a, c\n", "outln(terminal::chooseMany('Pick', ['a', 'b', 'c']));"), "['a', 'c']\n");
    assert_eq!(run("", "outln(terminal::chooseMany('Pick', ['a']));"), "[]\n");
}

#[test]
fn passwords_never_come_from_stdin() {
    assert!(error("secret\n", "terminal::password('Secret');").contains("IOError: can't ask 'Secret' without a terminal"));
}

#[test]
fn filter_without_a_terminal() {
    assert_eq!(run("green\n", "outln(terminal::filter(['red', 'green']));"), "green\n");
    assert_eq!(run("", "outln(terminal::filter(['red', 'green']));"), "null\n");
}

#[test]
fn progress_draws_nothing_without_a_terminal() {
    let code = "const $p = terminal::progress(3, 'Copying');\n$p.advance();\n$p.advance(2);\n$p.finish();\noutln($p.done);\nconst $s = terminal::progress(null);\n$s.advance();\n$s.finish();\noutln($s.done);";
    assert_eq!(run("", code), "3\n1\n");
}

#[test]
fn tables_line_up_their_columns() {
    assert_eq!(
        run("", "terminal::table([['ñandú', 12], ['b', null], ['c', 3, 'x']], ['name', 'size']);"),
        "name   size\nñandú  12\nb\nc      3     x\n"
    );
    assert_eq!(run("", "terminal::table([{'a' => 1, 'b' => 'xx'}, {'a' => 22}]);"), "a   b\n1   xx\n22\n");
}

#[test]
fn the_pager_writes_without_a_terminal() {
    assert_eq!(run("", "terminal::pager('a\\nb');"), "a\nb\n");
}

#[test]
fn keys_and_editors_need_a_terminal() {
    assert!(error("", "terminal::readKey();").contains("IOError: there's no terminal to read a key from"));
    assert!(error("", "terminal::edit('x');").contains("IOError: can't open an editor without a terminal"));
}

#[test]
fn the_native_part_is_hidden() {
    assert!(error("", "use std/terminal/native;").contains("there's no module `std/terminal/native`"));
}

#[test]
fn full_screen_without_a_terminal() {
    // It only runs the body, and puts nothing on the screen
    assert_eq!(run("", "terminal::fullScreen(() -> outln('drawn'));\noutln('after');"), "drawn\nafter\n");
    // An exception in it goes on
    assert!(error("", "terminal::fullScreen(() { throw ValueError('boom'); });").contains("boom"));
    // `moveTo()` and `clearLine()` write their escape codes, from 0
    assert_eq!(run("", "terminal::moveTo(0, 0);\nterminal::moveTo(4, 2);\nterminal::clearLine();"), "\x1b[1;1H\x1b[3;5H\x1b[K");
    assert!(error("", "terminal::moveTo(-1, 0);").contains("from 0"));
    // `readKey(timeout:)` gives `?string`, and still needs a terminal
    assert!(error("", "const ?string $k = terminal::readKey(timeout: 0.1);").contains("no terminal"));
    assert!(error("", "const string $k = terminal::readKey(timeout: 0.1);").contains("null"));
    assert!(error("", "terminal::readKey(timeout: -1);").contains("0 seconds or more"));
}
