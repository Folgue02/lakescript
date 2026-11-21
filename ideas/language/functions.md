# Functions

- Should allow parameter types
- Should allow default values. They're evaluated on every call, so `list<string> $items = []` gives a new list each time.
- Should allow varargs. Inside the function, `...string $names` is a `list<string>`.
- A list can be spread into the arguments with `...` *(i.e. `greet(...$names)`)*.
- Should allow calling them while naming parameters. Named and positional arguments can be mixed, but positional ones go first *(i.e. `createFile('daniel', extension: 'jpg')`)*.
- Must always have a return type. Functions that don't return anything use `void` *(i.e. `void log(string $message) { ... }`)*. This way a definition never looks like a call followed by a block.
- Should allow generics. The type parameters go after the name *(i.e. `T first<T>(list<T> $items) = $items[0];`)*. They're inferred when calling, so they're never written in a call: `first($names)`.
- Functions can call each other in any order: a function can call any function declared in the same block or around it, even one further down, so recursive functions that call each other work *(i.e. a parser with `expr()` and `term()`)*. A function can call itself.
    - The statements of the block itself can only call a function after its declaration, since it doesn't exist before that line runs *(i.e. `f();` above `void f() { ... }` is an error)*. The checker says so before anything runs.
    - The signatures of a block's functions are known from the start of the block, so calls to functions further down are checked like any other.

```
bool even(number $n) = if ($n == 0) true else odd($n - 1);
bool odd(number $n) = if ($n == 0) false else even($n - 1);
outln(even(10));    // true
```
- Functions can use the variables and constants declared outside them, as long as they're declared before the function. `let` variables can be read and changed, `const` ones can only be read:

