# Roadmap

The language is built in phases. Each phase ends with something that runs.

The first versions were a **tree-walking interpreter**: the script is parsed into a syntax tree and the tree is run directly. It's the quickest way to try the syntax. The type checker needs the syntax tree anyway, and bytecode comes later, in phase 5. Types never change what the code does, so the checker can be added later without changing how scripts behave.

| Phase | Goal | Contents |
|-------|------|----------|
| **1. Hello world** | Run simple scripts | Rust project, `lks file.lks`, shebang, comments, numbers, strings with escapes and f-strings, `bool`, `null`, `let`/`const`, the arithmetic, comparison and logic operators, `if`/`else`, `while`, `for` over ranges and lists, named functions (with recursion), lists, maps, `outln`, error messages with the line and column. Type annotations are parsed but ignored |
| **2. The whole language** | Every syntax feature in the notes | `switch`, `if` as an expression, anonymous functions and closures, symbols, `%w`/`%i`, destructuring, the null operators, `**`, the test operators, regexes, `try`/`throw`/`catch` with a basic exception tree, and the most-used methods of each type |
| **3. Bash replacement** | The first useful real scripts | `use`, `pub` and the module path (for `std/` and files next to the script), `%x"..."` and commands, `err`/`errln`, `exit`, `std/process` (with `process::args()`), `std/env`, `std/fs`, `std/path`, `std/io`, `std/json` |
| **4. Types** | The checker | Inference, generics, `.as()` and `.is()` narrowing, Rust-style error messages |
| **5. Speed** | Faster scripts | Bytecode compiler and a VM that replaces the tree-walker. The per-user cache moves to later |
| **6. Projects** | Bigger programs | `ferry`: the project file, `bin/` commands, dependencies from git repositories, the lock file, `ferry install` |
| **7. OOP** | Classes | Classes, interfaces, enums, custom exceptions as classes, `command` and `subcommand` for command-line arguments. Type aliases are removed, `pub const` needs its type, and modules can import each other in a cycle |
| **8. Editors** | A language server | `lks-lsp`: errors while typing, hover, go to definition and completion, first for farsub *(see [tooling/lsp.md](tooling/lsp.md))* |
| **9. Documentation** | Docs for scripts and projects | The documentation of the standard library, and `ferry doc` for the `pub` items of a project, from their `///` comments |
| **Later** | | The rest of the standard library, the per-user bytecode cache, formatter, concurrency |

## Versions

Lakescript's own versions follow [semver](https://semver.org), like its packages *(see [modules.md](language/modules.md#packages))*.

- Each finished phase is a new minor version with a git tag: phase 1 is `v0.1.0`, phase 5 is `v0.5.0`. Fixes between phases are patch versions *(i.e. `v0.5.1`)*.
- The version in `Cargo.toml` is the one of the last tag, so `lks --version` says which phase it is.
- Pushing a version tag runs `.github/workflows/release.yml`: it builds `lks`, `ferry` and `lks-lsp` for Linux (x86_64 and arm64, static with musl), macOS (arm64 and x86_64) and Windows (x86_64), and posts them on the tag's GitHub release, each as an archive with its SHA-256. Every archive has the pages of the standard library in `doc/std/`, made by the ferry just built *(`ferry doc std`)*, and the release has them on their own too, as `lakescript-<version>-stdlib.tar.gz`. `tools/release/package.sh <target>` builds one of those archives by hand, in `dist/`.
    - Windows is a supported platform, like Linux and macOS. Commands run without a shell on every system *(see [builtin_types.md](language/builtin_types.md))*, and on Windows `ferry install` writes `.cmd` launchers. The tests don't run on Windows yet, since many of them run Unix programs like `printf` and `sort`.
- While it's `0.x`, any version can change the language. `1.0.0` comes once the language is stable, and from then on only a new major version can break scripts.

### While in development

Lakescript has no users but its author yet, so nothing is kept for old code:

