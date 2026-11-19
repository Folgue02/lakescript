# std/json

Reading and writing JSON. `use std/json;`, then `json::parse($text)`.

```
use std/json;

const $data = json::parse('{"name": "Daniel", "tags": ["a", "b"]}');
const $name = $data['name'].as(string);
outln(json::stringify($data, indent: 2));
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `parse(string $text)` | `any` | The value the JSON text holds. Throws a `ValueError` with the line and column if it isn't valid |
| `stringify(any $value, ?number $indent = null)` | `string` | The value as JSON text. Compact without `$indent`, indented with that many spaces with it *(i.e. `json::stringify($data, indent: 2)`)* |
| `read(string $path)` | `any` | Reads and parses a file, like `json::parse(fs::read($path))` |
| `write(string $path, any $value, ?number $indent = 2)` | `void` | Writes the value to a file, indented by default since files are usually read by people. `indent: null` writes it compact |

## Reading

There's no schema: the values become the built-in types, and the result is `any`. Scripts use `.as()` to get typed values out of it.

| JSON | Lakescript |
|------|------------|
| object | `map<string, any>`, keeping the order of the keys |
| array | `list<any>` |
| string | `string` |
| number | `number`. Very big integers become floats, like in JavaScript |
| `true`, `false` | `bool` |
| `null` | `null` |

- Only strict JSON is accepted: no comments and no trailing commas, so a file Lakescript reads is also valid for other tools.
- Invalid JSON throws a `ValueError` with the line and column.

## Writing

| Lakescript | JSON |
|------------|------|
| `map` | object. Number and symbol keys become strings, since JSON keys can only be strings |
| `list` | array |
| `range` | array of its numbers *(i.e. `1..4` is `[1, 2, 3]`)* |
| `symbol` | string |
| `string`, `number`, `bool`, `null` | the same |

- Functions, regexes, commands, exceptions and results can't be written: they throw a `TypeError`. So do `NaN` and `Infinity`, which JSON can't represent.
- `json::stringify($value)` writes compact JSON, and `json::stringify($value, indent: 2)` writes it indented.

## Backlog

- Reading JSON into objects of a class, checking the types of its fields while the script runs, like Java's Jackson or Rust's serde *(i.e. `json::parse($text, User)`)*. It comes with OOP: the fields of a class keep their types, so a value of the wrong type throws a `TypeError` that says which field it was.
