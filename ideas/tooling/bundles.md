# Shipping a program in one file

*Being designed.* A **bundle**: a project shipped as a single file with every module it needs, its dependencies' too, run by `lks`, like Java's fat jars or Python's zipapps *(`.pyz`)*.

## Decided

- **The names**: a bundle, in a file ending in `.lkb` *(i.e. `todo.lkb`)*, made by `ferry bundle`. Not "package", which in ferry is already a dependency from a git repository *(see [ferry.md](ferry.md))*. The files that aren't code go in a folder `resources/` of the project, read with `std/resources` *(i.e. `resources::read('templates/page.html')`)*, like Java's resources.
- **A zip of source.** The project's modules and its dependencies', and a small manifest: the command it runs and the oldest `lks` that can run it. Not bytecode, which changes between versions of `lks`: source keeps working while the language doesn't change, and the manifest gives a clear error on an `lks` that's too old.
- **Run by `lks`**: `lks todo.lkb [arguments...]`. The modules are read from the zip instead of from folders, like `lks-lsp` gives the checker files that aren't saved. Its first line can be `#!/usr/bin/env lks`, which a zip allows since it's read from its end, so with `chmod +x` it runs as `./todo.lkb`.
- **It costs almost nothing**: reading a zip of a few hundred KB takes about a millisecond, and checking and compiling are the same as for a project. It's small, since the standard library is inside `lks`, and Lakescript packages have no native code, so the same file runs on Linux, macOS and Windows.
- **`process::scriptPath()`** gives the path of the file.
- **Files that aren't code, like Java's resources**: `resources/` holds them *(i.e. templates, default configs, images)*. While developing, they're read from that folder; when shipped, from inside the file. A module of the standard library reads them the same way in both cases, so the code doesn't change.

## Still open

- One file per command of `bin/`, or one with every command, chosen by its first argument like busybox.
- The functions of `std/resources` *(i.e. `read()`, `readBytes()`, `exists?()`, `list()`)*, and where `resources/` is for a script outside a project.
- Later: a program that doesn't need `lks` installed, `lks` itself with the file at its end, like `deno compile` and `bun build --compile`. Several MB each, and one per platform.
