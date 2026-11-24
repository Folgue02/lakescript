# std/format/json

Reading and writing JSON. `use std/format/json;`, then `json::parse($text)`.

```
use std/format/json;

const $data = json::parse('{"name": "Daniel", "tags": ["a", "b"]}');
const $name = $data['name'].as(string);
outln(json::stringify($data, indent: 2));
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `parse<T>(string $text, ?type<T> $type = null)` | `T` | The value the JSON text holds. Throws a `ValueError` with the line and column if it isn't valid. With a type, the value read into it *(i.e. `json::parse($text, User)`, see [Into types](#into-types))*, and `any` without one |
| `parseAll<T>(string $text, ?type<T> $type = null)` | `list<T>` | Every value in a text that holds several, separated by whitespace: streams like `{"a":1} {"a":2}` and JSON Lines, also when a value spans several lines. An empty text gives `[]`. With a type, each value read into it |
| `stringify(any $value, ?number $indent = null)` | `string` | The value as JSON text. Compact without `$indent`, indented with that many spaces with it *(i.e. `json::stringify($data, indent: 2)`)* |
| `read<T>(string $path, ?type<T> $type = null)` | `T` | Reads and parses a file, like `json::parse(fs::read($path), $type)` |
| `write(string $path, any $value, ?number $indent = 2)` | `void` | Writes the value to a file, indented by default since files are usually read by people. `indent: null` writes it compact |

## Reading

Without a type, the values become the built-in types, and the result is `any`. Scripts use `.as()` to get typed values out of it, or give a type *(see [Into types](#into-types))*.

| JSON | Lakescript |
|------|------------|
| object | `map<string, any>`, keeping the order of the keys |
| array | `list<any>` |
| string | `string` |
| number | `number`. Very big integers become floats, like in JavaScript *(i.e. `12345678901234567890` is `1.2345678901234567e19`)*, and whole floats print without the decimal point *(`189.0` is written back as `189`)*. jq 1.7 keeps the original literal, but Lakescript doesn't, since `number` doesn't remember how it was written |
| `true`, `false` | `bool` |
| `null` | `null` |

- Only strict JSON is accepted: no comments and no trailing commas, so a file Lakescript reads is also valid for other tools.
- Invalid JSON throws a `ValueError` with the line and column.

## Into types

`json::parse($text, User)` reads the JSON into that type, like Rust's serde or Java's Jackson: objects of classes, with their fields filled in, and the result has that type for the checker. The type is written as an argument, a class like `User` or a built-in type like `list<User>` or `map<string, User>`.

```
class Tag {
    pub string @name;
}
class User {
    pub string @name;
    pub ?string @email;
    pub list<Tag> @tags = [];
    pub Role @role = Role::USER;
}
enum Role { USER, ADMIN }

const $users = json::parse($text, list<User>);   // list<User>
outln($users[0]!.tags.map(($t) -> $t.name));
const $me = http::get($url).json(User);         // User
```

- **Classes**: a JSON object, by the names of the fields. Their values are read into the types of the fields, so nested classes are filled in too. A field that isn't in the object gets its default, or `null` if it's nullable, and one without either throws. Keys the class doesn't have are ignored, since APIs add fields. A class with its own `new` gets the keys as named arguments of it.
- **Enums**: the name of a value, as a string *(i.e. `"ADMIN"`)*.
- **Built-in types**: `string`, `number`, `bool`, `any`, `list<T>`, `map<string, T>`, and `?T` for `null` or a `T`.
- A value of the wrong type throws a `TypeError` that says where it is *(i.e. `` `$[2].age` is a string, not a number ``, `` `$.user` has no `name`, which `User` needs ``)*.
- A type given as an argument is a value of the type `type<T>` *(see Types as values in [type_checking.md](../../language/type_checking.md))*.

## Writing

| Lakescript | JSON |
|------------|------|
| `map` | object. Number keys become strings, since JSON keys can only be strings |
| `list` | array |
| `range` | array of its numbers *(i.e. `1..4` is `[1, 2, 3]`)* |
| `string`, `number`, `bool`, `null` | the same |

- Functions, regexes, commands, exceptions and results can't be written: they throw a `TypeError`. So do `NaN` and `Infinity`, which JSON can't represent.
- `json::stringify($value)` writes compact JSON, and `json::stringify($value, indent: 2)` writes it indented.

## Backlog

- Writing objects of classes, by their `pub` fields, the other way of reading them.
