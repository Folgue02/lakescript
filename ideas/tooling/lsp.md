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
    - It works on variables *(also where they're assigned)*, parameters, functions, modules *(`fs` in `fs::read`, and in `use` lines with what they import)*, the functions and constants of modules, classes *(in types and in calls)*, `Self`, type parameters, the built-in exceptions, fields, methods, static members and enum values. The types are the ones the checker found, also narrowed *(i.e. not `null` after `$x != null`)*.
    - The built-in functions and methods have their signature, but no comment yet: `prelude.txt` only has the signatures. Their comments are in `ideas/stdlib`, and can be added to it later.
    - The checker stops at the first error, so only the names before it are known. A file that can't be read at all *(i.e. a string that isn't closed)* keeps the names of its last good version.
- Milestone 3 is done. Completion gives what fits what's before the cursor:
    - after `.`: the fields and methods of the value before it, from its type *(i.e. `push` and `map` after `$words.`)*, and what every value has *(`toString`...)*. Private members only inside their class;
    - after `$`: the variables in scope there, with the `$` replaced by the whole name;
    - after `@`, in a class: its fields and methods, also the private ones;
    - after `name::`: the functions, constants and types of a module, or the static methods, static fields and values of a class *(`Self::` too)*;
    - after anything else: the variables, functions, classes and modules in scope, the reserved words and the built-in types.
    - Each one has its declaration as its detail *(i.e. `list<string> push(...string $values)`)*. It inserts the name only: the parentheses and arguments are written by hand.
    - The editor asks for it by itself after `.`, `$`, `@` and `:`.

### Code that's half written

- For an editor, the parser goes on after a syntax error *(`parser::parse_recovering`)*, so the rest of the file still has its names. `lks`, `ferry` and the compiler still stop at the first one.
    - A `;`, `)` or `]` missing at the end of a line is an error, and what it closes ends there *(i.e. `outln($task.` with the rest of the file below it)*.
    - `.` or `name::` with nothing after it, or with the name on the next line, is an error, and its name is empty *(i.e. `$words.` while a method is being written)*. The checker gives it any type, and notes the type before the `.`. Nothing goes on from it: what comes after is the next code.
    - A `(` or `[` that starts a line is the next code, not a call or an index of the line before.
    - The body of an `if` or a loop that isn't written yet *(a `}` or the end of the file instead)* is an error, and an empty block.
    - Any other error skips its statement, to its `;` or to the end of the block it opened.
    - Only the first error is sent to the editor until milestone 4.
- For completion, the file is checked again with the place of the cursor. The checker keeps the scope it was in at the statement or expression around the cursor, or after the last statement before it. A `$` alone gets a name for that check, so it can be read.
- The library has it too: `Engine::set_completion(file, offset)` before a check, and `Engine::completion()` after it.
- A value whose type isn't known gets what every value has *(`toString`, `typeName`...)* after `.`.

### Checked

- Hover, go to definition and completion were asked at every second character of every example, and every example was typed into the server one character at a time, with completion and hover after each one: no errors, and every answer in under 20 ms *(release build)*.
- At the 213 places of the examples where a `.` is typed before a method or a field, with the rest of the file below, 192 complete the members. The rest are inside `%w[...]`, after `@super.`, in an `if (` or `switch (` used as a value that isn't closed yet, or in `shop/store`, whose modules are found from `shop/` and not from their own folder.


### How names are found

- The library has it, for any program that embeds Lakescript: `Engine::set_names(true)` makes the checks note each name, and `Engine::names()` gives them after a check, each with its file, place, code, sentence, `///` comment and definition.
- The syntax tree only has the place of whole expressions, so the checker notes the text of the name and where to look for it *(i.e. `area` after the end of `$square` in `$square.area()`)*, and the engine finds it in the text once the check is done. Types are written out then too, when the types found out later are known *(i.e. the elements of `[]`)*.
- A `///` comment is the one before the first token of the declaration, or before its `pub`. A parameter doesn't take its function's comment.

## Using it

An editor starts `lks-lsp` with no arguments for files ending in `.lks`, and talks to it on its stdin and stdout. The language id is `lakescript`. Nothing else needs to be configured: a ferry project is found from the folder of each file.

## Editors

- The first one is farsub, the editor of its own project. Others *(i.e. VS Code, Neovim, Helix)* come later: they only need to be told to start `lks-lsp` for `.lks` files.
