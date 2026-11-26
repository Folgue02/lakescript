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
| **13. Projects, finished** | Every step of a project's life | `ferry uninstall`, `remove` and `update`, version ranges, and in tests `test::tempDir()`, before and after each test, and running them in parallel, and `std/format/toml` *(see [Plan](#plan))* |
| **14. Editors and docs** | The rest of `lks-lsp` | Milestones 4 and 5 *(every error at once, the outline, find references, rename)*, and syntax highlighting in `ferry doc` |
| **15. The toolchain** | Installing and choosing versions | A version manager like rustup |
| **1.0.0** | A stable language | Concurrency, tests on Windows, a formatter, the version manager and the rest of `lks-lsp` |
| **Later** | | The rest of the standard library, the per-user bytecode cache |

## Versions

Lakescript's own versions follow [semver](https://semver.org), like its packages *(see [modules.md](language/modules.md#packages))*.

- Each finished phase is a new minor version with a git tag: phase 1 is `v0.1.0`, phase 5 is `v0.5.0`. Fixes between phases are patch versions *(i.e. `v0.5.1`)*. A patch is only fixes: anything new, even small and without breaking anything, waits for the next phase, so a patch is always safe to take.
- The version in `Cargo.toml` is the one of the last tag, so `lks --version` says which phase it is.
- Pushing a version tag runs `.github/workflows/release.yml`: it builds `lks`, `ferry` and `lks-lsp` for Linux (x86_64 and arm64, static with musl), macOS (arm64 and x86_64) and Windows (x86_64), and posts them on the tag's GitHub release, each as an archive with its SHA-256. Every archive has the pages of the standard library in `doc/std/`, made by the ferry just built *(`ferry doc std`)*, and the release has them on their own too, as `lakescript-<version>-stdlib.tar.gz`. `tools/release/package.sh <target>` builds one of those archives by hand, in `dist/`.
    - Windows is a supported platform, like Linux and macOS. Code only for Windows *(`#[cfg(windows)]`)* isn't built on the Mac where Lakescript is made, so before a release it's checked with `cargo check -p lakescript --all-targets --no-default-features --features archive,bundle,csv,dates,html,terminal --target x86_64-pc-windows-msvc`, with the rustup toolchain. `http` is left out since `ring` needs a C compiler for Windows. Commands run without a shell on every system *(see [builtin_types.md](language/builtin_types.md))*, and on Windows `ferry install` writes `.cmd` launchers. The tests don't run on Windows yet, since many of them run Unix programs like `printf` and `sort`.
- While it's `0.x`, any version can change the language. `1.0.0` comes once the language is stable, and from then on only a new major version can break scripts.

### While in development

Lakescript has no users but its author yet, so nothing is kept for old code:

- Syntax that's removed is just gone. There are no errors of their own for it *(i.e. "there are no type aliases")*, no warnings and no ways to migrate: old code gets the error any wrong code gets, and the notes say what replaced it.
- This changes once other people write Lakescript, at the latest with `1.0.0`.

## Status

