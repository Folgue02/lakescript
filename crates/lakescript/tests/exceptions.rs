//! Exceptions, `try`/`catch`/`finally`, `throw` and `try` as an expression.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_type_check(false);
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("test.lks"), output.text());
    }
    output.text()
}

fn error(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_type_check(false);
    match engine.run(source) {
        Ok(_) => panic!("expected an error:\n{source}"),
        Err(e) => e.render("test.lks"),
    }
}

#[test]
fn catch_by_type() {
    let script = "
        void check(any $value) {
            try {
                outln($value.toNumber() * 2);
            } catch (ValueError $e) {
                outln(f\"bad number: {$e.message()}\");
            } catch (TypeError $e) {
                outln('not a string');
            }
        }
        check('21');
        check('abc');
        check(:x);
    ";
    assert_eq!(run(script), "42\nbad number: 'abc' isn't a number\nnot a string\n");
}

#[test]
fn parents_catch_their_children() {
    let script = "
        try {
            throw FileNotFound('config.txt');
        } catch (IOError $e) {
            outln(f\"{$e.typeName()}: {$e.message()}\");
            outln($e.is(IOError) && $e.is(Exception) && !$e.is(ValueError));
        }
    ";
    assert_eq!(run(script), "FileNotFound: config.txt\ntrue\n");
}

#[test]
fn catch_everything_and_order() {
    let script = "
        try {
            [1].removeAt(5);
        } catch (ValueError $e) {
            outln('wrong one');
        } catch ($e) {
            outln($e);
        }
    ";
    assert_eq!(run(script), "IndexError: index 5 is outside the list, which has 1 elements\n");
}

#[test]
fn uncaught_exceptions_keep_going_up() {
    let script = "
        void inner() { throw ValueError('deep'); }
        void outer() {
            try { inner(); } catch (TypeError $e) { outln('not this one'); }
        }
        try { outer(); } catch (ValueError $e) { outln(f\"caught {$e.message()} from line {$e.line()}\"); }
    ";
    assert_eq!(run(script), "caught deep from line 2\n");
}

#[test]
fn finally_always_runs() {
    let script = "
        number f() {
            try {
                return 1;
            } finally {
                outln('finally after return');
            }
        }
        outln(f());

        try {
            try {
                throw ValueError('x');
            } finally {
                outln('finally after throw');
            }
        } catch ($e) {
            outln('caught outside');
        }

        for ($i in 0..3) {
            try {
                if ($i == 1) break;
            } finally {
                out($i);
            }
        }
        outln();
    ";
    assert_eq!(run(script), "finally after return\n1\nfinally after throw\ncaught outside\n01\n");
}

#[test]
fn errors_have_types() {
    let cases = [
        ("1 % 0;", "ArithmeticError"),
        ("5.div(0);", "ArithmeticError"),
        ("'a' + 1;", "TypeError"),
        ("[1].removeAt(3);", "IndexError"),
        ("let $l = [1]; $l[3] = 2;", "IndexError"),
        ("'a'.codePoint(3);", "IndexError"),
        ("'abc'.toNumber();", "ValueError"),
        ("const $m = {'a' => 1}; $m['a'].as(string);", "TypeError"),
        ("'a'.padStart('x');", "TypeError"),
        ("%r\"(\";", "ValueError"),
        ("if (1) {}", "TypeError"),
        ("void f(number $a) {} f();", "TypeError"),
    ];
    for (code, kind) in cases {
        let script = format!("try {{ {code} }} catch ($e) {{ outln($e.typeName()); }}");
        assert_eq!(run(&script), format!("{kind}\n"), "{code}");
    }
}

#[test]
fn rethrowing_keeps_the_original_line() {
    let err = error("try {\n  throw ValueError('first');\n} catch ($e) {\n  throw $e;\n}");
    assert!(err.starts_with("test.lks:2:3: error: ValueError: first"), "{err}");
}

#[test]
fn uncaught_errors_show_their_type() {
    let err = error("const $n = 'x'.toNumber();");
    assert!(err.contains("error: ValueError: 'x' isn't a number"), "{err}");
}

#[test]
fn only_exceptions_can_be_thrown() {
    assert!(error("throw 'oops';").contains("only exceptions can be thrown"));
}

#[test]
fn try_needs_catch_or_finally() {
    assert!(error("try { outln(1); }").contains("needs a `catch` or a `finally`"));
    assert!(error("catch ($e) {}").contains("must come after a `try`"));
}

#[test]
fn try_expressions() {
    let script = "
        const $ok = try '42'.toNumber();
        const $bad = try 'x'.toNumber();
        outln($ok);
        outln($bad);
        outln($ok.isOk());
        outln($bad.isError());
        outln($bad.orElse(0));
        outln($ok.orElse(0));
        outln($bad.orElseGet(() -> -1));
        outln($ok.map(($n) -> $n + 1));
        outln($bad.map(($n) -> $n + 1).isError());
        outln($bad.orNull());
        outln($bad.error().is(ValueError));
        outln($ok.error());
        outln((try 'x'.toNumber()).orElse(7));
        outln($ok.is(result));
    ";
    assert_eq!(
        run(script),
        "ok(42)\nerror(ValueError: 'x' isn't a number)\ntrue\ntrue\n0\n42\n-1\nok(43)\ntrue\nnull\ntrue\nnull\n7\ntrue\n"
    );
    assert!(error("(try 'x'.toNumber()).throw();").contains("ValueError: 'x' isn't a number"));
}

#[test]
fn try_applies_to_the_whole_expression() {
    // Without parentheses, `.orElse()` is part of what `try` runs
    assert_eq!(run("outln(try 'x'.toNumber().toString());"), "error(ValueError: 'x' isn't a number)\n");
}

#[test]
fn throw_as_an_expression() {
    let script = "
        string name(?string $n) = $n ?? throw ValueError('no name');
        outln(name('Ana'));
        outln((try name(null)).isError());

        string describe(any $v) = switch ($v) {
            case string -> $v;
            case bool -> throw TypeError('no bools');
            default -> 'other';
        };
        outln(describe('x'));
        try { describe(true); } catch (TypeError $e) { outln($e.message()); }

        const $n = if (1 > 2) 1 else throw ValueError('never');
    ";
    assert_eq!(error(script).contains("never"), true);
    let script = script.replace("const $n = if (1 > 2) 1 else throw ValueError('never');", "");
    assert_eq!(run(&script), "Ana\ntrue\nx\nno bools\n");
}
