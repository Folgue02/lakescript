//! Runs scripts through the public API, like an embedding program would.

use lakescript::{CapturedOutput, Engine, Value};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("test.lks"), output.text());
    }
    output.text()
}

/// Prints one expression.
fn eval(expr: &str) -> String {
    run(&format!("outln({expr});")).trim_end_matches('\n').to_string()
}

fn error(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(source) {
        Ok(()) => panic!("expected an error:\n{source}"),
        Err(e) => e.render("test.lks"),
    }
}

#[test]
fn hello() {
    assert_eq!(run("outln('Hello'); out('a'); out('b'); outln();"), "Hello\nab\n");
}

#[test]
fn numbers() {
    assert_eq!(eval("1 + 2 * 3"), "7");
    assert_eq!(eval("5 / 2"), "2.5");
    assert_eq!(eval("4 / 2"), "2");
    assert_eq!(eval("5 % 2"), "1");
    assert_eq!(eval("-5 % 3"), "-2");
    assert_eq!(eval("5.5 % 2"), "1.5");
    assert_eq!(eval("2 ** 10"), "1024");
    assert_eq!(eval("2 ** -1"), "0.5");
    assert_eq!(eval("-2 ** 2"), "-4");
    assert_eq!(eval("2 ** 3 ** 2"), "512");
    assert_eq!(eval("0.1 + 0.2"), "0.30000000000000004");
    assert_eq!(eval("1 == 1.0"), "true");
    assert_eq!(eval("9223372036854775807 + 1"), "9.223372036854776e18");
    assert_eq!(eval("1 / 0"), "Infinity");
    assert_eq!(eval("-1 / 0"), "-Infinity");
    assert_eq!(eval("0 / 0 == 0 / 0"), "false");
    assert_eq!(eval("1_000_000"), "1000000");
    assert_eq!(eval("0xFF"), "255");
}

#[test]
fn bitwise() {
    assert_eq!(eval("6 & 3"), "2");
    assert_eq!(eval("6 | 3"), "7");
    assert_eq!(eval("6 ^ 3"), "5");
    assert_eq!(eval("~6"), "-7");
    assert_eq!(eval("1 << 3"), "8");
    assert_eq!(eval("-16 >> 2"), "-4");
    assert_eq!(eval("-16 >>> 60"), "15");
    assert_eq!(eval("4.0 & 1"), "0");
    assert!(error("outln(4.5 & 1);").contains("whole numbers"));
}

#[test]
fn division_by_zero() {
    assert!(error("outln(5 % 0);").contains("remainder by zero"));
    assert!(error("outln(5.5 % 0);").contains("remainder by zero"));
}

#[test]
fn booleans_and_comparisons() {
    assert_eq!(eval("1 < 2 && 2 <= 2"), "true");
    assert_eq!(eval("!true || false"), "false");
    assert_eq!(eval("true & false"), "false");
    assert_eq!(eval("true ^ true"), "false");
    assert_eq!(eval("'apple' < 'banana'"), "true");
    assert!(error("outln(1 && true);").contains("must be bools"));
    assert!(error("outln(1 < 'a');").contains("compares two numbers or two strings"));
}

#[test]
fn short_circuit() {
    let script = "
        let $calls = 0;
        bool check() { $calls++; return true; }
        const $a = false && check();
        const $b = true || check();
        outln($calls);
    ";
    assert_eq!(run(script), "0\n");
}

#[test]
fn strings() {
    assert_eq!(eval("'Hello ' + 'world'"), "Hello world");
    assert_eq!(eval("f\"{1 + 1} is {'two'.upper()}\""), "2 is TWO");
    assert_eq!(eval("'año'.length()"), "3");
    assert_eq!(eval("'hello'[0]"), "h");
    assert_eq!(eval("'hello'[-3..]"), "llo");
    assert_eq!(eval("'hello'[1..=2]"), "el");
    assert!(error("outln('Age: ' + 5);").contains("f-string"));
}

#[test]
fn null_operators() {
    assert_eq!(eval("null ?? 'default'"), "default");
    assert_eq!(eval("'set' ?? 'default'"), "set");
    assert_eq!(run("?string $n = null; outln($n?.upper()); outln($n?['x']);"), "null\nnull\n");
}

