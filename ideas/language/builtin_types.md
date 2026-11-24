# Built-in types

## Base types

All base types can have methods called on *(i.e. `1.even?()`, `'hello world'.replace('o', 'a')`, `true.toNumber()`)*

The full list of methods of each type is in [stdlib/types](../stdlib/types).

### Methods on every value

#### `.as(type)`

Tells the checker that a value has a type, and checks it while the script runs. Useful for `any` values, like data read from a JSON file:

```
const $name = $data['name'].as(string);    // throws an Exception if it's not a string
$data['name'].as(string).upper();          // can be chained

// With `try`, to have a fallback instead of an exception
const $name = (try $data['name'].as(string)).orElse('unknown');
```

- If the value doesn't have that type, it throws an `Exception`.
- Inside `.as( )` there is a type, not an expression. `as` is a reserved method name, so the parser reads `string` as a type and the `<` in `.as(list<string>)` as part of the type, not as less-than.
- `.as()` doesn't convert values: `'5'.as(number)` throws, use `'5'.toNumber()` to convert.
- Nullable types accept `null`: `.as(?string)` works with a string or with `null`.
- Generic types check everything inside: `.as(list<string>)` checks every element of the list, and `.as(map<string, list<number>>)` checks every key, every list and every number inside them. This takes longer the bigger the value is.
- Function types can't be used *(i.e. `.as((string) -> bool)` is an error)*: types are removed before running, so a function doesn't know its parameter types anymore.
- If the checker already knows the type of the value, using `.as()` with a different type is an error before the script runs *(i.e. `$name.as(number)` when `$name` is a `string`)*.

#### `.is(type)`

Returns `true` if the value has that type. It takes the same types as `.as()`. Inside an `if` that uses it, the checker treats the value as that type:

```
if ($value.is(string))
    outln($value.upper());   // $value is a string here
```

### Strings

