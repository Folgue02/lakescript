# Goals

Lakescript is a scripting language, meant to replace bash for scripts that outgrow it.

- **Standalone scripts**: a single file with a shebang (`#!/usr/bin/lks`) that can be run directly.
- **Projects**: bigger programs split into files, with dependencies, like with cargo or npm.
- **Fast startup**: scripts must start and run quickly. Raw performance isn't the goal, but people don't like slow scripts, so the time between running the command and the first line running matters most.

## Decisions that follow from this

- **Compiled code is cached**: the first run of a script compiles it to bytecode and saves it. Later runs of the same unchanged script skip parsing and compiling.
    - The cache is stored in a cache folder for the current user *(i.e. `~/.cache/lakescript` on Linux, `~/Library/Caches/lakescript` on macOS)*, not next to the script.
    - Each entry is keyed by a hash of the script's contents and the interpreter version, so an edited script or a new interpreter version is compiled again.
- **No JIT**: a JIT makes long programs faster but the first seconds slower, which is the opposite of what scripts need.
- **Lazy loading**: imports and the standard library are only loaded when they're used.
- **Regexes are compiled on first use**, not when the script starts, since a script might never reach the line that uses them.
- **Single-pass compiling**: functions must be defined before they're called, so the script can be compiled from top to bottom in one go.

# Syntax

- C-like syntax, uses curly braces and semicolons.
- `$` to prefix variables, like PHP or Perl
- use of `let` and `const` to declare variables, don't allow redeclarations *(see [Variables](#variables))*.
- use of operators such as `<<` to push things into collections.
- coalesce operator (`??`), ternary operator (`... ? ... : ...`), nullable operator like in JS (`$nullableThing?.method()`)
- conditional operators like in bash/powershell:
    - `-f` to check if the path points to a file.
    - `-d` to check if it's a directory.
    - `-x` to check if the path points to an executable.
    - `-r` to check if it's readable.
    - `-w` to check if it's writable.
    - `-like` to check if a string matches a wildcard pattern, like in PowerShell *(i.e. `$file -like '*.txt'`)*. `*` matches any number of characters, `?` matches one character and `[abc]` matches one of the characters inside.
    - `-match` to check if a string matches a regex *(i.e. `$name -match %r"^\d+$"`)*. The right side can also be a string, which gets compiled as a regex *(i.e. `$name -match $userPattern`)*. See the `regex` type in [builtin_types.md](builtin_types.md).
    - `-in` to check if the element is in a collection.

- try catch with exceptions like Java's, but with the option of getting a result type like Rust's. if function `readFile($filePath)` could throw an exception, use `try readFile($filePath)` and this should return a `result<string, Exception>` *(see `result` in [builtin_types.md](builtin_types.md))*
    - For now there is a single, generic `Exception` type. How to define custom exceptions is still to be decided.
- Support for shebangs `#!/usr/bin/lks`
- Comments: `// until the end of the line` and `/* block comments */`, which can span several lines.
    - Doc comments start with `///` and go before what they document, so editors can show them *(still to be discussed)*.
- Blocks used as values end with `};`, since they're part of a statement: maps, anonymous functions, `switch` and `if` expressions *(i.e. `const $scores = { ... };`)*. Blocks of statements like `if`, `for`, `while` or function definitions end with just `}`.
- Scripts run from top to bottom. Functions must be defined before they're called.

## Names and reserved words

- Names are made of ASCII letters, digits and `_`, and can't start with a digit *(i.e. `$user_name`, `$user2`, `loadConfig`)*.
- Variables start with `$`. Function and method names can end with `?` *(see [functions.md](functions.md))*.
- These words are reserved and can't be used as function names:

| Group           | Words |
|-----------------|-------|
| Declarations    | `let` `const` `type` |
| Control flow    | `if` `else` `for` `in` `while` `switch` `case` `default` `break` `continue` `return` `yield` |
| Errors          | `try` `catch` |
| Values          | `true` `false` `null` |
| Built-in types  | `void` `any` `string` `number` `bool` `symbol` `list` `map` `range` `regex` `result` |

- `as` and `is` are reserved method names *(see [builtin_types.md](builtin_types.md))*.
- Variables can use any of these names, since they start with `$` *(i.e. `$type`, `$default`)*.

## Variables

- A variable can't be declared twice, not even in an inner scope: there is no shadowing.

