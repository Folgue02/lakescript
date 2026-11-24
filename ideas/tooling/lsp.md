# The language server

Phase 8 *(see [roadmap.md](../roadmap.md))*. `lks-lsp` gives editors what the checker knows while code is written: errors, types, definitions and completion, through the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).

## How it's built

- It's its own program, `lks-lsp`, in its own crate (`crates/lks-lsp`), shipped next to `lks` and `ferry` in every release. Editors start it and talk to it on its stdin and stdout, like any language server.
- It uses the `lakescript` library directly: the same parser, checker and module loader as `lks`, so the editor and `lks` never disagree.
- It's built on `lsp-server` and `lsp-types`, the crates of rust-analyzer. They're synchronous, which fits the library: the parser and the checker share data with `Rc`, which can't move between threads.
- The modules of a file are found like when it runs:
    - In a ferry project, with the module path `ferry run` uses: `src/` and the dependencies from the lock file *(see [ferry.md](ferry.md))*.
    - For a standalone script, the folder it's in, and never the folders above it *(see [modules.md](../language/modules.md#module-path))*. A module of a subfolder opened alone only knows its own folder.
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
- A project that can't be used says why, at the start of the file: a `ferry.toml` that can't be read gives its error *(the file isn't checked as if there were no project)*, and a module of a dependency that isn't downloaded says that `ferry check` or `ferry run` downloads it.
- Every file the editor has open is checked again on each change, since a change in a module can break the files that use it. It takes a few milliseconds. Changes that arrive together *(i.e. the keys typed while a check ran)* are one check: it runs when no other message is waiting, or before a question is answered.
- `lks-lsp --version` says its version, and it's in every release archive.
- On Windows, paths go without the verbatim prefix that `canonicalize` gives there *(`\\?\C:\…`)*, through `lakescript::canonical`, also in errors and in the module loader. With it, errors went to `file:////%3F/C:/…`, a URI editors can't read, so they never showed. Shared folders are `file://server/share/…`. The conversions are tested as text, since the tests don't run on Windows.
- Milestone 2 is done. Hover shows a name's declaration as Lakescript code *(i.e. `list<number> $count`, `string greet(string $name, number $times = …)`, `class Dog extends Animal`)*, a sentence on what it is *(i.e. "A method of `list`.", "A parameter of `greet`.", "From `math.lks`.")* and the `///` comment of its declaration. Go to definition jumps to where it's declared, in the file or in a module.
    - It works on variables *(also where they're assigned)*, parameters, functions, modules *(`fs` in `fs::read`, and in `use` lines with what they import)*, the functions and constants of modules, classes *(in types and in calls)*, `Self`, type parameters, the built-in exceptions, fields, methods, static members and enum values. The types are the ones the checker found, also narrowed *(i.e. not `null` after `$x != null`)*.
    - The built-in functions and methods have their signature, but no comment yet: `prelude.txt` only has the signatures. Their comments are in `ideas/stdlib`, and can be added to it later.
    - The modules of the standard library written in Lakescript *(i.e. `std/http`)* have their `///` comments on hover, like any module. Go to definition doesn't jump into them, since they're inside the interpreter and not in a file.
    - The checker stops at the first error, so only the names before it are known. A file that can't be read at all *(i.e. a string that isn't closed)* keeps the names of its last good version.
- Milestone 3 is done. Completion gives what fits what's before the cursor:
    - after `.`: the fields and methods of the value before it, from its type *(i.e. `push` and `map` after `$words.`)*, and what every value has *(`toString`...)*. Private members only inside their class;
    - after `$`: the variables in scope there, with the `$` replaced by the whole name;
    - after `@`, in a class: its fields and methods, also the private ones;
    - after `name::`: the functions, constants and types of a module, or the static methods, static fields and values of a class *(`Self::` too)*;
    - after anything else: the variables, functions, classes and modules in scope, the reserved words and the built-in types.
    - Each one has its declaration as its detail *(i.e. `list<string> push(...string $values)`)*. It inserts the name only: the parentheses and arguments are written by hand.
    - The editor asks for it by itself after `.`, `$`, `@`, `:` and `/`. After a `/` that isn't in a `use` path *(i.e. a division)* there's nothing to complete.
- Part of milestone 4 is done: the parameters while a call is typed *(signature help)*, which the editor asks for by itself after `(` and `,`. It shows the function's declaration with the parameter being written marked, and the `$name:` lines of its comment *(see Doc comments in [functions.md](../language/functions.md))*. The function is the name that ends right before the `(` of the call the cursor is in, found in the text up to the cursor, without strings and comments.
- `use` paths are completed: `std` and the modules of the standard library, and the folders and modules of the module path *(the project's and its dependencies', or the folder of a script)*, with the first line of a folder's `module.md` and of a module's `//!` next to each. Hover on a folder of a path shows the first paragraph of its `module.md` *(for a folder of the standard library, like `std` or `std/format`, the modules in it)*, and on a module of the project its `//!` text.
- Parameters show their `$name:` line on hover, also on a named argument of a call, and a `$name:` for a parameter that isn't there is a warning. These are the only warnings: everything else the checker finds is an error.

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
- At the 213 places of the examples where a `.` is typed before a method or a field, with the rest of the file below, 192 complete the members. The rest are inside `%w[...]`, after `@super.`, in an `if (` or `switch (` used as a value that isn't closed yet, or in `shop/store`, whose modules are written for the module path of `shop/main.lks`: opened alone, they don't find each other, on purpose.


### How names are found

- The library has it, for any program that embeds Lakescript: `Engine::set_names(true)` makes the checks note each name, and `Engine::names()` gives them after a check, each with its file, place, code, sentence, `///` comment and definition.
- The syntax tree only has the place of whole expressions, so the checker notes the text of the name and where to look for it *(i.e. `area` after the end of `$square` in `$square.area()`)*, and the engine finds it in the text once the check is done. Types are written out then too, when the types found out later are known *(i.e. the elements of `[]`)*.
- A `///` comment is the one before the first token of the declaration, or before its `pub`. A parameter doesn't take its function's comment.

## Importing types from completion

Built. Like Java's in IntelliJ: completion also gives the types that aren't imported, and choosing one writes its `use` line.

- **Only types**: classes, interfaces and enums. Functions and constants aren't offered, since there would be too many of them.
- They come from the project's `src/`, the dependencies already downloaded and the standard library *(i.e. `Date`, `Zone`, `Style`)*. A script outside a project sees the standard library and the files next to it. Each one says its module, so two `Point`s can be told apart, and they come after the names already in scope.
- **The `use` line**:
    - A type module is imported as itself *(i.e. `use people/Person;`)*.
    - A type inside a module is imported by its name, without braces when it's the only one from that module *(i.e. `use folgue02/geo/shapes::Circle;`)*. Braces only when there are several: importing `Square` too turns it into `use folgue02/geo/shapes::{Circle, Square};`.
    - **Their order**: the standard library first, an empty line, then the rest, each group in alphabetical order. Two groups, like Go's `goimports`, not three like Python's *(the standard library, dependencies, the project)*: a project's modules and its dependencies are written alike *(`folgue02/...`, `acme/...`)*, and only `ferry.toml` tells them apart. A new line goes in its place in that order. Lines that are already there aren't moved: that's for a formatter, which will follow the same rule.
- The edit goes with the completion, as `additionalTextEdits`, which farsub already applies.
- No quick fix that imports an unknown type from its error.
- They're offered for a name that starts with a capital letter, like types do, so a variable or a function being written doesn't get a list of every type.
- It's an index of the `pub` types of every module that can be reached, made by parsing them only *(everything `pub` has its type written)*, kept by the time each file changed, so a saved file is read again. Open files use the editor's text. It's in `lks-lsp/src/imports.rs`, and the standard library's come from `lakescript::std_source()`.

## Using it

An editor starts `lks-lsp` with no arguments for files ending in `.lks`, and talks to it on its stdin and stdout. The language id is `lakescript`. Nothing else needs to be configured: a ferry project is found from the folder of each file.

- Positions are in UTF-16 units, the protocol's default.
- It answers `textDocument/hover`, `textDocument/definition` and `textDocument/completion` (triggered by `.`, `$`, `@` and `:`), and sends `textDocument/publishDiagnostics` after `didOpen`, `didChange`, `didSave` and `didClose`. The text is synced whole on every change. Other requests get a `MethodNotFound` error.
- Editors find it on the `PATH` and don't install it: it comes in the release archives, and `cargo install --path crates/lks-lsp` builds it from the source.

## Editors

- The first one is farsub, the editor of its own project. It uses `lks-lsp` for errors, hover, go to definition and completion, through the LSP client it already had: `lks-lsp` is found on the `PATH` and never installed, with `ferry.toml` as the root of a project. Without `lks-lsp`, a `.lks` file says once that it wasn't found, and works as before.
- Others *(i.e. VS Code, Neovim, Helix)* come later: they only need to be told to start `lks-lsp` for `.lks` files.
