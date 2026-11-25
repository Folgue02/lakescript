# lks

The interpreter. It runs one script:

```
lks [options] <script> [arguments for the script...]
```

- The options of `lks` go before the script. The first argument that isn't an option is the script, and every argument after it is passed to the script as it is, even if it looks like an option of `lks` *(i.e. in `lks deploy.lks --help`, `--help` belongs to `deploy.lks`)*.
- `--` ends the options: the next argument is the script, even if it starts with `-`.
- The exit code is the script's *(see [general.md](../stdlib/general.md))*. A wrong command line for `lks` itself exits with code `2`.

## Options

| Option | Meaning |
|--------|---------|
| `--module-path <folders>` | More folders where `use` looks for modules, separated by `:`, after the script's folder and before `LKS_PATH`. It's what the commands installed by [ferry](ferry.md) use |
| `--color <when>` | When errors and the help have colors: `auto` (on a terminal, the default), `always` or `never` *(see [diagnostics.md](diagnostics.md#colors))* |
| `--test` | Runs the tests of the script, its functions named `test...`, instead of the script *(see [testing.md](testing.md))* |
| `--name <name>` | The name of the command for the `Usage:` line of `--help`, instead of the name of the script's path *(i.e. `--name cinder`)*. `ferry run` and the commands `ferry install` writes pass it, since the person typed `cinder`, not `cinder.lks` *(see Parsing in [cli.md](../language/cli.md))* |
| `--no-type-check` | Runs the script without checking its types first, for scripts that were already checked. Since types never change what the code does, it behaves the same |
| `-h`, `--help` | Shows the help |
| `-V`, `--version` | Shows the version |
| `--tokens` | For debugging `lks`: prints the tokens of the script instead of running it |
| `--ast` | For debugging `lks`: prints the syntax tree of the script instead of running it |
| `--bytecode` | For debugging `lks`: prints the bytecode of the script instead of running it. It changes between versions |

## Environment variables

| Variable | Meaning |
|----------|---------|
| `NO_COLOR`, `CLICOLOR_FORCE` | Turn colors off, or on when the output isn't a terminal, with `--color auto` |
| `LKS_PATH` | Extra folders where `use` looks for modules, separated by `:` *(see the module path in [modules.md](../language/modules.md))* |
