# Lakescript

Lakescript is a scripting language, meant to replace bash for scripts that outgrow it.

- **Standalone scripts**: a single file with a shebang (`#!/usr/bin/lks`) that can be run directly.
- **Projects**: bigger programs split into files, with dependencies, like with cargo or npm. The tool for this is `ferry`, and `lks` is the interpreter.
- **Embeddable**: other programs can run Lakescript scripts, like Lua, for plugins or configuration.
- **Fast startup**: scripts must start and run quickly. Raw performance isn't the goal, but people don't like slow scripts, so the time between running the command and the first line running matters most.

## Decisions that follow from this

- **Scripts are compiled to bytecode** and run by a VM *(see [tooling/vm.md](tooling/vm.md))*.
- **Compiled code may be cached, later**: the first run of a script would compile it and save the bytecode, and later runs of the same unchanged script would skip parsing, checking and compiling. It's left for later because startup is already about 2 ms *(close to bash)*, so today a cache would save 1 to 3 ms. It comes back when there's more to save, i.e. with `ferry` projects of many modules.
    - The cache would be stored in a cache folder for the current user *(i.e. `~/.cache/lakescript` on Linux, `~/Library/Caches/lakescript` on macOS)*, not next to the script.
    - Each entry would be keyed by a hash of the script's contents and the interpreter version, so an edited script or a new interpreter version is compiled again.
- **No JIT**: a JIT makes long programs faster but the first seconds slower, which is the opposite of what scripts need.
- **Lazy loading**: imports and the standard library are only loaded when they're used.
- **Regexes are compiled on first use**, not when the script starts, since a script might never reach the line that uses them.
- **Compiled from top to bottom**: a block's functions and types are declared when it starts, so functions can call each other in any order, but the statements of a block can only use a function after its declaration. Everything else is compiled in one pass.
- **The language is a library**: the `lakescript` crate holds the lexer, parser, interpreter and standard library, and `lks` and `ferry` are programs built on it. So that any program can embed it, the library keeps no global state, never prints or exits on its own (output goes through something the host can replace, and errors are returned), never panics because of what a script does, lets the host leave out modules like `process` or `fs`, and lets the host add its own functions written in Rust.

## Notes

- [roadmap.md](roadmap.md): the phases the interpreter is built in.
- [language/](language): the language itself.
    - [syntax.md](language/syntax.md): general syntax, comments, names and reserved words.
    - [variables.md](language/variables.md): declarations, scopes and destructuring.
    - [type_checking.md](language/type_checking.md): how types are inferred and checked.
    - [builtin_types.md](language/builtin_types.md): strings, numbers, collections, ranges, results and regexes.
    - [operators.md](language/operators.md): every operator and their precedence.
    - [flow_control.md](language/flow_control.md): `if`, loops and `switch`.
    - [functions.md](language/functions.md): definitions, anonymous functions and function types.
    - [errors.md](language/errors.md): exceptions, `try`/`catch`/`finally`, `throw` and `try` as an expression.
    - [cli.md](language/cli.md): the command-line arguments of a script, `command`, `subcommand` and flags.
    - [modules.md](language/modules.md): `use`, `pub` and how modules match folders.
    - [classes.md](language/classes.md): classes, interfaces and enums *(phase 7, being designed)*.
- [stdlib/](stdlib): the standard library.
- [tooling/](tooling): the interpreter and the tools around it.
    - [lks.md](tooling/lks.md): the command line of the interpreter.
    - [vm.md](tooling/vm.md): the bytecode compiler and the VM.
    - [ferry.md](tooling/ferry.md): projects, dependencies, repositories and installed commands.
    - [diagnostics.md](tooling/diagnostics.md): how error messages look.
