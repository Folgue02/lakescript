//! Bugs found while writing a real program: ranges that end in a field or a
//! method of the object, `-in` with an object on its left, and the methods
//! of lists and maps that compare values, which ignored `IEquals`.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

const POINT: &str = "class P {\n    pub number @x;\n    implement IEquals {\n        bool equals?(Self $o) = @x == $o.x;\n    }\n}\nenum A { X, Y }\n";

fn with_point(code: &str) -> String {
    run(&format!("{POINT}{code}"))
}

#[test]
fn a_range_can_end_in_a_field_or_a_method() {
    let code = "class Grid {\n    number @passes = 2;\n    list<number> @cells = [7, 8, 9];\n    number @y = 1;\n    number bottom() = 3;\n\n\
        pub list<any> run() {\n        const $out = <any>[];\n\
        for ($i in 0..@passes) $out.push($i);\n\
        for ($i in 0..@cells.length()) $out.push($i);\n\
        $out.push((@y..=@bottom()).toList());\n\
        $out.push(@cells[0..@passes]);\n\
        return $out;\n    }\n}\noutln(Grid().run());";
    assert_eq!(run(code), "[0, 1, 0, 1, 2, [1, 2, 3], [7, 8]]\n");
}

#[test]
fn in_with_an_object_or_an_enum_on_the_left() {
    assert_eq!(with_point("outln([A::X -in [A::X], A::Y -in [A::X], A::Y -notin [A::X]]);"), "[true, false, true]\n");
    assert_eq!(with_point("const $p = P(1);\noutln([$p -in [$p], $p -notin [$p]]);"), "[true, false]\n");
    // An equal object is found, through `equals?`
    assert_eq!(with_point("outln([P(1) -in [P(2), P(1)], P(3) -in [P(1)], P(3) -notin [P(1)], [P(1)] -in [[P(1)]]]);"), "[true, false, true, true]\n");
    // An object on the right still decides with its own `contains`
    let bag = "class Bag {\n    implement IContains {\n        bool contains(any $v) = $v == 1;\n    }\n}\noutln([1 -in Bag(), 2 -in Bag(), P(1) -in Bag()]);";
    assert_eq!(with_point(bag), "[true, false, false]\n");
}

#[test]
fn list_methods_use_iequals() {
    assert_eq!(with_point("outln([[P(1)].equals([P(1)]), [P(1)].equals([P(2)]), [[P(1)]].equals([[P(1)]])]);"), "[true, false, true]\n");
    assert_eq!(with_point("outln([[P(1)].contains(P(1)), [P(1)].contains(P(2))]);"), "[true, false]\n");
    assert_eq!(with_point("outln([[P(2), P(1)].indexOf(P(1)), [P(1), P(2), P(1)].indexOfLast(P(1)), [P(1)].indexOf(P(3))]);"), "[1, 2, null]\n");
    assert_eq!(with_point("outln([[P(1), P(2), P(1)].count(P(1)), [P(1), P(1), P(2)].unique().length()]);"), "[2, 2]\n");
    let changing = "const $l = [P(1), P(2), P(1), P(1)];\noutln([$l.remove(P(1)), $l.length(), $l.removeAll(P(1)), $l.length(), $l.remove(P(1))]);";
    assert_eq!(with_point(changing), "[true, 3, 2, 1, false]\n");
    // Values that aren't objects are compared as before
    assert_eq!(run("outln([[1, 2].contains(2), [1, [2]].indexOf([2]), ['a', 'a'].count('a'), [1, 1, 2].unique()]);"), "[true, 1, 2, [1, 2]]\n");
}

#[test]
fn map_methods_use_iequals() {
    assert_eq!(with_point("outln([{'a' => P(1)}.equals({'a' => P(1)}), {'a' => P(1)}.equals({'a' => P(2)})]);"), "[true, false]\n");
    assert_eq!(with_point("const $m = {'a' => P(2), 'b' => P(1)};\noutln([$m.hasValue(P(1)), $m.hasValue(P(3)), $m.keyOf(P(1)), $m.keyOf(P(3))]);"), "[true, false, 'b', null]\n");
}

#[test]
fn an_assert_with_equals_passes_for_equal_objects() {
    assert_eq!(with_point("assert [P(1), P(2)].equals([P(1), P(2)]);\noutln('ok');"), "ok\n");
}

#[test]
fn lists_that_contain_themselves_still_compare() {
    assert_eq!(with_point("const $a = <any>[P(1)];\n$a.push($a);\nconst $b = <any>[P(1)];\n$b.push($b);\noutln($a.equals($b));"), "true\n");
}
