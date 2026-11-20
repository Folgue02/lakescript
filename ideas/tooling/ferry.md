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
└── bin/
    ├── backup.lks              // ferry run backup
    └── restore.lks             // ferry run restore
```

- The modules of a project live under `src/author/name/`, like Java's `src/com/acme/project/`. So every module has its full path everywhere, in the project itself and in the projects that depend on it, and two projects can never provide the same module.
- A project is a **library** or an **application**, as `ferry.toml` says.
    - A library is only its modules, for other projects to use.
    - An application also has **entry points**: every script in `bin/` is a command named after its file *(i.e. `bin/restore.lks` is `restore`)*, like Ruby's `bin/`. One project can have as many as it needs.
- The scripts in `bin/` use the project's modules like any other module *(i.e. `use folgue02/backup/config;`)*.

## ferry.toml

```toml
[project]
author = "folgue02"
name = "backup"
version = "0.1.0"
kind = "application"        # or "library"

[dependencies]
"github:acme/http" = "1.4.2"
"github:folgue02/strings" = "0.3.0"
```

- A dependency is written `repository:author/name`, and its version.
- Versions follow [semver](https://semver.org), and for now a dependency asks for an **exact version**. Ranges *(i.e. `^1.4`)* can come later.
- Since versions are exact, two dependencies that need different versions of the same package are an error that names both of them and both versions.

## Repositories

A repository is where `ferry` downloads packages from. Each one has a name, which is what comes before the `:` in a dependency *(i.e. `github` in `github:acme/http`)*, so there can be several: GitHub, GitLab, a company's own server.

- Packages are git repositories, and a version is a git tag *(i.e. `1.4.2` is the tag `v1.4.2`)*, like Go. There's no registry to publish to: tagging a commit and pushing it publishes a version.
- The lock file keeps the commit of every tag it used, so a tag that was moved later is noticed instead of silently giving different code.

## The local repository

Like Maven's `~/.m2`, everything `ferry` downloads or installs goes into one folder per user, `~/.ferry/`:

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

## Commands

| Command | Meaning |
|---------|---------|
| `ferry new <author/name>` | Creates a project in a new folder. `--lib` makes a library |
| `ferry run [command] [arguments...]` | Runs one of the project's commands from `bin/`, with its dependencies on the module path. The name can be left out when there's only one |
| `ferry check` | Type checks the whole project without running it |
| `ferry add <repository:author/name@version>` | Adds a dependency to `ferry.toml` and downloads it *(i.e. `ferry add github:acme/http@1.4.2`)* |
| `ferry install [repository:author/name@version]` | Installs the commands of an application into `~/.ferry/bin`: the project in the current folder, or one downloaded from a repository |

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

`ferry install` type checks the project, and then writes a small launcher in `~/.ferry/bin` for every command in `bin/`. The launcher is a shell script that runs the command with `lks`, giving it the module path of its project *(`lks --module-path`)*:

- An installed package runs from where it was downloaded, in `~/.ferry/repository/`.
- Installing the project in the current folder copies it to `~/.ferry/installed/<ID>/` first, so changing or deleting the folder doesn't break the installed commands.
- Installing again replaces the commands of that project. A command with the same name from another project is an error, so one project never silently replaces another's.
- The launcher skips the type check, since `ferry install` already did it.

## How a project runs

`ferry run` and the launchers run a command the same way, with this module path:

1. The folder of the script, `bin/`, like any script run by `lks`.
2. The project's `src/`.
3. The `src/` of each dependency, and of their dependencies.
4. The standard library.

- Dependencies are resolved before anything runs: every one of them is downloaded if it's not in the local repository yet, and every clash is found then.
- `ferry.lock` is written by `ferry` and should be committed. It has the commit of every package, and a package whose tag now points to another commit is an error.

## Later

- Version ranges *(i.e. `^1.4`)*.
- `ferry uninstall`, `ferry remove` and `ferry update`.