```
let $count = 0;
if ($ready) {
    let $count = 1;     // Error, $count already exists
}
```
- Variables are normally declared with `let` (can be reassigned) or `const` (can't be reassigned), without writing a type. Their type is inferred from their first value, like in TypeScript. They're only `any` when that value is `any` *(i.e. data read from a JSON file)*.

```
let $age = 5;                       // number, can be reassigned
const $city = 'Madrid';             // string, can't be reassigned
let $names = <string>[];            // list<string>
let $scores = <string, number>{};   // map<string, number>
let $data = readJson($path);        // any, because readJson returns any
const $name = $data['name'].as(string);  // string

$age = 'old';                       // Error, $age is a number
$data = 'anything';                 // OK, $data is any
```

- Writing the type is only needed in a few cases, when the first value doesn't have the type the variable should have. Then the type replaces `let`, like in Java, and `const` goes before the type:

```
?string $nickname = null;           // starts as null, will hold a string later
?number $best = null;
any $value = 5;                     // can hold anything, not only numbers
const list<any> $mixed = [];
```

- A statement that starts with a type followed by a `$name` is a declaration. A type followed by a name without `$` is a function definition *(i.e. `string $name = ...` vs `string greet(...)`)*.

- `const` only freezes the variable, not the value inside it:

```
const $fruits = ['Oranges'];
$fruits << 'Apples';        // OK, the list changes
$fruits = ['Pears'];        // Error, the variable can't be reassigned
```

- Lists can be destructured into variables:

```
let [$first, $second] = ['Daniel', 'Folgue'];
let [$head, ...$rest] = [1, 2, 3, 4];     // $head is 1, $rest is [2, 3, 4]

for ([$name, $age] in $pairs)
    outln(f"{$name}: {$age}");
```

- `...$rest` gets the remaining elements as a list, and it can only be the last one.

## Type checking

- Types are optional. They're checked once, before the script runs, and then removed, so there are no type checks while the script runs. Typed and untyped code run at the same speed.
- When a type isn't written and can't be inferred, it's `any`: function parameters without a type, `let $x = null;`, and values that come from other `any` values. 
- The type of an empty list or map is inferred from how it's used later. It's only `list<any>` or `map<any, any>` when nothing tells its type:

```
let $names = [];
$names << 'Daniel';         // $names is a list<string>

let $ages = {};
$ages['Daniel'] = 24;       // $ages is a map<string, number>
```
- A list or map with values of different types is a `list<any>` or a `map<K, any>` *(i.e. `[1, 'a']` is a `list<any>`)*.
- The checker doesn't check `any`: it can hold any value, including `null`, and it can be assigned to any typed variable.
- Generic types use `<>`: `list<string>`, `map<string, number>`, `result<string, Exception>`. When generics are nested, the lexer splits `>>` into two `>` *(i.e. `list<map<string, any>>`)*.
- Types never change what the code does: a script behaves the same whether its types were checked or not. Because of this, there's no overloading (choosing a function by the types of its arguments). `.as()` and `.is()` don't break this rule, because they check the value while the script runs, not its declared type *(see [builtin_types.md](builtin_types.md))*.

## Flow control structures

### Conditionals

- Curly braces are optional if there is only one line as body.
- The condition must be a `bool` *(i.e. `if ($list)` is an error, use `if ($list.empty?())`)*.

Examples:
```
if ($name.isUpperCase()) {

} else if ($name.isBlank()) {

} else {

}

if ($age > 18 && $age < 23) {

}

if (-f $file)
    outln("File exists.");
```

#### If as an expression

Like `switch`, an `if` can return a value. Then it must have an `else`, so there's always a value:

```
const $label = if ($count > 1) 'items' else 'item';

const $size = if ($bytes > 1_000_000) {
    const $mb = $bytes / 1_000_000;
    yield f"{$mb} MB";
} else if ($bytes > 1_000) 'KB' else 'bytes';
```

- A body with a single expression doesn't end with `;`, since the whole `if` is part of a statement that already ends with one.
- When the body is a block, `yield` gives its value, like in `switch`.
- For short cases, the ternary operator does the same: `$count > 1 ? 'items' : 'item'`.

### For loops

- `break` and `continue` can be used. There are no labelled breaks.
- Curly braces are optional if there is only one line as body.

```
for ($i in 1..5) {

}

for ($fruit in $fruits) {
    outln("Something");
}

// With the index
for ($i, $fruit in $fruits)
    outln(f"{$i}: {$fruit}");

// Maps, with the key and the value
for ($name => $score in $scores)
    outln(f"{$name}: {$score}");

// Maps, only the keys
for ($name in $scores)
    outln($name);
```

### While loops

- `break` and `continue` can be used. There are no labelled breaks, and there is no `do-while`.
- Curly braces are optional if there is only one line as body.

```
while ($iterations < 18) {
    $iterations++;
}
```


### Switch

```
switch ($variable) {
    case 1 -> {
        // ...
    }
    case 2 -> outln('test');
    case 1..20 -> {
        // ...
    }
    default -> {
        // ...
    }
}
```

- The first case that matches runs, and only that one: there is no fall-through, so there is no `break`.
- A case can be:
    - A value, compared with `==` *(i.e. `case 1`, `case 'Daniel'`, `case :admin`)*.
    - A range, which matches the values inside it *(i.e. `case 1..20`)*.
    - Several values or ranges separated by commas, which matches if any of them does *(i.e. `case 1, 2, 3`, `case :admin, :editor`)*.
    - A type, which matches like `.is()`. Inside the case, the value has that type. It takes the same types as `.is()`, so function types can't be used:

```
switch ($value) {
    case string -> outln($value.upper());
    case number -> outln($value * 2);
    case list<any> -> outln(f"{$value.length()} items");
    default -> outln('Something else');
}
```
- The body is a block in curly braces, or a single statement or expression, which must end with `;`.

#### Switch as an expression

A switch can return a value. Then it must be exhaustive: every possible value must match a case, which in practice means it needs a `default` *(except for a `bool`, if both `true` and `false` have a case)*.

```
const $label = switch ($age) {
    case 0..13 -> 'child';
    case 13..18 -> 'teenager';
    default -> 'adult';
};
```

When the body is a block, `yield` gives its value. `return` can't be used for this, since it would return from the function the switch is in:

```
const $price = switch ($product) {
    case :apple -> 1;
    default -> {
        const $base = lookupPrice($product);
        yield $base * 1.21;
    }
};
```

## Errors

Error messages should look like Rust's: the file and line number, a preview of the line, and a marker pointing to where the error is.

```
error: $age is a number, can't assign a string
 --> script.lks:12:8
   |
12 | $age = 'old';
   |        ^^^^^ this is a string
```

The first version can be simpler, like `script.lks:12:8: $age is a number, can't assign a string`, as long as it shows the file, line and column.
