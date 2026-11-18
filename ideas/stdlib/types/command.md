# command

A shell command created with `%x"..."` *(see Command in [builtin_types.md](../../language/builtin_types.md))*. It only runs when `run()`, `output()`, `lines()` or `status()` is called. The setup methods return the same command, so they can be chained.

## Running

| Method | Returns | Description |
|--------|---------|-------------|
| `run()` | `void` | Runs it, with its output going straight to the terminal *(i.e. `%x"cargo build".run()`)*. Throws a `CommandFailed` if the exit code isn't 0 |
| `output()` | `string` | Runs it and returns what it wrote to stdout, without the last new line. stderr still goes to the terminal. Throws a `CommandFailed` if the exit code isn't 0 |
| `lines()` | `list<string>` | Same as `output().lines()` *(i.e. `%x"git ls-files".lines()`)* |
| `status()` | `number` | Runs it like `run()` and returns the exit code. Never throws for the exit code *(i.e. `%x"grep -q {$word} {$file}".status() == 0`)* |
| `succeeds?()` | `bool` | Runs it without showing its output, and returns `true` if the exit code is 0 *(i.e. `%x"which docker".succeeds?()`)* |

## Setting up

| Method | Returns | Description |
|--------|---------|-------------|
| `cwd(string $path)` | `command` | Runs it in another folder |
| `env(string $name, string $value)` | `command` | Sets an environment variable only for this command |
| `envs(map<string, string> $vars)` | `command` | Sets several environment variables only for this command |
| `input(string $text)` | `command` | Sends `$text` to its stdin *(i.e. `%x"sort".input($names.join("\n")).output()`)* |
| `quiet()` | `command` | Hides its stderr |
| `timeout(number $seconds)` | `command` | Stops it after `$seconds` and throws a `CommandFailed` |

## Information

| Method | Returns | Description |
|--------|---------|-------------|
| `toString()` | `string` | The command as the shell will receive it, with the values already quoted. Useful to print it before running it |
