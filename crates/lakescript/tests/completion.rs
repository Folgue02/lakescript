//! What can be written at a place, for editors *(see `ideas/tooling/lsp.md`)*.

use lakescript::{CapturedOutput, Completion, CompletionKind, Engine};

/// What can be written where `|` is.
fn complete(source: &str) -> Completion {
    let offset = source[..source.find('|').expect("the source has a `|`")].chars().count();
    let source = source.replacen('|', "", 1);
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_completion(None, offset);
    let _ = engine.check_source(&source);
    engine.completion().expect("something is known there")
}

fn labels(items: &[lakescript::CompletionItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn members_after_a_dot() {
    let found = complete("let $xs = [1, 2];\n$xs.|\nouts(1);\n");
    let members = found.members.expect("the type of `$xs` is known");
    let push = members.iter().find(|i| i.label == "push").unwrap();
    assert_eq!(push.kind, CompletionKind::Method);
    assert_eq!(push.detail, "list<number> push(...number $values)");
    // And what every value has
    assert!(labels(&members).contains(&"toString"));
    // Also with the start of the name written
    let found = complete("let $name = 'ana';\noutln($name.up|);\n");
    assert!(labels(&found.members.unwrap()).contains(&"upper"));
}

#[test]
fn members_of_objects() {
    let class = "class Point {\n    pub number @x;\n    number @secret = 1;\n    pub number sum() = @x;\n    number hidden() = 1;\n}\n";
    let found = complete(&format!("{class}const $p = Point(1);\noutln($p.|);\n"));
    let members = found.members.unwrap();
    assert!(labels(&members).contains(&"x") && labels(&members).contains(&"sum"), "{members:?}");
    // Private members only inside the class
    assert!(!labels(&members).contains(&"secret") && !labels(&members).contains(&"hidden"));
    let found = complete(&class.replace("pub number sum() = @x;", "pub number sum() = @|x;"));
    assert!(labels(&found.own).contains(&"secret") && labels(&found.own).contains(&"hidden"), "{:?}", found.own);
}

#[test]
fn the_scope_at_the_place() {
    let source = "const $top = 1;\nvoid other(number $far) { let $no = 1; }\nnumber f(number $a) {\n    let $b = 2;\n    |\n    let $c = 3;\n    return $a;\n}\n";
    let found = complete(source);
    let names = labels(&found.scope);
    for here in ["$top", "$a", "$b", "f", "other", "outln"] {
        assert!(names.contains(&here), "`{here}` is in scope: {names:?}");
    }
    for not_here in ["$c", "$far", "$no"] {
        assert!(!names.contains(&not_here), "`{not_here}` isn't in scope: {names:?}");
    }
    let b = found.scope.iter().find(|i| i.label == "$b").unwrap();
    assert_eq!((b.kind, b.detail.as_str()), (CompletionKind::Variable, "number $b"));
    // A `for` variable, inside its loop only
    let found = complete("for ($item in [1]) {\n    outln(|);\n}\n");
    assert!(labels(&found.scope).contains(&"$item"));
    let found = complete("for ($item in [1]) {\n    outln(1);\n}\n|\n");
    assert!(!labels(&found.scope).contains(&"$item"));
}

#[test]
fn what_modules_and_classes_have() {
    let source = "use std/fs;\nenum Color { RED, GREEN }\nclass Point {\n    pub static Point origin() = Point();\n}\n|\n";
    let found = complete(source);
    assert!(labels(&found.paths["Color"]).contains(&"GREEN"));
    assert!(labels(&found.paths["Point"]).contains(&"origin"));
    let fs = labels(&found.paths["fs"]);
    assert!(fs.contains(&"read"), "{fs:?}");
    let kinds: Vec<_> = found.scope.iter().filter(|i| ["fs", "Color", "Point"].contains(&i.label.as_str())).map(|i| i.kind).collect();
    assert_eq!(kinds.len(), 3);
    assert!(kinds.contains(&CompletionKind::Module) && kinds.contains(&CompletionKind::Enum) && kinds.contains(&CompletionKind::Class));
    // While `fs::` is being written
    let found = complete("use std/fs;\nouts(fs::|);\n");
    assert!(labels(&found.paths["fs"]).contains(&"read"));
}
