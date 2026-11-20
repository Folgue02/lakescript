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

const SHAPES: &str = "
interface INamed {
    string name();
    string greeting() = f\"Hello, {@name()}\";
}
interface IPet extends INamed {
    string sound();
}
interface IEquals {
    bool equals?(Self $other);
    bool notEquals?(Self $other) = !@equals?($other);
}
class Dog {
    pub string @called;
    implement IEquals {
        bool equals?(Self $other) = $other.called == @called;
    }
    implement IPet {
        string name() = @called;
        string sound() = 'woof';
    }
}
class Robot {
    implement IEquals {
        bool equals?(Self $other) = true;
    }
    implement INamed {
        string name() = 'robot';
        string greeting() = 'BEEP';
    }
}
";

#[test]
fn interfaces_give_methods_and_defaults() {
    let script = format!(
        "{SHAPES}
        const $rex = Dog('rex');
        outln([$rex.equals?(Dog('rex')), $rex.notEquals?(Dog('max'))]);
        outln([$rex.greeting(), Robot().greeting()]);
        const list<INamed> $all = [$rex, Robot()];
        outln($all.map(($n) -> $n.name()));
        outln([$rex.is(IPet), $rex.is(INamed), $rex.is(IEquals), Robot().is(IPet), Robot().is(INamed)]);"
    );
    assert_eq!(run(&script), "[true, true]\n['Hello, rex', 'BEEP']\n['rex', 'robot']\n[true, true, true, false, true]\n");
}

#[test]
fn self_in_an_interface_is_the_class_that_implements_it() {
    let script = "
        interface ICopyable { Self copy(); }
        class Box {
            pub number @size;
            implement ICopyable { Self copy() = Box(@size + 1); }
        }
        outln(Box(1).copy().copy().size);
    ";
    assert_eq!(run(script), "3\n");
    // Through the interface, another class gets there, and the method says so
    let script = format!(
        "{SHAPES}
        IEquals $e = Dog('rex');
        try {{ $e.equals?(Robot()); }} catch (TypeError $x) {{ outln($x.message()); }}"
    );
    assert_eq!(run(&script), "`Dog.equals?` needs a `Dog` for `$other`, not a Robot\n");
}

#[test]
fn subclasses_keep_the_interfaces() {
    let script = format!(
        "{SHAPES}
        class Puppy extends Dog {{}}
        INamed $n = Puppy('bit');
        outln([$n.greeting(), $n.is(IPet), $n.is(Dog)]);"
    );
    assert_eq!(run(&script), "['Hello, bit', true, true]\n");
}

#[test]
fn the_checker_finds_mistakes_with_interfaces() {
    let i = "interface INamed { string name(); string hi() = 'hi ' + @name(); }\n";
    assert!(rejected(&format!("{i}class A {{ implement INamed {{ }} }}")).contains("`A` implements `INamed`, but it has no method `name`"));
    assert!(rejected(&format!("{i}class A {{ implement INamed {{ number name() = 1; }} }}")).contains("must be `string name()`"));
    assert!(rejected(&format!("{i}class A {{ implement INamed {{ string name() = 'a'; string x() = 'b'; }} }}")).contains("`INamed` has no method `x`"));
    assert!(rejected(&format!("{i}class A {{ string name() = 'a'; implement INamed {{ }} }}")).contains("goes in its `implement INamed"));
    assert!(rejected(&format!("{i}class A extends INamed {{}}")).contains("it doesn't extend it"));
    assert!(rejected(&format!("{i}INamed();")).contains("it doesn't create objects"));
    assert!(rejected("class X {}\nclass A { implement X {} }").contains("can only implement interfaces"));
    assert!(rejected("interface IA extends IB {}\ninterface IB extends IA {}").contains("can't extend itself"));
    assert!(rejected("interface IM { void m(); }\ninterface IN extends IM { void n(); }\nclass C { implement IN { void n() {} } }").contains("has no method `m`"));
    assert!(rejected("interface IM {}\nclass C { implement IM {} implement IM {} }").contains("two `implement` blocks"));
    assert!(rejected("interface Named {}").contains("starts with `I`"));
    assert!(rejected("interface IM { void m(); }\nclass A { implement IM { void m() {} } }\nclass B { }\nIM $i = B();").contains("its value is a B"));
}

const STATUS: &str = "
enum Status {
    AT_WORK(true),
    UNEMPLOYED(false),
    SICK(canWork: false);

    pub const bool @canWork;

    pub string label() =
        switch (@) {
            case Self::SICK -> 'sick';
            case Self::UNEMPLOYED -> 'looking';
            case Self::AT_WORK -> 'working';
        };
}
";

#[test]
fn enums_have_a_fixed_set_of_values() {
    let script = format!(
        "{STATUS}
        const $s = Status::SICK;
        outln($s);
        outln([$s.name(), $s.canWork, $s.label()]);
        outln(Status::values());
        outln([Status::from('UNEMPLOYED').label(), Status::fromIgnoreCase('at_work').name()]);
        outln([$s == Status::SICK, $s == Status::AT_WORK, $s.is(Status)]);
        try {{ Status::from('nope'); }} catch (ValueError $e) {{ outln($e.message()); }}"
    );
    assert_eq!(
        run(&script),
        "Status::SICK\n['SICK', false, 'sick']\n[Status::AT_WORK, Status::UNEMPLOYED, Status::SICK]\n['looking', 'AT_WORK']\n[true, false, true]\n`Status` has no value `nope`: it has AT_WORK, UNEMPLOYED, SICK\n"
    );
}

