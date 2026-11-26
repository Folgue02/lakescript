# std/tooling/ferry

The project a script is in, read from its `ferry.toml` and `ferry.lock`, for the project's own scripts that `ferry task` runs *(see Scripts of a project in [ferry.md](../../tooling/ferry.md))*. `use std/tooling/ferry;`, then `ferry::project()`.

*Built in phase 13 (see [roadmap.md](../../roadmap.md)).*

```
// scripts/release.lks, run with `ferry task release`
use std/tooling/ferry;

const $project = ferry::project();
outln(f"Releasing {$project.id} {$project.version}");
for ($dependency in $project.dependencies) {
    outln(f"  {$dependency.package} {$dependency.version ?? $dependency.wanted}");
}
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `project(?string $from = null)` | `Project` | The project of the folder `$from`, or of the current one: the first `ferry.toml` in it or in the folders above it. Throws a `FileNotFound` if there's none |

## Project

A ferry project: what its `ferry.toml` says, and where its folders are.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Its ID, the one its modules are written with *(i.e. `folgue02/backup`)* |
| `author` | `string` | Its author, as `ferry.toml` has it |
| `name` | `string` | Its name, as `ferry.toml` has it |
| `version` | `string` | Its version |
| `kind` | `string` | `'application'` or `'library'` |
| `lks` | `?string` | The oldest Lakescript it needs, or `null` if it doesn't say |
| `dir` | `string` | Its folder, the one with `ferry.toml` |
| `dependencies` | `list<Dependency>` | Its dependencies, in the order of `ferry.toml` |

| Method | Returns | Description |
|--------|---------|-------------|
| `src()` | `string` | Its `src/` folder |
| `bin()` | `string` | Its `bin/` folder, with its commands |
| `tests()` | `string` | Its `tests/` folder |
| `scripts()` | `string` | Its `scripts/` folder, with its tasks |
| `resources()` | `string` | Its `resources/` folder |
| `commands()` | `list<string>` | The names of its commands, the scripts of `bin/`, sorted |
| `tasks()` | `list<string>` | The names of its tasks, the scripts of `scripts/`, sorted |

## Dependency

A dependency of a project, as its `ferry.toml` asks for it.

| Field | Type | Description |
|-------|------|-------------|
| `package` | `string` | Its package *(i.e. `github:acme/http`)* |
| `wanted` | `string` | What `ferry.toml` asks for: a version, or a range *(i.e. `^1.4.2`)* |
| `version` | `?string` | The version `ferry.lock` has, or `null` before ferry chose one |

## Later

- Running ferry's steps from a script *(i.e. check, test, bundle)*. Not decided yet.

## How it's made

- `tooling/ferry.lks`, in Lakescript, on `std/format/toml`, so it's there when the `toml` feature is.
