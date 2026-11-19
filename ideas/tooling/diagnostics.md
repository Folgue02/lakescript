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
