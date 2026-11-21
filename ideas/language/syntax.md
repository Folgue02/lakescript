# Syntax

- C-like syntax, uses curly braces and semicolons.
- `$` to prefix variables, like PHP or Perl
- use of `let` and `const` to declare variables, don't allow redeclarations *(see [variables.md](variables.md))*.
- use of operators such as `<<` to push things into collections.
- coalesce operator (`??`), ternary operator (`... ? ... : ...`), nullable operator like in JS (`$nullableThing?.method()`)
- conditional operators like in bash/powershell:
    - `-f` to check if the path points to a file.
    - `-d` to check if it's a directory.
    - `-x` to check if the path points to an executable.
    - `-r` to check if it's readable.
    - `-w` to check if it's writable.
    - `-like` to check if a string matches a wildcard pattern, like in PowerShell *(i.e. `$file -like '*.txt'`)*. `*` matches any number of characters, `?` matches one character and `[abc]` matches one of the characters inside.
    - `-match` to check if a string matches a regex *(i.e. `$name -match %r"^\d+$"`)*. The right side can also be a string, which gets compiled as a regex *(i.e. `$name -match $userPattern`)*. See the `regex` type in [builtin_types.md](builtin_types.md).
    - `-in` to check if the element is in a collection.

- try catch with exceptions like Java's, but with the option of getting a result type like Rust's. if function `readFile($filePath)` could throw an exception, use `try readFile($filePath)` and this should return a `result<string, Exception>` *(see `result` in [builtin_types.md](builtin_types.md))*
    - See [errors.md](errors.md) for the built-in exceptions, `catch`, `finally` and `throw`.
- Support for shebangs `#!/usr/bin/lks`
- Comments: `// until the end of the line` and `/* block comments */`, which can span several lines.
    - Doc comments start with `///` and go before what they document, so editors can show them *(still to be discussed)*.
- Blocks used as values end with `};`, since they're part of a statement: maps, anonymous functions, `switch` and `if` expressions *(i.e. `const $scores = { ... };`)*. Blocks of statements like `if`, `for`, `while` or function definitions end with just `}`.
- Trailing commas are allowed in lists, maps, function parameters and arguments *(i.e. `['a', 'b',]`)*, so multi-line lists can be reordered and extended without touching the other lines.
- Scripts run from top to bottom. A function can be called by the statements after its declaration, and by other functions from anywhere *(see [functions.md](functions.md))*.

## Names and reserved words

- Names are made of ASCII letters, digits and `_`, and can't start with a digit *(i.e. `$user_name`, `$user2`, `loadConfig`)*.
- Variables start with `$`. Function and method names can end with `?` *(see [functions.md](functions.md))*.
- These words are reserved and can't be used as function names:

| Group           | Words |
|-----------------|-------|
| Declarations    | `let` `const` `class` `interface` `enum` |
| Modules         | `use` `pub` |
| Control flow    | `if` `else` `for` `in` `while` `switch` `case` `default` `break` `continue` `return` `yield` |
| Errors          | `try` `catch` `finally` `throw` `assert` |
| Values          | `true` `false` `null` |
| Built-in types  | `void` `any` `string` `number` `bool` `symbol` `list` `map` `range` `regex` `result` `command` |

- `as` and `is` are reserved method names *(see [builtin_types.md](builtin_types.md))*.
- Variables can use any of these names, since they start with `$` *(i.e. `$type`, `$default`)*. Methods can too, since they always follow a `.` *(i.e. `$result.throw()`)*.
