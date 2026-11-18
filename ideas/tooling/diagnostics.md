# Error messages

Error messages should look like Rust's: the file and line number, a preview of the line, and a marker pointing to where the error is.

```
error: $age is a number, can't assign a string
 --> script.lks:12:8
   |
12 | $age = 'old';
   |        ^^^^^ this is a string
```

The first version can be simpler, like `script.lks:12:8: $age is a number, can't assign a string`, as long as it shows the file, line and column.
