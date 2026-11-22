//! Tracebacks: the calls that led to an exception *(see Tracebacks in
//! `ideas/language/errors.md`)*.

use std::path::{Path, PathBuf};

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
        Err(e) => e.render("test.lks"),
    }
}

/// A folder with these files, for scripts that use modules.
fn folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lks-trace-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

#[test]
fn uncaught_errors_show_their_calls() {
    let e = error("void c(number $n) {\n    [1].removeAt($n);\n}\nvoid b() {\n    c(5);\n}\nb();");
    assert_eq!(e, "  at the script (test.lks:7:1)\n  at b (test.lks:5:5)\ntest.lks:2:5: error: IndexError: index 5 is outside the list, which has 1 elements");
    // A recursion that never ends is one line too
    let e = error("number f(number $n) = f($n + 1);\nf(0);");
    assert!(e.starts_with("  at the script (test.lks:2:1)\n  at f (test.lks:1:23), "), "{e}");
    assert!(e.contains(" times\ntest.lks:1:23: error: too many nested calls"), "{e}");
    // Without calls it looks as it always has
    assert_eq!(error("[1].removeAt(5);"), "test.lks:1:1: error: IndexError: index 5 is outside the list, which has 1 elements");
    // Errors of the checker have none
    assert_eq!(error("void f() {\n    number $n = 'x';\n}"), "test.lks:2:17: error: `$n` is a number, but its value is a string");
}

#[test]
fn with_the_line_of_code() {
    let mut engine = Engine::with_output(CapturedOutput::new());
    let source = "void f() {\n    throw ValueError('no');\n}\nf();\n";
    let e = engine.run(source).unwrap_err();
    assert_eq!(
        e.render_with_source("test.lks", source),
        "  at the script (test.lks:4:1)\nerror: ValueError: no\n --> test.lks:2:5\n  |\n2 |     throw ValueError('no');\n  |     ^^^^^^^^^^^^^^^^^^^^^^^"
    );
}

#[test]
fn names_of_functions() {
    // Methods have their class, lambdas the function they're in, and recursion is one line
    let e = error(
        "class Point {\n    pub number @x;\n    pub number half() = [@x].map(($n) -> [$n].removeAt(4))[0] ?? 0;\n}\n\
         number down(number $n) = $n == 0 ? Point(1).half() : down($n - 1);\n\
         down(100);",
    );
    assert_eq!(
        e,
        "  at the script (test.lks:6:1)\n  at down (test.lks:5:54), 100 times\n  at down (test.lks:5:36)\n  at Point.half (test.lks:3:25)\n\
         test.lks:3:42: error: IndexError: index 4 is outside the list, which has 1 elements"
    );
}

#[test]
fn modules_and_their_paths() {
    let dir = folder(
        "modules",
        &[
            ("report.lks", "pub void build(list<string> $files) {\n    $files.map(($f) -> load($f));\n}\nvoid load(string $f) {\n    [1].removeAt(9);\n}\n"),
            ("main.lks", "use report;\nreport::build(['a']);\n"),
            ("broken.lks", "pub const number $n = [1].removeAt(3);\n"),
            ("loads.lks", "use broken;\noutln(broken::$n);\n"),
        ],
    );
    let report = dir.join("report.lks").canonicalize().unwrap();
    let e = Engine::with_output(CapturedOutput::new()).run_file(&dir.join("main.lks")).unwrap_err();
    let main = dir.join("main.lks").display().to_string();
    let r = report.display();
    assert_eq!(
        e.render(&main),
        format!("  at the script ({main}:2:1)\n  at report::build ({r}:2:5)\n  at a lambda in report::build ({r}:2:24)\n{r}:5:5: error: IndexError: index 9 is outside the list, which has 1 elements")
    );
    // An error while a module loads, the first time it's used
    let e = Engine::with_output(CapturedOutput::new()).run_file(&dir.join("loads.lks")).unwrap_err();
    let shown = e.render("loads.lks");
    assert!(shown.starts_with("  at the script (loads.lks:2:7)\n") && shown.contains("broken.lks:1:23: error: IndexError"), "{shown}");
    let _ = std::fs::remove_dir_all(Path::new(&dir));
}

#[test]
fn trace_of_a_caught_exception() {
    let out = run(
        "void inner() {\n    throw ValueError('bad');\n}\nvoid outer() {\n    inner();\n}\n\
         try {\n    outer();\n} catch ($e) {\n    outln($e.trace().join('\\n'));\n\
         try {\n        throw $e;\n    } catch ($again) {\n        outln($again.trace().length());\n    }\n}\n\
         outln(ValueError('never thrown').trace());\n\
         const $r = try outer();\n\
         outln($r.error()?.trace()?.length());\n",
    );
    assert_eq!(out, "at the script (<script>:8:5)\nat outer (<script>:5:5)\nat inner (<script>:2:5)\n3\n[]\n3\n");
    // Caught, it prints nothing
    assert_eq!(run("try {\n    [1].removeAt(3);\n} catch ($e) {}\noutln('fine');"), "fine\n");
}

#[test]
fn failed_tests_show_their_calls() {
    let dir = folder("tests", &[("t.lks", "void check(number $n) {\n    assert $n == 4;\n}\nvoid testHelper() {\n    check(5);\n}\n")]);
    let results = Engine::with_output(CapturedOutput::new()).run_tests(&dir.join("t.lks"), None).unwrap();
    let shown = lakescript::render_result(&results[0], "t.lks", lakescript::terminal::Paint(false));
    assert!(shown.contains("             at testHelper (t.lks:5:5)\n             t.lks:2:5: assert failed: $n == 4\n"), "{shown}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(all(feature = "dates", feature = "html"))]
#[test]
fn the_standard_library_is_left_out() {
    // Thrown inside `time.lks`, shown where the script called it
    let e = error("use std/time;\nvoid load() {\n    time::parse('22/11/2026');\n}\nload();");
    assert!(e.starts_with("  at the script (test.lks:5:1)\ntest.lks:3:5: error: ValueError: '22/11/2026' isn't a date in ISO 8601"), "{e}");
    assert!(!e.contains("<std>"), "{e}");
    // One called right from the script
    let e = error("use std/html;\nhtml::parse('').select('a[');");
    assert!(e.starts_with("test.lks:2:1: error: ValueError: 'a[' isn't a valid CSS selector"), "{e}");
    // `line()` and `trace()` of a caught one say the same
    let out = run("use std/time;\ntry {\n    time::parse('x');\n} catch (ValueError $e) {\n    outln($e.line());\n    outln($e.trace());\n}");
    assert_eq!(out, "3\n['at the script (<script>:3:5)']\n");
}
