# Type checking

- Types are optional. They're checked once, before the script runs, and then removed, so there are no type checks while the script runs. Typed and untyped code run at the same speed.
- `lks` checks the types every time before running a script. It costs less than a millisecond for most scripts.
- A type error stops the script before its first line runs, like a syntax error. There are no type warnings.
- A check against `null` narrows the type: after `if ($x == null) return;`, or inside `if ($x != null) { ... }`, `$x` isn't nullable anymore, like in TypeScript and Kotlin. The same as `.is()` and `case string ->` already do.
    - It narrows `const` fields too, of `@` or of a `const` variable *(i.e. `if (@part2 != null) @part2`, `if ($day.part2 != null) ...`)*, since they can't change between the check and the use. Other fields aren't narrowed: any call in between could change them, like in Kotlin. `@part2!` says one isn't `null` *(see [operators.md](operators.md))*. An error about `null` on a field the script checked says so *(i.e. `` `@part2` isn't `const`, so checking it for `null` doesn't narrow it: make it `const`, or copy it into a `const` variable first ``)*, and so does one on a field of a variable that isn't `const`.
- **Each branch of an `if` or a `switch` used as a value must fit the type expected there**, when there is one *(a declared variable, a return type, an argument)*. A branch that doesn't is an error where it is *(i.e. `this is a ValueError, but a (string) -> any is expected here`, for a `throw` that was forgotten)*. Without a type expected, branches of different types give `any`, like a list of values of different types.
- The check can be turned off completely, for scripts that are shipped already checked. Since types never change what the code does, the script behaves the same.
- There are no union types *(i.e. `string | number`)*. A value with one of several types is `any`, or the functions that need it are split in two *(i.e. one for a `string` and one for a `regex`)*.
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
- `any` isn't meant for normal use. It's for values whose type really can't be known, like JSON read without a schema, and they should be turned into a specific type with `.as()` as soon as possible. Types should be as specific as possible: generics instead of `any`, and a proper type instead of a map used as an object *(see the philosophy in [stdlib/README.md](../stdlib/README.md))*.
- Generic types use `<>`: `list<string>`, `map<string, number>`, `result<string, Exception>`. When generics are nested, the lexer splits `>>` into two `>` *(i.e. `list<map<string, any>>`)*.
- `.as()` can't take a function type *(i.e. `.as((string) -> any)`)*: a function keeps no types while the script runs, and an untyped lambda never had any, so the only way to check it would be to call it. That stays: a function is given its type where it comes from, or an `any` is assigned to a typed variable *(i.e. `const (string) -> any $f = $solver;`)*, which the checker doesn't check.
- Types never change what the code does: a script behaves the same whether its types were checked or not. Because of this, there's no overloading (choosing a function by the types of its arguments). `.as()` and `.is()` don't break this rule, because they check the value while the script runs, not its declared type *(see [builtin_types.md](builtin_types.md))*.

## How the checker works

- It reads the script once, from top to bottom, keeping the type of every variable and function. Modules imported with `use` are checked too, once each.
- It's conservative: when it can't be sure of a type, it uses `any`, so it never rejects a script that would run without errors because of types.
- The types of the built-in methods and of the standard library come from the tables in `ideas/stdlib`, so the docs and the checker always agree *(`tools/docs/prelude.py` turns them into `crates/lakescript/src/checker/prelude.txt`)*.
- Lists are invariant: a `list<string>` can't be passed where a `list<?string>` is expected, since the function could add a `null` to it. `any` is accepted everywhere.
- Functions declared with a return type other than `void` must end with a `return` or a `throw` on every path.
- A function registered by a program that embeds Lakescript is `any`: it takes any arguments and returns `any`.
- Checking takes less than a millisecond for small scripts. Once there's a bytecode cache, its result will be cached with the bytecode.

