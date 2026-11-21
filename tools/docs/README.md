# Documentation tools

`stdlib.py` builds a single HTML page, in the style of javadoc, with the standard library described in [ideas/stdlib](../../ideas/stdlib): the philosophy, the methods of every built-in type and the functions of every module.

```
python3 tools/docs/stdlib.py
```

It writes `crates/ferry/src/stdlib.html`, or the path given as an argument. That copy is committed, and ferry has it inside, so `ferry doc std` works however ferry was installed *(i.e. with `cargo install`)*. Run the script after changing the notes of the standard library: a test of ferry says when the copy is out of date. It only needs Python 3, with no packages. Each release ships that page, inside every archive as `doc/stdlib.html` and on its own *(see `tools/release/package.sh`)*.

## What it reads

- `ideas/stdlib/README.md` becomes the overview page.
- Each file in `ideas/stdlib/types/` becomes the page of a type, and each module file (`fs.md`, `path.md`...) the page of a module. The lists `TYPES` and `MODULES` at the top of the script say which ones, in order.
- Tables whose first column is `Method`, `Function` or `Constant` become the members, with their signature, what they return and a description. Each `##` heading is a group. Everything else (text, examples, other tables) is shown as it is.

Add a new type or module to `TYPES` or `MODULES` when its notes are written.

## Signatures for the type checker

`prelude.py` reads the same tables and writes `crates/lakescript/src/checker/prelude.txt`, the signatures the type checker knows for every built-in method and standard library function:

```
python3 tools/docs/prelude.py
```

Run it after changing a signature in `ideas/stdlib`, so the checker and the docs agree.

