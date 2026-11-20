# Standard library

What comes with Lakescript. Its modules are under `std/` and are imported with `use` *(i.e. `use std/fs;`, see the module path in [modules.md](../language/modules.md))*. `out`, `outln`, `err`, `errln`, `exit` and the built-in exceptions can be used without `use`.

## Philosophy

- **If people keep writing the same helper in their scripts, the standard library is missing something.** Common operations get a function or a method, even the simple ones *(i.e. `$text.capitalize()`, `$list.unique()`, `$path.removeSuffix('.log')`)*. A script should read as what it does, not as the helpers it needed first.
- **Methods, not procedural functions.** Operations on a value are methods on that value: `$list.length()`, not `len($list)`, and `$list.push($item)`, not `array_push($list, $item)`. Global functions are only for things that don't belong to a value *(i.e. `outln`, `exit`)*.
- **Primitive types first.** If a built-in type is enough, it's used, so day-to-day scripts don't need to learn extra types. Paths are strings, the files of a folder are a `list<string>`, an exit code is a `number` *(like Python's `os.listdir()` and `os.path.join()`)*.
- **A new type only when the built-in ones aren't enough, never a map pretending to be one.** A map with fixed keys used as an object *(i.e. `{'size' => 10, 'modified' => ...}` for a file)* means a real type is missing. Until there are classes, the standard library uses separate functions instead *(i.e. `fs::size()`, `fs::modified()`)*. Maps are only for data whose keys aren't known in advance *(i.e. `env::all()`, JSON)*.
- **Types as specific as possible.** Signatures use generics instead of `any` *(i.e. `sortBy<K>((T) -> K $key)`)*. `any` is only for values whose type really can't be known *(i.e. JSON read without a schema)* or functions that accept every value *(i.e. `outln`)*.
- **Full names.** `length()`, not `len()`. `trimStart()`, not `lstrip()`. Abbreviations are only used when they're the common name of the thing *(i.e. `toJson()`, `max()`)*.

## Naming conventions

- **Questions end with `?`.** Methods that take no arguments and ask something about the value end with `?` *(i.e. `$list.empty?()`, `$n.even?()`, `$text.blank?()`)*. Methods that take an argument read as a sentence and don't *(i.e. `$text.startsWith('a')`, `$list.contains(5)`)*.
- **Strings and ranges never change.** Their methods always return a new value *(i.e. `$name.upper()` returns a new string, `$name` is the same)*.
- **Lists and maps: verbs change, past participles copy.** `$list.sort()` sorts the list itself, `$list.sorted()` returns a sorted copy and leaves the list as it was. The same goes for `reverse`/`reversed`, `shuffle`/`shuffled` and `merge`/`merged`. Methods that build something new from the values *(i.e. `map`, `filter`, `unique`)* always return a new list or map.
- **Missing things are `null`, broken things throw.** Searching for something that isn't there returns `null` *(i.e. `'abc'.indexOf('z')`, `[].first()`)*. Asking for something invalid throws *(i.e. `'abc'.toNumber()`, `$list.removeAt(10)`)*.
- **Errors** are the built-in exceptions *(`ValueError`, `IndexError`, ... see [errors.md](../language/errors.md))*.

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

| File | Contents |
|------|----------|
| [general.md](general.md) | The functions available without `use`: `out`, `outln`, `err`, `errln` and `exit` |
| [fs.md](fs.md) | `std/fs`: files and folders |
| [path.md](path.md) | `std/path`: building and splitting paths |
| [env.md](env.md) | `std/env`: environment variables |
| [time.md](time.md) | `std/time`: the current time |
| [io.md](io.md) | `std/io`: reading stdin |
| [process.md](process.md) | `std/process`: arguments, the current folder, finding and running programs |
| [json.md](json.md) | `std/json`: reading and writing JSON |

### Still planned

| File          | Contents |
|---------------|----------|
| `terminal.md` | Colors, the size of the terminal, asking the user for input |
| `time.md`     | Dates, times and durations |

## Backlog

- `list.zip()`, once there's a type for pairs *(OOP or tuples)*. A list of two values of different types would be a list pretending to be a pair.
