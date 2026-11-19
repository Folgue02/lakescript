//! A script must never crash the program that runs it, however it's written.
//! These run on the test's normal thread, with its default stack.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> Result<String, String> {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    engine.set_type_check(false);
    engine.run(source).map(|_| output.text()).map_err(|e| e.message)
}

#[test]
fn reasonable_nesting_works() {
    let source = format!("outln({}1{});", "(".repeat(100), ")".repeat(100));
    assert_eq!(run(&source).unwrap(), "1\n");
    let source = format!("{}outln('deep');{}", "if (true) { ".repeat(60), "}".repeat(60));
    assert_eq!(run(&source).unwrap(), "deep\n");
}

#[test]
fn deep_parentheses_are_an_error_not_a_crash() {
    let source = format!("outln({}1{});", "(".repeat(100_000), ")".repeat(100_000));
    assert!(run(&source).unwrap_err().contains("nested too deeply"));
}

#[test]
fn deep_unary_operators_are_an_error_not_a_crash() {
    for op in ["!", "-", "~", "+"] {
        let source = format!("outln({}1);", op.repeat(100_000));
        assert!(run(&source).unwrap_err().contains("nested too deeply"), "{op}");
    }
}

#[test]
fn deep_lists_and_blocks_are_an_error_not_a_crash() {
    let source = format!("const $l = {}1{};", "[".repeat(100_000), "]".repeat(100_000));
    assert!(run(&source).unwrap_err().contains("nested too deeply"));
    let source = format!("{}{}", "{ ".repeat(100_000), "}".repeat(100_000));
    assert!(run(&source).unwrap_err().contains("nested too deeply"));
    let source = format!("const $x = {}1{};", "true ? ".repeat(50_000), " : 0".repeat(50_000));
    assert!(run(&source).unwrap_err().contains("nested too deeply"));
}

#[test]
fn deep_interpolation_is_an_error_not_a_crash() {
    let mut source = String::from("1");
    for _ in 0..100 {
        source = format!("f\"{{{source}}}\"");
    }
    assert!(run(&format!("outln({source});")).unwrap_err().contains("nested too deeply"));
}

#[test]
fn endless_recursion_is_an_error_not_a_crash() {
    assert!(run("number f(number $n) = f($n + 1); f(0);").unwrap_err().contains("too many nested calls"));
    assert!(run("void f() { [1].map(($x) = f()); } f();").unwrap_err().contains("too many nested calls"));
}
