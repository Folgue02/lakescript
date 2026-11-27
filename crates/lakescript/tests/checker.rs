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

/// A block that gives a value must give one on every way through it, or
/// leave: reaching its end was only an error once it ran, after what it did.
#[test]
fn blocks_that_give_a_value_give_it_every_time() {
    let wrong = "this block must give its value with `yield`";
    for source in [
        "const $v = 1;\nnumber f() { return switch ($v) { default -> { errln('x'); } }; }\noutln(f());",
        "const $v = 1;\nnumber f() { return switch ($v) { case 1 -> 2; default -> { errln('x'); } }; }\noutln(f());",
        "const $v = 1;\nnumber f() { return if ($v == 1) { errln('x'); } else 2; }\noutln(f());",
        "const $v = 1;\nconst number $n = if ($v == 1) { errln('x'); } else { yield 2; };",
        // Only one of the ways gives it
        "const $v = 1;\nconst $n = if ($v == 1) { if ($v > 0) { yield 1; } } else 3;",
        "const $v = 1;\nconst $n = switch ($v) { default -> { for ($i in 0..3) { yield $i; } } };",
    ] {
        assert_eq!(rejected(source), wrong, "{source}");
    }
    let accepted = ok("use std/process;\nconst $v = 1;\n\
        number a() { return switch ($v) { case 1 -> 2; default -> { errln('x'); yield 3; } }; }\n\
        number b() { return switch ($v) { case 2 -> 2; default -> { throw ValueError('no'); } }; }\n\
        number c() { return if ($v == 1) { yield 1; } else { exit(2); }; }\n\
        number d() { return if ($v == 1) { if ($v > 0) { yield 1; } else { yield 2; } } else 3; }\n\
        number e() { return switch ($v) { default -> { switch ($v) { case 1 -> { yield 1; } default -> { yield 2; } } } }; }\n\
        number g() { return if ($v == 1) { return 5; } else 3; }\n\
        outln([a(), c(), d(), e(), g()]);\n");
    assert_eq!(accepted, "[2, 1, 1, 1, 5]\n");
}

/// Every error of a check, in the order of the code.
fn every_error(source: &str) -> Vec<String> {
    let mut engine = Engine::with_output(CapturedOutput::new());
    let first = engine.check_source(source).expect_err("the script has errors");
    let all: Vec<String> = engine.errors().iter().map(|e| format!("{}: {}", e.span.line, e.message)).collect();
    assert_eq!(all.first(), Some(&format!("{}: {}", first.span.line, first.message)), "the check gives the first one");
    all
}

/// The checker goes on after an error, so every error is there at once
/// *(see Every error at once in ideas/tooling/lsp.md)*.
#[test]
fn every_error_at_once() {
    let source = "\
const number $a = 'one';
outln($a.upper());
number twice(Strng $s) = 2;
outln(twice(3));
void f() {
    let $x = 1 + 'two';
    outln($x);
    const string $y = 5;
}
outln($missing);
const $ok = if (true) { errln('x'); } else 2;
class Point {
    pub number @x;
    pub string name() = @x;
}
outln(Point(1).x + 1);
";
    assert_eq!(
        every_error(source),
        [
            "1: `$a` is a number, but its value is a string",
            "3: unknown type `Strng`. Did you mean `string`?",
            "6: `+` joins two strings, but got a number and a string. Use an f-string instead (i.e. f\"Age: {$age}\")",
            "8: `$y` is a string, but its value is a number",
            "10: `$missing` isn't declared",
            "11: this block must give its value with `yield`",
            "14: `name` returns a string, but this is a number",
        ]
    );
    // Nothing to report: none
    let mut engine = Engine::with_output(CapturedOutput::new());
    assert!(engine.check_source("outln(1);").is_ok() && engine.errors().is_empty());
    // A run that fails on its types has them too, and runs nothing
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    assert!(engine.run("outln('ran');\nconst number $n = 'a';\nconst string $s = 1;").is_err());
    assert_eq!((engine.errors().len(), output.text()), (2, String::new()));
    // While running, an exception is one error, not a type error
    assert!(engine.run("throw ValueError('no');").is_err());
    assert!(engine.errors().is_empty());
}

