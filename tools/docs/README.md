# Documentation tools

`stdlib.py` builds a single HTML page, in the style of javadoc, with the standard library described in [ideas/stdlib](../../ideas/stdlib): the philosophy, the methods of every built-in type and the functions of every module.

```
python3 tools/docs/stdlib.py
```

It writes `target/docs/stdlib.html`, or the path given as an argument. It only needs Python 3, with no packages.

## What it reads

- `ideas/stdlib/README.md` becomes the overview page.
- Each file in `ideas/stdlib/types/` becomes the page of a type, and each module file (`fs.md`, `path.md`...) the page of a module. The lists `TYPES` and `MODULES` at the top of the script say which ones, in order.
- Tables whose first column is `Method`, `Function` or `Constant` become the members, with their signature, what they return and a description. Each `##` heading is a group. Everything else (text, examples, other tables) is shown as it is.

Add a new type or module to `TYPES` or `MODULES` when its notes are written.
