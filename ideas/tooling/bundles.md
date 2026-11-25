# Shipping a program in one file

*Built in phase 12.* A **bundle**: a project shipped as a single file with every module it needs, its dependencies' too, run by `lks`, like Java's fat jars or Python's zipapps *(`.pyz`)*.

## Decided

- **The names**: a bundle, in a file ending in `.lkb` *(i.e. `todo.lkb`)*, made by `ferry bundle`. Not "package", which in ferry is already a dependency from a git repository *(see [ferry.md](ferry.md))*. The files that aren't code go in a folder `resources/` of the project, read with `std/resources` *(i.e. `resources::read('templates/page.html')`)*, like Java's resources.
- **A zip of source.** The project's modules and its dependencies', and a small manifest: the command it runs and the oldest `lks` that can run it. Not bytecode, which changes between versions of `lks`: source keeps working while the language doesn't change, and the manifest gives a clear error on an `lks` that's too old.
- **Run by `lks`**: `lks todo.lkb [arguments...]`. The modules are read from the zip instead of from folders, like `lks-lsp` gives the checker files that aren't saved. Its first line can be `#!/usr/bin/env lks`, which a zip allows since it's read from its end, so with `chmod +x` it runs as `./todo.lkb`.
- **It costs almost nothing**: reading a zip of a few hundred KB takes about a millisecond, and checking and compiling are the same as for a project. It's small, since the standard library is inside `lks`, and Lakescript packages have no native code, so the same file runs on Linux, macOS and Windows.
- **`process::scriptPath()`** gives the path of the file.
- **Files that aren't code, like Java's resources**: `resources/` holds them *(i.e. templates, default configs, images)*. While developing, they're read from that folder; when shipped, from inside the file. A module of the standard library reads them the same way in both cases, so the code doesn't change.

- **One `.lkb` for each command of `bin/`**, like `ferry install` writes one launcher for each: `ferry bundle` makes `todo.lkb` and `backup.lkb`, and `ferry bundle todo` only that one. Each file is what someone runs, without a first argument to choose a command, unlike busybox. Decided after the speed review.
- **`std/resources`** reads the files of `resources/`, and only reads them, since a bundle can't change: `resources::read($name)` *(text)*, `readBytes($name)`, `exists?($name)` and `list($folder = '')`, named like the functions of `std/fs`. The names are paths inside `resources/`, with `/` *(i.e. `'templates/page.html'`)*.
- **Where `resources/` is**: in a project, the project's own folder; in a bundle, inside the file; and for a script run with `lks` outside a project, next to the script *(the real file's folder, through a link, like `use` finds modules)*. `lks --resources <folder>` gives another folder for a script.

## How it's made

- **Inside a `.lkb`**, a zip after its first line `#!/usr/bin/env lks`:
    - `bundle.toml`: the command, the project and its version, and the oldest `lks` that can run it, the one of the `ferry` that made it. An older `lks` stops with an error that says which version it needs *(i.e. `todo.lkb needs lks 0.12.0 or newer, and this is lks 0.11.2`)*.
    - `bin/`: every script of `bin/`, so a command can `use` the files next to it, like when it's run with `ferry run`.
    - `src/`: every module of the project and of its dependencies, in one tree. Each package's modules are under its own `author/name/`, so they never clash.
    - `resources/`: the project's resources. A dependency's aren't in it *(see [resources.md](../stdlib/resources.md))*.
- **`ferry bundle [command]`** type checks the project like `ferry check`, makes nothing if there are errors, and writes the bundles in `dist/`, which `ferry new` puts in the `.gitignore`. A bundle is the same every time for the same project, since the files go in in order.
- **`lks todo.lkb`** reads the whole file into memory, and its modules are read from there at paths inside it *(i.e. `todo.lkb/src/folgue02/todo/list.lks`)*, which errors show, so `use` finds them the same way as on disk. The module path is the bundle's `bin/`, then its `src/`, then the usual ones.
    - It runs without checking the types again, like the commands of `ferry install`: `ferry bundle` did it, and types never change what the code does. A newer `lks` with a stricter checker still runs a bundle made before.
    - `lks --test` on a bundle is an error: a project's tests run with `ferry test`.
- The bundle code is in `crates/lakescript/src/bundle.rs`, behind the feature `bundle` of the crate *(the `zip` crate)*, on by default. `lakescript::write_bundle` writes one and `Engine::run_bundle` runs one.

## Still open

- Later: a program that doesn't need `lks` installed, `lks` itself with the file at its end, like `deno compile` and `bun build --compile`. Several MB each, and one per platform.
