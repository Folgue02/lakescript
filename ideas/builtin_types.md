# Built-in types

## Base types

All base types can have methods called on *(i.e. `1.even?()`, `'hello world'.replaceAll('o', 'a')`, `true.toNumber()`)*

### Strings

- There is no `char` type, but there is `string`.
- Single quotes are used for normal strings: `'Hello'`.
- Double quotes can be used for normal strings too, `"Hello"`, but they're also the only ones that can have a prefix:
    - String interpolation, similar to python's: `f"Hello {$name}"`
    - Raw strings, with no escaping, just like in Rust: `r#"Text here"#`
    - Regexes: `%r"^\d+$"` *(see [Regex](#regex))*
- Strings are concatenated with `+` *(i.e. `'Hello ' + $name`)*. Both sides must be strings, so `'Age: ' + 5` is an error. Use interpolation instead: `f"Age: {$age}"`.

### Number

- `number` covers all types of numbers. The user never picks between integers and floats.
- Internally, a `number` is stored either as a 64-bit integer or as a 64-bit float. The interpreter picks one, and the difference is never visible to the user:
    - Literals without a decimal point (`5`) are stored as integers, literals with one (`2.5`, `5.0`) as floats.
    - `+`, `-`, `*` and `%` between two integers give an integer. If the result doesn't fit in 64 bits, it becomes a float.
    - Any operation with a float gives a float, and `/` always gives a float.
    - Integers and floats compare as numbers, so `1 == 1.0` is `true`.
    - Bitwise operators work on any whole value *(i.e. `4.0 & 1` works, `4.5 & 1` is an error)*.

### Bool

- `bool` holds two values, `true` and `false`. Bool shouldn't be treated as a number, neither a number should be treated as a bool.

### Null

- `null` means there is no value.
- A typed variable can only hold `null` if its type starts with `?`:

```
let ?string $nickname = null;   // OK
let string $name = null;        // Error
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
```

Elements are added at the end with `<<` *(i.e. `$fruits << 'Pears';`)*.

### Map

Type: `map<K, V>`

Defined with curly braces and `=>`
```
let $scores = {
    'Daniel' => 23,
    'Folgue' => 24
};

const $empty = {};

out($scores['Daniel']);
```

- `{` starts a map when it's where a value is expected *(i.e. after `=`, `(`, `,` or `return`)*. At the start of a statement, it starts a block.
- A missing key returns `null` *(i.e. `$scores['James']` is `null`)*.
- The order of the keys isn't kept, and it can change every time the script runs, like Rust's `HashMap`.
- Keys can be strings, numbers or symbols.
- To loop over a map, see the for loops in [language.md](language.md).

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

It holds either a value of type `T` or an error of type `E`. It's what `try` returns *(i.e. `try readFile($path)` returns `result<string, Exception>`, see [language.md](language.md))*.

| Method              | Returns          | Description |
|---------------------|------------------|-------------|
| `isOk()`            | `bool`           | `true` if it holds a value |
| `isError()`         | `bool`           | `true` if it holds an error |
| `orElse($default)`  | `T`              | The value, or `$default` if it holds an error |
| `orElseGet($fn)`    | `T`              | The value, or the result of calling `$fn` *(a `() -> T`)* if it holds an error. `$fn` is only called when needed |
| `map($fn)`          | `result<U, E>`   | Calls `$fn` *(a `(T) -> U`)* on the value and returns a new result with what it returns. If it holds an error, the error is kept as it is, like Java's `Optional.map()` |
| `orNull()`          | `?T`             | The value, or `null` if it holds an error |
| `throw()`           | `T`              | The value, or throws the error |

```
const $config = (try readFile('config.txt')).orElse('');
const $size = (try readFile($path)).map(($text) = $text.length()).orNull();
```

**NOTE**: `try` applies to the whole expression after it, so `try readFile($path).orElse('')` would mean `try (readFile($path).orElse(''))`. Parentheses are needed to call methods on the result.

### Regex

Type: `regex`

Regexes are written with `%r` and double quotes: `%r"^\d+$"`. Slashes don't need escaping, which is useful for paths. A `"` inside the pattern is written as `\"`.

```
const $binPath = %r"^/usr/local/bin/";
const $number = %r"^\d+$";
```

Flags go right after the closing `"` *(i.e. `%r"hello"i`)*.

Regexes are used with the `-match` operator *(see [language.md](language.md))*.
