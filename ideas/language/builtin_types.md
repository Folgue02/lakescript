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
    - They can be indexed and sliced like lists, with negative indexes and ranges. Each element is a one-character string: `'hello'[0]` is `'h'`, `'hello'[-3..]` is `'llo'`. An index outside the string throws an `Exception`.
    - `for ($c in $text)` loops over the characters.
    - `<`, `>`, `<=` and `>=` compare strings alphabetically, character by character *(i.e. `'apple' < 'banana'` is `true`)*.
- Internally, strings are stored like in Python (PEP 393): every character of a string takes the same number of bytes, chosen by the widest character in it. This is never visible to the user:
    - 1 byte per character if every character fits in Latin-1 *(i.e. `'hello'`, `'año'`)*.
    - 2 bytes per character if one doesn't, but all fit in 2 bytes *(i.e. `'привет'`)*.
    - 4 bytes per character otherwise *(i.e. a string with an emoji)*.
    
    Since every character has the same size, `.length()` and indexing are instant for any string. The cost is converting to UTF-8 when the string is written to a file or the terminal, so the UTF-8 version is kept after the first conversion.
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

### Symbols

Type: `symbol`

- Written with `:` followed by a name *(i.e. `:oranges`, `:not_found`)*.
- Two symbols with the same name are always the same value: `:active == :active` is `true`.
- They can be used as map keys and as fixed values:

```
let $status = :active;
let $prices = {:oranges => 2, :apples => 3};
```

- `%i[oranges apples]` creates a list of symbols *(see [List](#list))*.
- `:oranges.toString()` returns `'oranges'`.

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

There should also be two special ways to create arrays:
```
const $words = %w[Oranges Apples Tomatoes];
const $symbols = %i[oranges apples tomatoes]; // Same as `const $symbols = [:oranges, :apples, :tomatoes];`
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

Using an index outside the list throws an `Exception`, both to read and to replace *(i.e. `$fruits[10]` with 3 elements)*. Use `<<` to add elements.

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
- Keys can be strings, numbers or symbols. Keys are compared like `==`, so `1` and `1.0` are the same key.
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
const $size = (try readFile($path)).map(($text) = $text.length()).orNull();
```

**NOTE**: `try` applies to the whole expression after it, so `try readFile($path).orElse('')` would mean `try (readFile($path).orElse(''))`. Parentheses are needed to call methods on the result.

### Command

Type: `command`

`%x"..."` creates a shell command, like Perl's backticks. It doesn't run when it's created: it runs when one of its methods is called, so it can be set up first.

```
const $branch = %x"git rev-parse --abbrev-ref HEAD".output();   // runs it and returns stdout
%x"cargo build --release".run();                               // runs it, output goes to the terminal

%x"make install"
    .cwd($projectDir)
    .env('PREFIX', '/usr/local')
    .run();

const $count = %x"ls | wc -l".output().toNumber();    // pipes and redirects work
```

- `{...}` interpolates like in `f""`, but every value is quoted as a single argument, so values can never run as shell code:

```
const $file = 'my notes; rm -rf ~.txt';
%x"rm {$file}".run();             // removes one file with that exact name, nothing else runs

const $files = ['a.txt', 'b.txt'];
%x"cp {$files} backup/".run();    // a list becomes several arguments
```

- The rest of the text runs through the shell (`sh` on Linux and macOS), so `|`, `>`, `&&` and `*` work.
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

- `.match()` returns a `?list<string>`. Like `-match`, it also accepts a string as the pattern.
