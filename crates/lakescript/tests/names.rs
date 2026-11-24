//! What the checker finds about each name, for editors *(see `ideas/tooling/lsp.md`)*.

use lakescript::{CapturedOutput, Engine, Name};

fn names(source: &str) -> Vec<Name> {
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_names(true);
    engine.check_source(source).unwrap();
    engine.names()
}

/// The name at a character, and the text of its definition.
fn at<'a>(names: &'a [Name], source: &str, text: &str, nth: usize) -> &'a Name {
    let byte = source.match_indices(text).nth(nth).unwrap_or_else(|| panic!("`{text}` isn't in the source")).0;
    let char = source[..byte].chars().count();
    names.iter().filter(|n| n.file.is_none() && n.start <= char && char < n.end).min_by_key(|n| n.end - n.start).unwrap_or_else(|| panic!("nothing is known about `{text}`"))
}

fn definition(source: &str, name: &Name) -> String {
    let d = name.definition.as_ref().expect("it has a definition");
    source.chars().skip(d.start).take(d.end - d.start).collect()
}

fn line_of(source: &str, name: &Name) -> usize {
    let d = name.definition.as_ref().expect("it has a definition");
    source.chars().take(d.start).filter(|c| *c == '\n').count()
}

#[test]
fn variables_and_functions() {
    let source = "/// How many there are\nlet $count = [1, 2];\n$count.push(3);\n\n/// Says hello\nstring greet(string $name, number $times = 1) = $name.repeat($times);\noutln(greet('ana'));\n";
    let names = names(source);
    let count = at(&names, source, "$count", 1);
    assert_eq!(count.code, "list<number> $count");
    assert_eq!(count.doc.as_deref(), Some("How many there are"));
    assert_eq!(definition(source, count), "$count");
    assert_eq!(line_of(source, count), 1);

    let push = at(&names, source, "push", 0);
    assert_eq!(push.about.as_deref(), Some("A method of `list`."));
    assert_eq!(push.code, "list<number> push(...number $values)");
    assert!(push.definition.is_none());

    let greet = at(&names, source, "greet", 1);
    assert_eq!(greet.code, "string greet(string $name, number $times = …)");
    assert_eq!(greet.doc.as_deref(), Some("Says hello"));
    assert_eq!(line_of(source, greet), 5);

    let param = at(&names, source, "$times", 1);
    assert_eq!(param.code, "number $times");
    assert_eq!(param.about.as_deref(), Some("A parameter of `greet`."));
    assert_eq!(param.doc, None);
    assert_eq!(definition(source, param), "$times");
}

#[test]
fn classes_and_their_members() {
    let source = "/// A place\nclass Point {\n    /// Across\n    pub number @x;\n    pub number @y = 0;\n    pub number sum() = @x + @y;\n    pub static Point origin() = Point(0, 0);\n}\n\nPoint $p = Point::origin();\noutln($p.x + $p.sum());\n";
    let names = names(source);
    let class = at(&names, source, "Point $p", 0);
    assert_eq!(class.code, "class Point");
    assert_eq!(class.doc.as_deref(), Some("A place"));
    assert_eq!(line_of(source, class), 1);

    let origin = at(&names, source, "origin()", 1);
    assert_eq!(origin.code, "Point Point::origin()");
    assert_eq!(line_of(source, origin), 6);

    let x = at(&names, source, "x + $p", 0);
    assert_eq!(x.code, "number @x");
    assert_eq!(x.about.as_deref(), Some("A field of `Point`."));
    assert_eq!(x.doc.as_deref(), Some("Across"));
    assert_eq!(definition(source, x), "@x");

    let sum = at(&names, source, "sum()", 1);
    assert_eq!(sum.code, "number sum()");
    assert_eq!(line_of(source, sum), 5);

    // Inside the class, `@y` is the field
    let y = at(&names, source, "@y", 1);
    assert_eq!(definition(source, y), "@y");
    assert_eq!(line_of(source, y), 4);
}

#[test]
fn modules_exceptions_and_types() {
    let source = "use std/fs;\nuse std/path::{name};\nclass Box<T> {\n    pub T @item;\n    pub bool same?(Self $other) = true;\n}\nlet $count = 0;\n$count = 2;\ntry { outln(fs::read('x')); } catch (IOError $e) { outln(name('a')); }\n";
    let names = names(source);
    let fs = at(&names, source, "fs::", 0);
    assert_eq!(fs.code, "module std/fs");
    assert_eq!(fs.about.as_deref(), Some("A module of the standard library."));
    assert_eq!(at(&names, source, "fs;", 0).code, "module std/fs");
    assert_eq!(at(&names, source, "name}", 0).code, "string name(string $path)");
    assert_eq!(at(&names, source, "IOError", 0).code, "class IOError extends Exception");
    assert_eq!(at(&names, source, "Self", 0).code, "class Box<T>");
    assert_eq!(at(&names, source, "T @item", 0).about.as_deref(), Some("A type parameter."));
    // Assigned to: still the variable
    let count = at(&names, source, "$count = 2", 0);
    assert_eq!((count.code.as_str(), line_of(source, count)), ("number $count", 6));
}

#[test]
fn parameters_have_their_line_of_the_comment() {
    let source = "/// Says hello.\n///\n/// $name: who to greet\n/// $times: how many times,\n/// at least 1\nstring greet(string $name, number $times = 1) = $name.repeat($times);\noutln(greet('ana', times: 2));\n";
    let names = names(source);
    // Where it's declared, where it's used, and as a named argument
    for (text, nth) in [("$times", 1), ("$times", 2), ("times", 4)] {
        let param = at(&names, source, text, nth);
        assert_eq!(param.code, "number $times", "{text} {nth}");
        assert_eq!(param.about.as_deref(), Some("A parameter of `greet`."));
        assert_eq!(param.doc.as_deref(), Some("how many times, at least 1"), "{text} {nth}");
        assert_eq!(definition(source, param), "$times");
    }
    assert_eq!(at(&names, source, "$name", 1).doc.as_deref(), Some("who to greet"));
    // Methods too
    let source = "class Box {\n    /// $by: how much bigger\n    pub void grow(number $by) {}\n}\nBox().grow(by: 2);\n";
    let found = self::names(source);
    assert_eq!(at(&found, source, "by:", 1).doc.as_deref(), Some("how much bigger"));
}