#[test]
fn enums_can_have_a_constructor_and_interfaces() {
    let script = "
        interface ILabeled { string label(); }
        enum Size {
            SMALL(1), LARGE(10);
            pub const number @weight;
            pub new(number $kilos) {
                @weight = $kilos * 1000;
            }
            implement ILabeled { string label() = @name().lower(); }
        }
        outln([Size::LARGE.weight, Size::SMALL.label(), Size::SMALL.is(ILabeled)]);
    ";
    assert_eq!(run(script), "[10000, 'small', true]\n");
}

#[test]
fn the_checker_finds_mistakes_with_enums() {
    let e = "enum C { RED, GREEN, BLUE; }\n";
    assert!(rejected(&format!("{e}outln(switch (C::RED) {{ case C::RED -> 1; case C::GREEN -> 2; }});")).contains("no case for `C::BLUE`"));
    assert!(rejected(&format!("{e}C();")).contains("is an enum"));
    assert!(rejected(&format!("{e}outln(C::PURPLE);")).contains("`C` has no value `PURPLE`: it has RED, GREEN, BLUE"));
    assert!(rejected(&format!("{e}string $s = C::RED;")).contains("its value is a C"));
    assert!(rejected("enum C { red; }").contains("SCREAMING_CASE"));
    assert!(rejected("enum S { A(1), B; pub number @n; }").contains("`B` needs a value for `n`"));
    assert!(rejected("enum S { A; pub static void values() {} }").contains("every enum has `values()`"));
    assert!(rejected("enum C { A; }\nenum D extends C {}").contains("can't extend"));
    assert!(fails(&format!("{e}C();")).contains("is an enum"));
}

const STACK: &str = "
class Stack<T> {
    list<T> @items = [];
    pub void push(T $item) {
        @items.push($item);
    }
    pub ?T pop() = @items.removeLast();
    pub number size() = @items.length();
}
";

#[test]
fn generic_classes_keep_their_types() {
    let script = format!(
        "{STACK}
        const $names = Stack<string>();
        $names.push('ana');
        outln($names.pop()?.upper());
        const $numbers = Stack();
        $numbers.push(2);
        outln(($numbers.pop() ?? 0) + 1);
        class Pair<A, B> {{
            pub A @first;
            pub B @second;
            pub Pair<B, A> swapped() = Pair(@second, @first);
        }}
        const $q = Pair('x', 1).swapped();
        outln([$q.first + 1, $q.second.upper()]);
        class Counts extends Stack<number> {{
            pub number total() {{
                let $sum = 0;
                while (@size() > 0) {{ $sum += @pop() ?? 0; }}
                return $sum;
            }}
        }}
        const $c = Counts();
        $c.push(3);
        $c.push(4);
        outln($c.total());"
    );
    assert_eq!(run(&script), "ANA\n3\n[2, 'X']\n7\n");
}

#[test]
fn an_interface_can_be_implemented_with_several_types() {
    let script = "
        interface IMul<R, O> { O mul(R $by); }
        class Vector {
            pub number @x;
            pub number @y;
            implement IMul<number, Vector> {
                Vector mul(number $n) = Vector(@x * $n, @y * $n);
            }
            implement IMul<Vector, number> {
                number mul(Vector $other) = @x * $other.x + @y * $other.y;
            }
        }
        const $v = Vector(1, 2);
        outln([$v.mul(3).y, $v.mul(Vector(3, 4)) + 1]);
        IMul<number, Vector> $m = $v;
        outln($m.mul(5));
    ";
    assert_eq!(run(script), "[6, 12]\nVector(x: 5, y: 10)\n");
}

#[test]
fn the_checker_finds_mistakes_with_generics() {
    assert!(rejected(&format!("{STACK}const $s = Stack<string>();\n$s.push(1);")).contains("must be a string, but this is a number"));
    assert!(rejected(&format!("{STACK}Stack<string> $s = Stack<number>();")).contains("its value is a Stack<number>"));
    assert!(rejected(&format!("{STACK}Stack<string, number> $s = Stack();")).contains("takes 1 type parameter"));
    assert!(rejected(&format!("{STACK}const $s = Stack();\n$s.push(1);\n$s.push('a');")).contains("must be a number, but this is a string"));
    assert!(rejected("class Box<T> { pub T @v; }\nclass Ints extends Box<number> {}\nInts('x');").contains("must be a number"));
    assert!(rejected("interface IGet<T> { T get(); }\nclass C { implement IGet<number> { string get() = 'x'; } }").contains("must be `number get()`"));
    assert!(fails("class Box<T> { pub T @v; }\noutln(Box(1).is(Box<number>));").contains("Use `.is(Box)`"));
}
