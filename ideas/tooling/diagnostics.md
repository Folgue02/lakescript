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

## Did you mean

*Phase 13.* An error about a name that isn't there suggests the one that was likely meant, at the end of its message *(i.e. ``` `$nmae` isn't declared. Did you mean `$name`? ```)*. For people, and for tools and agents that fix code from what `lks` says.

- **The closest name** of the ones that are there, where the error is: the variables in scope, the functions, the types, the methods and fields of the value, the members of the module *(`fs::exist`)*, and the modules of a `use`. Close means a few letters added, removed, changed or swapped, case aside *(i.e. `.lenght()` → `.length()`, `$Name` → `$name`)*: only the case for a name of one letter, at most one for up to 4 letters, two up to 8, three for longer ones. The closest one wins, and a tie goes to the first in alphabetical order. None when nothing is close: a wrong guess is worse than none.
- **The names of other languages**, a short table of the ones most often written instead of Lakescript's *(i.e. `.toUpperCase()`, `.toUpper()`, `.upcase()` → `.upper()`; `.size()`, `.len()`, `.count()` → `.length()`; `.strip()` → `.trim()`; `.includes()`, `.include?()` → `.contains()`; `.append()` → `.push()`; `print`, `println`, `puts`, `echo` → `outln`; `len($x)` → `$x.length()`; `fs::exists` → `fs::exists?`)*. One is only suggested when it's really there for that value. After the naming refactor, the old names go in the same table.
- The table comes first: `print` means `outln`, even if a closer name exists.
- In JSON it's the key `suggestion`, the name alone *(i.e. `"$name"`)*, or `null` *(see Errors as JSON)*. `Diagnostic.suggestion` has it for a program that embeds Lakescript.

## Warnings

*Phase 13.* Things that aren't wrong but are likely mistakes, or left over. They never stop a script.

- **A variable that's never used**: `let` and `const`, and the variable of a `for`. Assigning to it counts as using it, so only a name that's never mentioned again warns. The variable of a `catch` never warns. *(Chosen while building it.)*
- **A parameter that's never used**, in named functions and methods. Not in methods that implement an interface or override a parent's, which must keep their parameters, nor in anonymous functions, whose parameters the caller decides *(i.e. `sort(($a, $b) -> ...)`)*.
- **A `use` that's never used**: a module whose `name::` never appears, or an imported name that's never written.
- **Code that never runs**: a statement after one that always leaves *(`return`, `throw`, `break`, `continue`, `yield`, `exit()` or a `never` function)*, once for the first one of the block. Functions and classes declared after it don't count, since they're declared before the block runs.
- **A result that's thrown away**, a statement whose value is never used, when making it does nothing else *(added in phase 13, after the first warnings)*:
    - A built-in method that doesn't change its value *(i.e. `$name.trim();`, `$list.sorted();`)*. Methods that change it *(`push`, `sort`, `set`...)* and the methods of commands never warn. When there's a method that does change it, the warning suggests it: `.sort()` for `.sorted()`, `.reverse()` for `.reversed()`, `.extend()` for `.concat()`, `.merge()` for `.merged()`, and `.each()` for `.map()`. The methods of classes and functions never warn, since they can do other things.
    - A command that's never run *(i.e. `%x"git pull";`, which only creates it)*.
    - A value or an operator *(i.e. `$a + 1;`, `$x == 2;`)*, but not `&&`, `||` and `??`, whose right side can be what's wanted, nor `<<`, which adds to a list.
- **A comparison with `null` that's always the same**: `$x == null` or `!= null` when `$x` can't be `null` there, from its type or a check before it.
- **A name that starts with `_` never warns** *(`$_unused`, `$_`, `($_, $b) -> $b`)*, like in Rust and Python: it says it's unused on purpose.
- **Where:** the editor, `lks --check` and `ferry check`, which show them before the errors, in yellow, with `warning:`, like the comments that don't match their code. As JSON, `"type":"warning"`. Not when a script runs with `lks`, `ferry run` or `ferry test`, where they'd be noise on every run. They never change the exit code.
- A program that embeds Lakescript gets them with `Engine::warnings()` after a check.

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
| `suggestion` | The name a "Did you mean" suggests *(i.e. `"$name"`)*, or `null`. Added in phase 13 |

```
{"type":"error","message":"`$age` is a number, but its value is a string","exception":null,"file":"script.lks","line":2,"column":21,"length":5,"trace":[],"suggestion":null}
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

