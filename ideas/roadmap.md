# Roadmap

The language is built in phases. Each phase ends with something that runs.

The first versions were a **tree-walking interpreter**: the script is parsed into a syntax tree and the tree is run directly. It's the quickest way to try the syntax. The type checker needs the syntax tree anyway, and bytecode comes later, in phase 5. Types never change what the code does, so the checker can be added later without changing how scripts behave.

| Phase | Goal | Contents |
|-------|------|----------|
| **1. Hello world** | Run simple scripts | Rust project, `lks file.lks`, shebang, comments, numbers, strings with escapes and f-strings, `bool`, `null`, `let`/`const`, the arithmetic, comparison and logic operators, `if`/`else`, `while`, `for` over ranges and lists, named functions (with recursion), lists, maps, `outln`, error messages with the line and column. Type annotations are parsed but ignored |
| **2. The whole language** | Every syntax feature in the notes | `switch`, `if` as an expression, anonymous functions and closures, symbols *(removed since)*, `%w`/`%i`, destructuring, the null operators, `**`, the test operators, regexes, `try`/`throw`/`catch` with a basic exception tree, and the most-used methods of each type |
| **3. Bash replacement** | The first useful real scripts | `use`, `pub` and the module path (for `std/` and files next to the script), `%x"..."` and commands, `err`/`errln`, `exit`, `std/process` (with `process::args()`), `std/env`, `std/fs`, `std/path`, `std/io`, `std/json` |
| **4. Types** | The checker | Inference, generics, `.as()` and `.is()` narrowing, Rust-style error messages |
| **5. Speed** | Faster scripts | Bytecode compiler and a VM that replaces the tree-walker. The per-user cache moves to later |
| **6. Projects** | Bigger programs | `ferry`: the project file, `bin/` commands, dependencies from git repositories, the lock file, `ferry install` |
| **7. OOP** | Classes | Classes, interfaces, enums, custom exceptions as classes, `command` and `subcommand` for command-line arguments. Type aliases are removed, `pub const` needs its type, and modules can import each other in a cycle |
| **8. Editors** | A language server | `lks-lsp`: errors while typing, hover, go to definition and completion, first for farsub *(see [tooling/lsp.md](tooling/lsp.md))* |
| **9. Documentation** | Docs for scripts and projects | The documentation of the standard library, and `ferry doc` for the `pub` items of a project, from their `///` comments |
| **10. Interactive scripts** | Talking with the person who runs the script | `std/terminal` *(prompts, styles, progress, a fuzzy finder, a pager, tables)*, Ctrl+C as an `Interrupted`, and dates, their formats and time zones in `std/time` |
| **11. Data** | Data that isn't text, and typed data | `bytes`, JSON read into classes with types given as values, `std/random`, `std/archive`, `never` and `$x!`, symbols removed, and in `lks-lsp` signature help, the docs of parameters and importing types from completion |
| **12. Shipping** | A program in one file | Bundles: `ferry bundle` makes a `.lkb` with every module and the `resources/` it needs, run by `lks` *(see [tooling/bundles.md](tooling/bundles.md))* |
| **Later** | | The rest of the standard library, the per-user bytecode cache, formatter, concurrency |

## Versions

Lakescript's own versions follow [semver](https://semver.org), like its packages *(see [modules.md](language/modules.md#packages))*.

- Each finished phase is a new minor version with a git tag: phase 1 is `v0.1.0`, phase 5 is `v0.5.0`. Fixes between phases are patch versions *(i.e. `v0.5.1`)*.
- The version in `Cargo.toml` is the one of the last tag, so `lks --version` says which phase it is.
- Pushing a version tag runs `.github/workflows/release.yml`: it builds `lks`, `ferry` and `lks-lsp` for Linux (x86_64 and arm64, static with musl), macOS (arm64 and x86_64) and Windows (x86_64), and posts them on the tag's GitHub release, each as an archive with its SHA-256. Every archive has the pages of the standard library in `doc/std/`, made by the ferry just built *(`ferry doc std`)*, and the release has them on their own too, as `lakescript-<version>-stdlib.tar.gz`. `tools/release/package.sh <target>` builds one of those archives by hand, in `dist/`.
    - Windows is a supported platform, like Linux and macOS. Code only for Windows *(`#[cfg(windows)]`)* isn't built on the Mac where Lakescript is made, so before a release it's checked with `cargo check -p lakescript --all-targets --no-default-features --features archive,csv,dates,html,terminal --target x86_64-pc-windows-msvc`, with the rustup toolchain. `http` is left out since `ring` needs a C compiler for Windows. Commands run without a shell on every system *(see [builtin_types.md](language/builtin_types.md))*, and on Windows `ferry install` writes `.cmd` launchers. The tests don't run on Windows yet, since many of them run Unix programs like `printf` and `sort`.
- While it's `0.x`, any version can change the language. `1.0.0` comes once the language is stable, and from then on only a new major version can break scripts.

### While in development

Lakescript has no users but its author yet, so nothing is kept for old code:

