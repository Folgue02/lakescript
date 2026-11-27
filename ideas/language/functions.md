# Functions

- Should allow parameter types
- Should allow default values. They're evaluated on every call, so `list<string> $items = []` gives a new list each time.
- Should allow varargs. Inside the function, `...string $names` is a `list<string>`.
- A list can be spread into the arguments of any call with `...`, also of a method, and next to other arguments *(i.e. `greet(...$names)`, `greet('Ana', ...$others)`, `$list.push(...$other)`)*. Inside a list literal it does the same *(i.e. `[...$a, ...$b]`, see [operators.md](operators.md))*.
- Should allow calling them while naming parameters. Named and positional arguments can be mixed, but positional ones go first *(i.e. `createFile('daniel', extension: 'jpg')`)*.
    - Methods of classes take them too *(i.e. `$zip.add('photos/', as: 'pictures')`)*. The methods of built-in types *(i.e. `'ab'.repeat(2)`)* only take their arguments in order, and naming one is an error.
- Must always have a return type. Functions that don't return anything use `void` *(i.e. `void log(string $message) { ... }`)*. This way a definition never looks like a call followed by a block.
    - A function that never returns, because it always throws or exits, has `never`, like Rust's `!`, TypeScript's `never` and Swift's `Never` *(i.e. `never fail(string $message) { errln($message); exit(1); }`)*. `exit()` is one.
        - A call to it ends a block, like `return` and `throw`: it narrows *(i.e. after `if ($user == null) fail('nobody is logged in');`, `$user` isn't nullable)*, a function that must return can end with it, and the code after it doesn't run.
        - It fits where any type is expected, like `throw` *(i.e. `const $name = $config['name'] ?? fail('the config needs a name');` is a `string`)*.
        - The checker checks that it can't reach its end: every way through it throws, exits, calls another `never` function or loops forever. A `return` in it is an error.
        - It's only a return type: a `never` variable or parameter is an error, and so is `.as(never)`.
- Should allow generics. The type parameters go after the name *(i.e. `?T first<T>(list<T> $items) = $items[0];`)*. They're inferred when calling, so they're never written in a call: `first($names)`.
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
- **One parameter without a type can go without parentheses**, like Java's and Kotlin's: `$v -> $v * 2` is `($v) -> $v * 2`, and `$v -> { ... }` takes a block. A parameter with a type keeps them *(`(number $v) -> ...`)*, and so do none or several. In a `case` pattern, `$v ->` is still the value `$v` and the arrow of the case. Added in phase 13.
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

## Doc comments

`///` comments go on the lines right before a declaration: a function, a class, an interface, an enum, a field, a method or a `const`. Editors show them on hover, and `command`s use them as their `--help` *(see [cli.md](cli.md))*.

A function or a method describes its parameters, what it returns and what it throws with lines that start with their name, after its description. A parameter is written as in the code, with its `$`:

```
/// Greets someone, as many times as asked.
///
/// $name: who to greet
/// $times: how many times, at least 1
/// returns: the greeting, one line per time
/// throws ValueError: when `$times` is less than 1
pub string greet(string $name, number $times = 1) = ...;
```

- `$name:` describes a parameter, `returns:` what it gives back, and `throws ValueError:` when it throws that exception, one line for each exception. A description goes on in the next lines, until an empty line or the next of these lines *(the lexer trims the lines of a `///` comment, so their indentation can't tell)*.
- It's like Javadoc's `@param`, `@return` and `@throws`, but written as the language writes things: `@` is a field in Lakescript *(i.e. `@x`)*, so `@name` in a comment would look like one.
- With these lines:
    - Hover on a parameter shows its line: where it's declared, where it's used, and on a named argument of a call *(i.e. `times:` in `greet('Ana', times: 2)`)*.
    - Editors show what the argument being written is for while a call is typed *(signature help, in `lks-lsp`)*: the function's declaration with the parameter being written marked, its line, and the text of the comment. A named argument is the parameter it names.
    - A `$name:` for a parameter the function doesn't have is a warning, so comments don't go stale after a rename *(i.e. `` the comment describes `$name`, but `greet` has no parameter `$name`: its parameters are `$who`, `$times` ``)*. The language has no warnings of its own, so `lks` ignores it and a comment never stops a script: `lks-lsp` shows it in the editor, and `ferry check` and `ferry doc` print it. It's checked for functions, methods and constructors.
    - `ferry doc` shows the parameters, what it returns and what it throws as tables *(see phase 9 in [roadmap.md](../roadmap.md))*.
- A folder of modules is documented by a `module.md` in it *(see Documenting folders in [modules.md](modules.md))*.
- **The text is Markdown** *(i.e. `` `code` ``, `*emphasis*`, lists and links)*, which editors and `ferry doc` show formatted. The lines `$name:`, `returns:` and `throws ...:` are the only syntax of their own, and their descriptions are Markdown too.
- **A module documents itself with `//!`** at the top of its file, before its `use` lines, like Rust's: what the module is for, in Markdown. Its `///` comments are for what it declares, and a folder of modules has a `module.md` *(see Documenting folders in [modules.md](modules.md))*.

```
//! Sizes in bytes, read and written the way people do *(i.e. `1.5 KB`)*.
//! Both commands of the project use it.

use std/format/json;
```

- Still open: examples in comments.

