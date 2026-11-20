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
| **Later** | | The rest of the standard library, the per-user bytecode cache, formatter, language server, concurrency |

## Versions

Lakescript's own versions follow [semver](https://semver.org), like its packages *(see [modules.md](language/modules.md#packages))*.

- Each finished phase is a new minor version with a git tag: phase 1 is `v0.1.0`, phase 5 is `v0.5.0`. Fixes between phases are patch versions *(i.e. `v0.5.1`)*.
- The version in `Cargo.toml` is the one of the last tag, so `lks --version` says which phase it is.
- Pushing a version tag runs `.github/workflows/release.yml`: it builds `lks` and `ferry` for Linux (x86_64 and arm64, static with musl), macOS (arm64 and x86_64) and Windows (x86_64), and posts them on the tag's GitHub release, each as an archive with its SHA-256. `tools/release/package.sh <target>` builds one of those archives by hand, in `dist/`.
    - Windows is a supported platform, like Linux and macOS. Commands run without a shell on every system *(see [builtin_types.md](language/builtin_types.md))*, and on Windows `ferry install` writes `.cmd` launchers. The tests don't run on Windows yet, since many of them run Unix programs like `printf` and `sort`.
- While it's `0.x`, any version can change the language. `1.0.0` comes once the language is stable, and from then on only a new major version can break scripts.

## Status

- Phases 1 to 7 are done.
- Phase 5 replaced the tree-walker with a bytecode compiler and a VM *(see [tooling/vm.md](tooling/vm.md))*. The per-user cache was moved to later, since starting a script already takes about 2 ms.
- Phase 6 added `ferry` *(see [tooling/ferry.md](tooling/ferry.md))*: projects, dependencies from git repositories with exact versions, the lock file, and installed commands. `examples/sizes/` is a project.
- Phase 7 added OOP, in steps *(see [language/classes.md](language/classes.md#status))*: classes, interfaces, enums, generics, operators through interfaces, custom exceptions, type modules, modules that use each other, and `command` for the arguments of a script *(see [language/cli.md](language/cli.md))*. Type aliases are gone, and `pub const` needs its type.
- `examples/backup/backup.lks` is the phase 3 milestone: a real backup script with a module next to it, a JSON config, commands and files.

## Notes

- Exceptions come in phase 2 as a fixed tree of built-in types, without OOP. In phase 7, classes can extend them for custom exceptions, without breaking the code written before. The built-in ones stay built in.
- OOP comes after the type checker, since classes change how types work (fields, methods, inheritance).
- Phase 3 is the first milestone: Lakescript can replace a real bash script.