- Syntax that's removed is just gone. There are no errors of their own for it *(i.e. "there are no type aliases")*, no warnings and no ways to migrate: old code gets the error any wrong code gets, and the notes say what replaced it.
- This changes once other people write Lakescript, at the latest with `1.0.0`.

## Status

- Phases 1 to 11 are done. Phase 12, bundles, is being designed.
- Phase 11 was released as `v0.11.0`, and `v0.11.1` fixes its build for Windows, which didn't compile, so `v0.11.0` has no binaries. `v0.11.2` makes `ferry` work on Windows: it finds its folder through `%USERPROFILE%`, and its launchers name `lks.exe` without `\\?\`:
    - Symbols are gone *(`:active`, `%i[...]`, the type `symbol`)*: enums do what they were for, and better, since the checker catches a typo in an enum's value, checks that a `switch` covers every one, and completes them. Strings do the rest. `process::platform()` gives a `Platform`, an enum in `std/process/Platform` *(see [stdlib/process.md](stdlib/process.md))*.
    - `!` takes any value: on anything but a `bool`, `!$x` is `$x == null` *(see Boolean operators in [language/operators.md](language/operators.md))*.
    - `ferry new` makes no sample code unless asked with `--example`, and `ferry run` shows wrong arguments to a command like `lks` does: the message and the usage, and exit code 2 *(see [tooling/ferry.md](tooling/ferry.md))*.
    - The `$name:` lines of doc comments are used: hover on parameters and named arguments, signature help in `lks-lsp`, and a warning for a `$name:` that isn't a parameter, in the editor, `ferry check` and `ferry doc` *(see Doc comments in [language/functions.md](language/functions.md))*. Editors complete `use` paths, with the first line of each folder's `module.md` and module's `//!`, and show them on hover *(see [tooling/lsp.md](tooling/lsp.md))*.
    - Methods of classes take named arguments *(see [language/functions.md](language/functions.md))*.
    - The docs of the standard library match its code: a test of `ferry` checks that every `pub` item of the modules written in Lakescript has its row in `ideas/stdlib`, like `prelude.txt` already does for the ones written in Rust. The missing ones were written *(`terminal::Progress` and `zone::Zone`)*.
    - `lks-lsp` imports types from completion: a class, interface or enum that isn't imported is offered with its module, and choosing it writes its `use` line, the standard library first *(see Importing types from completion in [tooling/lsp.md](tooling/lsp.md))*.
    - The library shows a program that embeds Lakescript only what it needs: the syntax tree, the lexer, the parser, the tokens and the insides of values stay inside *(see [README.md](README.md))*.
    - `std/random` *(see [stdlib/random.md](stdlib/random.md))* and `std/archive`, for zip, tar and tar.gz *(see [stdlib/archive.md](stdlib/archive.md))*.
    - A `bytes` type, for what isn't text: the literal `%b"..."`, `$text.toBytes()`, `std/bytes` for hex and base64, `fs::readBytes()` and `writeBytes()`, `$response.bytes()` and bytes as bodies, and `readBytes()` and `addBytes()` in archives *(see Bytes in [language/builtin_types.md](language/builtin_types.md))*.
    - JSON into types: `json::parse($text, list<User>)` and `$response.json(User)` fill in objects of classes, like serde, with errors that say where *(see Into types in [stdlib/format/json.md](stdlib/format/json.md))*. For it, a type can be given as an argument, a value of the type `type<T>` *(see Types as values in [language/type_checking.md](language/type_checking.md))*, and fields keep their types while the script runs.
    - `fs::modified()` gives a `time::Date` instead of seconds *(see [stdlib/fs.md](stdlib/fs.md))*.
- Phase 10 was released as `v0.10.0`: Ctrl+C throws an `Interrupted` *(see [language/errors.md](language/errors.md))*, `std/terminal` has prompts, styles, progress and ready-made pieces *(see [stdlib/terminal.md](stdlib/terminal.md))*, and `std/time` has dates, formats like Java's and time zones *(see [stdlib/time.md](stdlib/time.md))*. Errors thrown inside the standard library are shown where the script called it. After it: `never`, the return type of functions that never return, and `$x!`, which says a value isn't `null` *(see [language/functions.md](language/functions.md) and [language/operators.md](language/operators.md))*.
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

- **From the review after v0.10.0** *(see [tooling/vm.md](tooling/vm.md))*, not decided yet:
    - A checker that keeps the types it finds, instead of working them out again and keeping facts in tables by place in the source.
    - Not doing: a faster allocator for `lks` and `ferry`. It's a C library, which makes the builds of the releases more fragile, and after the fixes less time goes to allocating.

## Notes

- Exceptions come in phase 2 as a fixed tree of built-in types, without OOP. In phase 7, classes can extend them for custom exceptions, without breaking the code written before. The built-in ones stay built in.
- OOP comes after the type checker, since classes change how types work (fields, methods, inheritance).
- Phase 3 is the first milestone: Lakescript can replace a real bash script.
