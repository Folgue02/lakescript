//! The fast paths of the VM give the same results and the same errors as the
//! slow ones they skip *(see the review after phase 11 in
//! `ideas/tooling/vm.md`)*.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

/// Without the checker, so the VM's own errors are seen.
fn vm_error(source: &str) -> (String, u32) {
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_type_check(false);
    let e = engine.run(source).unwrap_err();
    (e.message, e.span.column)
}

#[test]
fn exact_divisions_stay_whole_numbers() {
    assert_eq!(run("outln([6 / 3, 7 / 2, 0 / -5, -9 / 3, 1 / 3, (6 / 3).whole?()]);"), "[2, 3.5, 0, -3, 0.3333333333333333, true]\n");
    // `%` of whole floats is done as integers, with the sign of `fmod`
    assert_eq!(run("outln([6.0 % 4, -7.5 % 2, 10 % 3.0, -4.0 % 2, 7 / 2 % 1]);"), "[2, -1.5, 1, 0, 0.5]\n");
    assert_eq!(run("outln((-9223372036854775807 - 1) / -1);"), "9.223372036854776e18\n");
    let mut x = String::from("let $x = 27;\nlet $steps = 0;\nwhile ($x != 1) {\n    $x = if ($x % 2 == 0) $x / 2 else 3 * $x + 1;\n    $steps++;\n}\noutln($steps);");
    assert_eq!(run(&x), "111\n");
    x.clear();
}

#[test]
fn and_and_or_in_conditions() {
    let code = "let $n = 0;\nfor ($i in 0..20) {\n    if ($i > 2 && $i < 8 || $i == 15) $n++;\n    if (!($i > 5) && ($i == 1 || $i == 3)) $n += 10;\n    const $v = if ($i % 2 == 0 || $i % 3 == 0 && $i > 10) 'a' else 'b';\n    $n += if ($v == 'a' && ($i < 4 || $i > 16)) 100 else 0;\n}\noutln($n);";
    assert_eq!(run(code), "326\n");
    // Only what's needed runs
    assert_eq!(run("let $calls = 0;\nbool yes() { $calls++; return true; }\nif (false && yes()) outln('no');\nif (true || yes()) outln('yes');\nwhile (yes() && $calls < 3) {}\noutln($calls);"), "yes\n3\n");
    // A side that isn't a bool: the same error as `&&` and `||` give outside a condition, where it is
    assert_eq!(vm_error("if (1 && true) outln(1);"), ("TypeError: both sides of `&&` must be bools".to_string(), 5));
    assert_eq!(vm_error("if (true && 1) outln(1);"), ("TypeError: both sides of `&&` must be bools".to_string(), 13));
    assert_eq!(vm_error("if (false || 1) outln(1);"), ("TypeError: both sides of `||` must be bools".to_string(), 14));
    assert_eq!(vm_error("if (1 || true) outln(1);"), ("TypeError: both sides of `||` must be bools".to_string(), 5));
    assert_eq!(vm_error("while (1) outln(1);").0, "TypeError: the condition of `while` must be a bool, but this is a number");
}

#[test]
fn loop_variables() {
    assert_eq!(run("let $sum = 0;\nfor ($i in 0..5) $sum += $i;\nfor ($x in [10, 20]) $sum += $x;\nfor ($k in {'a' => 1, 'b' => 2}) $sum += $k.length();\noutln($sum);"), "42\n");
    // A closure keeps the variable of its own pass, so that one stays a cell
    assert_eq!(run("const $fs = <() -> number>[];\nfor ($i in 0..3) $fs.push(() -> $i * 10);\noutln($fs.map(($f) -> $f()));"), "[0, 10, 20]\n");
    // A method with a parameter of the same name doesn't change it
    let code = "class Grid {\n    pub number get(number $x, number $y) = $x * 10 + $y;\n}\nconst $g = Grid();\nlet $total = 0;\nfor ($y in 0..2) for ($x in 0..3) $total += $g.get($x, $y);\noutln($total);";
    assert_eq!(run(code), "63\n");
}