```
const $usersPath = 'users.json';
let $loaded = 0;

void load() {
    readFile($usersPath);   // OK, reads a constant from outside
    $loaded++;              // OK, changes a variable from outside
}
```
- Should allow one line definitions with `=`. The expression after `=` is the return value *(i.e. `string greet(string $name) = f"Hello {$name}";`)*. Without `=`, the body must be inside curly braces.
- Must always be called with parentheses, even when there are no arguments *(i.e. `greet()`)*. A name without parentheses refers to the function itself, without calling it *(i.e. `$list.map(greet)`)*
- Should allow anonymous declarations *(see [Anonymous functions](#anonymous-functions))*
- Can be passed as parameters
- The type for a function is written as `(string, number) -> bool` *(see [Function types](#function-types))*

## Anonymous functions

Written like a function definition, without the return type and the name. They're the only functions without a written return type: it's inferred from what they return.

```
// With types
const $describe = (string $name, number $age) {
    return f"{$name} is {$age}";
};

// Without types, they're inferred
$scores.filter(($name, $score) {
    return $score > 20;
});

// One line, the expression after `->` is the return value
$fruits.map(($fruit) -> $fruit.upper());
```

- When an anonymous function is passed where a function type is expected, the parameters without a type take the types from it. In `$fruits.map(($fruit) -> ...)`, `$fruit` is a `string` because `$fruits` is a `list<string>`. Otherwise they're `any`.
- One-line anonymous functions use `->`, and named functions use `=`, like in Kotlin *(`fun f(x) = ...` and `{ x -> ... }`)*: `($x) -> $x + 1` and `() -> 42` are anonymous functions, `number inc(number $x) = $x + 1;` is a named one. `($x) = ...` would read like an assignment, especially next to another `=` *(i.e. `const $f = ($x) = $x + 1;`)*, so it's an error that points to `->`.
- `(...)` followed by `{` or `->` in an expression is an anonymous function.
- `=>` can't be used for this, since map literals and `for ($k => $v in ...)` already use it.
- Like named functions, they can use the variables around them *(i.e. `$fruits.filter(($f) -> $f -like $pattern)`)*. This works like closures in JavaScript:
    - They use the variable itself, not a copy of its value, so they see later changes, and changes they make are seen outside.
    - They keep working after the function that created them has returned, and the variables they use stay alive with them.

```
(() -> number) makeCounter() {
    let $count = 0;
    return () -> ++$count;
}

const $next = makeCounter();
$next();    // 1
$next();    // 2, $count is kept between calls
```

## Function types

A function type lists the parameter types between parentheses, then `->` and the return type:

```
(string, number) -> bool        // takes a string and a number, returns a bool
() -> string                    // no parameters
(string) -> void                // returns nothing
(...string) -> string           // varargs
(number) -> (number) -> number  // returns a function, `->` groups from the right
list<(string) -> bool>          // inside generics
```

- Default values and parameter names aren't part of the type. `string greet(string $name = "user")` has the type `(string) -> string`.
- `->` is used in function types, in one-line anonymous functions and after the cases of a `switch`. They never get mixed up:
    - Types have no `$` and anonymous functions name their parameters with `$`: `(number) -> number` is a type, `($n) -> $n * 2` is an anonymous function.
    - `() ->` has nothing inside to tell them apart, so the position decides. Each position takes either a type or an expression, never both, and `.as()` and `.is()` always take types.
    - After `case`, there is a value or a type name, but never a function type or an anonymous function, so `case ($a + 1) -> ...` is always read as the value `$a + 1` and the arrow of the case.
- Function types can be wrapped in parentheses to make them easier to read *(i.e. `((number) -> number) makeAdder(number $n) = ...;`)*.
- If nullable types are written with a `?` prefix, `?(number) -> number` is ambiguous. Use parentheses: `?((number) -> number)` is a nullable function, `(number) -> ?number` is a function that returns a nullable number.
- There are no C-style casts like `(number) $x`, since they would clash with this syntax. Conversions are done with methods *(i.e. `$x.toNumber()`)*.

### No type aliases

There is no `type` to give a type another name. A name for a type always means a real type: a built-in one, a class, an interface or an enum *(see [classes.md](classes.md))*.

- `type X = ...;` isn't valid code, and gets the error any other wrong code gets: there's no message of its own for it *(see [roadmap.md](../roadmap.md#while-in-development))*.

- A type with a name can be told apart at runtime by `.is()`, `.as()` and `catch`, in any module. An alias would be replaced by what it stands for, which makes `$value.is(core::Filter)` impossible to check across modules and lets two aliases refer to each other forever.
- A long function type is written out *(i.e. `(any) -> list<any>`)*. An interface with a single method can name it when that's clearer.

## Names ending in `?`

Function and method names can end with `?`, usually for functions that return a `bool` *(i.e. `1.even?()`, `$list.empty?()`)*.

The `?` is only part of the name when it's immediately followed by `(`. Otherwise it's read as an operator:

```
$n.even?()            // calls the method `even?`
$n.even?().toString() // calls `even?`, then `toString` on the result
$user.name?.upper()   // `name`, then the nullable operator `?.`
$n.even ? 'a' : 'b'   // `even`, then the ternary operator
```

- Since functions are always called with parentheses, `even?` can't be referenced without calling it. To pass it as a value, wrap it in an anonymous function *(i.e. `$numbers.filter(($n) -> $n.even?())`)*.
- Variable names can't end with `?`, so `$ok?(1) : 2` is always a ternary.

Examples:
```
// ---------------------
// Definition - One liner
string greet(string $name) = f"Hello {$name}";

// Execution
greet("Folgue");
greet(greet('Folgue'));

// ---------------------
// Definition - Standard
string greet(string $name = "user") {
    return f"Hello {$name}";
}

// Execution
greet();
greet("Folgue");

// ---------------------
// Definition - Varargs
string greet(...string $names) {
    // ...
}

greet("Folgue", "Daniel", "James");

// ---------------------
// Definition - Multiple parameters
string createFile(string $name, string $extension)
    = f"{$name}.{$extension}";

// Execution - Named parameters and without named parameters
createFile(extension: "jpg", name: "daniel");
// Or
createFile('daniel', 'jpg');
```