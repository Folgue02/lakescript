# The language server

Phase 8 *(see [roadmap.md](../roadmap.md))*. `lks-lsp` gives editors what the checker knows while code is written: errors, types, definitions and completion, through the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).

## How it's built

- It's its own program, `lks-lsp`, in its own crate (`crates/lks-lsp`), shipped next to `lks` and `ferry` in every release. Editors start it and talk to it on its stdin and stdout, like any language server.
- It uses the `lakescript` library directly: the same parser, checker and module loader as `lks`, so the editor and `lks` never disagree.
- It's built on `lsp-server` and `lsp-types`, the crates of rust-analyzer. They're synchronous, which fits the library: the parser and the checker share data with `Rc`, which can't move between threads.
- The modules of a file are found like when it runs:
    - In a ferry project, with the module path `ferry run` uses: `src/` and the dependencies from the lock file *(see [ferry.md](ferry.md))*.
    - For a standalone script, the folder it's in.
- The files open in the editor are read from the editor, not from the disk, so errors follow what's typed, before it's saved.

## What the library needs

Most of the work is in the library, and `lks` gets it too:

1. **Files that aren't saved yet:** the module loader can be given the text of a file instead of reading it.
2. **What the checker found:** while it checks, it records each name it sees: where it is, its type, where it's declared and its `///` comment. That's what hover, go to definition and completion read.
3. **Code that's half written:** the parser goes on after an error *(i.e. `$x.` while a method is being written)*, and the checker after a type error, so a file shows all its errors at once.

## Milestones

Each one is usable on its own, with tests that send real protocol messages to the server:

| # | What the editor gets | What it needs |
|---|---|---|
| 1 | Errors while typing, the first one of each file, in the right file of a project | The protocol, finding the project, files that aren't saved |
| 2 | Hover with the type and the `///` comment, go to definition | The checker recording names |
| 3 | Completion: methods and fields after `.`, members after `::`, the variables in scope, keywords | Going on after a syntax error |
| 4 | Every error at once, the outline of a file, the parameters while typing a call | The checker going on after an error |
| 5 | Find references, rename | The recorded names, across modules |

- Phase 8 is `v0.8.0` once milestones 1 to 3 work. Milestones 4 and 5 can be patch versions after it.

### Status

- Milestone 1 is done. `lks-lsp` takes the whole text of a file on every change, checks every open file with it, and sends each error to the file it's in: an error in a module goes to the module, also when only a file that uses it is open. A module of a ferry project is checked as a module, with the project's module path *(`src/` and the packages of the lock file that are already downloaded: `lks-lsp` never downloads)*, and any other file as a script.
- Every file the editor has open is checked again on each change, since a change in a module can break the files that use it. It takes a few milliseconds.
- `lks-lsp --version` says its version, and it's in every release archive.
- Milestone 2 is done. Hover shows a name's declaration as Lakescript code *(i.e. `list<number> $count`, `string greet(string $name, number $times = …)`, `class Dog extends Animal`)*, a sentence on what it is *(i.e. "A method of `list`.", "A parameter of `greet`.", "From `math.lks`.")* and the `///` comment of its declaration. Go to definition jumps to where it's declared, in the file or in a module.
    - It works on variables, parameters, functions, the functions and constants of modules, classes *(in types and in calls)*, fields, methods, static members and enum values. The types are the ones the checker found, also narrowed *(i.e. not `null` after `$x != null`)*.
    - The built-in functions and methods have their signature, but no comment yet: `prelude.txt` only has the signatures. Their comments are in `ideas/stdlib`, and can be added to it later.
    - The checker stops at the first error, so only the names before it are known. A file that doesn't parse keeps the names of its last good version, until milestone 3.

### How names are found

- The library has it, for any program that embeds Lakescript: `Engine::set_names(true)` makes the checks note each name, and `Engine::names()` gives them after a check, each with its file, place, code, sentence, `///` comment and definition.
- The syntax tree only has the place of whole expressions, so the checker notes the text of the name and where to look for it *(i.e. `area` after the end of `$square` in `$square.area()`)*, and the engine finds it in the text once the check is done. Types are written out then too, when the types found out later are known *(i.e. the elements of `[]`)*.
- A `///` comment is the one before the first token of the declaration, or before its `pub`. A parameter doesn't take its function's comment.

## Using it

An editor starts `lks-lsp` with no arguments for files ending in `.lks`, and talks to it on its stdin and stdout. The language id is `lakescript`. Nothing else needs to be configured: a ferry project is found from the folder of each file.

## Editors

- The first one is farsub, the editor of its own project. Others *(i.e. VS Code, Neovim, Helix)* come later: they only need to be told to start `lks-lsp` for `.lks` files.
