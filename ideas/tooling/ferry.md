# ferry

The tool for projects, like cargo or npm. `lks` runs one script, and `ferry` handles everything around a project: its file, its dependencies, and installing its commands.

```
ferry <command> [arguments...]
```

## Projects

Every project has an **author** and a **name**, and its ID is `author/name` *(i.e. `folgue02/backup`)*. Both parts follow the rules of names, like any part of a module path *(see [modules.md](../language/modules.md#packages))*.

```
backup/
├── ferry.toml
├── ferry.lock
├── src/
│   └── folgue02/
│       └── backup/
│           ├── config.lks      // use folgue02/backup/config;
│           └── archive.lks     // use folgue02/backup/archive;
├── bin/
│   ├── backup.lks              // ferry run backup
│   └── restore.lks             // ferry run restore
└── tests/
    └── config.lks              // ferry test
```

- The modules of a project live under `src/author/name/`, like Java's `src/com/acme/project/`. So every module has its full path everywhere, in the project itself and in the projects that depend on it, and two projects can never provide the same module.
- A project is a **library** or an **application**, as `ferry.toml` says.
    - A library is only its modules, for other projects to use.
    - An application also has **entry points**: every script in `bin/` is a command named after its file *(i.e. `bin/restore.lks` is `restore`)*, like Ruby's `bin/`. One project can have as many as it needs.
- The scripts in `bin/` use the project's modules like any other module *(i.e. `use folgue02/backup/config;`)*.
- The files in `tests/` have the project's tests, run by `ferry test` *(see [testing.md](testing.md))*.

## ferry.toml

```toml
[project]
author = "folgue02"
name = "backup"
version = "0.1.0"
kind = "application"        # or "library"
lks = "0.13.0"              # the oldest Lakescript it needs, optional

[dependencies]
"github:acme/http" = "1.4.2"
"github:folgue02/strings" = "^0.3.0"

[check]                     # optional
docs = true                 # a warning for each `pub` item without a `///` comment
```

- `lks` is the oldest Lakescript the project needs. An older `ferry` warns before what it does, since the project may use what it doesn't have. The version manager of phase 15 will read it too *(see [roadmap.md](../roadmap.md))*.
- `[check]` asks `ferry check` for more: with `docs = true`, a warning for each `pub` item and `pub` member without a `///` comment, but not for the values of an enum, whose names often say enough.

- A dependency is written `repository:author/name`, and its version.
- Versions follow [semver](https://semver.org). A dependency asks for an **exact version** *(i.e. `"1.4.2"`)*, or a **range**, like Cargo's: `^1.4.2` is that or newer below `2.0.0` *(below `0.5.0` for `^0.4.2`)*, and `~1.4.2` below `1.5.0`. A bare version stays exact, so ranges are asked for. Built in phase 13.
- A range gets the newest version that fits, from the tags of its repository, and the lock file keeps it until `ferry update`. Every package has one version for the whole project: the one chosen first has to fit what the others ask for, or it's an error that names both. ferry doesn't look for another version that fits them all.

## Repositories

A repository is where `ferry` downloads packages from. Each one has a name, which is what comes before the `:` in a dependency *(i.e. `github` in `github:acme/http`)*, so there can be several: GitHub, GitLab, a company's own server.

- Packages are git repositories, and a version is a git tag *(i.e. `1.4.2` is the tag `v1.4.2`)*, like Go. There's no registry to publish to: tagging a commit and pushing it publishes a version.
- The lock file keeps the commit of every tag it used, so a tag that was moved later is noticed instead of silently giving different code.

## The local repository

Like Maven's `~/.m2`, everything `ferry` downloads or installs goes into one folder per user, `~/.ferry/`. On Windows it's `%USERPROFILE%\.ferry\`, since Windows doesn't set `HOME`, also from Git Bash, which does, so every shell finds the same one. `FERRY_HOME` gives another folder:

```
~/.ferry/
├── config.toml                          // the repositories, see below
├── repository/
│   └── github/acme/http/1.4.2/          // every version of every package, once
└── bin/
    ├── backup                           // installed by `ferry install`
    └── restore
```

- A package is downloaded once per version, and every project that uses it shares it.
- Adding `~/.ferry/bin` to `PATH` makes every installed command available everywhere.
- The launchers run the `lks` next to `ferry`, by its full path, or the one on `PATH`. On Windows every path in a launcher *(`lks.exe`, the module path and the script)* is written without the `\\?\` Rust gives real paths there, which `cmd` can't run: it says it can't find the program.

## Commands

| Command | Meaning |
|---------|---------|
| `ferry new <author/name>` | Creates a project in a new folder named like the project *(i.e. `ferry new acme/http` makes `http/`)*, with only what every project needs: `ferry.toml`, `.gitignore`, the empty `src/<author>/<name>/` and, for an application, an empty command in `bin/<name>.lks`. `--lib` makes a library, without `bin/`, and with an empty `tests/` and a `module.md` to fill in, the front page of its docs. `--example` adds a module, `greeting.lks`, and a command that uses it, to try ferry out |
| `ferry run [command] [arguments...]` | Runs one of the project's commands from `bin/`, with its dependencies on the module path. The name can be left out when there's only one |
| `ferry task <name> [arguments...]` | Runs one of the project's own scripts, in `scripts/`, like a command *(see [Scripts of a project](#scripts-of-a-project))* |
| `ferry check` | Type checks the whole project without running it, its tests too. It also warns about comments that don't match the code *(i.e. a `$name:` for a parameter that was renamed)*, which never make it fail. Every module, command and file of tests is checked on its own, so each one shows its first error, and the count is the errors shown. A file whose first error is in a module it uses gets a note that it's checked once that one is fixed. Paths are the ones from the project's folder *(i.e. `src/me/app/math.lks`, `tests/math.lks`)* |
| `ferry doc [package\|std\|name] [--open]` | Makes the docs of the project, of a package or of the standard library, and opens them with `--open`. With a name *(i.e. `units::bytes`)*, writes its docs in the terminal *(see [docs.md](docs.md))* |
| `ferry test [filter] [--jobs <n>]` | Runs the tests in `tests/`, or only the ones whose name has `filter` *(see [testing.md](testing.md))*. Its files run at once, each in its own thread, and their results are printed whole, in order. `--jobs 1` runs them one at a time. A filter that no test's name has gets a note that says so |
| `ferry add <repository:author/name@version>` | Adds a dependency to `ferry.toml` and downloads it *(i.e. `ferry add github:acme/http@1.4.2`)*. A dependency that's already there at that version gets a note, and another version of it says `Changed ... from 1.0.0 to 1.1.0`. A version that isn't there lists the ones that are, from the tags. A repository that isn't there, or that's private, says so instead of git's text: git never waits for a password, and a private one can be reached through ssh with a repository of its own in `~/.ferry/config.toml`. An ID in another case than the project's says how it's written *(i.e. `github:folgue02/text` for `github:Folgue02/text`)* |
| `ferry remove <package>` | Takes a dependency out of `ferry.toml` and the lock file, by its package or its ID *(i.e. `ferry remove acme/http`)*, and the packages only it needed |
| `ferry update [package]` | Moves each dependency, or one, to the newest version its range takes, in the lock file. `ferry.toml` doesn't change, and an exact version never moves |
| `ferry bundle [command]` | Makes a bundle in `dist/` for each command of `bin/`, or only for one: the program in one file, `dist/todo.lkb`, with every module it needs and the project's `resources/`, run with `lks todo.lkb` or `./todo.lkb` *(see [bundles.md](bundles.md))*. It checks the project first, and makes nothing if it has errors |
| `ferry uninstall [author/name]` | Removes the commands a project installed in `~/.ferry/bin`, and its copy: the project in the current folder, or the one named. Another project's commands stay |
| `ferry install [repository:author/name@version]` | Installs the commands of an application into `~/.ferry/bin`: the project in the current folder, or one downloaded from a repository |

## What ferry says

Like cargo, `ferry` says what it does on stderr, one line for each step, with the verb in green and lined up on the right. Errors start with `error:` in red, hints with `note:` in cyan, and errors in scripts look like `lks`'s *(see [diagnostics.md](diagnostics.md#colors))*:

```
 Downloading github:acme/http 1.4.2
       Added github:acme/http 1.4.2
     Checked folgue02/backup: no errors
error: github:acme/http has no version 1.5.0: there's no tag v1.5.0 in https://github.com/acme/http.git
```

- `--color auto|always|never` goes before the command *(i.e. `ferry --color never check`)*, so it never mixes with the arguments of a script in `ferry run`.
- `ferry run` adds nothing to what the script prints.
- Everything after the name of the command goes to the script as it is, so its flags need no `--` to be told from ferry's *(i.e. `ferry run cinder --seed 42`)*, unlike `cargo run -- ...`. A `--` is passed on too, which is how a script gets positional arguments that start with `-` *(i.e. `ferry run tool -- -name`)*. Discussed after cinder, and kept.

## Configuring repositories

`github` is built in. Others go in `~/.ferry/config.toml`, each one with the URL of its packages, where `{author}` and `{name}` are replaced:

```toml
[repositories]
gitlab = "https://gitlab.com/{author}/{name}.git"
work = "https://git.acme.com/{author}/{name}.git"
```

- A repository in the config file can replace the built-in `github`.
- The `FERRY_HOME` environment variable replaces `~/.ferry`, i.e. for tests or to keep it on another disk.

## Authors and names

The author and name of a project are its real ones, as the repository has them *(i.e. `github:some-user/my.tool`)*. Its ID, the one modules are written with, turns `-` and `.` into `_` *(i.e. `some_user/my_tool`)*, so its modules live in `src/some_user/my_tool/`.

- Every other character must be valid in a name, and neither part can start with a digit.
- Two packages whose IDs end up the same *(i.e. `a-b/x` and `a_b/x`)* are a clash, like two packages with the same ID from different repositories.
- Every module of a package must be under `src/<its ID>/`. A file anywhere else in `src/` is an error, so packages can never provide the same module.

## Installed commands

`ferry install` type checks the project, and then writes a small launcher in `~/.ferry/bin` for every command in `bin/`. The launcher is a shell script that runs the command with `lks`, giving it the module path of its project *(`lks --module-path`)*. On Windows, it's a `.cmd` file with the same line *(i.e. `~/.ferry/bin/backup.cmd`)*:

- An installed package runs from where it was downloaded, in `~/.ferry/repository/`.
- Installing the project in the current folder copies it to `~/.ferry/installed/<ID>/` first, so changing or deleting the folder doesn't break the installed commands.
- Installing again replaces the commands of that project. A command with the same name from another project is an error, so one project never silently replaces another's.
- The launcher skips the type check, since `ferry install` already did it.
- The launcher copies `resources/` with the project, and passes it to `lks` with `--resources`, so `std/resources` reads the installed copy. `ferry run` and `ferry test` read the project's own.
- The launcher and `ferry run` pass the name of the command to `lks` with `--name`, so the `Usage:` line of `--help` says `cinder`, what was typed, not `cinder.lks` *(see Parsing in [language/cli.md](../language/cli.md))*.

## How a project runs

`ferry run` and the launchers run a command the same way, with this module path:

1. The folder of the script, `bin/`, like any script run by `lks`.
2. The project's `src/`.
3. The `src/` of each dependency, and of their dependencies.
4. The standard library.

- Dependencies are resolved before anything runs: every one of them is downloaded if it's not in the local repository yet, and every clash is found then.
- `ferry.lock` is written by `ferry` and should be committed. It has the commit of every package, and a package whose tag now points to another commit is an error.

## Scripts of a project

*Built in phase 13 (see [roadmap.md](../roadmap.md)).*

- **A project's own scripts**, to automate what's done on it *(i.e. a release, generated code, a deploy)*, like `cargo xtask` or the scripts of `npm run`, but in Lakescript. They're in `scripts/` *(i.e. `scripts/release.lks`)*.
- **`ferry task <name> [arguments...]`** runs one, with the project's module path, like `ferry run` *(i.e. `ferry task release 1.2.0`)*.
- They're apart from the commands of `bin/`: they're never installed, never bundled, and don't get docs pages. A name can be in both. `ferry check` checks them, like the tests.
- **`std/tooling/ferry`** gives them the project: its ID, version, kind, dependencies and folders, read from its `ferry.toml` *(on `std/format/toml`)*. Later it could also run ferry's steps *(i.e. check, test, bundle)*. See [stdlib/tooling/ferry.md](../stdlib/tooling/ferry.md).

## Later

- Data files in packages *(i.e. the levels of a game)*: being discussed with the bundles, as their `resources/` *(see [bundles.md](bundles.md))*. Raised again by cinder.
- **Dependencies on a folder**, a project somewhere on disk instead of a repository *(i.e. a library next to the program that uses it, worked on at the same time, like Cargo's `path = "../text"`)*. Decided, it should be supported: found when trying dependencies with `folgue02/text` and `folgue02/table`. How it's written in `ferry.toml`, and what the lock file keeps for it, aren't decided yet. Not in a phase of the plan yet.
- **Maybe, later: projects with several projects in them**, like Cargo's workspaces *(i.e. a library and the tools that use it, in one repository)*. Not decided.