#[test]
fn every_error_of_a_module() {
    let dir = std::env::temp_dir().join(format!("lks-every-error-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("util.lks"), "pub number one() = 'one';\npub number two() = 2;\npub string three() = 3;\n").unwrap();
    std::fs::write(dir.join("main.lks"), "use util;\noutln(util::two());\n").unwrap();
    let mut engine = Engine::with_output(CapturedOutput::new());
    let first = engine.check_file(&dir.join("main.lks")).unwrap_err();
    let shown: Vec<(Option<String>, u32)> = engine.errors().iter().map(|e| (e.file.as_ref().map(|f| f.rsplit('/').next().unwrap().to_string()), e.span.line)).collect();
    assert_eq!(shown, [(Some("util.lks".to_string()), 1), (Some("util.lks".to_string()), 3)]);
    assert_eq!(first.span.line, 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// For an editor: every syntax error, then the type errors of what could be read.
#[test]
fn every_error_while_it_is_written() {
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_names(true);
    let source = "const number $a = 'one';\noutln(1\nconst string $s = 2;\nlet $x = ;\n";
    assert!(engine.check_source(source).is_err());
    let lines: Vec<u32> = engine.errors().iter().map(|e| e.span.line).collect();
    // The two syntax errors first, then the two type errors
    assert_eq!(lines.len(), 4, "{:?}", engine.errors().iter().map(|e| &e.message).collect::<Vec<_>>());
    assert!(engine.errors()[2].message.contains("`$a` is a number"), "{}", engine.errors()[2].message);
    assert!(engine.errors()[3].message.contains("`$s` is a string"), "{}", engine.errors()[3].message);
}

/// An error about a name that isn't there says the one likely meant
/// *(see Did you mean in ideas/tooling/diagnostics.md)*.
#[test]
fn did_you_mean() {
    for (source, said) in [
        ("const $name = 'ana';\noutln($nmae);", "`$nmae` isn't declared. Did you mean `$name`?"),
        ("let $count = 1;\n$cont = 2;", "`$cont` isn't declared. Did you mean `$count`?"),
        ("outln('a'.lenght());", "a string has no method `.lenght()`. Did you mean `.length()`?"),
        ("outln('a'.toUpperCase());", "a string has no method `.toUpperCase()`. Did you mean `.upper()`?"),
        ("outln([1].size());", "a list<number> has no method `.size()`. Did you mean `.length()`?"),
        ("print('hi');", "there's no function named `print`. Did you mean `outln`?"),
        ("outln(len([1]));", "there's no function named `len`. Did you mean the method `.length()` (i.e. `$list.length()`)?"),
        ("use std/fs;\noutln(fs::exists('x'));", "`<std>/fs.lks` has no `pub` function named `exists`. Did you mean `fs::exists?()`?"),
        ("use std/fs;\noutln(fz::read('x'));", "there's no module named `fz`. Did you mean `fs`?"),
        ("const strng $s = 'a';", "unknown type `strng`. Did you mean `string`?"),
        ("const int $i = 1;", "unknown type `int`. Did you mean `number`?"),
        ("class Point { pub number @x = 1; }\noutln(Pont());", "there's no function named `Pont`. Did you mean `Point`?"),
        ("class Point { pub number @total = 1; }\noutln(Point().totl);", "`Point` has no field named `totl`. Did you mean `.total`?"),
        ("class Point { pub number sum() = 1; }\noutln(Point().summ());", "`Point` has no method `.summ()`. Did you mean `.sum()`?"),
    ] {
        assert_eq!(rejected(source), said, "{source}");
    }
    // Nothing close: no guess
    assert_eq!(rejected("outln('a'.zzz());"), "a string has no method `.zzz()`");
    assert_eq!(rejected("class Point { pub number @x = 1; }\noutln(Point().y);"), "`Point` has no field named `y`");
    assert_eq!(rejected("outln(nothingLikeIt());"), "there's no function named `nothingLikeIt`. Functions must be defined before they're used");
    // The name alone, for tools
    let mut engine = Engine::with_output(CapturedOutput::new());
    let error = engine.check_source("const $name = 1;\noutln($nmae);").unwrap_err();
    assert_eq!(error.suggestion.as_deref(), Some("$name"));
}

/// One parameter without a type can go without parentheses.
#[test]
fn anonymous_functions_with_one_parameter() {
    let out = ok("const $xs = [1, 2, 3];\n\
        outln($xs.map($v -> $v * 2));\n\
        outln($xs.filter($v -> { return $v > 1; }));\n\
        outln($xs.map($v -> $v.toString().length()));\n\
        const $add = $x -> $y -> $x + $y;\n\
        outln($add(1)(2));\n\
        const $n = 2;\n\
        outln(switch ($n) { case $n -> 'the case'; default -> 'other'; });\n");
    assert_eq!(out, "[2, 4, 6]\n[2, 3]\n[1, 1, 1]\n3\nthe case\n");
    // Its type comes from where it's passed, like `($v) -> ...`
    assert_eq!(rejected("const $xs = ['a'];\noutln($xs.map($v -> $v * 2));"), "`*` needs numbers, but got a string and a number");
}

/// The warnings of a check, as `line: message`.
fn warnings(source: &str) -> Vec<String> {
    let mut engine = Engine::with_output(CapturedOutput::new());
    if let Err(e) = engine.check_source(source) {
        eprintln!("the check failed: {}", e.message);
    }
    engine.warnings().iter().map(|w| format!("{}: {}", w.span.line, w.message)).collect()
}

/// Names never used and code that never runs *(see Warnings in ideas/tooling/diagnostics.md)*.
#[test]
fn warnings_for_what_is_never_used() {
    let source = "\
use std/fs;
use std/path;
use std/time::{Date};
const $unused = 1;
const $_quiet = 2;
let $assigned = 1;
$assigned = 2;
number twice(number $n, number $extra) = $n * 2;
outln(twice(2, 3));
for ($i in 0..2) { outln('x'); }
for ($_ in 0..2) { outln('y'); }
outln([1, 2].map(($a) -> 1));
outln(path::join('a', 'b'));
void f() {
    return;
    outln('never');
    outln('again');
}
f();
class Base { pub void hello(string $who) { outln($who); } }
class Child extends Base {
    override { pub void hello(string $who) { outln('child'); } }
}
Child().hello('x');
try { throw ValueError('x'); } catch (ValueError $e) { outln('caught'); }
const [$first, $_second] = [1, 2];
pub const number $shared = 3;
";
    assert_eq!(
        warnings(source),
        [
            "1: `fs` is imported but never used",
            "3: `Date` is imported but never used",
            "4: `$unused` is never used. If that's on purpose, call it `$_unused`",
            "8: the parameter `$extra` is never used. If that's on purpose, call it `$_extra`",
            "10: `$i` is never used. If that's on purpose, call it `$_i`",
            "16: this code never runs: the code before it always leaves (`return`, `throw`, `break`, `continue`, `yield` or `exit()`)",
            "26: `$first` is never used. If that's on purpose, call it `$_first`",
        ]
    );
    // Warnings never make a check fail, and a clean script has none
    let mut engine = Engine::with_output(CapturedOutput::new());
    assert!(engine.check_source("const $x = 1;\noutln($x);").is_ok() && engine.warnings().is_empty());
    let mut engine = Engine::with_output(CapturedOutput::new());
    assert!(engine.check_source("const $x = 1;").is_ok());
    assert_eq!(engine.warnings().len(), 1);
    // A function declared after a `return` isn't code that never runs
    assert!(warnings("void f() {\n    return;\n    void g() {}\n}\nf();").iter().all(|w| !w.contains("never runs")), "{:?}", warnings("void f() {\n    return;\n    void g() {}\n}\nf();"));
    // A method that implements an interface keeps its parameters
    assert!(warnings("class Size {\n    pub number @n;\n    implement IEquals {\n        pub bool equals?(Self $other) = true;\n    }\n}\noutln(Size(1).equals?(Size(2)));").is_empty());
}

/// A statement whose value is never used, and a comparison with `null` that's
/// always the same *(see Warnings in ideas/tooling/diagnostics.md)*.
#[test]
fn warnings_for_results_thrown_away() {
    let source = "\
const $name = ' Ana ';
const $list = [3, 1];
const map<string, number> $ages = {};
$name.trim();
$list.sorted();
$list.map($x -> outln($x));
$ages.merged({'b' => 2});
%x\"git status\";
$name + '!';
if ($name == null) { outln('never'); }
if ($list != null) { outln('always'); }
";
    assert_eq!(
        warnings(source),
        [
            "4: the result of `.trim()` is never used, and it doesn't change `$name`",
            "5: the result of `.sorted()` is never used, and it doesn't change `$list`. Did you mean `.sort()`, which changes the list itself?",
            "6: the result of `.map()` is never used, and it doesn't change `$list`. Did you mean `.each()`, which runs a function for each value?",
            "7: the result of `.merged()` is never used, and it doesn't change `$ages`. Did you mean `.merge()`, which changes the map itself?",
            "8: this command never runs: call `.run()` on it, or `.output()` for what it prints",
            "9: the value of this is never used",
            "10: `$name` is never `null` here, since it's a string: this is always false",
            "11: `$list` is never `null` here, since it's a list<number>: this is always true",
        ]
    );
    // What changes its value, calls of functions and methods of classes, `<<`, `&&` and nullable values never warn
    let quiet = "\
let $list = [3, 1];
$list.sort();
$list.push(2);
$list << 4;
const map<string, number> $ages = {};
$ages.set('a', 1);
number twice(number $n) = $n * 2;
twice(2);
class Counter { pub number @n = 0; pub number add() { @n += 1; return @n; } }
const $c = Counter();
$c.add();
const $verbose = true;
$verbose && $list.empty?();
let ?string $maybe = null;
if ($maybe == null) { outln('none'); }
outln($list);
";
    assert_eq!(warnings(quiet), Vec::<String>::new());
}

/// A `switch` with a case for every value, as a statement too, and `case null`
/// *(see ideas/language/flow_control.md)*.
#[test]
fn switches_with_a_case_for_every_value() {
    let colors = "enum Color { RED, GREEN, BLUE }\n";
    // As a statement, it ends a function when each case does
    let full = format!(
        "{colors}number code(Color $c) {{\n    switch ($c) {{\n        case Color::RED -> {{ return 1; }}\n        case Color::GREEN -> {{ return 2; }}\n        case Color::BLUE -> {{ return 3; }}\n    }}\n}}\noutln(code(Color::BLUE));"
    );
    assert_eq!(ok(&full), "3\n");
    let bools = "string yes(bool $b) {\n    switch ($b) {\n        case true -> { return 'y'; }\n        case false -> { return 'n'; }\n    }\n}\noutln(yes(false));";
    assert_eq!(ok(bools), "n\n");
    // One that misses a value doesn't
    let half = format!("{colors}number code(Color $c) {{\n    switch ($c) {{\n        case Color::RED -> {{ return 1; }}\n    }}\n}}\noutln(code(Color::RED));");
    assert_eq!(rejected(&half), "`code` must return a number at the end");
    // A value that can be `null` needs `case null`
    let nullable = format!("{colors}string name(?Color $c) = switch ($c) {{\n    case Color::RED, Color::GREEN -> 'warm';\n    case Color::BLUE -> 'cold';\n}};\noutln(name(null));");
    assert_eq!(rejected(&nullable), "this `switch` has no case for `null`: add it, or a `default`");
    let with_null = format!("{colors}string name(?Color $c) = switch ($c) {{\n    case Color::RED, Color::GREEN -> 'warm';\n    case Color::BLUE -> 'cold';\n    case null -> 'none';\n}};\noutln(name(null));");
    assert_eq!(ok(&with_null), "none\n");
    assert_eq!(
        rejected("const ?bool $b = null;\noutln(switch ($b) { case true -> 'y'; case false -> 'n'; });"),
        "this `switch` has no case for `null`: add it, or a `default`"
    );
    assert_eq!(
        rejected(&format!("{colors}const $c = Color::RED;\noutln(switch ($c) {{ case Color::RED -> 1; }});")),
        "this `switch` has no case for `Color::GREEN`, `Color::BLUE`: add them, or a `default`"
    );
}
