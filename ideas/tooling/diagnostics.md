# Error messages

Error messages should look like Rust's: the file and line number, a preview of the line, and a marker pointing to where the error is.

```
error: $age is a number, can't assign a string
 --> script.lks:12:8
   |
12 | $age = 'old';
   |        ^^^^^ this is a string
```

`lks` shows errors this way: syntax errors, type errors and exceptions that aren't caught. A program that embeds Lakescript gets the message, the file, the line and the column, and can show them however it wants *(`Diagnostic::render` for one line, `Diagnostic::render_with_source` for this format)*.

An exception that isn't caught also shows the calls that led to it, a line each above the error *(i.e. `  at report::build (/home/ana/report.lks:2:19)`, see Tracebacks in [errors.md](../language/errors.md))*. Both renders write them, and `Diagnostic::trace` has them for a program that shows errors its own way.

## Colors

On a terminal, errors have colors like Rust's: `error:` and the marker under the code in red, the message in bold, and the frame around the code *(`-->`, the line number and `|`)* in blue. The help texts of `lks` and `ferry` have their headings in green and their commands and options in cyan.

- Colors are only used when the output is a terminal. Piped into a file or another program, the text is the same without them.
- `--color auto|always|never` chooses, in both `lks` and `ferry`. `auto` is the default.
- With `auto`, the [`NO_COLOR`](https://no-color.org) environment variable turns them off, `CLICOLOR_FORCE` turns them on even when it's not a terminal, and `TERM=dumb` gets none.
- A program that embeds Lakescript decides for itself: `Diagnostic::render_with_source_painted` takes a `Paint`, and `lakescript::terminal` has the same rules for when to use colors.

