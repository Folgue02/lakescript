# Documentation pages

*Built in `v0.9.0` (phase 9, see [roadmap.md](../roadmap.md)). What's missing is at the end.*

How the pages that `ferry doc` makes from a project, and the page of the standard library, look. They're made from the `//!` and `///` comments of the modules and from the `module.md` of each folder *(see Doc comments in [functions.md](../language/functions.md) and Documenting folders in [modules.md](../language/modules.md))*.

The designs are on a canvas: https://claude.ai/artifact/RsYa7DqQ6GkRKH2YQUTaLb

## Decided

- **The layout is "Shore"**: a bar on top with the project, its version and a search; the modules on the left, with the current one open to its members; the page on the right.
    - A module page: its path, its `use` line to copy, its `//!` text, then its constants and functions, each as a card with its signature, a link to its line in the source, a grid of Parameters, Returns and Throws from the `$name:`, `returns:` and `throws` lines, and its source to unfold.
    - A class page: its fields as a table, then its methods as the same cards.
    - A project's front page: its version, how to install it, its `module.md`, its modules with the first line of their `//!`, and its commands.
- **Soft colors, like a lake**: a pale ground and one quiet accent, never strong colors. In signatures, types, parameters and keywords each have their own soft color.
- **A dark theme that follows the system** *(`prefers-color-scheme`)*, with the same colors at night.
- **The palette is "Mist"**: blue-grey, like a lake in the morning fog. Chosen over Shore *(blue-green)*, Deep *(darker teal)*, Reeds *(sage green)* and Dusk *(lavender-grey)*. Its text is at least 4.5:1 on its ground in both themes.

| Color | Light | Dark | For |
|-------|-------|------|-----|
| `bg` | `#F5F8FA` | `#10161B` | The ground of the page |
| `surface` | `#FFFFFF` | `#151D23` | Cards, the top bar |
| `panel` | `#EFF3F6` | `#121A20` | The sidebar, the heads of cards |
| `code` | `#EDF2F5` | `#1A242C` | Code blocks |
| `line` | `#DDE5EA` | `#26323B` | Borders |
| `ink` | `#1B2A35` | `#E4EBF0` | Headings and names |
| `text` | `#2B3B47` | `#C6D2DA` | Text |
| `muted` | `#5A6976` | `#8FA0AC` | Labels, positions in the source |
| `accent` | `#3F6E8C` | `#8DB8D4` | Links, the current module, types in signatures |
| `accent-soft` | `#E4EEF4` | `#1C2A35` | The ground of the current module and of badges |
| `param` | `#8A5A3C` | `#D9AE8A` | Parameters in signatures |
| `kw` | `#5D5A8E` | `#B3AEDF` | Keywords in signatures, the badge of classes |

- **Every kind has a badge of its own color**: modules and functions the accent, classes and methods purple, interfaces and commands blue-green, enums amber.

### Interfaces

- Like a class's page, with a box of **Extended by** *(the interfaces that extend it)* and **Implemented by** *(the classes of the project that implement it, also through another interface)*. A class's page says which interfaces it implements.
- Its methods in two groups: **To implement**, the ones without a default, and **Given**, the defaults, each with its code and the classes that write it again.
- An interface that gives an operator *(i.e. `IAdd`)* says which one *(i.e. "Gives `+`")*.

### Enums

- Its **values as a table**: a row for each value, with its `///` comment, and a column for each field *(i.e. `MERCURY · 0.39 · 0.06`)*. A filter above it, for enums with many values *(i.e. `terminal::Style`)*.
- Then its fields and its methods, like a class.
- What every enum has *(`values()`, `from()`, `fromIgnoreCase()`, `.name()`)* is one line that links to the language's docs, not cards on every page.

### Search

- The pages are files without a server, so the search is an index built into them.
- It finds names, signatures and the first line of each comment, fuzzily *(i.e. `htget` finds `http::get`)*, in the project, its dependencies and the standard library. The project's results come first, grouped by where they're from.
- A result has the badge of its kind, its full path *(i.e. `http::Response.header`)* with the match marked, and the first line of its comment.
- `/` searches from anywhere, the arrows move and Enter opens.

### Commands