- There is no `char` type, but there is `string`.
- Single quotes are used for normal strings: `'Hello'`.
- Double quotes can be used for normal strings too, `"Hello"`, but they're also the only ones that can have a prefix:
    - String interpolation, similar to python's: `f"Hello {$name}"`. It accepts any expression *(i.e. `f"{$i + 1}. {$name.upper()}"`)*. A literal `{` or `}` is written twice: `f"{{not interpolated}}"`.
    - Raw strings, with no escaping, just like in Rust: `r#"Text here"#`
    - Regexes: `%r"^\d+$"` *(see [Regex](#regex))*
    - Commands: `%x"git status"`, with interpolation like `f""` *(see [Command](#command))*
- Escape sequences work in both single and double quotes (but not in raw strings):

| Escape | Meaning         |
|--------|-----------------|
| `\n`   | New line        |
| `\t`   | Tab             |
| `\r`   | Carriage return |
| `\\`   | Backslash       |
| `\'`   | Single quote    |
| `\"`   | Double quote    |
| `\0`   | Null character  |
| `\u{1F600}` | Unicode character by its code |

- Multi-line strings use triple double quotes. They can have the `f` prefix too:

```
const $help = """
Usage: backup <folder>
Copies the folder to the backup disk.
""";

const $report = f"""
Name: {$name}
Age: {$age}
""";
```
- The indentation that all the lines of a multi-line string share is removed, like in Java and Kotlin, so the string can be indented along with the code. The new line right after the opening `"""` is removed too. When the closing `"""` is on its own line, it counts for the indentation and the string ends with a new line, like in Java:

```
void help() {
    outln("""
        Usage: backup <folder>
          --dry-run   Only show what would be copied
        """);
}
// Prints:
// Usage: backup <folder>
//   --dry-run   Only show what would be copied
```
- Strings are concatenated with `+` *(i.e. `'Hello ' + $name`)*. Both sides must be strings, so `'Age: ' + 5` is an error. Use interpolation instead: `f"Age: {$age}"`.
- Strings are made of characters (Unicode code points, like Rust's `char`):
    - `.length()` counts characters, not bytes: `'año'.length()` is `3`.
    - They can be indexed and sliced like lists, with negative indexes and ranges. Each element is a one-character string: `'hello'[0]` is `'h'`, `'hello'[-3..]` is `'llo'`. An index outside the string gives `null`, like with lists, so `$text[$i]` is a `?string`.
    - `for ($c in $text)` loops over the characters.
    - `<`, `>`, `<=` and `>=` compare strings alphabetically, character by character *(i.e. `'apple' < 'banana'` is `true`)*.
- **`.length()` and indexing are instant for any string**, like in Python, so a loop over the characters of a long string takes time in proportion to its length. How it's stored is never visible to the user:
    - A string is UTF-8, where a character takes 1 to 4 bytes. Python *(PEP 393)* makes every character of a string the same size instead, 1, 2 or 4 bytes by its widest one, which was the first plan here. It would mean converting every string that reaches a file, a regex or a command, and writing every method of `string` three times.
    - Instead, the strings a script is indexing are remembered *(the last 8 of 32 bytes or more)*. One that's only ASCII needs nothing else, since its characters are its bytes. For any other, where each character starts is written down the first time it's needed, which takes 4 bytes for each character, about what Python's wider strings take.
    - So a string that's never indexed costs nothing, and a short one is walked, which is as fast. A loop that indexes more than 8 long strings in turn finds each one out again, at the speed of walking it.
    - It's in `crates/lakescript/src/strings.rs`, used by `.length()`, `$text[$i]`, slices, `indexOf` and the methods that take positions.
- Converting a string to another type throws an `Exception` if it isn't valid *(i.e. `'abc'.toNumber()`)*. Use `try` to get a fallback: `(try $input.toNumber()).orElse(0)`.

### Number

- `number` covers all types of numbers. The user never picks between integers and floats.
- Internally, a `number` is stored either as a 64-bit integer or as a 64-bit float. The interpreter picks one, and the difference is never visible to the user:
    - Literals without a decimal point (`5`) are stored as integers, literals with one (`2.5`, `5.0`) as floats.
    - `+`, `-`, `*` and `%` between two integers give an integer. If the result doesn't fit in 64 bits, it becomes a float.
    - Any operation with a float gives a float, and `/` always gives a float.
    - Integers and floats compare as numbers, so `1 == 1.0` is `true`.
    - Bitwise operators work on any whole value *(i.e. `4.0 & 1` works, `4.5 & 1` is an error)*.
- Literals:

| Literal              | Example        | Value      |
|----------------------|----------------|------------|
| Decimal              | `255`          | 255        |
| Hexadecimal          | `0xFF`         | 255        |
| Binary               | `0b1010`       | 10         |
| Octal                | `0o17`         | 15         |
| With separators      | `1_000_000`    | 1000000    |
| Decimal point        | `2.5`          | 2.5        |
| Scientific notation  | `1e6`, `2.5e-3`| 1000000, 0.0025 |

`_` can go between any two digits, in any of these forms *(i.e. `0xFF_FF`)*. Literals with a decimal point or an exponent are stored as floats.

### Bool

- `bool` holds two values, `true` and `false`. Bool shouldn't be treated as a number, neither a number should be treated as a bool.

### Null

- `null` means there is no value.
- A typed variable can only hold `null` if its type starts with `?`:

```
?string $nickname = null;       // OK
string $name = null;            // Error
let $anything = null;           // OK, `any` can hold null
```

- `?.` and `??` are used to work with nullable values *(see [operators.md](operators.md))*.

## Collections

### List

Type: `list<string>`

Defined just like in PHP
```
let $fruits = ['Oranges', 'Apples', 'Tomatoes'];
```

The type of the elements can be written before the `[`, which is useful for empty lists:
```
let $names = <string>[];          // a list<string>
let $ages = <number>[18, 24];
```

There's also a short way to write a list of words:
```
const $words = %w[Oranges Apples Tomatoes];   // ['Oranges', 'Apples', 'Tomatoes']
```

Lists use the standard indexing syntax *(`$list[$index]`)*, starting at 0. Negative indexes count from the end, like in Python:

```
$fruits[0]    // 'Oranges'
$fruits[-1]   // 'Tomatoes', the last element
$fruits[-2]   // 'Apples'
```

Indexing with a range returns a new list with that part of the list *(see [Ranges](#ranges))*:

```
$fruits[0..2]     // ['Oranges', 'Apples']
$fruits[1..=2]    // ['Apples', 'Tomatoes']
$fruits[-2..=-1]  // ['Apples', 'Tomatoes'], the last two
$fruits[0..-1]    // ['Oranges', 'Apples'], all but the last one
```

When a range is used as an index, its ends are positions: negative ends are turned into positions first, and the slice never counts down. In `$fruits[0..-1]`, `-1` is position 2, so it's the same as `$fruits[0..2]`. If the start ends up after the end, the slice is empty. Ranges used as values still count down *(see [Ranges](#ranges))*.

Elements are added at the end with `<<` *(i.e. `$fruits << 'Pears';`)*, and replaced with an index *(i.e. `$fruits[0] = 'Lemons';`)*.

Reading an index outside the list gives `null`, like a missing key of a map, so `$fruits[0]` is a `?string` and `??` gives a default:

```
const $first = $args[0] ?? 'world';     // `null` when there are no arguments
$fruits[10]                             // null, with 3 elements
```

- The checker knows it can be `null`: `$args[0].toNumber()` is an error, and `$args[0]?.toNumber()`, `??` or a check for `null` are what work.
- Replacing an index outside the list throws an `IndexError` *(i.e. `$fruits[10] = 'Kiwis';` with 3 elements)*. Use `<<` to add elements.
- Ranges never give `null`: a slice outside the list is shorter or empty.
- A class's `[]` *(`IIndex<K, V>`, see [classes.md](classes.md))* decides for itself: `V` is `?string` when it can be missing, like lists.

### Map

Type: `map<K, V>`

Defined with curly braces and `=>`
```
let $scores = {
    'Daniel' => 23,
    'Folgue' => 24
};

const $empty = {};
const $emptyScores = <string, number>{};  // a map<string, number>

out($scores['Daniel']);
```

- `{` starts a map when it's where a value is expected *(i.e. after `=`, `(`, `,` or `return`)*. At the start of a statement, it starts a block.
- The `<...>` before a list or a map can only be where a value is expected, so it's never confused with less-than: `<` can't start an expression.
- A missing key returns `null` *(i.e. `$scores['James']` is `null`)*.
- `$scores['James'] = 30;` adds the key, or replaces its value if it already exists.
- The keys keep the order in which they were added, so looping over a map or printing it gives the same order on every run. *(The Rust `indexmap` crate does this at about the same speed as a `HashMap`.)*
- Keys can be strings or numbers. Keys are compared like `==`, so `1` and `1.0` are the same key.
- To loop over a map, see [flow_control.md](flow_control.md).

## Others

### Ranges

Type: `range`

Syntax:
```
let $range = 1..5;
$range = 1..=5;

const $a = 50;
$range = 1..$a;
```

- `1..5` doesn't include the end: 1, 2, 3, 4.
- `1..=5` includes it: 1, 2, 3, 4, 5.
- If the start is bigger than the end, the range counts down: `5..1` is 5, 4, 3, 2 and `5..=1` is 5, 4, 3, 2, 1.
- Steps are set with the `step` method *(i.e. `(1..10).step(2)` is 1, 3, 5, 7, 9 and `(10..=0).step(5)` is 10, 5, 0)*. The step is always positive, the direction comes from the range. The parentheses are needed because `.` binds tighter than `..`.

### Result

Type: `result<T, E>`

It holds either a value of type `T` or an error of type `E`. It's what `try` returns *(i.e. `try readFile($path)` returns `result<string, Exception>`, see [syntax.md](syntax.md))*.

| Method              | Returns          | Description |
|---------------------|------------------|-------------|
| `isOk()`            | `bool`           | `true` if it holds a value |
| `isError()`         | `bool`           | `true` if it holds an error |
| `orElse($default)`  | `T`              | The value, or `$default` if it holds an error |
| `orElseGet($fn)`    | `T`              | The value, or the result of calling `$fn` *(a `() -> T`)* if it holds an error. `$fn` is only called when needed |
| `map($fn)`          | `result<U, E>`   | Calls `$fn` *(a `(T) -> U`)* on the value and returns a new result with what it returns. If it holds an error, the error is kept as it is, like Java's `Optional.map()` |
| `orNull()`          | `?T`             | The value, or `null` if it holds an error |
| `throw()`           | `T`              | The value, or throws the error |
| `error()`           | `?E`             | The error, or `null` if it holds a value |

```
const $config = (try readFile('config.txt')).orElse('');
const $size = (try readFile($path)).map(($text) -> $text.length()).orNull();
```

**NOTE**: `try` applies to the whole expression after it, so `try readFile($path).orElse('')` would mean `try (readFile($path).orElse(''))`. Parentheses are needed to call methods on the result.

### Command

Type: `command`

`%x"..."` creates a command, like Perl's backticks. It doesn't run when it's created: it runs when one of its methods is called, so it can be set up first. It runs the program directly, without a shell, so it behaves the same on Linux, macOS and Windows.

```
const $branch = %x"git rev-parse --abbrev-ref HEAD".output();   // runs it and returns stdout
%x"cargo build --release".run();                               // runs it, output goes to the terminal

%x"make install"
    .cwd($projectDir)
    .env('PREFIX', '/usr/local')
    .run();

const $count = %x"git ls-files".pipe(%x"wc -l").output().toNumber();   // pipes are a method
```

- `{...}` interpolates like in `f""`, but every value is a whole argument, so values can never run as code:

```
const $file = 'my notes; rm -rf ~.txt';
%x"rm {$file}".run();             // removes one file with that exact name, nothing else runs

const $files = ['a.txt', 'b.txt'];
%x"cp {$files} backup/".run();    // a list becomes several arguments
```

- The text is split into the program and its arguments: spaces separate them, and `'...'` or `"..."` *(written `\"` inside `%x"..."`)* keep spaces in one *(i.e. `%x"git commit -m 'Fix the parser'"` has 4)*. A value next to text is part of the same argument *(i.e. `--file={$archive}`)*, and a list there is an error. Backslashes stay as they are, so Windows paths work *(i.e. `%x"dir C:\Users"`)*.
- There's no shell, so `|`, `>`, `&&`, `$HOME` and `*` are plain text. They're methods instead:
    - `.pipe(%x"sort")` sends the output to another command. Setup methods on a pipeline are for all its commands, `.input()` and `.readFrom()` for the first one, and `.writeTo()` and `.appendTo()` for the last one. Like `set -o pipefail`, it fails when any of its commands does.
    - `.writeTo('log.txt')`, `.appendTo('log.txt')` and `.readFrom('names.txt')` are the redirects.
    - `fs::glob('*.txt')` gives the files of a pattern, as a list of arguments *(i.e. `%x"rm {fs::glob('*.tmp')}"`)*, and `env::get('HOME')` a variable.
    - When a shell is really needed, `process::shell('ls *.txt | wc -l')` runs one: `sh -c` on Linux and macOS, `cmd /C` on Windows. Its text is the shell's own, so it isn't portable.
- A literal `{` or `}` is written twice, like in `f""` *(i.e. `%x"echo {{a,b}}"`)*.
- Backslashes are passed to the shell as they are, like in regexes *(i.e. `%x"grep 'a\.b' notes.txt"`)*. Only `\"` is turned into `"`. A command can span several lines.
- If the command fails (its exit code isn't 0), `run()` and `output()` throw a `CommandFailed`, like `set -e` in bash. With `try` you get a fallback: `(try %x"git describe".output()).orElse('dev')`. `status()` runs it without throwing.
- The methods are listed in [command.md](../stdlib/types/command.md).

### Regex

Type: `regex`

Regexes are written with `%r` and double quotes: `%r"^\d+$"`. Slashes don't need escaping, which is useful for paths. A `"` inside the pattern is written as `\"`.

```
const $binPath = %r"^/usr/local/bin/";
const $number = %r"^\d+$";
```

Flags go right after the closing `"` *(i.e. `%r"hello"i`)*.

Regexes are used with the `-match` operator *(see [operators.md](operators.md))*, which only says if the string matches. To get the matched text and the groups, use the `.match()` method on the string. It returns `null` if there is no match, or a list with the whole match first and then each group:

```
const $parts = '2026-11-18'.match(%r"(\d+)-(\d+)-(\d+)");
// ['2026-11-18', '2026', '11', '18']

const $year = '2026-11-18'.match(%r"^(\d+)")?[1];   // '2026'
```

- `.match()` returns a `?list<string>`. Unlike `-match`, it only takes a regex: to use a string as a pattern, make it a regex first.
- The other string methods that work with regexes end in `Matches` or `Match`: `replaceMatches`, `replaceFirstMatch`, `replaceLastMatch`, `removeMatches` and `splitMatches` *(see [string.md](../stdlib/types/string.md))*. There are no union types, so each method takes either a string or a regex, never both.
