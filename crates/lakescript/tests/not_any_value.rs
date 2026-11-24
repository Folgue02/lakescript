//! `!` on any value: NOT on a `bool`, and on anything else whether it's
//! `null` *(see Boolean operators in `ideas/language/operators.md`)*.

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
fn not_null_is_true_and_other_values_are_false() {
    assert_eq!(run("outln(!null);"), "true\n");
    assert_eq!(run("outln(!true);\noutln(!false);"), "false\ntrue\n");
    // Empty values aren't false
    assert_eq!(run("outln(!'');\noutln(!0);\noutln(![]);\noutln(!{});"), "false\nfalse\nfalse\nfalse\n");
    assert_eq!(run("?string $s = null;\noutln(!$s);\n$s = 'a';\noutln(!$s);"), "true\nfalse\n");
    // `!!` gives a value as a `bool`
    assert_eq!(run("?string $s = 'a';\nbool $has = !!$s;\noutln($has);"), "true\n");
    // A `?bool`: NOT when it's a bool
    assert_eq!(run("?bool $b = false;\noutln(!$b);\n$b = null;\noutln(!$b);"), "true\ntrue\n");
}

#[test]
fn it_narrows_like_a_check_for_null() {
    let code = "string name(?string $n) {\n    if (!$n) return 'nobody';\n    return $n.upper();\n}\noutln(name(null));\noutln(name('ana'));";
    assert_eq!(run(code), "nobody\nANA\n");
    let code = "string name(?string $n) {\n    if (!!$n) return $n.upper();\n    return 'nobody';\n}\noutln(name('bo'));";
    assert_eq!(run(code), "BO\n");
    // A `?bool` doesn't narrow: `!$b` is also true for `false`
    let code = "bool f(?bool $b) {\n    if (!$b) return true;\n    return $b;\n}\noutln(f(true));";
    assert!(error(code).contains("bool"), "{}", error(code));
}

#[test]
fn conditions_still_need_a_bool() {
    assert!(error("?string $s = 'a';\nif ($s) outln('yes');").contains("must be a bool"));
    assert!(error("?string $s = 'a';\nwhile ($s) {}").contains("must be a bool"));
    assert!(error("?string $s = 'a';\noutln($s && true);").contains("bool"));
}
