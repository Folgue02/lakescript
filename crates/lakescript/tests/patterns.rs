//! Regexes, `-like`, `-match` and the path tests.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

fn eval(expr: &str) -> String {
    run(&format!("outln({expr});")).trim_end_matches('\n').to_string()
}

fn error(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(source) {
        Ok(()) => panic!("expected an error:\n{source}"),
        Err(e) => e.message,
    }
}

#[test]
fn like() {
    assert_eq!(eval("'notes.txt' -like '*.txt'"), "true");
    assert_eq!(eval("'notes.txt' -notlike '*.log'"), "true");
    assert_eq!(eval("'file1.log' -like 'file?.log'"), "true");
    assert_eq!(eval("'cat' -like '[bc]at'"), "true");
    assert!(error("outln(5 -like '*');").contains("needs two strings"));
}

#[test]
fn match_operator() {
    assert_eq!(eval("'123' -match %r\"^\\d+$\""), "true");
    assert_eq!(eval("'12a' -match %r\"^\\d+$\""), "false");
    assert_eq!(eval("'HELLO' -match %r\"hello\"i"), "true");
    assert_eq!(eval("'abc' -notmatch %r\"\\d\""), "true");
    // A string on the right is compiled as a regex
    assert_eq!(run("const $pattern = '^a.c$'; outln('abc' -match $pattern);"), "true\n");
    // Slashes don't need escaping
    assert_eq!(eval("'/usr/local/bin/lks' -match %r\"^/usr/local/bin/\""), "true");
    assert!(error("outln('a' -match %r\"(\");").contains("invalid regex"));
}

#[test]
fn regex_values() {
    assert_eq!(eval("%r\"^\\d+$\"i"), "%r\"^\\d+$\"i");
    assert_eq!(eval("%r\"a+\"i.pattern()"), "a+");
    assert_eq!(eval("%r\"a+\"i.flags()"), "i");
    assert_eq!(eval("%r\"a\".is(regex)"), "true");
    assert_eq!(eval("%r\"a\".typeName()"), "regex");
}

#[test]
fn regex_in_a_loop_is_compiled_once() {
    // The same literal gives the same compiled regex
    assert_eq!(run("let $all = []; for ($i in 0..2) $all << %r\"x\"; outln($all[0] == $all[1]);"), "true\n");
}

#[test]
fn string_methods_with_regexes() {
    assert_eq!(eval("'v1.2'.match(%r\"(\\d+)\\.(\\d+)\")"), "['1.2', '1', '2']");
    assert_eq!(eval("'abc'.match(%r\"\\d\")"), "null");
    assert_eq!(eval("'2026-11-18'.match(%r\"^(\\d+)\")?[1]"), "2026");
    assert_eq!(eval("'a1 b2'.matchAll(%r\"\\w(\\d)\")"), "[['a1', '1'], ['b2', '2']]");
    assert_eq!(eval("'a1b22c'.replace(%r\"\\d+\", '#')"), "a#b#c");
    assert_eq!(eval("'John Smith'.replace(%r\"(\\w+) (\\w+)\", '$2, $1')"), "Smith, John");
    assert_eq!(eval("'a1b2'.replaceFirst(%r\"\\d\", '_')"), "a_b2");
    assert_eq!(eval("'a1b2'.replaceLast(%r\"\\d\", '_')"), "a1b_");
    assert_eq!(eval("'a1b22c'.remove(%r\"\\d\")"), "abc");
    assert_eq!(eval("'a, b;c'.split(%r\"[,;]\\s*\")"), "['a', 'b', 'c']");
    assert_eq!(eval("'1+1'.escapeRegex()"), "1\\+1");
    assert_eq!(eval("'x.y' -match '1+1'.escapeRegex()"), "false");
}

#[test]
fn path_tests() {
    let dir = std::env::temp_dir().join(format!("lks-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.txt");
    std::fs::write(&file, "hi").unwrap();
    let (dir_s, file_s) = (dir.display().to_string(), file.display().to_string());

    let script = format!(
        "const $dir = '{dir_s}';
        outln(-d $dir);
        outln(-f $dir);
        outln(-f $dir + '/notes.txt');
        outln(!-f $dir + '/missing.txt');
        outln(-r '{file_s}' && -w '{file_s}');
        outln(-x '{file_s}');
        outln(-f $dir + '/notes.txt' && $dir -like '*lks-test*');"
    );
    assert_eq!(run(&script), "true\nfalse\ntrue\ntrue\ntrue\nfalse\ntrue\n");
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(error("outln(-f 5);").contains("needs a path string"));
}
