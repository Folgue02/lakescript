//! Types in the same folder, used without `use` *(see Types in the same
//! folder in `ideas/language/modules.md`)*.

use std::path::PathBuf;

use lakescript::{CapturedOutput, Engine};

/// A folder that's removed at the end of the test.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lks-siblings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path.canonicalize().unwrap())
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Checks and runs `main.lks`, and gives what it printed.
fn run(dir: &TempDir, source: &str) -> String {
    let script = dir.file("main.lks", source);
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run_file(&script) {
        panic!("{}\n--- script:\n{source}\n--- printed:\n{}", e.render("main.lks"), output.text());
    }
    output.text()
}

fn error(dir: &TempDir, source: &str) -> String {
    let script = dir.file("main.lks", source);
    match Engine::with_output(CapturedOutput::new()).run_file(&script) {
        Ok(_) => panic!("expected an error:\n{source}"),
        Err(e) => e.render("main.lks"),
    }
}

#[test]
fn a_script_uses_the_types_next_to_it() {
    let dir = TempDir::new("script");
    dir.file("Customer.lks", "pub class Customer {\n    pub string @name;\n    pub Role @role = Role::USER;\n}\n");
    dir.file("Role.lks", "pub enum Role { USER, ADMIN }\n");
    let out = run(&dir, "const $c = Customer('Ana', Role::ADMIN);\noutln(f\"{$c.name} is {$c.role}, {Customer('Bo').role}\");\n");
    assert_eq!(out, "Ana is Role::ADMIN, Role::USER\n");
}

#[test]
fn modules_in_a_folder_use_each_other() {
    let dir = TempDir::new("folder");
    dir.file("shop/Order.lks", "pub class Order {\n    pub Customer @customer;\n    pub list<Item> @items = [];\n\n    pub string describe() = f\"{@items.length()} for {@customer.name}\";\n}\n");
    dir.file("shop/Customer.lks", "pub class Customer {\n    pub string @name;\n    pub list<Order> @orders = [];\n}\n");
    dir.file("shop/Item.lks", "pub class Item {\n    pub string @name;\n\n    pub static Item pen() = Item('pen');\n}\n");
    let out = run(&dir, "use shop/Order;\nuse shop/Customer;\nuse shop/Item;\nconst $o = Order(Customer('Ana'));\n$o.items.push(Item::pen());\noutln($o.describe());\n");
    assert_eq!(out, "1 for Ana\n");
    // From another folder, they still need their `use`
    let err = error(&dir, "const $i = Item('x');\n");
    assert!(err.contains("Item"), "{err}");
}

#[test]
fn what_the_file_gives_comes_first() {
    let dir = TempDir::new("first");
    dir.file("Item.lks", "pub class Item {\n    pub string @name = 'from the folder';\n}\n");
    dir.file("other/Item.lks", "pub class Item {\n    pub string @name = 'from a use';\n}\n");
    // A `use` of another type with that name
    assert_eq!(run(&dir, "use other/Item;\noutln(Item().name);\n"), "from a use\n");
    // A type of the file
    assert_eq!(run(&dir, "class Item {\n    pub string @name = 'from the file';\n}\noutln(Item().name);\n"), "from the file\n");
    // A type parameter
    assert_eq!(run(&dir, "class Box<Item> {\n    pub Item @value;\n}\noutln(Box(5).value);\n"), "5\n");
    // And the folder last
    assert_eq!(run(&dir, "outln(Item().name);\n"), "from the folder\n");
    // A `use` of it still works
    assert_eq!(run(&dir, "use Item;\noutln(Item().name);\n"), "from the folder\n");
}

#[test]
fn only_types() {
    let dir = TempDir::new("types");
    dir.file("helpers.lks", "pub number twice(number $n) = $n * 2;\n");
    let err = error(&dir, "outln(helpers::twice(2));\n");
    assert!(err.contains("helpers"), "{err}");
    // A name that's in no file is the error it always was
    let err = error(&dir, "const $x = Missing();\n");
    assert!(err.contains("Missing"), "{err}");
}

#[test]
fn the_checker_sees_their_types() {
    let dir = TempDir::new("checker");
    dir.file("Point.lks", "pub class Point {\n    pub number @x;\n}\n");
    let script = dir.file("main.lks", "const string $s = Point(1).x;\n");
    let err = Engine::with_output(CapturedOutput::new()).check_file(&script).unwrap_err().render("main.lks");
    assert!(err.contains("number") && err.contains("string"), "{err}");
}

#[test]
fn a_file_named_like_a_type_must_declare_it() {
    let dir = TempDir::new("named");
    dir.file("Thing.lks", "pub number thing() = 1;\n");
    let err = error(&dir, "outln(\n    Thing());\n");
    // The error is where the type is used
    assert!(err.contains("must declare `pub class Thing`") && err.contains("main.lks:2:"), "{err}");
}

#[test]
fn enum_values_and_files_named_in_another_case_arent_types() {
    // On macOS and Windows `b.lks` is found as `B.lks` too
    let dir = TempDir::new("case");
    dir.file("b.lks", "pub number x() = 1;\n");
    assert_eq!(run(&dir, "enum E { A, B }\noutln(E::B);\n"), "E::B\n");
    // A type named like a file of another case isn't that file
    let script = dir.file("other.lks", "const B $b = null;\n");
    let err = Engine::with_output(CapturedOutput::new()).run_file(&script).unwrap_err().message;
    assert!(err.contains("`B`"), "{err}");
}

/// A lowercase module also sees the types in the folder named like it, after
/// its own folder *(i.e. `config.lks` and `config/AppConfig.lks`)*.
#[test]
fn a_module_sees_the_types_in_its_own_folder() {
    let dir = TempDir::new("named-folder");
    dir.file("something/else/config/AppConfig.lks", "pub class AppConfig {\n    pub string @name = 'app';\n    pub Level @level = Level::LOW;\n}\n");
    dir.file("something/else/config/Level.lks", "pub enum Level { LOW, HIGH }\n");
    dir.file("something/else/config.lks", "pub AppConfig load() = AppConfig();\npub Level level() = load().level;\n");
    assert_eq!(run(&dir, "use something/else/config;\noutln([config::load().name, config::level()]);\n"), "['app', Level::LOW]\n");
    // The folder next to it first: a type there is the one it means
    dir.file("something/else/Level.lks", "pub enum Level { NEXT }\n");
    dir.file("something/else/config.lks", "pub Level level() = Level::NEXT;\n");
    assert_eq!(run(&dir, "use something/else/config;\noutln(config::level());\n"), "Level::NEXT\n");
    // A type module's folder isn't its parts
    dir.file("shop/Order.lks", "pub class Order { pub Line @line = Line(); }\n");
    dir.file("shop/Order/Line.lks", "pub class Line {}\n");
    assert!(error(&dir, "use shop/Order;\noutln(Order());\n").contains("unknown type `Line`"));
    // And a standalone script, with the folder named like it
    dir.file("main/Point.lks", "pub class Point { pub number @x = 1; }\n");
    assert_eq!(run(&dir, "outln(Point().x);\n"), "1\n");
}
