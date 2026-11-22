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

`process::args()` is from phase 3. The rest of this file comes with OOP, in phase 7, since it's built on classes *(see [roadmap.md](../roadmap.md))*.

## Commands

A `command` describes the arguments of a program, like Rust's `clap` with `#[derive(Parser)]` or Java's picocli: its fields are the arguments, and parsing the command line gives an object with them. The `///` comments are the `--help` text.

```
/// A tiny git
command Git {
    /// More output, -vv for even more
    flag(v, count) number @verbose;

    /// Records changes
    subcommand Commit {
        /// Message for the commit
        flag(m) string @message;
    }

    /// Pushes all commits to the remote
    subcommand Push {
        /// The remote to push to
        string @remote;
        /// Optional, the branch to push
        ?string @branch;
    }
}

const $args = Git::parse();      // reads the arguments of the script

switch ($args.subcommand) {
    case Git::Commit -> outln(f"Committing: {$args.subcommand.message}");
    case Git::Push -> outln(f"Pushing to {$args.subcommand.remote}");
}
```

```
$ git.lks -v commit -m 'Fix the parser'
$ git.lks push origin main
$ git.lks push --help
```

- Commands are only data: they have fields and subcommands, and no methods, so a script keeps running from top to bottom, and using a command never forces the rest of the script into OOP.
- A command is a class *(see [classes.md](classes.md))* whose objects only come from `parse()`: calling it *(i.e. `Git()`)* is an error. Its fields are written with `@`, like the fields of any class, and read with `.` *(i.e. `$args.verbose`)*.
- `command`, `subcommand`, `flag` and `optional` are only words where they're used: `command` is still the type of `%x"..."`, and `command Git {` declares a command because a PascalCase name and `{` follow.

### Parsing

- `Git::parse()` reads the arguments of the script *(i.e. `process::args()`)*. `Git::parse($list)` reads a `list<string>`, which is useful for tests, for wrappers and for programs that embed Lakescript.
- Arguments that don't match throw a `UsageError`, which is a `ValueError`, whose message says what's wrong and then the usage. With `try` the script can handle it: `const $args = (try Git::parse()).orElse(null);`.
- If the exception isn't caught, the script stops: `lks` prints the message and the usage to stderr, without the code around it, and exits with code `2`, like most command-line tools. `ferry run` doesn't yet: it shows it as an error in the script *(see the Backlog of [roadmap.md](../roadmap.md#backlog))*.
- `--help` and `-h` print the help, generated from the `///` comments, the types, the choices and the defaults, and exit with code `0`.
- `Git::help()` returns the help text as a string.

### Subcommands

- A command with subcommands has a field `@subcommand` with the one that was given. Each subcommand is a class reached through its command *(i.e. `Git::Commit`)*, so a `switch` with type cases tells them apart, and inside each case `$args.subcommand` has that subcommand's fields. It doesn't need a `default` when every subcommand has a case.
    - Outside a `switch`, `.as(Git::Commit)` gives a subcommand with its fields.
    - After `::`, a name in SCREAMING_CASE is a value of an enum *(i.e. `Status::PAID`)*, and any other name is a type *(i.e. `Git::Commit`)*. So a subcommand's name has a lowercase letter.
- On the command line, a subcommand's name is in kebab case *(i.e. `DryRun` is `dry-run`)*.
- Subcommands can have their own subcommands *(i.e. `git remote add`)*.
- The fields of a command go before the name of the subcommand *(i.e. `-v` in `git -v commit`)*. The arguments after it are the subcommand's.
- If the command can be used without a subcommand, a subcommand is declared with `optional subcommand`, and `@subcommand` can be `null`.

### Fields

The fields follow the same rules as the parameters of a function:

| Declaration | Meaning |
|---|---|
| `string @source;` | Positional and required |
| `?string @dest;` | Positional and optional, `null` when it's not given |
| `flag(k) number @keep = 5;` | Optional, with a default |
| `flag(n) bool @dryRun;` | A switch: always optional, `false` unless it's given |
| `flag ...string @exclude;` | Can be given several times, and collects into a list |

- Fields without `flag` are positional, filled in the order they're declared. Required ones go before optional ones, and a `...` one goes last. The checker says so before the script runs.
- `flag` fields are only given by name, `--keep 3` or `--keep=3`, in any order and before or after the positional ones. They can be required too *(i.e. `flag string @token;`)*.
- `bool` fields are always flags.
- Names become flags in kebab case: `@dryRun` is `--dry-run`. `--no-dry-run` turns a switch off again.
- `--` ends the flags: everything after it is positional, even if it starts with `-`.
- Default values are evaluated when the command is parsed, so they can come from the environment *(i.e. `flag string @token = env::get('TOKEN') ?? '';`)*.

### Short flags

`flag(n)` gives a field a short flag: `-n` is the same as `--dry-run`.

- Short flags can be combined: `-nv` is `-n -v`.
- A short flag with a value takes it after a space or right after it: `-k 3` or `-k3`.
- Two fields with the same short flag are an error before the script runs. `-h` is reserved for `--help`.

### Counting flags

`flag(v, count) number @verbose;` counts how many times the flag is given: nothing is `0`, `-v` is `1`, `-vvv` and `-v -v -v` are `3`. It's the convention of `ssh`, `ansible` and `pip`, which most argument libraries support.

### Types

The type of a field decides how its text is read:

- `string`: as it's given.
- `number`: like `.toNumber()` *(i.e. `--keep abc` is an error)*.
- `bool`: a switch, with no value.
- An enum: one of its values, ignoring case, with `-` for `_` *(i.e. `--mode fast` for `Mode::FAST`, `--status at-work` for `Status::AT_WORK`)*. The help lists the choices.
- `...T`: a list, filled by giving the flag several times.
- Any other type is an error before the script runs.

## A script without subcommands

A command doesn't need subcommands. For a simple script, its fields are the arguments:

```
#!/usr/bin/lks
/// Copies a folder to the backup disk.

command Args {
    /// The folder to copy
    string @source;
    /// How many old backups to keep
    flag(k) number @keep = 5;
    /// Only show what would be copied
    flag(n) bool @dryRun;
}

const $args = Args::parse();
outln(f"Copying {$args.source}, keeping {$args.keep}");
```

- There is only this way to declare arguments. Parsing them is a normal line of the script, so it can fail with `try`, and a list can be given instead of the script's arguments.
- `examples/todo.lks` is a whole script with subcommands.
