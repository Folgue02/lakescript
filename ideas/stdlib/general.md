# General

Functions that can always be used, without `use` *(see the module path in [modules.md](../language/modules.md))*.

## Output

| Function | Returns | Description |
|----------|---------|-------------|
| `out(any $value = '')` | `void` | Writes the value to stdout, as `toString()` shows it |
| `outln(any $value = '')` | `void` | Same as `out`, followed by a new line |
| `err(any $value = '')` | `void` | Writes the value to stderr |
| `errln(any $value = '')` | `void` | Same as `err`, followed by a new line |

- They take a single value. Several values are joined with an f-string *(i.e. `outln(f"{$a} {$b}")`)*.
- A program that embeds Lakescript decides where stdout and stderr go *(i.e. to a log, or to a window)*.

## Exit

| Function | Returns | Description |
|----------|---------|-------------|
| `exit(number $code = 0)` | `void` | Stops the script with an exit code |

- `finally` blocks still run on the way out, like when an exception goes through them.
- The exit codes of `lks`:

| Code | When |
|------|------|
| `0` | The script finished |
| `1` | An exception wasn't caught |
| `2` | The command-line arguments were wrong *(see [cli.md](../language/cli.md))* |
| `N` | The script called `exit(N)` |

- `exit` never ends the process of a program that embeds Lakescript. The program decides what it does:
    - Allow it: the script stops, and the program is told the exit code. This is what `lks` does.
    - Forbid it: `exit` throws an `Exception`, which the script can catch like any other.
