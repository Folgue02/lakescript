//! What the compiler and the VM must get right besides what the other tests
//! cover: closures, leaving `try` blocks early, `yield` out of loops, and
//! calls that go deep.

use lakescript::{CapturedOutput, Engine, Outcome};

/// Runs a script, type checked like `lks` does.
fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

/// Runs a script without the type check, for what the checker would reject.
fn run_unchecked(source: &str) -> Result<String, String> {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_type_check(false);
    engine.run(source).map(|_| output.text()).map_err(|e| e.message)
}

#[test]
fn closures_see_later_changes_and_make_them() {
    let script = "
        let $count = 0;
        void bump() { $count += 1; }
        bump();
        bump();
        outln($count);
        type Counter = () -> number;
        Counter counter() {
            let $n = 0;
            return () = ++$n;
        }
        const $a = counter();
        const $b = counter();
        $a();
        $a();
        outln(f\"{$a()} {$b()}\");
    ";
    assert_eq!(run(script), "2\n3 1\n");
}

#[test]
fn every_pass_of_a_loop_has_its_own_variables() {
    let script = "
        const $later = <() -> number>[];
        for ($i in 1..4) {
            const $double = $i * 2;
            $later << () = $i + $double;
        }
        outln($later.map(($f) = $f()));
    ";
    assert_eq!(run(script), "[3, 6, 9]\n");
}

#[test]
fn closures_use_parameters_and_functions_around_them() {
    let script = "
        type Adder = (number) -> number;
        Adder adder(number $n) = ($x) = $x + $n;
        outln(adder(10)(5));
        void outer() {
            number fact(number $n) = if ($n < 2) 1 else $n * fact($n - 1);
            outln(fact(5));
        }
        outer();
    ";
    assert_eq!(run(script), "15\n120\n");
}

#[test]
fn defaults_see_the_parameters_before_them() {
    let script = "number f(number $a, number $b = $a * 2) = $a + $b;\noutln(f(1));\noutln(f(1, 1));\noutln(f(b: 5, a: 1));";
    assert_eq!(run(script), "3\n2\n6\n");
}

#[test]
fn finally_runs_when_a_try_is_left_early() {
    let script = "
        for ($i in 1..4) {
            try {
                if ($i == 2) continue;
                if ($i == 3) break;
                outln($i);
            } finally {
                outln(f\"finally {$i}\");
            }
        }
        number f() {
            try {
                return 1;
            } finally {
                outln('cleanup');
            }
        }
        outln(f());
    ";
    assert_eq!(run(script), "1\nfinally 1\nfinally 2\nfinally 3\ncleanup\n1\n");
}

#[test]
fn finally_runs_on_exit_but_catch_does_not_catch_it() {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    let outcome = engine.run("try {\n  try { exit(3); } catch ($e) { outln('caught'); }\n} finally {\n  outln('bye');\n}\noutln('after');");
    assert_eq!(outcome.unwrap(), Outcome::Exit(3));
    assert_eq!(output.text(), "bye\n");
}

#[test]
fn exceptions_cross_rust_code() {
    // Thrown in a function that `.map()` calls, and caught outside it
    let script = "
        try {
            [1, 2].map(($x) { if ($x == 2) throw ValueError('two'); return $x; });
        } catch (ValueError $e) {
            outln(f\"caught {$e.message()}\");
        }
        outln([1, 2].map(($x) = (try 10 % ($x - 1)).isError()));
    ";
    assert_eq!(run(script), "caught two\n[true, false]\n");
}

#[test]
fn an_exception_in_finally_replaces_the_first_one() {
    let script = "try {\n  try { throw ValueError('first'); } finally { throw IOError('second'); }\n} catch ($e) {\n  outln($e.message());\n}";
    assert_eq!(run(script), "second\n");
}

#[test]
fn yield_and_break_leave_loops_and_values() {
    let script = "
        const $first = if (true) {
            for ($x in [5, 6, 7]) {
                if ($x > 5) yield $x;
            }
            yield 0;
        } else 0;
        outln($first);
    ";
    assert_eq!(run(script), "6\n");
    // A `break` in the middle of an expression drops what the expression had started
    let script = "let $n = 0;\nwhile (true) {\n  $n += 1;\n  outln([$n, if ($n == 3) { break; } else $n * 10]);\n}\noutln($n);";
    assert_eq!(run_unchecked(script).unwrap(), "[1, 10]\n[2, 20]\n3\n");
}

#[test]
fn recursion_goes_deep_without_using_the_rust_stack() {
    let script = "number down(number $n) = if ($n == 0) 0 else 1 + down($n - 1);\noutln(down(10000));";
    assert_eq!(run(script), "10000\n");
}

#[test]
fn functions_can_be_called_before_they_are_declared_when_they_exist_by_then() {
    // The checker wants them in order, but without it, this works like before
    let script = "void a(number $n) { if ($n > 0) b($n - 1); else outln('done'); }\nvoid b(number $n) { a($n); }\na(3);";
    assert_eq!(run_unchecked(script).unwrap(), "done\n");
    assert!(run_unchecked("missing();").unwrap_err().contains("no function named `missing`"));
}

#[test]
fn errors_happen_when_their_line_runs() {
    // Declaring a variable twice is found while running, after what came before
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_type_check(false);
    let err = engine.run("outln('first');\nlet $a = 1;\nif (true) { let $a = 2; }").unwrap_err();
    assert!(err.message.contains("already exists"), "{}", err.message);
    assert_eq!(err.span.line, 3);
    assert_eq!(output.text(), "first\n");
}

#[test]
fn the_bytecode_can_be_listed() {
    let listing = Engine::with_output(CapturedOutput::new()).bytecode("number f(number $x) = $x + 1;\noutln(f(1));").unwrap();
    assert!(listing.contains("Call(1)") && listing.contains("script/0:f(x)"), "{listing}");
}
