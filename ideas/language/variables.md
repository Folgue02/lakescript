# Variables

- A variable can't be declared twice, not even in an inner scope: there is no shadowing. This includes the parameters of anonymous functions: in `($n) -> $n * 2`, `$n` can't be the name of a variable around it. It was discussed after the jq rewrite *(lq)*, and kept: a different name is always clearer than a reused one.

```
let $count = 0;
if ($ready) {
    let $count = 1;     // Error, $count already exists
}
```
- Each pass of a loop has its own scope, so a variable declared inside the body is a new variable on every pass, not a redeclaration. Closures created in the body keep the variable of their own pass, like `let` in JavaScript:

```
for ($file in $files) {
    const $path = f"{$dir}/{$file}";    // OK, a new $path on every pass
    // ...
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
