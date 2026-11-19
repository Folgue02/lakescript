# std/process

The script's own process, and running programs. `use std/process;`, then `process::args()`.

## The script

| Function | Returns | Description |
|----------|---------|-------------|
| `args()` | `list<string>` | The command-line arguments, without the script's own path *(see [cli.md](../language/cli.md))* |
| `scriptPath()` | `?string` | The absolute path of the script being run, or `null` when there is no file *(i.e. code given by a program that embeds Lakescript)* |
| `cwd()` | `string` | The current folder |
| `chdir(string $path)` | `void` | Changes the current folder. Throws a `FileNotFound` if it doesn't exist |
| `pid()` | `number` | The ID of the process |
| `platform()` | `symbol` | `:linux`, `:macos` or `:windows` |
| `sleep(number $seconds)` | `void` | Waits *(i.e. `process::sleep(0.5)`)* |

## Running programs

Commands written with `%x"..."` run through the shell *(see Command in [builtin_types.md](../language/builtin_types.md))*. These functions are for finding programs, and for running one without a shell.

| Function | Returns | Description |
|----------|---------|-------------|
| `which(string $name)` | `?string` | The path of a program in `PATH`, or `null` *(i.e. `if (process::which('docker') == null) errln('Install Docker first');`)* |
| `command(string $program, ...string $args)` | `command` | A `command` that runs the program directly, without a shell, with each argument passed as it is. No quoting is needed, and it works the same on every system *(i.e. `process::command('git', 'commit', '-m', $message).run()`)* |

- `exit` needs no `use` *(see [general.md](general.md))*.
