//! `never`, the return type of functions that never return, and `$x!`, which
//! says a value isn't `null` *(see `ideas/language/functions.md` and
//! `ideas/language/operators.md`)*.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

fn error(source: &str) -> String {
    match Engine::with_output(CapturedOutput::new()).run(source) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.message,
    }
}

const FAIL: &str = "never fail(string $message) = throw ValueError($message);\n";

#[test]
fn a_never_call_ends_its_block() {
    // It narrows, like `return`
    let code = format!("{FAIL}string name(?string $n) {{\n    if ($n == null) fail('no name');\n    return $n;\n}}\noutln(name('Ann'));");
    assert_eq!(run(&code), "Ann\n");
    // It ends a function that must return
    let code = format!("{FAIL}number positive(number $n) {{\n    if ($n > 0) {{\n        return $n;\n    }}\n    fail('not positive');\n}}\noutln(positive(2));");
    assert_eq!(run(&code), "2\n");
    // `exit()` is one
    assert_eq!(run("string name(?string $n) {\n    if ($n == null) exit(1);\n    return $n;\n}\noutln(name('Bo'));"), "Bo\n");
    // And so is a method
    let code = "class C {\n    never fail(string $m) = throw ValueError($m);\n    pub string get(?string $s) {\n        if ($s == null) @fail('none');\n        return $s;\n    }\n}\noutln(C().get('x'));";
    assert_eq!(run(code), "x\n");
}

#[test]
fn a_never_call_fits_any_type() {
    assert_eq!(run(&format!("{FAIL}const ?string $n = 'Ann';\nconst string $name = $n ?? fail('no name');\noutln($name);")), "Ann\n");
    assert_eq!(run(&format!("{FAIL}number f(number $n) = if ($n > 0) $n else fail('no');\noutln(f(3));")), "3\n");
    let thrown = format!("{FAIL}try {{ fail('boom'); }} catch (ValueError $e) {{ outln($e.message()); }}");
    assert_eq!(run(&thrown), "boom\n");
}

#[test]
fn a_never_function_cant_reach_its_end() {
    assert_eq!(error("never f() {\n    outln(1);\n}"), "`f` is `never`, so it can't reach its end: it must throw, call `exit()` or another `never` function");
    assert_eq!(error("never f(bool $b) {\n    if ($b) exit(1);\n}"), "`f` is `never`, so it can't reach its end: it must throw, call `exit()` or another `never` function");
    assert_eq!(error("never f() {\n    return;\n}"), "a `never` function can't return: it must throw, call `exit()` or another `never` function");
    assert!(error("never f() = 3;").contains("never"));
    // Every way through it ends
    run("never f(bool $b) {\n    if ($b) exit(1);\n    else throw ValueError('x');\n}\nnever g() {\n    while (true) {}\n}\nnever h() = f(true);\noutln('ok');");
}

#[test]
fn never_is_only_a_return_type() {
    assert_eq!(error("void g(never $p) {}"), "the parameter `$p` can't be `never`: it's only the return type of a function that never returns");
    assert_eq!(error("const $x = exit(1);"), "`$x` can't be `never`: it's only the return type of a function that never returns, and there's no value of it");
    assert!(error("outln(1.as(never));").contains("`never` isn't the type of any value"));
}

#[test]
fn non_null_gives_the_value() {
    assert_eq!(run("const $list = [1, 2];\noutln($list[0]! + 1);"), "2\n");
    assert_eq!(run("const $m = {'a' => ' x '};\noutln($m['a']!.trim());"), "x\n");
    let day = "class Day {\n    pub ?((string) -> any) @part2;\n    pub ((string) -> any) part() = @part2!;\n}\noutln(Day(($s) -> 2).part()('x'));";
    assert_eq!(run(day), "2\n");
    // `!=` right after a value is still not-equal
    assert_eq!(run("const $list = [1];\noutln($list[0]!=null);"), "true\n");
    assert_eq!(run("const $list = [1];\noutln($list[0]! == 1);"), "true\n");
}

#[test]
fn non_null_throws_a_null_error() {
    let caught = |code: &str| run(&format!("const $m = <string, string>{{}};\nconst $l = <number>[];\ntry {{\n    {code};\n}} catch (NullError $e) {{\n    outln($e.message());\n}}"));
    assert_eq!(caught("$m['b']!.trim()"), "`$m['b']` is null\n");
    assert_eq!(caught("$l[0]!"), "`$l[0]` is null\n");
    assert_eq!(caught("$l.first()!"), "`$l.first()` is null\n");
    assert_eq!(caught("($l[0] ?? null)!"), "the value is null\n");
    // A `NullError` is a `TypeError`
    assert_eq!(run("try {\n    <number>[][0]!;\n} catch (TypeError $e) {\n    outln($e.typeName());\n}"), "NullError\n");
}

#[test]
fn non_null_is_only_for_values_that_can_be_null() {
    assert_eq!(error("const $a = 1;\nconst $b = $a!;"), "this is a number, which can't be `null`, so `!` isn't needed");
    assert_eq!(error("outln(null!);"), "this is always `null`, so `!` would always throw");
    // `any` can be anything, `null` too
    assert_eq!(run("const any $a = 3;\noutln($a!);"), "3\n");
}

#[test]
fn there_is_no_not_double_equals() {
    assert_eq!(error("outln(1 !== 2);"), "there's no `!==`: write `!=` to compare, or `$a! == $b`, with a space, to say `$a` isn't `null`");
}
