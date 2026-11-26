# lks

The interpreter. It runs one script:

```
lks [options] <script> [arguments for the script...]
```

- The options of `lks` go before the script. The first argument that isn't an option is the script, and every argument after it is passed to the script as it is, even if it looks like an option of `lks` *(i.e. in `lks deploy.lks --help`, `--help` belongs to `deploy.lks`)*.
- `--` ends the options: the next argument is the script, even if it starts with `-`.
- **A script ending in `.lkb` is a bundle** made by `ferry bundle`, run with the modules and the resources inside it *(see [bundles.md](bundles.md))*.
- The exit code is the script's *(see [general.md](../stdlib/general.md))*. A wrong command line for `lks` itself exits with code `2`.

## Options

| Option | Meaning |
|--------|---------|
| `--module-path <folders>` | More folders where `use` looks for modules, separated by `:`, after the script's folder and before `LKS_PATH`. It's what the commands installed by [ferry](ferry.md) use |
| `--color <when>` | When errors and the help have colors: `auto` (on a terminal, the default), `always` or `never` *(see [diagnostics.md](diagnostics.md#colors))* |
| `--test` | Runs the tests of the script, its functions named `test...`, instead of the script *(see [testing.md](testing.md))* |
| `--resources <folder>` | Where `std/resources` reads the files of a script outside a project, instead of the folder `resources/` next to it *(see [bundles.md](bundles.md))* |
| `--name <name>` | The name of the command for the `Usage:` line of `--help`, instead of the name of the script's path *(i.e. `--name cinder`)*. `ferry run` and the commands `ferry install` writes pass it, since the person typed `cinder`, not `cinder.lks` *(see Parsing in [cli.md](../language/cli.md))* |
| `--no-type-check` | Runs the script without checking its types first, for scripts that were already checked. Since types never change what the code does, it behaves the same |
| `-h`, `--help` | Shows the help |
| `-V`, `--version` | Shows the version |
| `--tokens` | For debugging `lks`: prints the tokens of the script instead of running it |
| `--ast` | For debugging `lks`: prints the syntax tree of the script instead of running it |
| `--bytecode` | For debugging `lks`: prints the bytecode of the script instead of running it. It changes between versions |

## Checking without running

*Backlog, see [roadmap.md](../roadmap.md).* `lks --check script.lks` checks the types of a script and the modules it uses without running it, and says nothing when there's no error, like `ferry check` for a project. A script's whole job is often to delete files or run commands, so checking it shouldn't need running it.

## Interactive

*Decided, for phase 14 in the plan of [roadmap.md](../roadmap.md). Not built yet.*

Like Ruby's irb, to try the language and the standard library without a file:

- **`lks` without a script opens a prompt.** Each line runs as soon as it's entered, and its variables, functions and `use` lines stay for the next ones. A line that gives a value shows it with its type *(i.e. `[1, 2] : list<number>`)*.
- **It colors the code while it's typed**, with the same highlighting of Lakescript as the code blocks of `ferry doc`, and **completes it** with the completion of `lks-lsp`: methods after `.`, members after `::`, the variables there are. Everything typed so far is one script for them.
- **A line that isn't finished asks for another one** *(i.e. after an open `{`, or a `(` that isn't closed)*, instead of an error.
- History, kept between sessions, and the keys of a line editor. A crate does it *(i.e. `reedline`, MIT, the one of Nushell)*.
- **`lks -e '...'`** runs one line from the shell *(i.e. `lks -e 'outln(1.5.round())'`)*, checked like a script.
- Later, maybe: a mode that runs the code once for each line of stdin, like `perl -ne`.

## Environment variables

| Variable | Meaning |
|----------|---------|
| `NO_COLOR`, `CLICOLOR_FORCE` | Turn colors off, or on when the output isn't a terminal, with `--color auto` |
| `LKS_PATH` | Extra folders where `use` looks for modules, separated by `:` *(see the module path in [modules.md](../language/modules.md))* |
