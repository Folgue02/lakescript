# Examples

Scripts that show what `lks` can run today. Every one of them is run by the tests (`crates/lks/tests/examples.rs`), so they always work with the current interpreter.

Run them from the root of the repository:

```
cargo run -p lks -- examples/hello.lks
```

Or build once with `cargo build -p lks` and use `target/debug/lks examples/hello.lks`.

Every example is type checked before it runs, like any script. `lks --no-type-check` skips it.

## The language

| Example | Shows |
|---------|-------|
| [hello.lks](hello.lks) | Constants, a one-line function, a `for` over a range, f-strings |
| [features.lks](features.lks) | Phase 1: FizzBuzz, primes, maps, default and named arguments, varargs, numbers |
| [closures.lks](closures.lks) | Anonymous functions with `->`, closures, functions as values, functions that call each other |
| [control_flow.lks](control_flow.lks) | `if` and `switch` as values, `yield`, cases by type, loops, destructuring, also with the index or the key |
| [strings.lks](strings.lks) | String literals, multi-line strings, slicing and string methods |
| [collections.lks](collections.lks) | List and map methods: grouping, sorting (also with a comparison function), folding, counting, and `...` inside a list |
| [patterns.lks](patterns.lks) | `-like`, regexes with `-match` and `.match()`, path tests |
| [errors.lks](errors.lks) | Exceptions, `try`/`catch`/`finally`, `throw` (also as an expression), `try` returning a `result`, custom exceptions as classes |
| [types.lks](types.lks) | Types: inference, generics, nullable values, narrowing, `.as()` and `.is()`, function types, `let` and `for` variables with a type |
| [classes.lks](classes.lks) | Classes: fields, the constructor that takes every field, `new`, methods, static members, `extends`, `override` and `@super` |
| [interfaces.lks](interfaces.lks) | Interfaces: `implement` blocks, default methods, `Self`, interfaces that extend others, `.is()` with interfaces |
| [nulls.lks](nulls.lks) | Values that can be `null`: checks that narrow, a `never` function that stops the script, `?? fail(...)`, `$x!` with its `NullError`, `!` on any value, and `??` with a nullable right side | |
| [enums.lks](enums.lks) | Enums: values with fields, methods, a `switch` over every value, `values()`, `from()` and `fromIgnoreCase()` |
| [generics.lks](generics.lks) | Generic classes, `Stack<string>()`, and an interface implemented with several types |
| [operators.lks](operators.lks) | Operators on objects through `IAdd`, `IMul`, `IEquals`, `IOrdered`, `IIndex` and `IContains`, and sorting objects |

## Scripts that use the system

