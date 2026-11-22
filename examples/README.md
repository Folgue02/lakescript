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
| [control_flow.lks](control_flow.lks) | `if` and `switch` as values, `yield`, cases by type, loops, destructuring |
| [strings.lks](strings.lks) | String literals, multi-line strings, slicing and string methods |
| [collections.lks](collections.lks) | List and map methods: grouping, sorting (also with a comparison function), folding, counting |
| [patterns.lks](patterns.lks) | `-like`, regexes with `-match` and `.match()`, path tests |
| [errors.lks](errors.lks) | Exceptions, `try`/`catch`/`finally`, `throw` (also as an expression), `try` returning a `result`, custom exceptions as classes |
| [types.lks](types.lks) | Types: inference, generics, nullable values, narrowing, `.as()` and `.is()`, function types |
| [classes.lks](classes.lks) | Classes: fields, the constructor that takes every field, `new`, methods, static members, `extends`, `override` and `@super` |
| [interfaces.lks](interfaces.lks) | Interfaces: `implement` blocks, default methods, `Self`, interfaces that extend others, `.is()` with interfaces |
| [nulls.lks](nulls.lks) | Values that can be `null`: checks that narrow, a `never` function that stops the script, `?? fail(...)`, and `$x!` with its `NullError` | |
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
| [files.lks](files.lks) | `std/fs` and `std/path`, in a temporary folder | |
| [json.lks](json.lks) | `std/format/json`: parsing, JSON Lines with `parseAll`, writing, typed values with `.as()` | |
| [csv.lks](csv.lks) | `std/format/csv`: rows as lists or as maps by the names of the columns, quotes, other separators | |
| [html.lks](html.lks) | `std/html`: a page, CSS selectors, text and attributes, repaired HTML, the JSON in a `<script>` | |
| [terminal.lks](terminal.lks) | `std/terminal`: styles with `Style`, `ask` with a check, `choose`, `confirm`, a progress bar and a table; answers from a pipe without a terminal | `printf 'Ann\\n\\ntea\\ny\\n' \| lks examples/terminal.lks` |
| [http.lks](http.lks) | `std/http`: a request, its status and headers, JSON answers, `NetworkError` and downloading | `lks examples/http.lks https://api.github.com/repos/rust-lang/rust` |
| [templates/templates.lks](templates/templates.lks) | `std/template`: `{{ }}` tags, `for` and `if` blocks, a text template and an HTML template, `page.html.elks`, that escapes its values, `raw()` | |
| [testing.lks](testing.lks) | Tests: functions named `test...`, `assert` and `test::throws` | `lks --test examples/testing.lks` |
| [time.lks](time.lks) | `std/time`: timing code with `time::now()` in milliseconds | |
| [dates.lks](dates.lks) | `std/time` and `std/time/zone`: parsing and formatting dates with Java's letters, ISO 8601, the same instant in other zones, comparing and sorting dates | |
| [modules/main.lks](modules/main.lks) | `use` and `pub` with modules next to the script, in folders, with aliases, typed `pub const`s and a module's private `let` | |
| [shop/main.lks](shop/main.lks) | A file per type: `use store/Customer;` imports the class itself, and `Customer` and `Order` use each other | |
| [backup/backup.lks](backup/backup.lks) | A real script: arguments, a JSON config, a helper module, `tar`, rotating old backups | `lks examples/backup/backup.lks <folder> <backups folder>` |

## A project

| Example | Shows | Run it with |
|---------|-------|-------------|
| [sizes/](sizes) | A `ferry` project: `ferry.toml`, a module in `src/folgue02/sizes/` shared by two commands in `bin/`, its tests in `tests/`, and its docs: `//!` and `///` comments and a `module.md` | `ferry run human 1536`, `ferry test`, or `ferry doc --open`, from `examples/sizes` |

The tests of `ferry` run it (`crates/ferry/tests/ferry.rs`).

## Not yet

- Classes, enums and `command` for command-line arguments: phase 7.
