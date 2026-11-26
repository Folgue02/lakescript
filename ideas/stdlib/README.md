# Standard library

What comes with Lakescript. Its modules are under `std/` and are imported with `use` *(i.e. `use std/fs;`, see the module path in [modules.md](../language/modules.md))*. `out`, `outln`, `err`, `errln`, `exit` and the built-in exceptions can be used without `use`.

## Philosophy

- **If people keep writing the same helper in their scripts, the standard library is missing something.** Common operations get a function or a method, even the simple ones *(i.e. `$text.capitalize()`, `$list.unique()`, `$path.removeSuffix('.log')`)*. A script should read as what it does, not as the helpers it needed first.
- **Methods, not procedural functions.** Operations on a value are methods on that value: `$list.length()`, not `len($list)`, and `$list.push($item)`, not `array_push($list, $item)`. Global functions are only for things that don't belong to a value *(i.e. `outln`, `exit`)*.
- **Primitive types first.** If a built-in type is enough, it's used, so day-to-day scripts don't need to learn extra types. Paths are strings, the files of a folder are a `list<string>`, an exit code is a `number` *(like Python's `os.listdir()` and `os.path.join()`)*.
- **A new type only when the built-in ones aren't enough, never a map pretending to be one.** A map with fixed keys used as an object *(i.e. `{'size' => 10, 'modified' => ...}` for a file)* means a real type is missing. Older modules use separate functions instead *(i.e. `fs::size()`, `fs::modified()`)*, from before the standard library could declare classes. Maps are only for data whose keys aren't known in advance *(i.e. `env::all()`, JSON)*.
- **Types as specific as possible.** Signatures use generics instead of `any` *(i.e. `sortBy<K>((T) -> K $key)`)*. `any` is only for values whose type really can't be known *(i.e. JSON read without a schema)* or functions that accept every value *(i.e. `outln`)*.
- **Values, not class hierarchies.** Classes of the standard library are values that functions give back *(i.e. the `http::Response` of `http::get()`)*: scripts read their fields and call their methods, but never have to create them, extend them or implement interfaces to use a module.
- **Full names.** `length()`, not `len()`. `trimStart()`, not `lstrip()`. Abbreviations are only used when they're the common name of the thing *(i.e. `toJson()`, `max()`)*.

## Naming conventions

- **Questions end with `?`.** Methods that take no arguments and ask something about the value end with `?` *(i.e. `$list.empty?()`, `$n.even?()`, `$text.blank?()`)*. Methods that take an argument read as a sentence and don't *(i.e. `$text.startsWith('a')`, `$list.contains(5)`)*.
- **Strings and ranges never change.** Their methods always return a new value *(i.e. `$name.upper()` returns a new string, `$name` is the same)*.
- **Lists and maps: verbs change, past participles copy.** `$list.sort()` sorts the list itself, `$list.sorted()` returns a sorted copy and leaves the list as it was. The same goes for `reverse`/`reversed`, `shuffle`/`shuffled` and `merge`/`merged`. Methods that build something new from the values *(i.e. `map`, `filter`, `unique`)* always return a new list or map.
- **Missing things are `null`, broken things throw.** Searching for something that isn't there returns `null` *(i.e. `'abc'.indexOf('z')`, `[].first()`)*. Asking for something invalid throws *(i.e. `'abc'.toNumber()`, `$list.removeAt(10)`)*.
- **Errors** are the built-in exceptions *(`ValueError`, `IndexError`, ... see [errors.md](../language/errors.md))*.

## Leaving modules out

A program that embeds Lakescript can leave out the modules that need big crates. They're features of the `lakescript` crate, all on by default, so `lks` and `ferry` have every module:

| Feature | Module | Crates it needs |
|---------|--------|-----------------|
| `archive` | `std/archive`. It needs `dates` too | `zip`, `tar`, `flate2` |
| `bundle` | Running bundles, `.lkb` files, and writing them for `ferry bundle` *(see [bundles.md](../tooling/bundles.md))*. `std/resources` is always there, and reads folders without it | `zip` |
| `csv` | `std/format/csv` | `csv` |
| `dates` | The dates of `std/time`, `std/time/zone`, and `fs::modified()` as a date. `time::now()` is always there, and without dates `fs::modified()` gives seconds | `jiff` |
| `html` | `std/html` | `scraper` |
| `http` | `std/http` | `ureq`, with TLS |
| `terminal` | `std/terminal` | `inquire`, `crossterm` |

- `lakescript = { version = "...", default-features = false, features = ["dates"] }` has only the dates. Without any, the crate needs 14 crates instead of 163.
- A script that uses a module that was left out is told so *(i.e. `` `std/http` isn't in this program: it was built without the `http` feature of Lakescript ``)*.
- Regexes aren't a feature: they're part of the language.

## How modules are made

A module is written in Rust, in Lakescript, or both:

- **Rust**, in `crates/lakescript/src/stdlib/`, for what needs the system or speed *(i.e. `std/fs`)*. Their signatures come from the tables in these notes *(see `tools/docs/prelude.py`)*.
- **Lakescript**, in a `.lks` file embedded in the interpreter, for classes and for what's easier to write in the language itself *(i.e. `std/http`, `stdlib/http.lks`)*. They're checked like any other module, so their types and the `///` comments of their declarations reach the checker and editors.
- **Both**: the Lakescript part uses its Rust part as `std/<name>/native` *(i.e. `use std/http/native;`)*. Only modules of the standard library can use the `native` modules, and they aren't documented, since scripts never see them.

Errors in an embedded module name it as `<std>/http.lks`.

- **The docs are these notes**, for both: `tools/docs/stdlib.py` makes the pages of `ferry doc std` from their tables. Every `pub` item needs its row, which tests check: `prelude.txt` for the modules in Rust, and a test of `ferry` that reads the `pub` items of each module in Lakescript.

- **Rust data in a value**: a native part can give the Lakescript part data of its own, like a parsed page, as a value scripts never see *(a `Value::Handle`)*. It's kept in a field of a class that isn't `pub`, and freed when nothing uses it anymore *(i.e. `html::Element`, see [html.md](html.md))*. Printing an object leaves those fields out.
- **Functions that run Lakescript code**: most native functions only get their arguments, but some get the VM, to run code they're given or make *(a `Function::Vm`, i.e. `template::render`, `test::throws`)*.

## Files

What's coming next is in [planned.md](planned.md).

| File | Contents |
|------|----------|
| [types/any.md](types/any.md) | Methods every value has |
| [types/string.md](types/string.md) | `string` |
| [types/number.md](types/number.md) | `number` |
| [types/bool.md](types/bool.md) | `bool` |
| [types/list.md](types/list.md) | `list<T>` |
| [types/map.md](types/map.md) | `map<K, V>` |
| [types/range.md](types/range.md) | `range` |
| [types/regex.md](types/regex.md) | `regex` |
| [types/bytes.md](types/bytes.md) | `bytes` |
| [types/command.md](types/command.md) | `command`, created with `%x"..."` |

`result` is described in [builtin_types.md](../language/builtin_types.md).

| File | Contents |
|------|----------|
| [general.md](general.md) | The functions available without `use`: `out`, `outln`, `err`, `errln` and `exit` |
| [fs.md](fs.md) | `std/fs`: files and folders |
| [path.md](path.md) | `std/path`: building and splitting paths |
| [dirs.md](dirs.md) | `std/dirs`: the folders of a tool for its configuration, data and cache, and the user's folders |
| [env.md](env.md) | `std/env`: environment variables |
| [time.md](time.md) | `std/time`: the current time, dates, their formats and zones *(`std/time/zone`)* |
| [io.md](io.md) | `std/io`: reading stdin |
| [process.md](process.md) | `std/process`: arguments, the current folder, the platform, finding and running programs |
| [bytes.md](bytes.md) | `std/bytes`: bytes from hex and base64 |
| [archive.md](archive.md) | `std/archive`: zip, tar and tar.gz archives, read, extracted and written |
| [resources.md](resources.md) | `std/resources`: the files that aren't code, in `resources/` or inside a bundle |
| [random.md](random.md) | `std/random`: random numbers, `true` or `false`, UUIDs and the seed |
| [html.md](html.md) | `std/html`: reading HTML and finding its parts with CSS selectors |
| [http.md](http.md) | `std/http`: requests to web servers |
| [format/json.md](format/json.md) | `std/format/json`: reading and writing JSON |
| [format/csv.md](format/csv.md) | `std/format/csv`: reading and writing CSV |
| [format/toml.md](format/toml.md) | `std/format/toml`: reading and writing TOML, also into classes |
| [crypto/hash.md](crypto/hash.md) | `std/crypto/hash`: SHA-256, SHA-512, SHA-1, MD5, CRC-32 and HMAC |
| [crypto/random.md](crypto/random.md) | `std/crypto/random`: tokens, keys and numbers for secrets, from the system |
| [crypto/seal.md](crypto/seal.md) | `std/crypto/seal`: data sealed with a key, so only who has it reads it |
| [crypto/password.md](crypto/password.md) | `std/crypto/password`: passwords kept hashed with Argon2, and checked |
| [tooling/ferry.md](tooling/ferry.md) | `std/tooling/ferry`: the project a script is in, for its tasks |
| [test.md](test.md) | `std/test`: what tests need besides `assert` *(see [testing.md](../tooling/testing.md))* |
| [template.md](template.md) | `std/template`: templates with `{{ }}` tags, like Ruby's ERB |
| [terminal.md](terminal.md) | `std/terminal`: prompts, styles, progress and ready-made interactive pieces |

## Backlog

- `list.zip()`, once there's a type for pairs *(OOP or tuples)*. A list of two values of different types would be a list pretending to be a pair.