#[test]
fn variables() {
    assert_eq!(run("let $a = 1; $a += 2; $a *= 10; outln($a);"), "30\n");
    assert_eq!(run("let $i = 5; outln($i++); outln($i); outln(++$i);"), "5\n6\n7\n");
    assert!(error("const $a = 1; $a = 2;").contains("is a constant"));
    assert!(error("let $a = 1; let $a = 2;").contains("already exists"));
    assert!(error("let $a = 1; if (true) { let $a = 2; }").contains("already exists"));
    assert!(error("$b = 1;").contains("isn't declared"));
    assert!(error("outln($nope);").contains("isn't declared"));
}

#[test]
fn const_freezes_only_the_variable() {
    assert_eq!(run("const $f = ['a']; $f << 'b'; outln($f);"), "['a', 'b']\n");
}

#[test]
fn lists() {
    assert_eq!(eval("[1, 'a', [true]]"), "[1, 'a', [true]]");
    assert_eq!(run("let $l = [1, 2, 3]; $l[0] = 10; $l << 4 << 5; outln($l); outln($l[-1]);"), "[10, 2, 3, 4, 5]\n5\n");
    assert_eq!(eval("[1, 2, 3, 4][1..3]"), "[2, 3]");
    assert_eq!(eval("[1, 2, 3][0..-1]"), "[1, 2]");
    assert_eq!(eval("[1, 2] == [1, 2]"), "false");
    assert_eq!(eval("[1, 2].equals([1, 2])"), "true");
    assert_eq!(eval("%w[a b].join('-')"), "a-b");
    assert!(error("outln([1, 2][5]);").contains("outside the list"));
    assert!(error("let $l = [1]; $l[3] = 1;").contains("outside the list"));
}

#[test]
fn maps() {
    let script = "
        let $scores = {'Daniel' => 23, 'Folgue' => 24};
        $scores['James'] = 30;
        outln($scores);
        outln($scores['Nobody']);
        outln($scores.keys());
        const $m = {1 => 'one'};
        outln($m[1.0]);
        outln({:a => 1});
    ";
    assert_eq!(
        run(script),
        "{ 'Daniel' => 23, 'Folgue' => 24, 'James' => 30 }\nnull\n['Daniel', 'Folgue', 'James']\none\n{ :a => 1 }\n"
    );
}

#[test]
fn in_operator() {
    assert_eq!(eval("'a' -in ['a', 'b']"), "true");
    assert_eq!(eval("[1, 2] -in [[1, 2]]"), "true");
    assert_eq!(eval("'x' -notin {'x' => 1}"), "false");
    assert_eq!(eval("5 -in 1..10"), "true");
    assert_eq!(eval("10 -in 1..10"), "false");
}

#[test]
fn conditionals() {
    let script = "
        const $age = 20;
        if ($age > 18 && $age < 23) outln('young adult');
        else outln('other');
        if ($age < 10) {
            outln('child');
        } else if ($age < 30) {
            outln('under 30');
        }
        outln($age > 1 ? 'items' : 'item');
    ";
    assert_eq!(run(script), "young adult\nunder 30\nitems\n");
    assert!(error("if (1) outln('x');").contains("must be a bool"));
}

#[test]
fn while_loops() {
    assert_eq!(run("let $i = 0; while ($i < 3) { $i++; } outln($i);"), "3\n");
    let script = "
        let $i = 0;
        while (true) {
            $i++;
            if ($i == 2) continue;
            if ($i > 4) break;
            out($i);
        }
        outln();
    ";
    assert_eq!(run(script), "134\n");
}

#[test]
fn for_loops() {
    assert_eq!(run("for ($i in 1..5) out($i); outln();"), "1234\n");
    assert_eq!(run("for ($i in 1..=5) out($i); outln();"), "12345\n");
    assert_eq!(run("for ($i in 5..1) out($i); outln();"), "5432\n");
    assert_eq!(run("for ($i in (10..=0).step(5)) out($i); outln();"), "1050\n");
    assert_eq!(run("for ($i, $f in ['a', 'b']) outln(f\"{$i}: {$f}\");"), "0: a\n1: b\n");
    assert_eq!(run("for ($k => $v in {'a' => 1}) outln(f\"{$k}={$v}\");"), "a=1\n");
    assert_eq!(run("for ($k in {'a' => 1, 'b' => 2}) out($k); outln();"), "ab\n");
    assert_eq!(run("for ($c in 'abc') out($c.upper()); outln();"), "ABC\n");
    // Every pass has its own scope
    assert_eq!(run("for ($x in [1, 2]) { const $double = $x * 2; out($double); } outln();"), "24\n");
}

