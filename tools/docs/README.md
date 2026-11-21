# Documentation tools

`stdlib.py` reads the standard library described in [ideas/stdlib](../../ideas/stdlib) into the data `ferry doc std` makes its pages from: the philosophy, the methods of every built-in type, the functions of every module and its classes. The pages have the same design as the ones of a project *(see [docs.md](../../ideas/tooling/docs.md))*.

```
python3 tools/docs/stdlib.py
```

It writes `crates/ferry/src/stdlib.toml`, or the path given as an argument. That file is committed, and ferry has it inside, so `ferry doc std` works however ferry was installed *(i.e. with `cargo install`)*. Run the script after changing the notes of the standard library: a test of ferry says when the file is out of date. It only needs Python 3, with no packages. Each release ships the pages, inside every archive in `doc/std/` and on their own *(see `tools/release/package.sh`)*.

## What it reads

- `ideas/stdlib/README.md`, up to "How modules are made", becomes the front page.
- `general.md` becomes the page of the built-in functions, each file in `ideas/stdlib/types/` the page of a type, and each module file (`fs.md`, `path.md`...) the page of a module. The lists `TYPES` and `MODULES` at the top of the script say which ones, in order.
- Tables whose first column is `Function` or `Constant` become the functions and constants of a module, grouped by their `##` heading. A table before the first heading is read too, without a group. A section whose tables are `Field` or `Method` becomes a class of the module *(i.e. `Response` in `http.md`)*.
- Other sections become text after the items, except the ones for the people who make Lakescript *(i.e. "Backlog", "How it's made")*: details of how a module is implemented go there, not in its intro. Links to notes of the standard library become links to their pages, and links to other notes point to them on GitHub.

Add a new type or module to `TYPES` or `MODULES` when its notes are written.

## Signatures for the type checker

`prelude.py` reads the same tables and writes `crates/lakescript/src/checker/prelude.txt`, the signatures the type checker knows for every built-in method and standard library function:

```
python3 tools/docs/prelude.py
```

Run it after changing a signature in `ideas/stdlib`, so the checker and the docs agree.