- Syntax that's removed is just gone. There are no errors of their own for it *(i.e. "there are no type aliases")*, no warnings and no ways to migrate: old code gets the error any wrong code gets, and the notes say what replaced it.
- This changes once other people write Lakescript, at the latest with `1.0.0`.

## Status

- Phases 1 to 9 are done.
- Phase 9 added `ferry doc`, released as `v0.9.0`: it makes the pages of a project and of a package, and writes the docs of one thing in the terminal *(see [tooling/docs.md](tooling/docs.md))*. What's missing of it *(i.e. search across the dependencies and the standard library)* can come as patch versions.
- Phase 5 replaced the tree-walker with a bytecode compiler and a VM *(see [tooling/vm.md](tooling/vm.md))*. The per-user cache was moved to later, since starting a script already takes about 2 ms.
- Phase 6 added `ferry` *(see [tooling/ferry.md](tooling/ferry.md))*: projects, dependencies from git repositories with exact versions, the lock file, and installed commands. `examples/sizes/` is a project.
- Phase 8 added `lks-lsp` *(see [tooling/lsp.md](tooling/lsp.md#status))*, released as `v0.8.0`: it sends the errors of the open files while they're typed, answers hover and go to definition, and completes names. Milestones 4 and 5 can come as patch versions.
- Phase 7 added OOP, in steps *(see [language/classes.md](language/classes.md#status))*: classes, interfaces, enums, generics, operators through interfaces, custom exceptions, type modules, modules that use each other, and `command` for the arguments of a script *(see [language/cli.md](language/cli.md))*. Type aliases are gone, and `pub const` needs its type.
- The standard library is growing alongside phase 8 *(see [stdlib/planned.md](stdlib/planned.md))*: its modules can be written in Lakescript and declare classes, `std/http`, `std/html`, `std/format/csv` and `std/template` are new, and `std/json` moved to `std/format/json`.
- Uncaught exceptions show the calls that led to them, and `$e.trace()` gives them *(see Tracebacks in [language/errors.md](language/errors.md))*.
- Tests: functions named `test...`, the `assert` statement, `std/test`, `ferry test` and `lks --test` *(see [tooling/testing.md](tooling/testing.md))*.
- `examples/backup/backup.lks` is the phase 3 milestone: a real backup script with a module next to it, a JSON config, commands and files.

## Backlog

Decided, and not built yet.

- **Ctrl+C as an exception**, `Interrupted`, beside `Exception` and thrown anywhere in a script, like Python's `KeyboardInterrupt` *(see Ctrl+C in [language/errors.md](language/errors.md))*. `std/terminal` needs it.
- **Remove symbols** *(`:active`, `%i[...]`, the type `symbol`)*. Since phase 7, enums do what they were for, and better: the checker catches a typo in an enum's value, checks that a `switch` covers every one, and completes them, while any `:name` is a valid symbol. Strings do the rest. Symbols also gave two ways to write the same key *(`{:a => 1}['a']` is `null`)*, didn't survive JSON or CSV, which write them as strings, and cost syntax next to named arguments, `? :` and `::`.
    - Fixed sets of values become enums, and loose values strings *(i.e. `case :admin` becomes `case 'admin'`, or an enum)*.
    - `process::platform()` gives a string or an enum instead of `:linux`, `:macos` and `:windows`. Which one is decided when it's built.
    - It touches the lexer, the parser, the checker, the methods of `symbol`, `std/format/json` and `std/format/csv`, the examples and the notes *(i.e. [types/symbol.md](stdlib/types/symbol.md))*, and the grammars of editors, like farsub's.

## Notes

- Exceptions come in phase 2 as a fixed tree of built-in types, without OOP. In phase 7, classes can extend them for custom exceptions, without breaking the code written before. The built-in ones stay built in.
- OOP comes after the type checker, since classes change how types work (fields, methods, inheritance).
- Phase 3 is the first milestone: Lakescript can replace a real bash script.
