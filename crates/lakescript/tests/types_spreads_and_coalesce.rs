//! Decided after cinder: `let` and `for` variables with a type, an index or
//! a key with destructuring in a `for`, `...` inside list literals, and a
//! nullable right side of `??`.

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

const ANIMALS: &str = "interface IAnimal {\n    string name();\n}\nclass Wolf {\n    implement IAnimal {\n        string name() = 'wolf';\n    }\n}\n";

#[test]
fn let_takes_a_type() {
    let code = "class P {\n    pub number @x;\n}\nlet ?P $best = null;\nfor ($x in [3, 1]) if ($best == null || $x > $best.x) $best = P($x);\noutln($best?.x);";
    assert_eq!(run(code), "3\n");
    assert_eq!(run("let number $total = 0;\n$total += 2;\noutln($total);"), "2\n");
    // The same checks as without `let`
    assert!(error("let number $n = 1;\n$n = 'a';").contains("number"));
    assert!(error("let string $s = 5;").contains("string"));
}

#[test]
fn a_for_variable_takes_a_type() {
    let code = format!("{ANIMALS}const $wolves = [Wolf()];\nfor (IAnimal $a in $wolves) outln($a.name());");
    assert_eq!(run(&code), "wolf\n");
    assert_eq!(run("for ($i, string $line in ['a', 'b']) outln(f\"{$i}{$line}\");"), "0a\n1b\n");
    assert_eq!(run("for ($k => number $v in {'a' => 1}) outln($v + 1);"), "2\n");
    // Checked against the elements
    let err = error("for (string $s in [1, 2]) outln($s);");
    assert!(err.contains("`$s` is a string, but the elements are a number"), "{err}");
    // A key of a map can't have one
    assert!(error("for (string $k => $v in {'a' => 1}) outln($k);").contains("before the variable of the element"));
}

#[test]
fn spread_in_a_list() {
    assert_eq!(run("const $rows = [[1], [2]];\noutln([[0], ...$rows, ...[[3]]]);"), "[[0], [1], [2], [3]]\n");
    assert_eq!(run("const list<string> $s = ['a', ...['b', 'c'], 'd'];\noutln($s);"), "['a', 'b', 'c', 'd']\n");
    assert_eq!(run("outln([...<number>[]]);"), "[]\n");
    // Its type is the one of the elements
    assert!(error("const list<string> $s = [...[1]];\nconst string $x = $s[0]!;\nconst number $n = [...['a']][0]!;").contains("string"));
    assert!(error("const $x = [1, ...'ab'];").contains("only lists can be spread"));
    // Without the checker, when it runs
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_type_check(false);
    let err = engine.run("const $x = [1, ...5];").unwrap_err().message;
    assert!(err.contains("only lists can be spread with `...`, not a number"), "{err}");
}

#[test]
fn the_right_side_of_coalesce_can_be_nullable() {
    assert_eq!(run("const ?number $a = null;\nconst ?number $r = $a ?? (if (true) 5 else null);\noutln($r);"), "5\n");
    assert_eq!(run("const ?string $a = null;\nconst ?string $b = null;\noutln(($a ?? $b) ?? 'none');"), "none\n");
    // Then so is the result
    let err = error("const ?number $a = null;\nconst number $r = $a ?? (if (true) 5 else null);");
    assert!(err.contains("can be `null`"), "{err}");
    // And a list on the right still gets the type of the left
    assert_eq!(run("const ?list<number> $a = null;\nconst $l = $a ?? [];\n$l.push(1);\noutln($l);"), "[1]\n");
}

#[test]
fn a_for_with_an_index_or_a_key_and_destructuring() {
    assert_eq!(run("for ($i, [$name, $age] in [['Ana', 30], ['Bo', 41]]) outln(f\"{$i}: {$name} {$age}\");"), "0: Ana 30\n1: Bo 41\n");
    assert_eq!(run("for ($i, [$first, ...$others] in [[1, 2, 3], [4]]) outln([$i, $first, $others]);"), "[0, 1, [2, 3]]\n[1, 4, []]\n");
    assert_eq!(run("for ($city => [$lat, $lon] in {'Lima' => [-12, -77]}) outln(f\"{$city} {$lat} {$lon}\");"), "Lima -12 -77\n");
    // Typed like the other patterns: the index is a number, and the names the elements of the inner lists
    assert_eq!(run("const list<list<string>> $w = [['a', 'b']];\nfor ($i, [$x, $y] in $w) outln($x.upper() + $y + ($i + 1).toString());"), "Ab1\n");
    assert!(error("const list<list<number>> $n = [[1]];\nfor ($i, [$a] in $n) { const string $s = $a; }").contains("number"));
    assert!(error("const map<string, list<number>> $m = {'k' => [1]};\nfor ($k => [$a] in $m) { const number $s = $k; }").contains("string"));
    // The same mistakes as before
    assert!(error("for ($i, [$a] in {'k' => [1]}) outln($a);").contains("for ($key => [...] in $map)"));
    assert!(error("for ($i, [$a] in [1, 2]) outln($a);").contains("needs a list of lists"));
    assert!(error("for ($i, [$i] in [[1]]) outln($i);").contains("can't be declared twice"));
}

#[test]
fn literals_fit_the_type_expected_for_them() {
    let at = |code: &str| {
        let e = Engine::with_output(CapturedOutput::new()).run(code).unwrap_err();
        (e.message, e.span.column)
    };
    // Each element, where it is
    assert_eq!(at("const list<number> $x = [1, 'a'];"), ("this is a string, but a number is expected here".to_string(), 29));
    assert_eq!(at("const list<list<number>> $x = [[1], ['a']];"), ("this is a string, but a number is expected here".to_string(), 38));
    assert_eq!(at("const map<string, number> $m = {'a' => 1, 'b' => 'c'};"), ("this is a string, but a number is expected here".to_string(), 50));
    assert_eq!(at("const map<string, number> $m = {1 => 1};").0, "this is a number, but a string is expected here");
    assert!(error("const $x = <number>[1, 'a'];").contains("this list holds number values, not a string"));
    // Wherever a type is expected: an argument, a return type, a field, `=`
    assert!(error("number total(list<number> $l) = $l.sum();\noutln(total([1, '2']));").contains("a number is expected here"));
    assert!(error("list<string> f() = ['a', 2];").contains("a string is expected here"));
    assert!(error("class C {\n    pub list<number> @xs = [1, 'a'];\n}").contains("a number is expected here"));
    assert!(error("let list<number> $x = [];\n$x = [1, 'a'];").contains("a number is expected here"));
    assert!(error("const list<number> $x = [1, ...['a']];").contains("a number is expected here"));
    assert!(error("const $s = ['a'];\nconst list<number> $x = [1, ...$s];").contains("`...` gives string values, but number values are expected here"));
    // What fits still does, and without a type expected nothing changes
    assert_eq!(run("const list<?number> $x = [1, null];\nconst list<any> $y = [1, 'a'];\nconst $z = [1, 'a'];\noutln([$x, $y, $z]);"), "[[1, null], [1, 'a'], [1, 'a']]\n");
    assert_eq!(run("const map<string, any> $m = {'port' => 80, 'host' => 'x'};\noutln($m);"), "{ 'port' => 80, 'host' => 'x' }\n");
    assert_eq!(run(&format!("{ANIMALS}const list<IAnimal> $a = [Wolf(), Wolf()];\noutln($a.length());")), "2\n");
}
