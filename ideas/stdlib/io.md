# std/io

Reading what's sent to the script through stdin *(i.e. `cat notes.txt | notes.lks`)*. `use std/io;`, then `io::lines()`. Writing to stdout and stderr is done with `out`, `outln`, `err` and `errln`, which need no `use` *(see [general.md](general.md))*.

| Function | Returns | Description |
|----------|---------|-------------|
| `readLine()` | `?string` | The next line, without its new line, or `null` when there's nothing more to read *(i.e. `while (($line = io::readLine()) != null)`)* |
| `readAll()` | `string` | Everything that's left |
| `lines()` | `list<string>` | Every line that's left |
| `piped?()` | `bool` | `true` if stdin comes from a pipe or a file, `false` if it's the terminal. Useful to know if there's anything to read |
| `flush()` | `void` | Writes out anything `out` and `err` haven't written yet. They're flushed at the end of the script, before reading stdin and before running a command anyway *(see [general.md](general.md))* |

- Asking the person running the script for input *(i.e. `ask`, `confirm`, passwords)* belongs to `std/terminal`, later.