- **A script of `bin/` that declares a `command` gets a page of its own**: its description, its usage, its options, and each subcommand with its arguments, flags, choices and defaults. The same text as its `--help` *(see [cli.md](../language/cli.md))*.
- It comes from the `command` declaration, never from running the script: `ferry doc` compiles it, and the description of the arguments the compiler makes is the one `--help` prints, so they can't disagree.
    - The `///` of the command and of its fields are the descriptions. A field with `flag(...)` is an option, one without it an argument, a `subcommand` a subcommand. An enum type gives the choices, `= ...` the default, and `...` or `count` that it can be given several times.
    - The values of an enum come from its declaration, which the checker knows, also in another module. Today the compiler only gets them when the script runs, so that changes for the docs.
- The command is the one whose `::parse()` the script calls, declared in it or in a module it uses.
- A script without a `command` only gets its `//!` text and `ferry run <name> [arguments...]`, since it has nothing to read its arguments from. A script whose command can't be told *(no `::parse()`, or two commands parsed)* gets a warning from `ferry check` and no page of a command.

## ferry doc

**`ferry doc` makes these pages for a project, and opens them to read**, in ferry like the rest of what a project needs *(see [ferry.md](ferry.md))*.

```
ferry doc                                  # the docs of this project
ferry doc --open                           # and opens them in the browser
ferry doc github:acme/http@1.4.2 --open    # the docs of a package
ferry doc std --open                       # the standard library
ferry doc units::bytes                     # one thing, in the terminal
```

- **The pages of a project** go to `.ferry/doc/` in its folder, which git doesn't keep, like cargo's `target/doc`. `--open` opens them in the browser when they're made.
    - They have the `pub` items of its modules, the `module.md` of each folder, the `//!` of each module and the commands of `bin/`.
    - The project is type checked first, like `ferry check`: a project with errors gets no docs, since their signatures could be wrong.
- **The docs of a package** are made from its copy in `~/.ferry/repository/`, once per version, in `~/.ferry/doc/`. A project's pages link to the pages of its dependencies made this way, so everything works without the internet.
- **The standard library** is the page `tools/docs/stdlib.py` makes from `ideas/stdlib`, which ferry has inside *(a copy committed in `crates/ferry/src/stdlib.html`)*, so it works however ferry was installed, also with `cargo install`. `ferry doc std` writes it once per version to `~/.ferry/doc/std/<version>/` and `--open` opens it. Releases also ship it as `doc/stdlib.html`.
- **In the terminal**, like `go doc` or Ruby's `ri`: `ferry doc` with the name of a function, a class, a method or a module *(i.e. `units::bytes`, `units`, `http::get`)* writes its signature, where it is, its comment, and its parameters, what it returns and what it throws. From the same data as the pages:

```
number bytes(string $text)                                         units.lks:17

Reads a size written as text, in any case, with or without a unit
(i.e. 1.5 KB, 2 mb, 100).

  $text    a number, then one of $units. Without a unit, it's bytes
  returns  the size in bytes, rounded
  throws   ValueError when the unit isn't one of $units
```

## How it's made

- **Reading** is in the library, `lakescript::docs`: `module_doc` parses a module *(never runs it)* and gives its `//!` text and its `pub` items, with the `///` comment of each one split by `parse_doc` into its text and its `$name:`, `returns:` and `throws` lines. The comments are found by place in the tokens, like hover does. `command_doc` gives the command a script parses, from the same `command_info` of the compiler that `--help` uses, with the choices of an enum read from its declaration.
- **The pages** are made by `ferry/src/doc.rs`: one for the project *(`index.html`)*, one for each module *(i.e. `api.users.html`)*, each class, interface and enum *(i.e. `api.users.User.html`)*, each folder with a `module.md` *(`folder.api.html`)* and each command *(`command.todo.html`)*, plus `style.css` and `search.js`, with the index of search inside.
- Markdown is read with `pulldown-cmark` (MIT). HTML written in a comment is shown as text, so the docs of a package can't add scripts to the pages.
- A method of an `implement` block is shown on its class's page under "From `IShape`", with the comment of the interface's method when it has none of its own.
- `ferry new` writes a `.gitignore` with `.ferry/`.

## Not built yet

- Search only finds the project's own items, not the ones of its dependencies and the standard library.
- The types of a dependency or of the standard library in a signature aren't links yet: only the project's own types are.
- `ferry doc <name>` only finds the project's own items, not the standard library's *(i.e. `http::get`)*.
- The filter above the values of a long enum, and "Gives `+`" for the interfaces of operators.

## Being discussed

- Searching by type *(i.e. the functions that take a `string` and give a `number`, like Haskell's Hoogle)*, later.
- Examples in doc comments.
