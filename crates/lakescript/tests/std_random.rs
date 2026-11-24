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
