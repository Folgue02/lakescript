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
- Scripts run from top to bottom. Functions must be defined before they're called.

## Variables

- Declared with `let` (can be reassigned) or `const` (can't be reassigned). A variable can't be declared twice.
- The type goes between `let`/`const` and the name. Without a type, the variable is `any`.

```
let string $name = 'Folgue';
const number $max = 10;
let $anything = 5;          // any
```

- `const` only freezes the variable, not the value inside it:

```
const $fruits = ['Oranges'];
$fruits << 'Apples';        // OK, the list changes
$fruits = ['Pears'];        // Error, the variable can't be reassigned
```

- Lists can be destructured into variables:

```
let [$first, $second] = ['Daniel', 'Folgue'];
```

## Type checking

- Types are optional. They're checked once, before the script runs, and then removed, so there are no type checks while the script runs. Typed and untyped code run at the same speed.
- Values without a type are `any`. The checker doesn't check `any`: it can hold any value, including `null`, and it can be assigned to any typed variable.
- Generic types use `<>`: `list<string>`, `map<string, number>`, `result<string, Exception>`.

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
```

### While loops

- `break` and `continue` can be used. There are no labelled breaks, and there is no `do-while`.
- Curly braces are optional if there is only one line as body.

```
while ($iterations < 18) {
    $iterations++;
}
```