#[test]
fn functions() {
    let script = "
        string greet(string $name = 'user') = f\"Hello {$name}\";
        outln(greet());
        outln(greet('Folgue'));

        string createFile(string $name, string $extension) = f\"{$name}.{$extension}\";
        outln(createFile(extension: 'jpg', name: 'daniel'));
        outln(createFile('daniel', extension: 'png'));

        number sum(...number $numbers) {
            let $total = 0;
            for ($n in $numbers) $total += $n;
            return $total;
        }
        outln(sum());
        outln(sum(1, 2, 3));
        const $nums = [4, 5];
        outln(sum(...$nums));

        void nothing() {}
        outln(nothing());
    ";
    assert_eq!(run(script), "Hello user\nHello Folgue\ndaniel.jpg\ndaniel.png\n0\n6\n9\nnull\n");
}

#[test]
fn recursion() {
    let script = "
        number fib(number $n) = $n < 2 ? $n : fib($n - 1) + fib($n - 2);
        outln(fib(20));
    ";
    assert_eq!(run(script), "6765\n");
}

#[test]
fn functions_see_outer_variables() {
    let script = "
        const $path = 'users.json';
        let $loaded = 0;
        void load() {
            outln($path);
            $loaded++;
        }
        load();
        load();
        outln($loaded);
    ";
    assert_eq!(run(script), "users.json\nusers.json\n2\n");
}

#[test]
fn defaults_are_evaluated_on_every_call() {
    let script = "
        list<number> add(number $n, list<number> $to = []) { $to << $n; return $to; }
        outln(add(1));
        outln(add(2));
    ";
    assert_eq!(run(script), "[1]\n[2]\n");
}

#[test]
fn functions_as_values() {
    let script = "
        number double(number $n) = $n * 2;
        number apply((number) -> number $f, number $x) = $f($x);
        outln(apply(double, 21));
        outln(double);
    ";
    assert_eq!(run(script), "42\n<function double>\n");
}

#[test]
fn call_errors() {
    assert!(error("void f(number $a) {} f();").contains("needs a value for `$a`"));
    assert!(error("void f() {} f(1);").contains("got more"));
    assert!(error("void f(number $a) {} f(b: 1);").contains("needs a value for `$a`"));
    assert!(error("void f(number $a = 1) {} f(b: 1);").contains("no parameter named `b`"));
    assert!(error("outln(missing());").contains("no function named `missing`"));
    assert!(error("void f() {} void f() {}").contains("already exists"));
    assert!(error("outln(1, 2);").contains("takes one value"));
}

#[test]
fn deep_recursion_is_an_error_not_a_crash() {
    let err = error("number down(number $n) = down($n + 1); down(0);");
    assert!(err.contains("too many nested calls"), "{err}");
}

#[test]
fn flow_outside_loops_and_functions() {
    assert!(error("break;").contains("outside a loop"));
    assert!(error("return 1;").contains("outside a function"));
    assert!(error("void f() { break; } f();").contains("outside a loop"));
}

#[test]
fn errors_point_to_the_line() {
    let err = error("let $a = 1;\nlet $b = $a + 'x';");
    assert!(err.starts_with("test.lks:2:10: error:"), "{err}");
}

#[test]
fn engines_keep_state_between_runs() {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.run("let $count = 1; void bump() { $count++; }").unwrap();
    engine.run("bump(); outln($count);").unwrap();
    assert_eq!(output.text(), "2\n");
}

#[test]
fn host_functions() {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.register("double", |_, args| match args.as_slice() {
        [Value::Int(n)] => Ok(Value::Int(n * 2)),
        _ => Err("double takes a whole number".into()),
    });
    engine.run("outln(double(21));").unwrap();
    assert_eq!(output.text(), "42\n");
    let err = engine.run("double('x');").unwrap_err();
    assert_eq!(err.message, "double takes a whole number");
}

#[test]
fn unsupported_features_say_so() {
    assert!(error("const $c = %x\"ls\";").contains("aren't supported yet"));
}
