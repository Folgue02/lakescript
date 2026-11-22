//! Ctrl+C as an `Interrupted`, beside `Exception` *(see Ctrl+C in
//! `ideas/language/errors.md`)*. The tests set the engine's flag, as the
//! handler of Ctrl+C of `lks` does.

use std::sync::atomic::Ordering;

use lakescript::{CapturedOutput, Engine, Outcome};

/// Runs `source` with the flag set from another thread a moment after it starts.
fn interrupted(source: &str) -> (Result<Outcome, String>, String) {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    let flag = engine.interrupt_flag();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        flag.store(true, Ordering::Relaxed);
    });
    let result = engine.run(source).map_err(|e| e.render("test.lks"));
    (result, output.text())
}

#[test]
fn only_a_catch_of_interrupted_catches_it() {
    let (result, out) = interrupted(
        "try {\n    while (true) {}\n} catch (Interrupted $e) {\n    outln($e.typeName());\n    outln($e.message());\n}\noutln('goes on');",
    );
    assert_eq!(result, Ok(Outcome::Finished));
    assert_eq!(out, "Interrupted\nthe script was interrupted with Ctrl+C\ngoes on\n");

    // `catch ($e)` and `catch (Exception $e)` don't, but `finally` runs, and the script stops with 130
    let (result, out) = interrupted("try {\n    for ($i in 0..1000000000) {}\n} catch ($e) {\n    outln('wrong');\n} finally {\n    outln('finally');\n}");
    assert_eq!(result, Ok(Outcome::Exit(130)));
    assert_eq!(out, "finally\n");
    let (result, out) = interrupted("try {\n    while (true) {}\n} catch (Exception $e) {\n    outln('wrong');\n}");
    assert_eq!((result, out.as_str()), (Ok(Outcome::Exit(130)), ""));
}

#[test]
fn neither_do_try_expressions_nor_test_throws() {
    let (result, out) = interrupted("number spin() {\n    while (true) {}\n    return 1;\n}\nconst $r = try spin();\noutln('wrong');");
    assert_eq!((result, out.as_str()), (Ok(Outcome::Exit(130)), ""));
    let (result, out) = interrupted("use std/test;\ntest::throws(() { while (true) {} });\noutln('wrong');");
    assert_eq!((result, out.as_str()), (Ok(Outcome::Exit(130)), ""));
}

#[test]
fn recursion_is_interrupted_too() {
    // Calls without a loop, so the check at each call stops it
    let (result, _) = interrupted("void again(number $n) {\n    if ($n > 100) {\n        again(0);\n    } else {\n        again($n + 1);\n    }\n}\nagain(0);");
    // Too deep a recursion could come first; either way it stops
    assert!(matches!(result, Ok(Outcome::Exit(130))) || result.as_ref().is_err_and(|e| e.contains("too many nested calls")), "{result:?}");
}

#[test]
fn it_can_be_thrown_and_is_a_type_of_its_own() {
    let mut engine = Engine::with_output(CapturedOutput::new());
    assert_eq!(engine.run("throw Interrupted('by hand');").unwrap(), Outcome::Exit(130));
    let out = CapturedOutput::new();
    let mut engine = Engine::with_output(out.clone());
    engine.run("const $e = Interrupted('x');\noutln([$e.is(Interrupted), $e.is(Exception)]);").unwrap();
    assert_eq!(out.text(), "[true, false]\n");
}