#[test]
fn fields_of_local_objects() {
    let code = "class Counter {\n    number @n = 0;\n    pub const string @name;\n    pub new(@name) {\n        @n = 5;\n    }\n    pub number add(number $by) {\n        @n += $by;\n        return @n;\n    }\n    pub number twice() = (@n = @n * 2);\n}\nconst $c = Counter('a');\noutln([$c.add(1), $c.add(2), $c.twice(), $c.name]);";
    assert_eq!(run(code), "[6, 8, 16, 'a']\n");
    // A `const` field still can't change after the constructor
    let err = vm_error("class C {\n    pub const number @x = 1;\n    pub void set() { @x = 2; }\n}\nC().set();").0;
    assert!(err.contains("x"), "{err}");
    // Of a variable, also when it isn't an object
    assert_eq!(vm_error("void f() {\n    const $p = 5;\n    outln($p.x);\n}\nf();").0, "a number has no fields: `.x` only works on objects");
    // The fields of a custom exception
    assert_eq!(run("class Oops extends ValueError {\n    pub number @code = 7;\n    pub number twice() = @code * 2;\n}\noutln(Oops('x').twice());"), "14\n");
}

#[test]
fn objects_and_their_operators() {
    let point = "class P {\n    pub number @x;\n    pub number @y = 0;\n    implement IEquals {\n        bool equals?(Self $o) = @x == $o.x && @y == $o.y;\n    }\n    implement IAdd<P, P> {\n        P add(P $o) = P(@x + $o.x, @y + $o.y);\n    }\n}\n";
    assert_eq!(run(&format!("{point}outln([P(1, 2) == P(1, 2), P(1) != P(1, 0), P(1, 2) + P(3, 4), P(1) == P(2)]);")), "[true, false, P(x: 4, y: 6), false]\n");
    // The constructor that takes every field: all, some, by name, and too many
    assert_eq!(run(&format!("{point}outln([P(1, 2), P(3), P(y: 4, x: 5)]);")), "[P(x: 1, y: 2), P(x: 3, y: 0), P(x: 5, y: 4)]\n");
    assert!(vm_error(&format!("{point}P(1, 2, 3);")).0.contains("takes 2 arguments"));
    // A class without IEquals is only equal to itself
    assert_eq!(run("class Q {\n    pub number @x;\n}\nconst $q = Q(1);\noutln([$q == $q, Q(1) == Q(1)]);"), "[true, false]\n");
}

#[test]
fn plain_field_defaults() {
    let code = "class Base {\n    pub number @a = 1;\n    pub list<number> @xs = [];\n}\nclass Item extends Base {\n    pub string @name = 'x';\n    pub ?number @maybe = null;\n    pub map<string, number> @m = {};\n    pub bool @on = true;\n}\nconst $i = Item();\nconst $j = Item();\n$i.xs.push(1);\n$i.m['k'] = 1;\noutln([$i, $j]);";
    // Each object gets a list and a map of its own
    assert_eq!(run(code), "[Item(a: 1, xs: [1], name: 'x', maybe: null, m: { 'k' => 1 }, on: true), Item(a: 1, xs: [], name: 'x', maybe: null, m: {}, on: true)]\n");
    // A constructor sees them and can change them, also `const` ones
    assert_eq!(run("class C {\n    pub number @n = 5;\n    pub const string @s = 'c';\n    pub new() {\n        @n += 1;\n    }\n}\noutln(C());"), "C(n: 6, s: 'c')\n");
    // Given or not, by position and by name
    assert_eq!(run("class P {\n    pub number @x;\n    pub number @y = 7;\n}\noutln([P(1), P(1, 2), P(y: 3, x: 4)]);"), "[P(x: 1, y: 7), P(x: 1, y: 2), P(x: 4, y: 3)]\n");
    // A value that needs code still runs where it's declared, every time
    assert_eq!(run("let $made = 0;\nnumber next() {\n    $made++;\n    return $made;\n}\nclass Q {\n    pub number @id = next();\n    pub list<number> @l = [1];\n}\nQ();\nQ();\noutln([Q().id, $made]);"), "[3, 3]\n");
}
