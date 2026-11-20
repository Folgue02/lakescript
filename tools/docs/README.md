# Documentation tools

These are Lakescript scripts, run with `lks`.

`stdlib.lks` builds a single HTML page, in the style of javadoc, with the standard library described in [ideas/stdlib](../../ideas/stdlib): the philosophy, the methods of every built-in type and the functions of every module.

```
lks tools/docs/stdlib.lks
```

It writes `target/docs/stdlib.html`, or the path given as an argument. Each release ships that page, inside every archive as `doc/stdlib.html` and on its own *(see `tools/release/package.sh`)*.

## What it reads

- `ideas/stdlib/README.md` becomes the overview page.
- Each file in `ideas/stdlib/types/` becomes the page of a type, and each module file (`fs.md`, `path.md`...) the page of a module. The lists `$types` and `$modules` at the top of the script say which ones, in order.
- Tables whose first column is `Method`, `Function` or `Constant` become the members, with their signature, what they return and a description. Each `##` heading is a group. Everything else (text, examples, other tables) is shown as it is.

Add a new type or module to `$types` or `$modules` when its notes are written.

## Signatures for the type checker

`prelude.lks` reads the same tables and writes `crates/lakescript/src/checker/prelude.txt`, the signatures the type checker knows for every built-in method and standard library function:

```
lks tools/docs/prelude.lks
```

Run it after changing a signature in `ideas/stdlib`, and build `lks` again, so the checker and the docs agree. It needs an `lks` that's already built; `prelude.txt` is committed, so there's always one. If the script itself uses a method that the `lks` running it doesn't know yet, run it once with `lks --no-type-check`.
