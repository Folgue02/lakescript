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
| [closures.lks](closures.lks) | Anonymous functions, closures, functions as values |
| [control_flow.lks](control_flow.lks) | `if` and `switch` as values, `yield`, cases by type, loops, destructuring |
| [strings.lks](strings.lks) | String literals, multi-line strings, slicing and string methods |
| [collections.lks](collections.lks) | List and map methods: grouping, sorting, folding, counting |
| [patterns.lks](patterns.lks) | `-like`, regexes with `-match` and `.match()`, path tests |
| [errors.lks](errors.lks) | Exceptions, `try`/`catch`/`finally`, `throw`, `try` returning a `result` |
| [types.lks](types.lks) | Types: inference, generics, nullable values, narrowing, `.as()` and `.is()`, type aliases |

## Scripts that use the system

| Example | Shows | Run it with |
|---------|-------|-------------|
| [greet.lks](greet.lks) | Arguments, environment variables, stderr, exit codes | `lks examples/greet.lks Ana Bo` |
| [wordcount.lks](wordcount.lks) | Reading piped input | `cat ideas/README.md \| lks examples/wordcount.lks 5` |
| [commands.lks](commands.lks) | `%x"..."` commands, safe quoting, `process::command()` | |
| [files.lks](files.lks) | `std/fs` and `std/path`, in a temporary folder | |
| [json.lks](json.lks) | `std/json`: parsing, writing, typed values with `.as()` | |
| [time.lks](time.lks) | `std/time`: timing code with `time::now()` in milliseconds | |
| [modules/main.lks](modules/main.lks) | `use` and `pub` with modules next to the script, in folders, with aliases | |
| [backup/backup.lks](backup/backup.lks) | A real script: arguments, a JSON config, a helper module, `tar`, rotating old backups | `lks examples/backup/backup.lks <folder> <backups folder>` |

## Not yet

- `ferry`, the package and build tool: phase 6.
- Classes, enums and `command` for command-line arguments: phase 7.
