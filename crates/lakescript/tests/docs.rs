//! Reading the documentation of a module *(see `ideas/tooling/docs.md`)*.

use lakescript::docs::{ItemKind, command_doc, module_doc, parse_doc};

#[test]
fn the_lines_of_a_doc_comment() {
    let doc = parse_doc(
        "Reads a size written as text.\n\nIn any case.\n\n$text: a number,\nthen a unit\nreturns: the size\nthrows ValueError: when the unit\nisn't known\nthrows IOError: never",
    );
    assert_eq!(doc.text, "Reads a size written as text.\n\nIn any case.");
    assert_eq!(doc.params, [("text".to_string(), "a number, then a unit".to_string())]);
    assert_eq!(doc.returns.as_deref(), Some("the size"));
    assert_eq!(doc.throws, [("ValueError".into(), "when the unit isn't known".into()), ("IOError".into(), "never".into())]);
    assert_eq!(doc.summary(), "Reads a size written as text.");
    // `$` that isn't a parameter's line stays text
    assert_eq!(parse_doc("Costs $5: cheap").text, "Costs $5: cheap");
}

#[test]
fn a_module_and_its_items() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/sizes/src/folgue02/sizes/units.lks")).unwrap();
    let module = module_doc(&source).unwrap();
    assert!(module.text.as_deref().unwrap().starts_with("Sizes in bytes, read and written the way people do"));
    let names: Vec<&str> = module.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["$units", "human", "bytes"]);
    let bytes = &module.items[2];
    assert_eq!(bytes.kind, ItemKind::Function);
    assert_eq!(bytes.code, "pub number bytes(string $text)");
    assert_eq!(bytes.doc.params[0].0, "text");
    assert_eq!(bytes.doc.throws[0].0, "ValueError");
    assert_eq!(module.items[0].code, "pub const list<string> $units = ['B', 'KB', 'MB', 'GB', 'TB']");
}

#[test]
fn classes_interfaces_and_enums() {
    let module = module_doc(
        "/// A shape.\npub interface IShape {\n    /// Its area.\n    number area();\n    string describe() = 'a shape';\n}\n\
         pub interface ISolid extends IShape {\n    number volume(number $depth = 1);\n}\n\
         /// A circle.\npub class Circle {\n    /// The radius.\n    pub number @radius;\n    number @hidden = 0;\n    implement IShape {\n        number area() = 3 * @radius ** 2;\n    }\n    /// Makes one.\n    pub static Circle unit() = Circle(1);\n}\n\
         pub enum Planet {\n    /// The closest.\n    MERCURY(0.39, 0.06),\n    EARTH(1, 1);\n    pub const number @distance;\n    pub const number @mass;\n}\n\
         class Private {}\nnumber hidden() = 1;\n",
    )
    .unwrap();
    let names: Vec<&str> = module.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["IShape", "ISolid", "Circle", "Planet"]);

    let shape = &module.items[0];
    assert_eq!((shape.kind, shape.doc.text.as_str()), (ItemKind::Interface, "A shape."));
    assert_eq!(shape.members.iter().map(|m| (m.name.as_str(), m.default)).collect::<Vec<_>>(), [("area", false), ("describe", true)]);
    assert_eq!(shape.members[0].doc.text, "Its area.");
    assert_eq!(module.items[1].extends, ["IShape"]);
    assert_eq!(module.items[1].members[0].code, "number volume(number $depth = 1)");

    let circle = &module.items[2];
    assert_eq!(circle.implements, ["IShape"]);
    let members: Vec<&str> = circle.members.iter().map(|m| m.code.as_str()).collect();
    // The methods of an `implement` block are public through their interface
    assert_eq!(members, ["pub number @radius", "number area()", "pub static Circle unit()"]);
    assert_eq!(circle.members[1].from.as_deref(), Some("IShape"));
    assert_eq!(circle.members[2].doc.text, "Makes one.");

    let planet = &module.items[3];
    assert_eq!(planet.members[0].kind, ItemKind::Value);
    assert_eq!(planet.members[0].args, ["0.39", "0.06"]);
    assert_eq!(planet.members[0].doc.text, "The closest.");
    assert_eq!(planet.members.iter().filter(|m| m.kind == ItemKind::Field).count(), 2);
}

#[test]
fn the_command_a_script_parses() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/todo.lks")).unwrap();
    let command = command_doc(&source, "todo").unwrap().expect("todo.lks parses a command");
    assert_eq!(command.text.as_deref(), Some("A small todo list, to show commands"));
    assert_eq!(command.usage, "todo [options] <command>");
    assert_eq!(command.args[0].name, "-v, --verbose");
    assert!(command.args[0].repeats);
    let add = &command.subcommands[0];
    assert_eq!(add.name, "todo add");
    assert_eq!(add.usage, "todo add [options] <task>");
    let names: Vec<&str> = add.args.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["<task>", "-p, --priority", "-t, --tag"]);
    assert!(add.args[0].required);
    assert_eq!(add.args[1].choices, ["low", "normal", "high"]);
    assert_eq!(add.args[1].default.as_deref(), Some("normal"));
    assert!(add.args[2].repeats);
    // A script that doesn't parse a command has none
    assert_eq!(command_doc("command A {}\noutln(1);", "a").unwrap(), None);
    assert_eq!(command_doc("outln(1);", "a").unwrap(), None);
}

#[test]
fn a_comment_for_a_parameter_that_isnt_there_is_a_warning() {
    use lakescript::docs::stale_params;
    let source = "/// Greets.\n///\n/// $name: who\n/// $times: how often\npub string greet(string $who, number $times = 1) = $who;\n";
    let warnings = stale_params(source).unwrap();
    assert_eq!(warnings.len(), 1);
    let w = &warnings[0];
    assert_eq!(w.message, "the comment describes `$name`, but `greet` has no parameter `$name`: its parameters are `$who`, `$times`");
    assert_eq!((w.line, w.column), (3, 5));
    assert_eq!(&source.chars().skip(w.start).take(w.end - w.start).collect::<String>(), "$name");
    // Methods and constructors too, and not the parameters that are there
    let source = "class Box {\n    pub number @size;\n    /// $n: how much\n    pub void grow(number $by) {}\n    /// $by: fine\n    pub void shrink(number $by) {}\n}\n";
    let warnings = stale_params(source).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("`Box.grow` has no parameter `$n`: its only parameter is `$by`"), "{}", warnings[0].message);
    assert_eq!(warnings[0].line, 3);
    assert!(stale_params("/// $x: nothing\nvoid f() {}\n").unwrap()[0].message.ends_with("it has no parameters"));
    // It never stops a script
    let mut engine = lakescript::Engine::with_output(lakescript::CapturedOutput::new());
    engine.run("/// $nope: gone\nvoid f() {}\nf();").unwrap();
}

/// Interfaces named with `implements` are implemented like those with a block.
#[test]
fn interfaces_named_in_the_signature() {
    let module = module_doc(
        "pub interface ITagged {\n    string tag() = 'untitled';\n}\npub interface IShape {\n    number area();\n}\n\
         pub class Note implements ITagged {\n    pub string @text;\n    implement IShape {\n        number area() = 1;\n    }\n}\n",
    )
    .unwrap();
    let note = module.items.iter().find(|i| i.name == "Note").unwrap();
    assert_eq!(note.implements, ["ITagged", "IShape"]);
    assert_eq!(note.code, "pub class Note");
}
