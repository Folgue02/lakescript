# Classes, interfaces and enums

Phase 7 *(see [roadmap.md](../roadmap.md))*. This note is being designed: what's here is decided, and what isn't yet is listed at the end.

## Where they're declared

Classes, interfaces and enums are declared inside a module, with `class`, `interface` and `enum`, like functions. There is no special kind of file for them: a file is always a module, so the same code means the same thing in every file.

```
// src/folgue02/people/ChildClass.lks
use folgue02/people/BaseClass;

pub class ChildClass extends BaseClass {
    ...
}
```

- A module can declare any number of types. A standalone script often declares several.
- In a project, the convention is a file per type, named like the type *(i.e. `ChildClass.lks` for `ChildClass`)*.
- The name of an interface starts with `I` *(i.e. `IEquals`, `IShape`)*, so it's never confused with a class. Any other name is an error.
- They're called interfaces, not traits: a class says which interfaces it implements when it's declared, and they can't be added to existing types *(i.e. `string`)* later.

## Names

- Types are written in PascalCase *(i.e. `ChildClass`, `PersonStatus`)*. So are the files that hold a single type.
- Modules and folders are written in lowercase, with words separated by `_` *(i.e. `text_utils`)*.
- So the case tells what a name is: `ChildClass::calculateAge()` is a static method, and `paths::load()` is a function of a module.
- There are no type aliases *(see [functions.md](functions.md#no-type-aliases))*, so a type's name always means a class, an interface, an enum or a built-in type.

## Classes

```
class ChildClass extends BaseClass {
    pub string @name;
    pub number @age;

    pub new(@name, number $age) {
        @age = Self::calculateAge($age);
    }

    pub static number calculateAge(number $age) = $age + 10;

    override {
        void sayHello() = outln(f"Hello, I'm {@name}");
    }

    implement IEquals {
        bool equals?(Self $other) = $other.name == @name && $other.age == @age;
    }
}

const $child = ChildClass('Ana', 20);
outln($child.name);
```

- A field can get its value where it's declared *(i.e. `list<T> @items = [];`)*. It's computed again for every new object, before `new` runs, so two objects never share it. A `const` field gets its value there or in `new`, never in both.
- Inside a class, `@` is the object itself and `@name` its field `name`. From outside, a field is reached with `.` *(i.e. `$child.name`)*.
- A field declared with `const` *(i.e. `pub const string @id;`)* can only be set in the constructor, and never changes after it.
- The constructor is declared as `new`, and objects are created by calling the class, like in Python *(i.e. `ChildClass('Ana', 20)`)*. Exceptions are already created this way *(i.e. `ValueError('bad port')`)*.
    - A function can't have the name of a class, so `Point(1, 2)` always creates a `Point`.
    - The types of a generic class go between `<>` after its name when nothing infers them *(i.e. `Stack<string>()`)*.
    - A parameter written as a field *(i.e. `new(@name, ...)`)* is stored in that field. Its type is the field's, so it isn't written twice.
- A class without its own `new` gets one that takes every field, in the order they're declared. A field's value becomes the parameter's default, and a nullable field without one defaults to `null`:

```
class Point {
    pub number @x;
    pub number @y = 0;
    pub ?string @label;
}

Point(1, 2);              // x: 1, y: 2, label: null
Point(x: 5);              // y: 0, label: null
Point();                  // error: `x` has no value
```

- It only exists while the class has no `new` of its own: once there is one, it's the only constructor.
    - Private fields are parameters too, so no field is ever left without a value. To hide a field from the callers, give it a value or write a `new`.
    - The defaults are computed again on every call, so two objects never share a list.
    - With a superclass, its fields come first. This only works when the superclass also has the constructor that takes every field. If it has its own `new`, the class has to write one too, and call the superclass's with `@super(...)`.
    - Enum values use it too *(i.e. `SICK(canWork: false)` without a `new` in the enum)*.
- `@super(...)` can go anywhere in `new`, like in Java 25. Before it, `new` can do any work and set its own class's fields, but can't read `@` or call its methods, since the superclass's fields aren't set yet. It has to run exactly once on every path through `new`. The checker finds both mistakes before the script runs.
- `@super` is the object seen as its superclass: `@super(...)` calls its constructor, and `@super.sayHello()` calls the superclass's version of a method that's overridden. `super` can't be the name of a field.
- Methods that replace the superclass's go in an `override { ... }` block, and the methods of an interface in an `implement Interface { ... }` block. So where a method is says where it comes from, and two interfaces with a method of the same name don't clash.
    - A class implements an interface when it has its `implement` block, so `.is(IEquals)` can be checked when the script runs.

## Generic classes

Classes can be generic, with the same `<T>` as functions and interfaces. The types are inferred from what's given to `new`, and written only when nothing tells them *(i.e. an empty stack)*:

```
class Stack<T> {
    list<T> @items = [];

    pub void push(T $item) {
        @items.push($item);
    }

    pub ?T pop() = @items.removeLast();
}

const $names = Stack<string>();
$names.push('Ana');
```

- Like everywhere else, the types are only checked before the script runs, so a `Stack<string>` and a `Stack<number>` are the same class when it runs.
    - So `.is(Stack<string>)` is an error that says to use `.is(Stack)`.
    - A generic class can extend a class or implement an interface with types *(i.e. `class Counts extends Stack<number>`)*.

## Static members

- A field written with `$` instead of `@` belongs to the class, not to each object *(i.e. `number $created = 0;`, `pub const string $kind = 'child';`)*. It's set the first time the class is used, like a module's constants.
- A method marked with `static` belongs to the class, and can't use `@` *(i.e. `pub static number calculateAge(number $age) = ...;`)*.
- They're always reached with `::` on the class *(i.e. `ChildClass::$kind`, `ChildClass::calculateAge(3)`)*, never through an object. Inside the class too, so a name without a prefix in a method is always a local or something of the module:

| Written | Lives in |
|---|---|
| `@name`, `@sayHello()` | the object |
| `Self::$created`, `Self::calculateAge()` | the class |
| `$x`, `helper()` | the method, or the module |

- `Self` means exactly the same as writing the name of the class *(i.e. `Self::$created` is `ChildClass::$created`)*.

## Interfaces

- In a class, `Self` is the class. In an interface, it's the class that implements it, so `Self copy();` returns a `ChildClass` for a `ChildClass`.
- An interface has methods, with or without a body. Every class that implements it writes the ones without a body in its `implement` block, and can replace the others, its defaults. Inside a default, `@` is the object, seen as the interface.
- The methods of an interface, and of the blocks that implement it, are always public, so they don't take `pub`.
- An interface has no fields and no constructor, and calling it is an error. It can have static methods.
- A class with a superclass keeps its interfaces. A method of an interface goes in its `implement` block, never with the class's own methods.
- An interface can extend other interfaces *(i.e. `interface IOrdered extends IEquals { ... }`)*. A class that implements it also implements the ones it extends, so `.is()` is true for all of them.

## Enums

Like Java's: an enum has a fixed set of values, each created once, and can have fields, methods and `implement` blocks. Values that each carry their own data *(Rust's enums)* are written as classes that extend the same base class.

```
enum PersonStatus {
    AT_WORK,
    UNEMPLOYED,
    SICK;

    bool canWork() =
        switch (@) {
            case Self::SICK, Self::UNEMPLOYED -> false;
            default -> true;
        };
}

const $status = PersonStatus::SICK;
outln($status);                                  // PersonStatus::SICK
outln($status.name());                           // SICK
outln(PersonStatus::values());                   // every value, in order
outln(PersonStatus::from('SICK'));               // throws if there's no such value
outln(PersonStatus::fromIgnoreCase('sick'));
```

- Values are written in SCREAMING_CASE and reached with `::` *(i.e. `PersonStatus::SICK`)*. Inside the enum, `@` is the value itself.
- A `switch` with a case for every value doesn't need a `default`.
- Printing a value shows its enum and its name *(i.e. `PersonStatus::SICK`)*.
- Values are created once, in order, when the enum is declared, and calling the enum is an error. Two values are `==` only if they're the same value.
- `from()` and `fromIgnoreCase()` throw a `ValueError` that lists the values when there's no value with that name.
- `values`, `from`, `fromIgnoreCase` and `name` belong to every enum, so an enum can't declare its own.
- Without the checker, a `switch` used as a value with a case for every value is still accepted, and it throws when no case matches.
- Like in Java, values can have fields, given when they're declared through the enum's constructor. It's the one that takes every field unless the enum writes a `new`, and it can't be called from anywhere else:

```
enum PersonStatus {
    AT_WORK(true),
    UNEMPLOYED(false),
    SICK(canWork: false);

    pub const bool @canWork;
}

outln(PersonStatus::SICK.canWork);   // false
```

## Printing

An object prints as its class and its fields, in the order of the constructor that takes every field *(i.e. `Point(x: 3, y: 4, label: 'home')`)*. Strings get quotes, like inside a list. A class prints as `<class Point>`.

## Equality and operators

- `==` on two objects calls `equals?` when their class implements `IEquals`. Otherwise it's true only for the same object. Objects of classes that aren't related *(neither is the other's superclass)* are never equal, without calling `equals?`.
- `$a.same?($b)` is always true only for the same object. Every value has it.
- The other operators work the same way: on objects, each one calls the method of an interface the class implements. Numbers, strings, lists and maps keep working as they do now, without calling anything.
- These interfaces are built in, so they're used without `use`. Each operator has its own, so a class only gets the ones that make sense for it *(i.e. a `Path` with `/` but no `+`)*:

| Interface | Method | Gives |
|---|---|---|
| `IEquals` | `bool equals?(Self $other)` | `==` and `!=` |
| `IOrdered` | `number compare(Self $other)`: negative, zero or positive | `<`, `<=`, `>`, `>=`, and `.sort()` |
| `IAdd<R, O>` | `O add(R $other)` | `+` |
| `ISub<R, O>` | `O sub(R $other)` | `-` |
| `IMul<R, O>` | `O mul(R $other)` | `*` |
| `IDiv<R, O>` | `O div(R $other)` | `/` |
| `IMod<R, O>` | `O mod(R $other)` | `%` |
| `IPow<R, O>` | `O pow(R $other)` | `**` |
| `INeg<O>` | `O neg()` | `-$x` |
| `IIndex<K, V>` | `V get(K $key)` | `$x[...]` |
| `IContains<T>` | `bool contains(T $value)` | `-in` and `-notin` |

- `R` is the type of the right side and `O` the type of the result, so `$vector * 2` and a dot product `$vector * $other` can both exist. The checker knows the type of `$a + $b` from the method.
- The object goes on the left side: `$v * 2` works, `2 * $v` doesn't. For `-in`, the object is on the right *(i.e. `'x' -in $bag`)*.
- `-` followed by a letter is a test operator *(i.e. `-f $path`)*, so the negative of an object is written `- Vector(1, 2)` or `-$v`.
- `&&`, `||` and `??` stay built-in, since they skip their right side, and so do `=`, `.`, `..`, `.is()` and `.as()`. Setting an index *(i.e. `$x[0] = 1`)* doesn't work on objects yet.
- Interfaces can be generic, with the same `<T>` as functions, and a class can implement the same interface several times with different types. The types of the arguments pick which method runs: the checker picks it before the script runs, and when it runs, the first version whose parameters' types match the arguments runs. With an operator, that's the type of the right side:

```
class Vector {
    pub const number @x;
    pub const number @y;

    implement IMul<number, Vector> {
        Vector mul(number $n) = Vector(@x * $n, @y * $n);
    }

    implement IMul<Vector, number> {
        number mul(Vector $other) = @x * $other.x + @y * $other.y;
    }
}

const $v = Vector(1, 2);
outln(($v * 3).x);                 // 3
outln($v * Vector(3, 4));          // 11
```
- A `Self` parameter called through a value whose type is the interface accepts any value of that interface. The method checks when it runs that it got its own class, and throws a `TypeError` if not.

## Circular imports

With a file per type, types that use each other *(i.e. `Order` has a `Customer`, and `Customer` a `list<Order>`)* need their modules to import each other, so modules can import each other in a cycle.

- Everything `pub` has its type written: functions their return type, `pub const` its type, fields theirs. So a module's signatures are read straight from its declarations, without checking any code, and a module in the middle of being checked can already give them to the modules that use it. There is no separate pass over every module first.
- When a module is loaded, its functions and classes are defined before its constants are computed, so calling a function of a module that's still loading always works.
- Reading a constant that isn't computed yet is an error that names the chain *(i.e. `a::$x` needs `b::f()`, which reads `a::$x`)*. Since the top level of a module only has declarations, constants are the only code that can get there.
- A class that extends itself, directly or through others, is an error.

## Importing a type

When the last part of a path is in PascalCase, the module is the type: `use` imports the type itself, not a namespace.

```
use folgue02/people/ChildClass;              // the class ChildClass
use folgue02/people/ChildClass as Child;     // with another name
use folgue02/geo/shapes::{Circle, Square};   // a lowercase module, as always
```

- A PascalCase module declares a `pub` type with exactly its name *(i.e. `pub class ChildClass` in `ChildClass.lks`)*, and that's the only thing it makes `pub`. It can have private functions, constants, `let`s and types to help it.
- `::` after a type module is an error for now *(i.e. `use people/ChildClass::{calculateAge}`)*. It could later import static members *(i.e. `use x/PersonStatus::{SICK}`)*.
- The case of the file has to match exactly, even on file systems that ignore it *(i.e. macOS)*, so a script that works on one machine works on all. Two files in the same folder whose names only differ in case or in `_` *(i.e. `ChildClass.lks` and `child_class.lks`)* are an error.
- Only the last part of the path can be in PascalCase: folders are always lowercase.
- Importing two types with the same name is an error, so one of them needs `as` *(i.e. `use b/User as RemoteUser;`)*.

## Status

Built in steps, each with tests and examples:

1. Done: classes. Fields with and without values, `const` fields, the constructor that takes every field, `new` with `@field` parameters, methods, `@`, static fields and methods with `Self::`, `extends`, `override`, `@super(...)` and `@super.method()`, `.is()` and `.as()` with classes (also of another module, i.e. `shapes::Point`), and `pub class` in modules. The checker knows them, and private fields and methods can only be used by their own class. See `examples/classes.lks`.
2. Done: interfaces. `implement` blocks, default methods, interfaces that extend others, `Self`, the `TypeError` for a `Self` parameter of another class, `.is()` and `.as()` with interfaces, and the checker's errors for missing or wrong methods. See `examples/interfaces.lks`.
3. Done: enums. Values with fields through the constructor, methods, `implement` blocks, `name()`, `values()`, `from()`, `fromIgnoreCase()`, and the checker's error for a `switch` used as a value that misses a value. See `examples/enums.lks`.
4. Done: generic classes and interfaces. `Stack<string>()`, types found out from the constructor's arguments, superclasses and interfaces with types *(i.e. `extends Stack<number>`)*, and a class that implements the same interface with different types. See `examples/generics.lks`.
5. Done: operators. The built-in interfaces behind them, `==` with `IEquals`, `.sort()` with `IOrdered`, and `same?()`. See `examples/operators.lks`.
6. Done: exceptions as classes. A class extends a built-in exception, gets the field `message`, and is thrown and caught like one *(see [errors.md](errors.md))*. See `examples/errors.lks`.
7. Done: importing a type from its own file *(i.e. `use people/Person;`)* with its rules, and modules that use each other. See `examples/shop/`.
8. Done: `command` and `subcommand` *(see [cli.md](cli.md))*. See `examples/todo.lks`.

Not working yet: two different interfaces with a method of the same name in one class.

Not checked yet: that `@` isn't read before `@super(...)`, and that `@super(...)` runs once on every path. The checker only checks that the constructor has one.

## Not decided yet

- Nothing right now.
