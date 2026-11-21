//! `assert`, `std/test` and running the tests of a file *(see
//! `ideas/tooling/testing.md`)*.

use lakescript::terminal::Paint;
use lakescript::{CapturedOutput, Engine, render_result, render_summary};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("test.lks"), output.text());
    }
    output.text()
}

fn error(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(source) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.render("test.lks"),
    }
}

#[test]
fn assert_passes_quietly() {
    assert_eq!(run("assert 1 + 1 == 2;\nassert [1, 2].contains(2);\nassert true;\noutln('done');"), "done\n");
    // It runs its condition once
    assert_eq!(run("let $n = 0;\nnumber next() {\n    $n += 1;\n    return $n;\n}\nassert next() == 1;\nassert next() < 3;\noutln($n);"), "2\n");
}

#[test]
fn a_failed_comparison_shows_both_sides() {
    assert_eq!(
        error("assert 'abc'.upper() == 'abd';"),
        "test.lks:1:1: error: AssertionError: assert failed: 'abc'.upper() == 'abd'\n  left:  'ABC'\n  right: 'abd'"
    );
    assert_eq!(error("const $n = 3;\nassert $n\n    <= 2;"), "test.lks:2:1: error: AssertionError: assert failed: $n <= 2\n  left:  3\n  right: 2");
    assert!(error("assert [1, 'a'] == [1, 'b'];").ends_with("left:  [1, 'a']\n  right: [1, 'b']"));
    assert!(error("assert null == {'a' => :b};").ends_with("left:  null\n  right: { 'a' => :b }"));
    // `.equals()` shows both sides too, and `==` on lists with the same values says why it failed
    assert!(error("assert [1, 2].equals([1, 3]);").ends_with("assert failed: [1, 2].equals([1, 3])\n  left:  [1, 2]\n  right: [1, 3]"));
    assert!(error("assert [1, 2] == [1, 2];").ends_with("They're two lists with the same values: `==` checks if they're the same list, and `.equals()` compares their values"));
    assert!(error("assert {'a' => 1} == {'a' => 1};").contains("two maps with the same values"));
    assert!(!error("assert [1] == [2];").contains("same values"));
    assert_eq!(run("const $l = [1];\nassert $l == $l;\nassert [1, [2]].equals([1, [2]]);\noutln('ok');"), "ok\n");
    // Other conditions show only their code
    assert_eq!(error("assert [1].empty?();"), "test.lks:1:1: error: AssertionError: assert failed: [1].empty?()");
    assert_eq!(error("assert !true || false;"), "test.lks:1:1: error: AssertionError: assert failed: !true || false");
}

#[test]
fn it_is_an_exception_like_others() {
    let out = run("try {\n    assert 1 == 2;\n} catch (AssertionError $e) {\n    outln($e.message().lines()[0]);\n    outln($e.line());\n}\ntry { assert false; } catch (Exception $e) { outln($e.typeName()); }");
    assert_eq!(out, "assert failed: 1 == 2\n2\nAssertionError\n");
    // Inside functions, loops and lambdas
    assert!(error("void f(number $x) {\n    for ($i in 0..$x) {\n        assert $i < 2;\n    }\n}\nf(5);").contains("test.lks:3:9: error: AssertionError: assert failed: $i < 2\n  left:  2"));
    assert!(error("const $check = (string $s) {\n    assert $s.length() > 0;\n};\n$check('');").contains("left:  0"));
}

#[test]
fn the_checker_knows_assert() {
    assert!(error("assert 5;").contains("this must be a bool, but it's a number"));
    // After it, the condition holds
    assert_eq!(run("?string $s = ['a'].first();\nassert $s != null;\noutln($s.upper());"), "A\n");
    assert!(error("?string $s = null;\noutln($s.upper());").contains("null"));
    // It's a reserved word
    assert!(error("void assert() {}").contains("assert"));
    assert!(error("assert 1 == 1").contains("after the condition of `assert`"));
}

