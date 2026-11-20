//! Classes *(phase 7, see `ideas/language/classes.md`)*: fields, constructors,
//! methods, statics, inheritance, and what the checker rejects.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

/// The checker's error, or the error when it runs.
fn rejected(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(source) {
        Ok(_) => panic!("expected an error:\n{source}"),
        Err(e) => e.message,
    }
}

/// The error while running, without the checker.
fn fails(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    engine.set_type_check(false);
    match engine.run(source) {
        Ok(_) => panic!("expected an error:\n{source}"),
        Err(e) => e.message,
    }
}

const POINT: &str = "
class Point {
    pub number @x;
    pub number @y = 0;
    pub ?string @label;

    pub number length() = (@x ** 2 + @y ** 2) ** 0.5;
    pub Point moved(number $dx) = Point(@x + $dx, @y, label: @label);
    pub static Point origin() = Point(0, 0, label: 'origin');
}
";

#[test]
fn objects_are_created_by_calling_the_class() {
    let script = format!(
        "{POINT}
        const $p = Point(3, 4);
        outln($p);
        outln($p.length());
        outln($p.moved(1).x);
        outln(Point(x: 5));
        outln(Point::origin().label);
        outln([$p.is(Point), (1).is(Point), $p.typeName()]);"
    );
    assert_eq!(
        run(&script),
        "Point(x: 3, y: 4, label: null)\n5\n4\nPoint(x: 5, y: 0, label: null)\norigin\n[true, false, 'Point']\n"
    );
}

#[test]
fn fields_can_be_changed() {
    let script = format!(
        "{POINT}
        const $p = Point(1);
        $p.x = 10;
        $p.y += 2;
        $p.y++;
        outln(f\"{{$p.x}} {{$p.y}} {{$p.y++}} {{$p.y}}\");"
    );
    assert_eq!(run(&script), "10 3 3 4\n");
}

#[test]
fn constructors_store_fields_and_run_their_code() {
    let script = "
        class User {
            pub string @name;
            pub const number @id;
            list<string> @tags = [];
            pub new(string $raw, @id) {
                @name = $raw.trim();
                @tags.push('new');
            }
            pub list<string> tags() = @tags;
        }
        const $a = User('  ana ', 1);
        const $b = User('bo', 2);
        $a.tags().push('admin');
        outln([$a.name, $a.id, $a.tags(), $b.tags()]);
    ";
    assert_eq!(run(script), "['ana', 1, ['new', 'admin'], ['new']]\n");
}

#[test]
fn static_members_belong_to_the_class() {
    let script = "
        class Ticket {
            pub number $issued = 0;
            pub const string $prefix = 'T-';
            pub const string @code;
            pub new() {
                Self::$issued += 1;
                @code = f\"{Self::$prefix}{Ticket::$issued}\";
            }
            pub static number count() = Self::$issued;
        }
        Ticket();
        outln(Ticket().code);
        outln([Ticket::count(), Ticket::$issued]);
    ";
    assert_eq!(run(script), "T-2\n[2, 2]\n");
}

#[test]
fn subclasses_override_and_call_super() {
    let script = "
        class Animal {
            pub string @name;
            pub new(string $name) {
                @name = $name;
            }
            pub string sound() = '...';
            pub string describe() = f\"{@name} says {@sound()}\";
        }
        class Dog extends Animal {
            pub bool @good;
            pub new(string $name, @good) {
                const $upper = $name.upper();
                @super($upper);
            }
            override {
                pub string sound() = 'woof';
                pub string describe() = @super.describe() + (@good ? '!' : '?');
            }
        }
        const $d = Dog('rex', true);
        outln($d.describe());
        Animal $a = $d;
        outln([$a.sound(), $a.is(Dog), $a.is(Animal), Animal('cat').is(Dog)]);
    ";
    assert_eq!(run(script), "REX says woof!\n['woof', true, true, false]\n");
}

#[test]
fn the_constructor_with_every_field_includes_the_superclass() {
    let script = "
        class Base { pub string @kind; }
        class Item extends Base { pub number @size = 1; }
        outln(Item('box'));
        outln(Item('box', 3).size);
    ";
    assert_eq!(run(script), "Item(kind: 'box', size: 1)\n3\n");
}

#[test]
fn closures_in_methods_keep_the_object() {
    let script = "
        class Counter {
            number @count = 0;
            pub () -> number counter() = () -> ++@count;
        }
        const $f = Counter().counter();
        $f();
        $f();
        outln($f());
    ";
    assert_eq!(run(script), "3\n");
}

#[test]
fn classes_in_blocks_and_functions() {
    let script = "
        void main() {
            Box make() = Box(7);
            class Box { pub number @x; }
            outln(make().x);
        }
        main();
    ";
    assert_eq!(run(script), "7\n");
}

#[test]
fn the_checker_finds_mistakes_with_classes() {
    let p = "class P { pub number @x; number @hidden = 0; pub const number @id = 1; pub void m() {} }\n";
    assert!(rejected(&format!("{p}P('a');")).contains("`x` of `P` must be a number"));
    assert!(rejected(&format!("{p}P();")).contains("`P` needs a value for `x`"));
    assert!(rejected(&format!("{p}outln(P(1).hidden);")).contains("private to `P`"));
    assert!(rejected(&format!("{p}outln(P(1).nope);")).contains("`P` has no field named `nope`"));
    assert!(rejected(&format!("{p}P(1).id = 2;")).contains("`const` field"));
    assert!(rejected(&format!("{p}P(1).nope();")).contains("`P` has no method `.nope()`"));
    assert!(rejected(&format!("{p}string $s = P(1);")).contains("its value is a P"));
    assert!(rejected("outln(@);").contains("`@` can only be used"));
    assert!(rejected("class S { pub static void s() = outln(@); }").contains("a static method has no `@`"));
    assert!(rejected("class B { pub void hi() {} }\nclass C extends B { pub void hi() {} }").contains("goes in an `override"));
    assert!(rejected("class B {}\nclass C extends B { override { pub void hi() {} } }").contains("has no method with that name"));
    assert!(rejected("class B { pub new() {} }\nclass C extends B {}").contains("needs its own `new`"));
    assert!(rejected("class B { pub new() {} }\nclass C extends B { pub new() {} }").contains("must call `@super(...)`"));
    assert!(rejected("P(1);\nclass P { pub number @x; }").contains("declared further down"));
    assert!(rejected("class C { pub const number $max = 3; }\nC::$max = 4;").contains("is a constant"));
    assert!(rejected("class lower {}").contains("PascalCase"));
    assert!(rejected("class A extends A {}").contains("can't extend itself"));
    assert!(rejected("outln([1].length);").contains("Methods are called with parentheses"));
}

#[test]
fn without_the_checker_the_vm_still_says_what_is_wrong() {
    assert!(fails(&format!("{POINT}Point();")).contains("`Point` needs a value for `x`"));
    assert!(fails(&format!("{POINT}Point(1, 2, 'a', 4);")).contains("takes 3 arguments"));
    assert!(fails(&format!("{POINT}Point(1, z: 2);")).contains("no field named `z`"));
    assert!(fails("class P { pub const number @x; }\nP(1).x = 2;").contains("`const` field"));
    assert!(fails("outln(1.is(Thing));").contains("unknown type `Thing`"));
}
