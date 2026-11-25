//! JSON read into types, and types given as values *(see Into types in
//! `ideas/stdlib/format/json.md`)*.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(&format!("use std/format/json;\n{source}")) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

fn error(source: &str) -> String {
    match Engine::with_output(CapturedOutput::new()).run(&format!("use std/format/json;\n{source}")) {
        Ok(_) => panic!("it should have failed:\n{source}"),
        Err(e) => e.message,
    }
}

const CLASSES: &str = "enum Role { USER, ADMIN }\nclass Tag {\n    pub string @name;\n}\nclass User {\n    pub string @name;\n    pub ?string @email;\n    pub list<Tag> @tags = [];\n    pub Role @role = Role::USER;\n}\n";

#[test]
fn classes_lists_maps_and_enums_are_filled_in() {
    let code = format!("{CLASSES}const $users = json::parse('[{{\"name\": \"Ana\", \"tags\": [{{\"name\": \"dev\"}}], \"role\": \"ADMIN\", \"other\": 1}}, {{\"name\": \"Bo\"}}]', list<User>);\noutln($users);\nconst list<User> $typed = $users;\noutln($typed[0]!.tags[0]!.name);");
    assert_eq!(
        run(&code),
        "[User(name: 'Ana', email: null, tags: [Tag(name: 'dev')], role: Role::ADMIN), User(name: 'Bo', email: null, tags: [], role: Role::USER)]\ndev\n"
    );
    let code = format!("{CLASSES}const $m = json::parse('{{\"a\": {{\"name\": \"x\"}}}}', map<string, Tag>);\noutln($m['a']!.name);\noutln(json::parse('[1, 2]', list<number>).sum());\noutln(json::parse('\"ADMIN\"', Role) == Role::ADMIN);");
    assert_eq!(run(&code), "x\n3\ntrue\n");
    // parseAll reads each value into it
    let code = format!("{CLASSES}outln(json::parseAll('{{\"name\": \"a\"}}\\n{{\"name\": \"b\"}}', Tag).map(($t) -> $t.name));");
    assert_eq!(run(&code), "['a', 'b']\n");
    // Without a type, it's `any`
    assert_eq!(run("const $v = json::parse('{\"a\": [1]}');\noutln($v['a'][0]);"), "1\n");
}

#[test]
fn values_that_dont_fit_say_where() {
    let e = error(&format!("{CLASSES}json::parse('[{{\"name\": \"a\"}}, {{\"name\": 3}}]', list<User>);"));
    assert!(e.contains("`$[1].name` is a number, not a string"), "{e}");
    let e = error(&format!("{CLASSES}json::parse('{{\"tags\": []}}', User);"));
    assert!(e.contains("`$` has no `name`, which `User` needs"), "{e}");
    let e = error(&format!("{CLASSES}json::parse('{{\"name\": \"a\", \"tags\": [{{}}]}}', User);"));
    assert!(e.contains("`$.tags[0]` has no `name`, which `Tag` needs"), "{e}");
    let e = error(&format!("{CLASSES}json::parse('{{\"name\": \"a\", \"role\": \"BOSS\"}}', User);"));
    assert!(e.contains("'BOSS', which isn't a value of `Role`: USER, ADMIN"), "{e}");
    let e = error(&format!("{CLASSES}json::parse('{{\"name\": \"a\", \"tags\": {{}}}}', User);"));
    assert!(e.contains("`$.tags` is an object, not an array for `list<Tag>`"), "{e}");
    assert!(error("json::parse('{}', map<number, string>);").contains("the keys of JSON objects are strings"));
}

#[test]
fn the_checker_knows_the_type() {
    // The result has the type given, so a wrong use is an error before it runs
    let e = error(&format!("{CLASSES}const number $n = json::parse('{{}}', User);"));
    assert!(e.contains("User"), "{e}");
    let e = error(&format!("{CLASSES}outln(json::parse('[]', list<User>)[0]!.nope);"));
    assert!(e.contains("nope"), "{e}");
    // Types are values, of the type `type<T>`
    assert_eq!(run("const $t = list<string>;\noutln($t);\noutln($t.is(type<list<string>>));"), "<type list<string>>\ntrue\n");
}

#[test]
fn classes_of_other_modules_are_found() {
    let dir = std::env::temp_dir().join(format!("lks-json-types-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("shapes.lks"), "pub class Point {\n    pub number @x;\n    pub number @y;\n}\npub class Line {\n    pub Point @from;\n    pub Point @to;\n}\n").unwrap();
    let main = dir.join("main.lks");
    std::fs::write(&main, "use std/format/json;\nuse shapes;\nclass Drawing {\n    pub list<shapes::Line> @lines;\n}\nconst $d = json::parse('{\"lines\": [{\"from\": {\"x\": 1, \"y\": 2}, \"to\": {\"x\": 3, \"y\": 4}}]}', Drawing);\noutln($d.lines[0]!.to.x);\n").unwrap();
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run_file(&main) {
        panic!("{}", e.render("main.lks"));
    }
    assert_eq!(output.text(), "3\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn objects_are_written_with_every_field() {
    let code = format!("{CLASSES}class Account extends User {{\n    number @visits = 3;\n}}\nconst $u = User('Ana', null, [Tag('a')], Role::ADMIN);\noutln(json::stringify($u));");
    assert_eq!(run(&code), "{\"name\":\"Ana\",\"email\":null,\"tags\":[{\"name\":\"a\"}],\"role\":\"ADMIN\"}\n");
    // Private fields too, so reading it back gives the same object
    let secret = "class Secret {\n    pub string @name;\n    number @visits = 0;\n}\nconst $s = Secret('x', 7);\nconst $text = json::stringify($s);\noutln($text);\noutln(json::parse($text, Secret));";
    assert_eq!(run(secret), "{\"name\":\"x\",\"visits\":7}\nSecret(name: 'x', visits: 7)\n");
    let back = format!("{CLASSES}const $u = User('Ana', 'a@b.c', [Tag('a'), Tag('b')], Role::ADMIN);\nconst $again = json::parse(json::stringify([$u]), list<User>);\noutln($again);");
    assert_eq!(run(&back), "[User(name: 'Ana', email: 'a@b.c', tags: [Tag(name: 'a'), Tag(name: 'b')], role: Role::ADMIN)]\n");
    assert_eq!(run(&format!("{CLASSES}outln(json::stringify(Tag('a'), indent: 2));")), "{\n  \"name\": \"a\"\n}\n");
}

#[test]
fn what_json_cant_hold_says_where() {
    let code = "class Keys {\n    pub any @onKey = () -> 1;\n}\njson::stringify({'screen' => [Keys()]});";
    assert_eq!(error(code), "TypeError: `$.screen[0].onKey` is a function, which JSON can't have");
    assert_eq!(error("json::stringify(() -> 1);"), "TypeError: a function can't be written as JSON");
    assert_eq!(error("json::stringify({'a b' => 0 / 0});"), "TypeError: `$[\"a b\"]` is NaN, which JSON can't have");
}
