//! Fixes from the review of the code: values that contain themselves, the
//! check of `@super(...)`, and what the VM remembers about fields and methods.

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

#[test]
fn a_value_inside_itself_is_written_once() {
    assert_eq!(run("const $l = <any>[1];\n$l << $l;\noutln($l);\noutln(f\"{$l}\");\noutln($l.toString());"), "[1, [...]]\n[1, [...]]\n[1, [...]]\n");
    assert_eq!(run("const $m = <string, any>{};\n$m['me'] = $m;\noutln($m);"), "{ 'me' => {...} }\n");
    assert_eq!(run("class N {\n    pub ?N @next = null;\n    pub number @v = 1;\n}\nconst $a = N();\n$a.next = $a;\noutln($a);"), "N(next: N(...), v: 1)\n");
    // The same list twice isn't a list inside itself
    assert_eq!(run("const $l = [1];\noutln([$l, $l]);"), "[[1], [1]]\n");
}

#[test]
fn equals_on_values_inside_themselves() {
    let code = "const $a = <any>[1];\n$a << $a;\nconst $b = <any>[1];\n$b << $b;\nconst $c = <any>[2];\n$c << $c;\noutln([$a.equals($b), $a.equals($c)]);";
    assert_eq!(run(code), "[true, false]\n");
}

#[test]
fn text_that_looks_like_super_isnt_a_call_of_it() {
    let classes = "class A {\n    pub number @x;\n    new(number $x) {\n        @x = $x;\n    }\n}\n";
    let e = error(&format!("{classes}class B extends A {{\n    new() {{\n        outln('(@super 1)');\n    }}\n}}"));
    assert_eq!(e, "the constructor of `B` must call `@super(...)`, the constructor of its superclass");
    assert_eq!(run(&format!("{classes}class B extends A {{\n    new() {{\n        @super(3);\n    }}\n}}\noutln(B().x);")), "3\n");
}

#[test]
fn the_length_of_a_range() {
    let lengths = "outln([(0..10).length(), (0..=10).length(), (10..0).length(), (5..5).length(), (5..=5).length(), (0..10).step(3).length(), (0..=9).step(3).length(), (0..3000000000).length()]);";
    assert_eq!(run(lengths), "[10, 11, 10, 0, 1, 4, 4, 3000000000]\n");
    assert_eq!(run("outln([(5..5).empty?(), (5..=5).empty?()]);"), "[true, false]\n");
    // The same as counting them
    assert_eq!(run("for ($r in [0..7, 7..0, 0..=7, (0..7).step(2), (7..=0).step(3)]) {\n    assert $r.length() == $r.toList().length();\n}\noutln('ok');"), "ok\n");
}

#[test]
fn one_place_with_objects_of_several_classes() {
    // The same `.name`, `.area()` and `.size = ` run for classes whose fields and methods are in other places
    let code = "interface IShape {\n    number area();\n}\n\
        class Square {\n    pub number @side;\n    pub string @name = 'square';\n    pub number @size = 0;\n    implement IShape {\n        number area() = @side * @side;\n    }\n}\n\
        class Rect {\n    pub string @name = 'rect';\n    pub number @size = 0;\n    pub number @w;\n    pub number @h;\n    implement IShape {\n        number area() = @w * @h;\n    }\n}\n\
        class Big extends Square {\n    override {\n        number area() = 1000;\n    }\n}\n\
        const $shapes = <any>[Square(side: 2), Rect(w: 2, h: 3), Big(side: 1), Square(side: 3), Rect(w: 1, h: 1)];\n\
        let $out = <string>[];\n\
        for ($turn in 0..3) {\n    for ($s in $shapes) {\n        $s.size = $s.area() + $turn;\n        $out << f\"{$s.name}:{$s.size}\";\n    }\n}\n\
        outln($out.join(' '));";
    assert_eq!(
        run(code),
        "square:4 rect:6 square:1000 square:9 rect:1 square:5 rect:7 square:1001 square:10 rect:2 square:6 rect:8 square:1002 square:11 rect:3\n"
    );
}

