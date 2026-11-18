# Type checking

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
