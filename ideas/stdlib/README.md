# Standard library

What comes with Lakescript. How its modules are imported is still to be decided *(see [modules.md](../language/modules.md))*.

## Philosophy

- **If people keep writing the same helper in their scripts, the standard library is missing something.** Common operations get a function or a method, even the simple ones *(i.e. `$text.capitalize()`, `$list.unique()`, `$path.removeSuffix('.log')`)*. A script should read as what it does, not as the helpers it needed first.
- **Methods, not procedural functions.** Operations on a value are methods on that value: `$list.length()`, not `len($list)`, and `$list.push($item)`, not `array_push($list, $item)`. Global functions are only for things that don't belong to a value *(i.e. `outln`, `exit`)*.
- **Full names.** `length()`, not `len()`. `trimStart()`, not `lstrip()`. Abbreviations are only used when they're the common name of the thing *(i.e. `toJson()`, `max()`)*.

## Naming conventions

- **Questions end with `?`.** Methods that take no arguments and ask something about the value end with `?` *(i.e. `$list.empty?()`, `$n.even?()`, `$text.blank?()`)*. Methods that take an argument read as a sentence and don't *(i.e. `$text.startsWith('a')`, `$list.contains(5)`)*.
- **Strings and ranges never change.** Their methods always return a new value *(i.e. `$name.upper()` returns a new string, `$name` is the same)*.
- **Lists and maps: verbs change, past participles copy.** `$list.sort()` sorts the list itself, `$list.sorted()` returns a sorted copy and leaves the list as it was. The same goes for `reverse`/`reversed`, `shuffle`/`shuffled` and `merge`/`merged`. Methods that build something new from the values *(i.e. `map`, `filter`, `unique`)* always return a new list or map.
- **Missing things are `null`, broken things throw.** Searching for something that isn't there returns `null` *(i.e. `'abc'.indexOf('z')`, `[].first()`)*. Asking for something invalid throws *(i.e. `'abc'.toNumber()`, `$list.removeAt(10)`)*.
- **Exception names** are the ones from the proposed exception tree *(`ValueError`, `IndexError`, ...)*, which is still to be decided.

## Files

| File | Contents |
|------|----------|
| [types/any.md](types/any.md) | Methods every value has |
| [types/string.md](types/string.md) | `string` |
| [types/number.md](types/number.md) | `number` |
| [types/bool.md](types/bool.md) | `bool` |
| [types/list.md](types/list.md) | `list<T>` |
| [types/map.md](types/map.md) | `map<K, V>` |
| [types/range.md](types/range.md) | `range` |
| [types/symbol.md](types/symbol.md) | `symbol` |
| [types/regex.md](types/regex.md) | `regex` |
| [types/command.md](types/command.md) | `command`, created with `%x"..."` |

`result` is described in [builtin_types.md](../language/builtin_types.md).

### Still planned

| File          | Contents |
|---------------|----------|
| `general.md`  | Global functions, available without `use` *(i.e. `out`, `outln`, `exit`)* |
| `fs.md`       | Files, folders and paths |
| `process.md`  | Running shell commands, environment variables, the current folder |
| `terminal.md` | stdin, stdout and stderr, colors, asking the user for input |
| `cli.md`      | Command-line arguments and parsing them |
| `json.md`     | Reading and writing JSON |
| `time.md`     | Dates, times and durations |
| `errors.md`   | The built-in exception types |
