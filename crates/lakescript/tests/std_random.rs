//! `std/random` *(see `ideas/stdlib/random.md`)*.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(&format!("use std/random;\n{source}")) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

fn error(source: &str) -> String {
    match Engine::with_output(CapturedOutput::new()).run(&format!("use std/random;\n{source}")) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.message,
    }
}

#[test]
fn numbers_are_in_their_range() {
    let code = "for ($i in 0..2000) {\n    const $n = random::number();\n    assert $n >= 0 && $n < 1;\n    const $d = random::between(1, 6);\n    assert $d >= 1 && $d <= 6 && $d.whole?();\n}\noutln('ok');";
    assert_eq!(run(code), "ok\n");
    // Every value comes up, both ends too
    assert_eq!(run("const $seen = <number>[];\nfor ($i in 0..500) $seen.push(random::between(-1, 1));\noutln($seen.unique().sorted());"), "[-1, 0, 1]\n");
    assert_eq!(run("outln(random::between(5, 5));"), "5\n");
    assert_eq!(run("outln(random::between(-9007199254740991, 9007199254740991).whole?());"), "true\n");
    let both = run("const $seen = <bool>[];\nfor ($i in 0..200) $seen.push(random::bool());\noutln(true -in $seen && false -in $seen);");
    assert_eq!(both, "true\n");
}

#[test]
fn a_seed_repeats_the_numbers_also_of_lists() {
    let code = "random::seed(7);\nconst $a = [random::number(), random::between(1, 100), random::bool(), [1, 2, 3, 4, 5].shuffled(), [1, 2, 3].random()];\nrandom::seed(7);\nconst $b = [random::number(), random::between(1, 100), random::bool(), [1, 2, 3, 4, 5].shuffled(), [1, 2, 3].random()];\noutln($a.equals($b));\nrandom::seed(8);\noutln(random::number() != $a[0]);";
    assert_eq!(run(code), "true\ntrue\n");
}

#[test]
fn uuids_are_version_4_and_not_seeded() {
    let code = "const $re = %r\"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$\";\nrandom::seed(1);\nconst $a = random::uuid();\nrandom::seed(1);\nconst $b = random::uuid();\noutln($a -match $re);\noutln($a != $b);";
    assert_eq!(run(code), "true\ntrue\n");
}

#[test]
fn wrong_arguments() {
    assert!(error("random::between(6, 1);").contains("`$min` to be at most `$max`, but got 6 and 1"));
    assert!(error("random::between(1.5, 3);").contains("whole number"));
    assert!(error("random::seed('x');").contains("number"));
}

#[test]
fn the_state_can_be_saved_and_restored() {
    // The numbers after `restore()` are the ones after `state()`, also for shuffling
    let code = "random::seed(42);\nconst $s = random::state();\nconst $a = [random::between(1, 100), random::number(), [1, 2, 3, 4].shuffled()];\nrandom::restore($s);\nconst $b = [random::between(1, 100), random::number(), [1, 2, 3, 4].shuffled()];\noutln($a.equals($b));";
    assert_eq!(run(code), "true\n");
    // A string, which JSON can hold
    assert_eq!(run("use std/format/json;\nconst $s = random::state();\nconst $n = random::number();\nrandom::restore(json::parse(json::stringify($s)));\noutln(random::number() == $n);"), "true\n");
    assert!(error("random::restore('nope');").contains("isn't a state of a generator"));
    assert!(error("random::restore('0000000000000000');").contains("isn't a state"));
}

#[test]
fn a_random_of_its_own() {
    // The same seed gives the same numbers, whatever else uses the shared generator
    let code = "const $a = random::Random(7);\nconst $first = [$a.between(1, 1000), $a.between(1, 1000)];\nrandom::seed(1);\nrandom::number();\nconst $b = random::Random(7);\noutln($first.equals([$b.between(1, 1000), $b.between(1, 1000)]));";
    assert_eq!(run(code), "true\n");
    // It doesn't move the shared one forward
    assert_eq!(run("random::seed(3);\nconst $n = random::number();\nrandom::seed(3);\nrandom::Random(9).number();\noutln(random::number() == $n);"), "true\n");
    let code = "const $r = random::Random(5);\nconst $s = $r.state();\nconst $x = $r.number();\n$r.restore($s);\nconst $y = $r.number();\n$r.seed(5);\noutln([$x == $y, $r.number() == $x, $r.bool() == true || true, random::Random().state() != random::Random().state()]);";
    assert_eq!(run(code), "[true, true, true, true]\n");
    // A state of one goes into another, and into the shared one
    assert_eq!(run("const $r = random::Random(5);\nrandom::restore($r.state());\noutln(random::number() == $r.number());"), "true\n");
    assert_eq!(run("outln(random::Random(1));"), "Random()\n");
}
