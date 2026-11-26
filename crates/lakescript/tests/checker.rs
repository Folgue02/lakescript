//! The type checker: what it accepts, what it rejects, and that a rejected
//! script never starts running.

use lakescript::{CapturedOutput, Engine};

/// Checks and runs a script that must be accepted.
fn ok(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("rejected: {}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

/// The error of a script that must be rejected. Nothing of it may run.
fn rejected(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    // `use` lines must stay at the top
    let uses: Vec<&str> = source.lines().take_while(|l| l.starts_with("use ")).collect();
    let rest: Vec<&str> = source.lines().skip(uses.len()).collect();
    let script = format!("{}\noutln('started');\n{}", uses.join("\n"), rest.join("\n"));
    match engine.run(&script) {
        Ok(_) => panic!("accepted:\n{source}"),
        Err(e) => {
            assert_eq!(output.text(), "", "the script ran before being rejected: {}", e.message);
            e.message
        }
    }
}

#[test]
fn inferred_variables_keep_their_type() {
    assert!(rejected("let $age = 5;\n$age = 'old';").contains("`$age` is a number, so it can't hold a string"));
    assert!(rejected("const $city = 'Madrid';\n$city = 'Paris';").contains("is a constant"));
    assert_eq!(ok("let $data = null;\n$data = 'anything';\n$data = 5;\noutln($data);"), "5\n");
    assert!(rejected("string $name = 5;").contains("`$name` is a string, but its value is a number"));
    assert!(rejected("string $name = null;").contains("is a string, but its value is `null`"));
    assert_eq!(ok("?string $nick = null;\noutln($nick ?? 'none');"), "none\n");
}

#[test]
fn empty_collections_learn_their_type_from_use() {
    assert!(rejected("let $names = [];\n$names << 'Daniel';\n$names << 5;").contains("this list holds string values, so a number can't be added"));
    assert!(rejected("let $ages = {};\n$ages['Daniel'] = 24;\n$ages['Bo'] = 'old';").contains("can't hold a string"));
    assert_eq!(ok("let $names = <string>[];\n$names << 'Ana';\noutln($names);"), "['Ana']\n");
    assert_eq!(ok("const $mixed = [1, 'a'];\noutln($mixed);"), "[1, 'a']\n");
}

#[test]
fn operators() {
    assert!(rejected("outln('Age: ' + 5);").contains("`+` joins two strings, but got a string and a number"));
    assert!(rejected("outln(true + 1);").contains("`+` adds two numbers or joins two strings"));
    assert!(rejected("outln(1 == '1');").contains("a number and a string can never be equal"));
    assert!(rejected("outln(1 < 'a');").contains("compares two numbers or two strings"));
    assert!(rejected("outln(1 && true);").contains("both sides of `&&` must be bools"));
    assert!(rejected("if (1) outln('x');").contains("must be a bool"));
    assert!(rejected("outln(-'a');").contains("`-` needs a number"));
    assert!(rejected("outln(5 -in 'abc');").contains("needs a list, a map or a range"));
    assert_eq!(ok("outln(1 == 1.0 && 'a' < 'b' && 5 -in 1..10);"), "true\n");
}

#[test]
fn functions_and_calls() {
    let defs = "string greet(string $name = 'you') = f\"Hello {$name}\";\nnumber sum(...number $n) { let $t = 0; for ($x in $n) $t += $x; return $t; }\n";
    assert_eq!(ok(&format!("{defs}outln(greet());\noutln(greet(name: 'Ana'));\noutln(sum(1, 2, 3));")), "Hello you\nHello Ana\n6\n");
    assert!(rejected(&format!("{defs}greet(5);")).contains("`$name` of `greet` must be a string, but this is a number"));
    assert!(rejected(&format!("{defs}greet('a', 'b');")).contains("takes 1 arguments, but got more"));
    assert!(rejected(&format!("{defs}greet(nope: 'x');")).contains("has no parameter named `nope`"));
    assert!(rejected(&format!("{defs}sum(1, 'two');")).contains("must be a number, but this is a string"));
    assert!(rejected("void f(number $a) {}\nf();").contains("needs a value for `$a`"));
    assert!(rejected("number f() = 'x';").contains("`f` returns a number, but this is a string"));
    assert!(rejected("number f() { return 'x'; }").contains("this function returns a number, but this is a string"));
    assert!(rejected("number f(number $n) { if ($n > 0) return 1; }").contains("must return a number at the end"));
    assert!(rejected("void f() { return 1; }").contains("a `void` function can't return a value"));
    assert!(rejected("outln(missing());").contains("no function named `missing`"));
    assert_eq!(ok("number abs(number $n) { if ($n < 0) return -$n; else return $n; }\noutln(abs(-3));"), "3\n");
    assert_eq!(ok("number forever() { while (true) { return 1; } }\noutln(forever());"), "1\n");
    assert_eq!(ok("number fail() { throw ValueError('no'); }\noutln((try fail()).isError());"), "true\n");
}

#[test]
fn generics() {
    let first = "T first<T>(list<T> $items, T $default) = $items[0] ?? $default;\n";
    assert_eq!(ok(&format!("{first}outln(first(['a', 'b'], 'z').upper());")), "A\n");
    assert!(rejected(&format!("{first}outln(first([1, 2], 0).upper());")).contains("a number has no method `.upper()`"));
    // An index can be past the end, so it can be `null`
    assert!(rejected("const $l = ['a'];\noutln($l[0].upper());").contains("can be `null`"));
    assert!(rejected("string $s = 'ab'[0];").contains("can be `null`"));
    assert_eq!(ok("const $l = ['a'];\noutln(($l[1] ?? 'b').upper());"), "B\n");
}

#[test]
fn methods_of_built_in_types() {
    assert_eq!(ok("outln('hello'.padStart(7, '*').upper());"), "**HELLO\n");
    assert!(rejected("'a'.nope();").contains("a string has no method `.nope()`"));
    assert!(rejected("'a'.padStart('x');").contains("must be a number, but this is a string"));
    assert!(rejected("const $n = [1, 2].map(($x) -> $x * 2);\n($n[0] ?? 0).upper();").contains("a number has no method `.upper()`"));
    assert!(rejected("'a1'.replace(%r\"\\d\", 'x');").contains("For a regex, use `.replaceMatches()`"));
    assert!(rejected("'a1'.replaceMatches('1', 'x');").contains("For a string, use `.replace()`"));
    assert_eq!(ok("outln(%w[b a].sorted().join('-'));"), "a-b\n");
}

#[test]
fn anonymous_functions_take_their_parameter_types_from_where_they_go() {
    assert_eq!(ok("outln(['a', 'bb'].map(($s) -> $s.length()));"), "[1, 2]\n");
    assert!(rejected("['a'].map(($s) -> $s * 2);").contains("`*` needs numbers, but got a string and a number"));
    assert!(rejected("[1].filter(($n) -> $n + 1);").contains("must be a (number) -> bool"));
    assert_eq!(ok("number apply((number) -> number $f) = $f(3);\noutln(apply(($x) -> $x * $x));"), "9\n");
}

#[test]
fn nullable_values() {
    assert!(rejected("const $m = {'a' => 'x'};\noutln($m['a'].upper());").contains("can be `null`, so `.upper()` can't always be called"));
    assert_eq!(ok("const $m = {'a' => 'x'};\noutln($m['a']?.upper());\noutln(($m['b'] ?? 'none').upper());"), "X\nNONE\n");
    assert!(rejected("outln('abc'.indexOf('z') + 1);").contains("`+`"));
}

#[test]
fn narrowing() {
    // After a null check, the value isn't nullable anymore
    let find = "?string find(bool $found) = $found ? 'Ana' : null;\n";
    assert_eq!(ok(&format!("{find}const $n = find(true);\nif ($n != null) outln($n.upper());")), "ANA\n");
    assert_eq!(ok(&format!("{find}void show(bool $f) {{\n  const $n = find($f);\n  if ($n == null) return;\n  outln($n.upper());\n}}\nshow(true);")), "ANA\n");
    assert_eq!(ok(&format!("{find}const $n = find(true);\noutln($n != null && $n.length() == 3);")), "true\n");
    assert!(rejected(&format!("{find}const $n = find(true);\noutln($n.upper());")).contains("can be `null`"));
    // .is() and case types
    assert_eq!(ok("any $v = 'hi';\nif ($v.is(string)) outln($v.upper());"), "HI\n");
    assert_eq!(ok("void show(any $v) {\n  switch ($v) {\n    case number -> outln($v * 2);\n    default -> outln('other');\n  }\n}\nshow(21);"), "42\n");
    // Assigning a value narrows too
    assert_eq!(ok("?string $s = null;\n$s = 'x';\noutln($s.upper());"), "X\n");
}

#[test]
fn any_is_not_checked() {
    assert_eq!(ok("any $data = {'name' => 'Ana'};\noutln($data['name'].upper());\noutln($data.length() + 1);"), "ANA\n2\n");
}

#[test]
fn loops_and_destructuring() {
    assert!(rejected("for ($i in 1..3) $i.upper();").contains("a number has no method `.upper()`"));
    assert!(rejected("for ($k => $v in {'a' => 1}) $v.upper();").contains("a number has no method"));
    assert!(rejected("for ($x in 5) {}").contains("can't loop over a number"));
    assert!(rejected("let [$a, $b] = 'xy';").contains("only lists can be destructured"));
    assert_eq!(ok("for ([$n, $a] in [['Ana', 30]]) outln(f\"{$n} {$a}\");"), "Ana 30\n");
}

#[test]
fn switch_and_if_as_values() {
    assert!(rejected("const $x = switch (5) { case 'a' -> 1; default -> 2; };").contains("this case can never match"));
    assert!(rejected("const $s = if (true) 'a' else 'b';\n$s * 2;").contains("`*` needs numbers"));
}

#[test]
fn exceptions() {
    assert!(rejected("throw 'oops';").contains("only exceptions can be thrown"));
    assert!(rejected("try {} catch (string $e) {}").contains("`catch` needs an exception type"));
    assert_eq!(ok("try { throw ValueError('bad'); } catch (ValueError $e) { outln($e.message().upper()); }"), "BAD\n");
    assert_eq!(ok("const $r = try '5'.toNumber();\noutln($r.orElse(0) + 1);"), "6\n");
}

#[test]
fn standard_library_modules() {
    assert_eq!(ok("use std/path;\noutln(path::name('/a/b.txt').upper());"), "B.TXT\n");
    assert!(rejected("use std/path;\npath::join(5);").contains("must be a string, but this is a number"));
    assert!(rejected("use std/env;\noutln(env::get('HOME').length());").contains("can be `null`"));
    assert!(rejected("use std/fs;\nfs::nope();").contains("has no `pub` function named `nope`"));
    assert!(rejected("use std/format/json;\njson::stringify([1], indent: 'x');").contains("`$indent` of `stringify` must be a ?number"));
}

#[test]
fn types_written_in_the_code() {
    assert!(rejected("Thing $t = 1;").contains("unknown type `Thing`"));
    assert!(rejected("outln(1.is(Nope));").contains("unknown type `Nope`"));
    assert_eq!(ok("list<string> $n = ['a'];\noutln($n);"), "['a']\n");
    assert!(rejected("(string) -> bool $c = ($x) -> $x.length();").contains("its value is a (string) -> number"));
}

#[test]
fn state_is_kept_between_runs() {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.run("let $count = 1;\nnumber twice(number $n) = $n * 2;").unwrap();
    engine.run("outln(twice($count));").unwrap();
    let err = engine.run("$count = 'x';").unwrap_err();
    assert!(err.message.contains("is a number"));
    assert_eq!(output.text(), "2\n");
}

#[test]
fn the_check_can_be_turned_off() {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_type_check(false);
    // Without the check the script starts, and the error is only found when the line runs
    let err = engine.run("outln('started');\noutln('a' + 1);").unwrap_err();
    assert!(err.message.contains("TypeError"));
    assert_eq!(output.text(), "started\n");
}

#[test]
fn throw_expressions_fit_any_type() {
    assert_eq!(ok("string f(?string $n) = $n ?? throw ValueError('none');\noutln(f('a').upper());"), "A\n");
    assert_eq!(ok("number f(any $v) = switch ($v) {\n case number -> $v;\n default -> throw TypeError('x');\n};\noutln(f(2) + 1);"), "3\n");
    assert!(rejected("const $x = null ?? throw 'oops';").contains("only exceptions can be thrown"));
}

#[test]
fn functions_can_call_each_other_in_any_order() {
    let script = "
        bool even(number $n) = if ($n == 0) true else odd($n - 1);
        bool odd(number $n) = if ($n == 0) false else even($n - 1);
        outln([even(10), odd(7)]);
        void parse() {
            let $pos = 0;
            number expr() = term() + 1;
            number term() {
                $pos += 1;
                return if ($pos < 3) expr() else 0;
            }
            outln(expr());
        }
        parse();
    ";
    assert_eq!(ok(script), "[true, true]\n3\n");
    // The block's own statements still can't use a function before it's declared
    assert!(rejected("f();\nvoid f() {}").contains("`f` is declared further down"));
    assert!(rejected("void g() {\n  h();\n  void h() {}\n}").contains("`h` is declared further down"));
    // Signatures are known from the start, so wrong calls are found too
    assert!(rejected("number a() = b('x');\nnumber b(number $n) = $n;").contains("string"));
    assert!(rejected("void f() {}\nvoid f() {}").contains("a function named `f` already exists"));
}


#[test]
fn every_branch_of_a_value_fits_the_type_expected() {
    // Joined, the branches would be `any`, which fits anywhere
    assert_eq!(rejected("const number $y = if (1 == 1) 2 else 'b';"), "this is a string, but a number is expected here");
    assert_eq!(rejected("number f(number $p) = switch ($p) { case 1 -> 2; default -> ValueError('x'); };"), "this is a ValueError, but a number is expected here");
    assert_eq!(
        rejected("((string) -> any) part(number $n) = if ($n == 1) ($s) -> 1 else ValueError('no part');"),
        "this is a ValueError, but a (string) -> any is expected here"
    );
    assert_eq!(rejected("const ?string $s = null;\nconst string $t = if (1 == 1) $s else 'x';"), "this can be `null`, but a string is expected here");
    // `throw` takes the type expected, and branches without one can still differ
    assert_eq!(ok("number f(number $p) = switch ($p) { case 1 -> 2; default -> throw ValueError('x'); };\noutln(f(1));"), "2\n");
    assert_eq!(ok("const $v = if (1 == 1) 2 else 'b';\noutln($v);"), "2\n");
}

#[test]
fn const_fields_narrow_like_variables() {
    let day = "class Day {\n    const (string) -> any @part1;\n    const ?((string) -> any) @part2;\n    pub ((string) -> any) part(number $n) =\n        if ($n == 1) @part1\n        else if ($n == 2 && @part2 != null) @part2\n        else throw ValueError('no part');\n}\n";
    assert_eq!(ok(&format!("{day}const $d = Day(($s) -> 1, ($s) -> 2);\noutln($d.part(2)('x'));")), "2\n");
    assert_eq!(ok("class A {\n    pub const ?string @name;\n}\nconst $a = A('x');\nif ($a.name != null) {\n    outln($a.name.upper());\n}"), "X\n");
    assert_eq!(ok("class A {\n    const ?string @name;\n    pub string show() {\n        if (@name == null) return '-';\n        return @name;\n    }\n}\noutln(A(null).show());"), "-\n");
    // A field that isn't `const` can change between the check and its use, and the error says why the check didn't count
    assert_eq!(
        rejected("class A {\n    ?string @name = null;\n    pub string show() = if (@name != null) @name else '-';\n}"),
        "this can be `null`, but a string is expected here. `@name` isn't `const`, so checking it for `null` doesn't narrow it: make it `const`, or copy it into a `const` variable first"
    );
    // And so can a `let` variable's object
    assert!(
        rejected("class A {\n    pub const ?string @name;\n}\nlet $a = A('x');\nif ($a.name != null) {\n    outln($a.name.upper());\n}")
            .ends_with("`$a` isn't `const`, so checking `$a.name` for `null` doesn't narrow it: declare `$a` with `const`, or copy the field into a `const` variable first")
    );
    // Without a check, there's no hint
    assert_eq!(rejected("class A {\n    ?string @name = null;\n    pub string show() = @name;\n}"), "`show` returns a string, but this is a ?string, which can be `null`");
}

#[test]
fn the_prelude_is_up_to_date() {
    // The signatures the checker knows are made from the tables of ideas/stdlib
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fresh = std::env::temp_dir().join(format!("lks-prelude-{}.txt", std::process::id()));
    let made = std::process::Command::new("python3").arg(root.join("tools/docs/prelude.py")).arg(&fresh).output();
    // Without Python there's nothing to compare with
    let Ok(made) = made else { return };
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let committed = std::fs::read_to_string(root.join("crates/lakescript/src/checker/prelude.txt")).unwrap();
    let fresh_text = std::fs::read_to_string(&fresh).unwrap();
    let _ = std::fs::remove_file(&fresh);
    assert!(committed == fresh_text, "crates/lakescript/src/checker/prelude.txt is out of date: run `python3 tools/docs/prelude.py`");
}
