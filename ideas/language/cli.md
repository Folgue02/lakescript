# Command-line arguments

## The plain list

`process::args()` from `std/process` returns the arguments of the script as a `list<string>`, without the script's own path:

```
use std/process;

const $args = process::args();
if ($args.empty?()) {
    errln('usage: greet.lks <name>');
    exit(2);
}
outln(f"Hello {$args[0]}");
```

This is the only part of this file in phase 3. The rest comes with OOP, in phase 7, since it's built on classes *(see [roadmap.md](../roadmap.md))*.

## Commands

A `command` describes the arguments of a program, like Rust's `clap` with `#[derive(Parser)]` or Java's picocli: its fields are the arguments, and parsing the command line gives an object with them. The `///` comments are the `--help` text.

```
/// A tiny git
command Git {
    /// More output, -vv for even more
    flag(v, count) number $verbose;

    /// Records changes
    subcommand Commit {
        /// Message for the commit
        flag(m) string $message;
    }

    /// Pushes all commits to the remote
    subcommand Push {
        /// The remote to push to
        string $remote;
        /// Optional, the branch to push
        ?string $branch;
    }
}

const $args = Git.parse();      // reads the arguments of the script

switch ($args.subcommand) {
    case Git.Commit -> outln(f"Committing: {$args.subcommand.message}");
    case Git.Push -> outln(f"Pushing to {$args.subcommand.remote}");
}
```

```
$ git.lks -v commit -m 'Fix the parser'
$ git.lks push origin main
$ git.lks push --help
```

- Commands are only data: they have fields and no methods, so a script keeps running from top to bottom, and using a command never forces the rest of the script into OOP.
- Commands can be used without knowing anything about classes. They're built on classes, but declaring one and calling `.parse()` is all a script needs.
- A `command` is a final class that extends the built-in `Command`. The example is the same as:

```
final class Git extends Command {
    number $verbose;
    GitSubcommand $subcommand;      // exactly one of the subcommands
}

abstract class GitSubcommand extends Command {}

final class Commit extends GitSubcommand {
    string $message;
}

final class Push extends GitSubcommand {
    string $remote;
    ?string $branch;
}
```

### Parsing

- `Git.parse()` reads the arguments of the script *(i.e. `process::args()`)*. `Git.parse($list)` reads a `list<string>`, which is useful for tests, for wrappers and for programs that embed Lakescript.
- Arguments that don't match throw a `ValueError` whose message includes the usage. With `try` the script can handle it: `const $args = (try Git.parse()).orElse(null);`.
- If the exception isn't caught, the script stops: `lks` prints the message and the usage to stderr and exits with code `2`, like most command-line tools.
- `--help` and `-h` print the help, generated from the `///` comments, the types and the defaults, and exit with code `0`.
- `Git.help()` returns the help text as a string.

### Subcommands

- A command has a field `$subcommand` with exactly one of its subcommands, and a `switch` with type cases tells them apart. It doesn't need a `default` when every subcommand has a case.
- Subcommands can have their own subcommands *(i.e. `git remote add`)*.
- The fields of a command are shared by all its subcommands *(i.e. `-v` in `git -v commit`)*.
- If the command can be used without a subcommand, it's declared with `optional subcommand` and `$subcommand` can be `null`.

### Fields

The fields follow the same rules as the parameters of a function:

| Declaration | Meaning |
|---|---|
| `string $source;` | Positional and required |
| `?string $dest;` | Positional and optional, `null` when it's not given |
| `flag(k) number $keep = 5;` | Optional, with a default |
| `flag(n) bool $dryRun;` | A switch: always optional, `false` unless it's given |
| `flag ...string $exclude;` | Can be given several times, and collects into a list |

- Fields without `flag` are positional, filled in the order they're declared. Required ones go before optional ones, and a `...` one goes last.
- `flag` fields are only given by name, `--keep 3` or `--keep=3`, in any order and before or after the positional ones. They can be required too *(i.e. `flag string $token;`)*.
- `bool` fields are always flags.
- Names become flags in kebab case: `$dryRun` is `--dry-run`. `--no-dry-run` turns a switch off again.
- `--` ends the flags: everything after it is positional, even if it starts with `-`.
- Default values are evaluated when the command is parsed, so they can come from the environment *(i.e. `flag string $token = env::get('TOKEN') ?? '';`)*.

### Short flags

`flag(n)` gives a field a short flag: `-n` is the same as `--dry-run`.

- Short flags can be combined: `-nv` is `-n -v`.
- A short flag with a value takes it after a space or right after it: `-k 3` or `-k3`.
- Two fields with the same short flag are an error before the script runs. `-h` is reserved for `--help`.

### Counting flags

`flag(v, count) number $verbose;` counts how many times the flag is given: nothing is `0`, `-v` is `1`, `-vvv` and `-v -v -v` are `3`. It's the convention of `ssh`, `ansible` and `pip`, which most argument libraries support.

### Types

The type of a field decides how its text is read:

- `string`: as it's given.
- `number`: like `.toNumber()` *(i.e. `--keep abc` is an error)*.
- `bool`: a switch, with no value.
- `...T`: a list, filled by giving the flag several times.

Fields keep their types while the script runs, like the fields of any class, so this doesn't break the rule that types are removed before running *(see [type_checking.md](type_checking.md))*.

## A script without subcommands

A command doesn't need subcommands. For a simple script, its fields are the arguments:

```
#!/usr/bin/lks
/// Copies a folder to the backup disk.

command Args {
    /// The folder to copy
    string $source;
    /// How many old backups to keep
    flag(k) number $keep = 5;
    /// Only show what would be copied
    flag(n) bool $dryRun;
}

const $args = Args.parse();
outln(f"Copying {$args.source}, keeping {$args.keep}");
```

- There is only this way to declare arguments. Parsing them is a normal line of the script, so it can fail with `try`, and a list can be given instead of the script's arguments.

## Still to be decided

- Whether `command`, `subcommand` and `flag` are reserved everywhere or only where they're used.

## Backlog

- Limiting a value to some choices *(i.e. `--mode fast|slow`)*. It needs enums, which come with OOP.
