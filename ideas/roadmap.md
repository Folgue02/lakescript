# Roadmap

The language is built in phases. Each phase ends with something that runs.

The first versions are a **tree-walking interpreter**: the script is parsed into a syntax tree and the tree is run directly. It's the quickest way to try the syntax. The type checker needs the syntax tree anyway, and bytecode and the cache come later, in phase 5. Types never change what the code does, so the checker can be added later without changing how scripts behave.

| Phase | Goal | Contents |
|-------|------|----------|
| **1. Hello world** | Run simple scripts | Rust project, `lks file.lks`, shebang, comments, numbers, strings with escapes and f-strings, `bool`, `null`, `let`/`const`, the arithmetic, comparison and logic operators, `if`/`else`, `while`, `for` over ranges and lists, named functions (with recursion), lists, maps, `outln`, error messages with the line and column. Type annotations are parsed but ignored |
| **2. The whole language** | Every syntax feature in the notes | `switch`, `if` as an expression, anonymous functions and closures, symbols, `%w`/`%i`, destructuring, the null operators, `**`, the test operators, regexes, `try`/`throw`/`catch` with a basic exception tree, and the most-used methods of each type |
| **3. Bash replacement** | The first useful real scripts | `use`, `pub` and the module path (for `std/` and files next to the script), `%x"..."` and commands, `err`/`errln`, `exit`, `std/process` (with `process::args()`), `std/env`, `std/fs`, `std/path`, `std/io`, `std/json` |
| **4. Types** | The checker | Inference, generics, `.as()` and `.is()` narrowing, Rust-style error messages |
| **5. Speed** | Fast startup | Bytecode compiler, a VM, the per-user cache |
| **6. Projects** | Bigger programs | `ferry`, the project file, dependencies, the CLI |
| **7. OOP** | Classes | Classes, enums, custom exceptions as classes, `command` and `subcommand` for command-line arguments |
| **Later** | | The rest of the standard library, formatter, language server, concurrency |

## Status

- Phases 1 to 4 are done, except commands written with `%x"..."` on Windows.
- `examples/backup/backup.lks` is the phase 3 milestone: a real backup script with a module next to it, a JSON config, commands and files.

## Notes

- Exceptions come in phase 2 as a fixed tree of built-in types, without OOP. In phase 7 they become classes, without breaking the code written before.
- OOP comes after the type checker, since classes change how types work (fields, methods, inheritance).
- Phase 3 is the first milestone: Lakescript can replace a real bash script.
