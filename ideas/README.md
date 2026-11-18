# Lakescript

Lakescript is a scripting language, meant to replace bash for scripts that outgrow it.

- **Standalone scripts**: a single file with a shebang (`#!/usr/bin/lks`) that can be run directly.
- **Projects**: bigger programs split into files, with dependencies, like with cargo or npm.
- **Fast startup**: scripts must start and run quickly. Raw performance isn't the goal, but people don't like slow scripts, so the time between running the command and the first line running matters most.

## Decisions that follow from this

- **Compiled code is cached**: the first run of a script compiles it to bytecode and saves it. Later runs of the same unchanged script skip parsing and compiling.
    - The cache is stored in a cache folder for the current user *(i.e. `~/.cache/lakescript` on Linux, `~/Library/Caches/lakescript` on macOS)*, not next to the script.
    - Each entry is keyed by a hash of the script's contents and the interpreter version, so an edited script or a new interpreter version is compiled again.
- **No JIT**: a JIT makes long programs faster but the first seconds slower, which is the opposite of what scripts need.
- **Lazy loading**: imports and the standard library are only loaded when they're used.
- **Regexes are compiled on first use**, not when the script starts, since a script might never reach the line that uses them.
- **Single-pass compiling**: functions must be defined before they're called, so the script can be compiled from top to bottom in one go.

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
    - [modules.md](language/modules.md): `use`, `pub` and how modules match folders.
- [stdlib/](stdlib): the standard library.
- [tooling/](tooling): the interpreter and the tools around it.
    - [diagnostics.md](tooling/diagnostics.md): how error messages look.