| Example | Shows | Run it with |
|---------|-------|-------------|
| [greet.lks](greet.lks) | Arguments, environment variables, stderr, exit codes | `lks examples/greet.lks Ana Bo` |
| [todo.lks](todo.lks) | A `command` with subcommands, flags, an enum, `--help` and usage errors | `lks examples/todo.lks add 'docs' -p high` |
| [wordcount.lks](wordcount.lks) | Reading piped input | `cat ideas/README.md \| lks examples/wordcount.lks 5` |
| [commands.lks](commands.lks) | `%x"..."` commands, safe quoting, `process::command()` | |
| [bytes.lks](bytes.lks) | `bytes`: the `%b"..."` literal, text as UTF-8, hex and base64 with `std/bytes`, and a binary file written and read | |
| [archive.lks](archive.lks) | `std/archive`: a backup as a `.tar.gz` with a folder and a text file, its entries, reading one, and extracting it, in a temporary folder | |
| [random.lks](random.lks) | `std/random`: dice, a coin, choosing and shuffling lists, a seed that makes it repeatable, a UUID, saving and restoring the state, and a `Random` of its own | |
| [fullscreen.lks](fullscreen.lks) | `std/terminal`'s full screen: a dot that moves on its own, steered with the arrows, with `moveTo()`, `clearLine()` and `readKey(timeout:)` | `lks examples/fullscreen.lks`, in a terminal |
| [platform.lks](platform.lks) | `process::platform()` and the `Platform` enum in a `switch` | |
| [dirs.lks](dirs.lks) | `std/dirs`: a tool's settings kept in its configuration folder, read with defaults when they aren't there, and the folder created only to write them | |
| [files.lks](files.lks) | `std/fs` and `std/path`, in a temporary folder | |
| [json.lks](json.lks) | `std/format/json`: parsing, JSON Lines with `parseAll`, writing, typed values with `.as()`, reading into classes with `json::parse($text, list<Package>)`, and writing an object back | |
| [toml.lks](toml.lks) | `std/format/toml`: a configuration read as maps and into classes with defaults, written back with its sections, and the `null` TOML can't have | |
| [crypto.lks](crypto.lks) | `std/crypto`: SHA-256 and MD5, a file's hash, a webhook signed and checked with HMAC, secure tokens, data sealed with a key, and a password hashed with Argon2 | |
| [dotenv.lks](dotenv.lks) | `.env` files with `env::load()`, which never override a variable already set, and `env::parse()` | |
| [csv.lks](csv.lks) | `std/format/csv`: rows as lists or as maps by the names of the columns, quotes, other separators | |
| [html.lks](html.lks) | `std/html`: a page, CSS selectors, text and attributes, repaired HTML, the JSON in a `<script>` | |
| [terminal.lks](terminal.lks) | `std/terminal`: styles with `Style`, `ask` with a check, `choose`, `confirm`, a progress bar and a table; answers from a pipe without a terminal | `printf 'Ann\\n\\ntea\\ny\\n' \| lks examples/terminal.lks` |
| [http.lks](http.lks) | `std/http`: a request, its status and headers, JSON answers, `NetworkError` and downloading | `lks examples/http.lks https://api.github.com/repos/rust-lang/rust` |
| [templates/templates.lks](templates/templates.lks) | `std/template`: `{{ }}` tags, `for` and `if` blocks, a text template and an HTML template, `page.html.elks`, that escapes its values, `raw()` | |
| [testing.lks](testing.lks) | Tests: functions named `test...`, `assert`, `test::throws`, `beforeEach` and `afterEach`, `test::tempDir()`, and `withoutAccents()` in a slug | `lks --test examples/testing.lks` |
| [time.lks](time.lks) | `std/time`: timing code with `time::now()` in milliseconds | |
| [dates.lks](dates.lks) | `std/time` and `std/time/zone`: parsing and formatting dates with Java's letters, ISO 8601, the same instant in other zones, comparing and sorting dates | |
| [modules/main.lks](modules/main.lks) | `use` and `pub` with modules next to the script, in folders, with aliases, typed `pub const`s and a module's private `let` | |
| [shop/main.lks](shop/main.lks) | A file per type: `use store/Customer;` imports the class itself, and `Customer` and `Order` use each other, with no `use` for the types in their own folder | |
| [backup/backup.lks](backup/backup.lks) | A real script: arguments, a JSON config, a helper module, `tar`, rotating old backups | `lks examples/backup/backup.lks <folder> <backups folder>` |

## A project

| Example | Shows | Run it with |
|---------|-------|-------------|
| [sizes/](sizes) | A `ferry` project: `ferry.toml`, a module in `src/folgue02/sizes/` shared by two commands in `bin/`, its tests in `tests/`, its docs: `//!` and `///` comments and a `module.md`, a help text in `resources/` read with `std/resources`, bundles of its commands, and a task of its own in `scripts/` that reads the project with `std/tooling/ferry` | `ferry run human 1536`, `ferry run bytes`, `ferry test`, `ferry doc --open`, `ferry task info`, or `ferry bundle` and then `lks dist/human.lkb 1536`, from `examples/sizes` |

The tests of `ferry` run it (`crates/ferry/tests/ferry.rs`).

## Not yet

- Classes, enums and `command` for command-line arguments: phase 7.
