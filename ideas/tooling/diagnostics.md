# Error messages

Error messages should look like Rust's: the file and line number, a preview of the line, and a marker pointing to where the error is.

```
error: $age is a number, can't assign a string
 --> script.lks:12:8
   |
12 | $age = 'old';
   |        ^^^^^ this is a string
```

- **Messages are plain text**, never Markdown: an example goes in parentheses *(i.e. ``the values of an enum are written in SCREAMING_CASE (i.e. `LEFT`)``)*, not in the `*(i.e. ...)*` of these notes. Names and code keep their backticks. Found when trying dependencies with two packages, in `v0.12.2`.
- **A message says what's wrong where it points** *(i.e. ``the values of an enum are reached with `::`: `E::A`, not `E.A` ``, ``` `x` returns a string, but this is a number ```)*, and the fix when there's one *(i.e. `$a.concat($b)` for `+` on two lists, `use text/case as c;` for a module named like a reserved word)*.

- **Backlog:** a name that isn't there suggests the closest one, and the names of other languages suggest Lakescript's *(i.e. `.toUpper()` → `.upper()`, `len($l)` → `$l.length()`)* *(see Backlog in [roadmap.md](../roadmap.md))*.

## Errors as JSON

*In phase 13, see [roadmap.md](../roadmap.md).* `--message-format json`, in `lks` and `ferry`, writes what they say on stderr as JSON, one object on each line, like cargo's `--message-format json`. For tools and agents, which shouldn't have to read the text, and can read each object as it comes.

- **Every error `lks` shows**: syntax and type errors, uncaught exceptions with their calls, a wrong command line, and a script that can't be read.
- **Everything `ferry` says on stderr**: errors, warnings *(i.e. a `pub` item without docs, a project or dependency for a newer Lakescript)*, notes, and the lines of what it does *(`Added`, `Installed`)*, so every line on stderr is one object. A line of what it does is a `status` whose message starts with its verb *(i.e. `"Added github:acme/http 1.4.2"`)*.
- What the script writes is its own, unchanged: `outln` on stdout, `errln` on stderr. The results of tests stay text: on stdout with `lks --test`, and with `ferry test` they move from stderr to stdout, so stderr is only objects. Giving them as JSON isn't decided.
- An object has:

| Key | Value |
|-----|-------|
| `type` | `"error"`, `"warning"`, `"note"`, or `"status"` for a line of what ferry does |
| `message` | The message, as the text shows it |
| `exception` | For an uncaught exception, its type *(i.e. `"ValueError"`)*, otherwise `null` |
| `file` | The file it's in, as the text shows it, or `null` |
| `line`, `column` | Where, from 1, or `null` when it isn't at a place in the code |
| `length` | How many characters the text marks, or `null` |
| `trace` | For an uncaught exception, the calls that led to it, the first one first: `function`, `file`, `line`, `column`, `times`. Otherwise `[]` |

```
{"type":"error","message":"`$age` is a number, but its value is a string","exception":null,"file":"script.lks","line":2,"column":21,"length":5,"trace":[]}
```

- The keys are always there, with `null` when they don't apply, so a tool reads every object the same way. Keys can be added later *(i.e. a `suggestion` with "did you mean")*, never removed.
- A wrong value for it is a wrong command line *(i.e. `--message-format xml`)*, code `2`.
- A program that embeds Lakescript gets the same with `Diagnostic::render_json`, and `json_message` for a message without a place. `Diagnostic.exception` has the type of an uncaught exception.

`lks` shows errors this way: syntax errors, type errors and exceptions that aren't caught. A program that embeds Lakescript gets the message, the file, the line and the column, and can show them however it wants *(`Diagnostic::render` for one line, `Diagnostic::render_with_source` for this format)*.

An exception that isn't caught also shows the calls that led to it, a line each above the error *(i.e. `  at report::build (/home/ana/report.lks:2:19)`, see Tracebacks in [errors.md](../language/errors.md))*. Both renders write them, and `Diagnostic::trace` has them for a program that shows errors its own way.

## Colors

On a terminal, errors have colors like Rust's: `error:` and the marker under the code in red, the message in bold, and the frame around the code *(`-->`, the line number and `|`)* in blue. The help texts of `lks` and `ferry` have their headings in green and their commands and options in cyan.

- Colors are only used when the output is a terminal. Piped into a file or another program, the text is the same without them.
- `--color auto|always|never` chooses, in both `lks` and `ferry`. `auto` is the default.
- With `auto`, the [`NO_COLOR`](https://no-color.org) environment variable turns them off, `CLICOLOR_FORCE` turns them on even when it's not a terminal, and `TERM=dumb` gets none.
- A program that embeds Lakescript decides for itself: `Diagnostic::render_with_source_painted` takes a `Paint`, and `lakescript::terminal` has the same rules for when to use colors.