- Phases 1 to 12 are done. Phase 12 was released as `v0.12.0`: bundles. `ferry bundle` makes a `.lkb` for each command, which runs with `lks todo.lkb` or as `./todo.lkb`, and `std/resources` reads the files of `resources/` from the folder or from inside the bundle *(see [tooling/bundles.md](tooling/bundles.md) and [stdlib/resources.md](stdlib/resources.md))*. It also has what was built after `v0.11.2`, which waited for it instead of being a patch version, since it was mostly features: No path in a launcher has `\\?\`, also the module path and the script, and `std/dirs` gives the folders of a tool on every system, for its configuration, data, cache and state, and the user's folders like downloads *(see [stdlib/dirs.md](stdlib/dirs.md))*. Types in the same folder can be used without `use`, like the classes of a package in Java *(see Types in the same folder in [language/modules.md](language/modules.md))*. Fixes: a range can end in a field or a method of the object *(i.e. `0..@passes`, `@y..=@bottom()`)*; `-in` and `-notin` work with an object or an enum value on the left of a list; and `.equals()`, `-in` and the methods of lists and maps that look for a value *(`contains`, `indexOf`, `count`, `unique`, `remove`...)* compare objects through `equals?`, like `==` *(see [language/classes.md](language/classes.md))*. Decided after cinder, and built: `let` takes a type, like `const` *(`let ?Point $best = null;`)*, and so does the variable of a `for`, which can also destructure the element after an index or a key *(`for ($i, [$name, $age] in $rows)`, see [language/variables.md](language/variables.md) and [language/flow_control.md](language/flow_control.md))*; `...` inside list literals, and a nullable right side of `??` *(see [language/operators.md](language/operators.md))*; the `Usage:` line of `--help` says what was typed, with `lks --name` for `ferry run` and the launchers *(see [language/cli.md](language/cli.md))*; `terminal::fullScreen()`, `moveTo()`, `clearLine()` and `readKey(timeout:)` *(see [stdlib/terminal.md](stdlib/terminal.md))*; `json::stringify()` and `write()` write objects of classes with every field *(see [stdlib/format/json.md](stdlib/format/json.md))*; and `random::state()`, `restore()` and `random::Random` *(see [stdlib/random.md](stdlib/random.md))*. A list or map literal is checked against the type expected for it, element by element, so `const list<number> $x = [1, 'a'];` is an error at the `'a'` *(see [language/type_checking.md](language/type_checking.md))*. The VM got faster: strings and objects 2 to 3 times as fast, cinder's bot 30% fewer cycles, and every benchmark but `fib` as fast as Python or faster *(see The review after phase 11 in [tooling/vm.md](tooling/vm.md))*. Kept as they are: a regex group that didn't take part is `''`, and a field holding a function is called as `(@field)(...)`.
- Phase 11 was released as `v0.11.0`, and `v0.11.1` fixes its build for Windows, which didn't compile, so `v0.11.0` has no binaries. `v0.11.2` makes `ferry` work on Windows: it finds its folder through `%USERPROFILE%`, and its launchers name `lks.exe` without `\\?\`. Phase 11 itself:
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

## Plan

What comes next, in order, decided after `v0.12.0`. Patches are only fixes; anything new waits for its phase. A phase is a step that's worth a version, so a single module goes into the phase it fits *(i.e. `std/format/toml` in phase 13)* instead of being one of its own.

### `v0.12.1`, fixes

- Built: in `ferry doc`, search finds the items of the dependencies, and their types in a signature link to their pages *(see ferry doc in [tooling/docs.md](tooling/docs.md))*.
- Removing `dirs::app()` breaks code, so it isn't in it: it waits for `v0.13.0`.

### Phase 13, `v0.13.0`: projects, finished

- `ferry uninstall`, `ferry remove` and `ferry update` *(see Later in [tooling/ferry.md](tooling/ferry.md))*.
- Version ranges for dependencies *(i.e. `^1.4`)*, which `ferry update` needs.
- `test::tempDir()`, a folder that is removed after the test, functions that run before and after each test, and running tests in parallel *(see Later in [tooling/testing.md](tooling/testing.md))*.
- `std/format/toml`, reading and writing TOML like `std/format/json` *(see [stdlib/planned.md](stdlib/planned.md))*. The `toml` crate is already in the workspace, for `ferry.toml`, and a project's configuration is often TOML. XML is later, not planned.

### Phase 14, `v0.14.0`: editors and docs

- Milestones 4 and 5 of `lks-lsp`: every error at once and the outline of a file, then find references and rename *(see [tooling/lsp.md](tooling/lsp.md#milestones))*.
- Syntax highlighting for the code blocks of `ferry doc` *(see [tooling/docs.md](tooling/docs.md))*. With the editor in the same phase, both can use the same highlighting of Lakescript.

### Phase 15, `v0.15.0`: the toolchain

- A version manager like rustup. It's late in the plan since it matters most once other people install Lakescript. The idea so far:
    - A manager installed first, on its own, with a script *(`curl … | sh`, and an `.exe` for Windows)*. It then installs versions of Lakescript from the releases *(i.e. `install 0.12.0`)*, checking their `.sha256`, into `~/.ferry/toolchains/<version>/`. It can also list them, set the default, update to the newest, update itself and uninstall a version.
    - `lks`, `ferry` and `lks-lsp` in `~/.ferry/bin` are proxies of the manager, like `cargo` and `rustc` under rustup. Each one runs the version for where it's run: `lks +0.11.2 script.lks`, then the pin of the project, then the default. So an editor's `lks-lsp` checks each project with its own version.
    - It's possible because the releases already have an archive for each target, with a `.sha256`, and the binaries don't need anything installed. Others from the same idea: a `doc` command for the std pages the archives already have, and a `link` for a local build, like `rustup toolchain link`, instead of `scripts/install-all.sh`.
    - Still to decide:
        - its name, and whether it's a tool of its own or part of `ferry`;
        - where a project pins its version *(i.e. `lks = "0.12.0"` in `ferry.toml`, or a file of its own)*;
        - whether it's released with Lakescript or has versions of its own;
        - which `lks` the commands `ferry install` writes run.
    - Until then, mise's `github:` backend installs from the releases as they are *(`mise use github:Folgue02/lakescript@0.12.0`)*, and hasn't been tried.

### Before `1.0.0`

- Concurrency. It changes the language, so it has to come before the language is stable. How it works isn't decided yet.
- The tests run on Windows, in CI, and not only a check of the code built for it.
- A formatter.
- The version manager of phase 15.
- Milestones 4 and 5 of `lks-lsp`, from phase 14.
- And the language is stable *(see [Versions](#versions))*.

### Later, not planned

- The rest of the standard library, `std/format/xml` *(an element tree like `std/html`'s, see [stdlib/planned.md](stdlib/planned.md))* and `std/tui`.
- The per-user bytecode cache.
- Searching the docs by type, and running a test from an editor.
- From the review after v0.10.0 *(see [tooling/vm.md](tooling/vm.md))*, not decided yet: a checker that keeps the types it finds, instead of working them out again and keeping facts in tables by place in the source. ~~Not doing: a faster allocator.~~ Done after phase 11, in pure Rust: `lakescript::SmallAlloc` *(see The review after phase 11 in [tooling/vm.md](tooling/vm.md))*.

## Notes

- Exceptions come in phase 2 as a fixed tree of built-in types, without OOP. In phase 7, classes can extend them for custom exceptions, without breaking the code written before. The built-in ones stay built in.
- OOP comes after the type checker, since classes change how types work (fields, methods, inheritance).
- Phase 3 is the first milestone: Lakescript can replace a real bash script.