#[test]
fn const_fields_stay_const_at_a_remembered_place() {
    let code = "class P {\n    pub const number @x;\n    pub number @y = 0;\n    pub void move(number $n) {\n        @y = $n;\n        @x = $n;\n    }\n}\nconst $p = P(1);\n$p.y = 5;\n$p.move(2);";
    assert!(error(code).starts_with("`x` is a `const` field of `P`"), "{}", error(code));
    // Without the checker, the VM says so, also after `@y = ` was remembered at that place
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_type_check(false);
    let e = engine.run(code).unwrap_err().message;
    assert_eq!(e, "`x` is a `const` field of `P`: it can only be set while the object is created");
    // A method that isn't there, after the same name was called on another class
    let code = "class A {\n    pub number go() = 1;\n}\nclass B {}\nfor ($o in <any>[A(), B()]) {\n    outln($o.go());\n}";
    assert!(error(code).contains("go"), "{}", error(code));
}

#[test]
fn a_regex_in_a_loop() {
    assert_eq!(run("let $n = 0;\nfor ($w in ['ab', 'cd', 'a1', 'zz']) {\n    if ($w -match %r\"^a\") {\n        $n += 1;\n    }\n}\noutln($n);"), "2\n");
}

/// A module that needs a feature this build doesn't have says so.
#[cfg(not(feature = "http"))]
#[test]
fn a_module_that_was_left_out() {
    assert_eq!(error("use std/http;"), "`std/http` isn't in this program: it was built without the `http` feature of Lakescript");
}

/// Without dates, `std/time` still has the current time.
#[cfg(not(feature = "dates"))]
#[test]
fn time_without_dates() {
    assert_eq!(run("use std/time;\noutln(time::now() > 0);"), "true\n");
    assert_eq!(error("use std/time/zone;"), "`std/time/zone` isn't in this program: it was built without the `dates` feature of Lakescript");
}

#[test]
fn positions_in_long_strings() {
    // Long enough to be remembered *(see `strings.rs`)*: only ASCII, with
    // characters of 2 bytes, and with characters of 4
    for unit in ["abcdefghij", "añb€cdéfgh", "a😀b🎉cdñfgh"] {
        let code = format!(
            "const $s = '{unit}'.repeat(12);\n\
             const $chars = <string>[];\n\
             for ($c in $s) {{\n    $chars << $c;\n}}\n\
             assert $s.length() == 120;\n\
             assert $chars.length() == 120;\n\
             for ($i in 0..120) {{\n    assert $s[$i] == $chars[$i];\n    assert $s[$i - 120] == $chars[$i];\n}}\n\
             assert $s[120] == null;\n\
             assert $s[-121] == null;\n\
             assert $s[3..7] == $chars[3..7].join('');\n\
             assert $s[115..] == $chars[115..].join('');\n\
             assert $s[0..=4] == $chars[0..5].join('');\n\
             assert $s[-3..] == $chars[117..].join('');\n\
             assert $s[50..40] == '';\n\
             const $ten = $chars[0..10].join('');\n\
             assert $s.indexOf($ten) == 0;\n\
             assert $s.indexOfLast($ten) == 110;\n\
             assert $s.indexesOf($ten).equals((0..12).map(($n) -> $n * 10));\n\
             assert $s.indexOf($chars[7]! + $chars[8]!) == 7;\n\
             assert $s.codePoint(119) == $chars[119]!.codePoint();\n\
             assert $s.insert(13, '--')[13..15] == '--';\n\
             assert $s.insert(13, '--').length() == 122;\n\
             assert $s.insert(120, '!')[120] == '!';\n\
             outln('ok');"
        );
        assert_eq!(run(&code), "ok\n", "{unit}");
    }
    // More long strings in turn than are remembered
    let many = "const $all = <string>[];\nfor ($n in 0..20) {\n    $all << f\"{$n}ñ\".repeat(20);\n}\nlet $total = 0;\nfor ($turn in 0..3) {\n    for ($t in $all) {\n        assert $t[$t.length() - 1] == 'ñ';\n        $total += $t.length();\n    }\n}\noutln($total);";
    assert_eq!(run(many), "3000\n");
}
