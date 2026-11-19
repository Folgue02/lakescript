//! Phase 2 features: anonymous functions, `if` and `switch` as values,
//! destructuring, `.is()` and `.as()`.

use lakescript::{CapturedOutput, Engine};

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
        Ok(()) => panic!("expected an error:\n{source}"),
        Err(e) => e.render("test.lks"),
    }
}

#[test]
fn anonymous_functions() {
    let script = "
        const $describe = (string $name, number $age) {
            return f\"{$name} is {$age}\";
        };
        outln($describe('Daniel', 23));

        const $double = ($n) = $n * 2;
        outln($double(21));

        number apply((number) -> number $f, number $x) = $f($x);
        outln(apply(($x) = $x + 1, 1));
        outln(($x) = $x);
    ";
    assert_eq!(run(script), "Daniel is 23\n42\n2\n<function anonymous>\n");
}

#[test]
fn closures_work_like_javascript() {
    let script = "
        (() -> number) makeCounter() {
            let $count = 0;
            return () = ++$count;
        }
        const $next = makeCounter();
        $next();
        outln($next());

        const $other = makeCounter();
        outln($other());

        // They see later changes to the variables they use
        let $greeting = 'Hello';
        const $greet = ($name) = f\"{$greeting} {$name}\";
        $greeting = 'Bye';
        outln($greet('you'));

        // Each loop pass has its own variable
        let $fns = [];
        for ($i in 0..3) $fns << () = $i;
        for ($f in $fns) out($f());
        outln();
    ";
    assert_eq!(run(script), "2\n1\nBye you\n012\n");
}

#[test]
fn parenthesized_expressions_are_not_lambdas() {
    assert_eq!(run("const $a = 2; outln(($a) == 2); outln(($a + 1) * 2);"), "true\n6\n");
}

#[test]
fn if_expressions() {
    let script = "
        const $count = 3;
        const $label = if ($count > 1) 'items' else 'item';
        outln($label);

        const $bytes = 2_500_000;
        const $size = if ($bytes > 1_000_000) {
            const $mb = $bytes / 1_000_000;
            yield f\"{$mb} MB\";
        } else if ($bytes > 1_000) 'KB' else 'bytes';
        outln($size);
    ";
    assert_eq!(run(script), "items\n2.5 MB\n");
    assert!(error("const $x = if (true) 1;").contains("needs an `else`"));
    assert!(error("const $x = if (true) { outln(1); } else 2;").contains("must give its value with `yield`"));
}

#[test]
fn switch_statements() {
    let script = "
        void describe(any $v) {
            switch ($v) {
                case 1 -> outln('one');
                case 2, 3 -> outln('two or three');
                case 4..10 -> {
                    outln('four to nine');
                }
                case :admin -> outln('admin');
                case 'x' -> outln('x');
                default -> outln('something else');
            }
        }
        for ($v in [1, 3, 7, 10, :admin, 'x', true]) describe($v);
    ";
    assert_eq!(run(script), "one\ntwo or three\nfour to nine\nsomething else\nadmin\nx\nsomething else\n");
}

#[test]
fn switch_without_default_does_nothing() {
    assert_eq!(run("switch (5) { case 1 -> outln('one'); } outln('done');"), "done\n");
}

#[test]
fn switch_expressions() {
    let script = "
        string stage(number $age) = switch ($age) {
            case 0..13 -> 'child';
            case 13..18 -> 'teenager';
            default -> 'adult';
        };
        outln(stage(5));
        outln(stage(15));
        outln(stage(40));

        const $price = switch (:pear) {
            case :apple -> 1;
            default -> {
                const $base = 2;
                yield $base * 1.5;
            }
        };
        outln($price);

        const $yes = switch (true) {
            case true -> 'yes';
            case false -> 'no';
        };
        outln($yes);
    ";
    assert_eq!(run(script), "child\nteenager\nadult\n3\nyes\n");
    assert!(error("const $x = switch (1) { case 1 -> 'a'; };").contains("add a `default`"));
}

#[test]
fn switch_on_types() {
    let script = "
        void show(any $value) {
            switch ($value) {
                case string -> outln($value.upper());
                case number -> outln($value * 2);
                case list<number> -> outln(f\"{$value.length()} numbers\");
                case list<any> -> outln('a list');
                case ?bool -> outln('bool or null');
                default -> outln('something else');
            }
        }
        for ($v in ['hi', 21, [1, 2], [1, 'a'], null, {}]) show($v);
    ";
    assert_eq!(run(script), "HI\n42\n2 numbers\na list\nbool or null\nsomething else\n");
}

#[test]
fn yield_outside_a_value_block() {
    assert!(error("yield 1;").contains("`yield` can only be used"));
    assert!(error("void f() { yield 1; } f();").contains("`yield` can only be used"));
}

#[test]
fn destructuring() {
    let script = "
        let [$first, $second] = ['Daniel', 'Folgue'];
        outln(f\"{$first} {$second}\");
        let [$head, ...$rest] = [1, 2, 3, 4];
        outln(f\"{$head} {$rest}\");
        const [$only, ...$none] = [1];
        outln($none);

        const $pairs = [['Daniel', 23], ['Folgue', 24]];
        for ([$name, $age] in $pairs)
            outln(f\"{$name}: {$age}\");
    ";
    assert_eq!(run(script), "Daniel Folgue\n1 [2, 3, 4]\n[]\nDaniel: 23\nFolgue: 24\n");
    assert!(error("let [$a, $b] = [1];").contains("with 2 elements, but it has 1"));
    assert!(error("let [$a] = 'x';").contains("only lists can be destructured"));
    assert!(error("const [$a] = [1]; $a = 2;").contains("is a constant"));
}

#[test]
fn is_and_as() {
    let script = "
        const $data = {'name' => 'Daniel', 'tags' => ['a', 'b'], 'age' => 23};
        const $name = $data['name'].as(string);
        outln($name.upper());
        outln($data['tags'].is(list<string>));
        outln($data['tags'].is(list<number>));
        outln($data.is(map<string, any>));
        outln($data['missing'].is(?string));
        outln($data['missing']?.as(string));
        outln(5.is(number));
    ";
    assert_eq!(run(script), "DANIEL\ntrue\nfalse\ntrue\ntrue\nnull\ntrue\n");
    assert!(error("outln({'a' => 1}['a'].as(string));").contains("`.as(string)` failed: the value is `1`"));
    assert!(error("outln(1.is((number) -> number));").contains("function types can't be checked"));
    assert!(error("outln(1.is(Thing));").contains("unknown type `Thing`"));
}