#[test]
fn throws_gives_the_exception() {
    let out = run("use std/test;\nconst $e = test::throws(() -> 'x'.toNumber());\noutln($e.typeName());\nconst $f = test::throws(() { throw ValueError('mine'); });\noutln($f.message());");
    assert_eq!(out, "ValueError\nmine\n");
    assert_eq!(error("use std/test;\ntest::throws(() -> 5);"), "test.lks:2:1: error: AssertionError: the code given to `test::throws` threw nothing");
    assert!(error("use std/test;\ntest::throws(5);").contains("`$code` of `throws` must be a () -> any"));
    // `exit()` isn't caught
    let mut engine = Engine::with_output(CapturedOutput::new());
    assert_eq!(engine.run("use std/test;\ntest::throws(() -> exit(3));").unwrap().code(), 3);
}

fn tests_in(name: &str, source: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("lks-testing-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tests.lks");
    std::fs::write(&file, source).unwrap();
    (dir, file)
}

const TESTS: &str = "\
use std/test;
let $runs = 0;
outln('the top level');

void testFirst() {
    $runs += 1;
    assert 1 + 1 == 2;
}

void testSecond() {
    outln(f\"printed by the second, after {$runs} runs\");
    errln('and to stderr');
    assert [1, 2].length() == 3;
}

void testThird() {
    [1].removeAt(4);
}

void testFourth() {
    exit(4);
}

void testWithArguments(number $n) {}
void helper() {}
";

#[test]
fn running_the_tests_of_a_file() {
    let (dir, file) = tests_in("run", TESTS);
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    let results = engine.run_tests(&file, None).unwrap();
    assert_eq!(output.text(), "the top level\n");
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["testFirst", "testSecond", "testThird", "testFourth"]);
    assert!(results[0].passed() && results[0].output.is_empty());

    let second = results[1].failure.as_ref().unwrap();
    assert_eq!(second.message, "assert failed: [1, 2].length() == 3\n  left:  2\n  right: 3");
    assert_eq!((second.span.line, second.span.column), (13, 5));
    assert_eq!(results[1].output, "printed by the second, after 1 runs\nand to stderr\n");
    assert!(results[2].failure.as_ref().unwrap().message.starts_with("IndexError: "));
    assert_eq!(results[3].failure.as_ref().unwrap().message, "the test called `exit(4)`");

    let shown = render_result(&results[1], "tests.lks", Paint(false));
    assert_eq!(
        shown,
        "        FAIL testSecond\n\
         \x20            tests.lks:13:5: assert failed: [1, 2].length() == 3\n\
         \x20              left:  2\n\
         \x20              right: 3\n\
         \x20            It printed:\n\
         \x20              printed by the second, after 1 runs\n\
         \x20              and to stderr\n"
    );
    assert_eq!(render_result(&results[0], "tests.lks", Paint(false)), "          ok testFirst\n");
    assert_eq!(render_summary(&results, 0.5, Paint(false)), "    Finished 1 passed, 3 failed in 0.50 s\n");

    // Only the ones whose name has the filter
    let mut engine = Engine::with_output(CapturedOutput::new());
    let results = engine.run_tests(&file, Some("ir")).unwrap();
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["testFirst", "testThird"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_fails_runs_no_test() {
    let (dir, file) = tests_in("broken", "void testA() {}\nthrow ValueError('at the top');\n");
    let e = Engine::with_output(CapturedOutput::new()).run_tests(&file, None).err().unwrap();
    assert_eq!(e.message, "ValueError: at the top");
    std::fs::write(&file, "void testA() {\n    number $n = 'x';\n}\n").unwrap();
    let e = Engine::with_output(CapturedOutput::new()).run_tests(&file, None).err().unwrap();
    assert!(e.message.contains("`$n` is a number"), "{}", e.message);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn errors_in_modules_name_their_file() {
    let (dir, file) = tests_in("module", "use helper;\nvoid testA() {\n    helper::check(5);\n}\n");
    std::fs::write(dir.join("helper.lks"), "pub void check(number $n) {\n    assert $n == 4;\n}\n").unwrap();
    let results = Engine::with_output(CapturedOutput::new()).run_tests(&file, None).unwrap();
    let failure = results[0].failure.as_ref().unwrap();
    assert!(failure.file.as_deref().is_some_and(|f| f.ends_with("helper.lks")), "{:?}", failure.file);
    assert_eq!(failure.span.line, 2);
    let _ = std::fs::remove_dir_all(&dir);
}
