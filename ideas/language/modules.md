# Modules

Modules work like Java's packages: they're namespaces, and they match the folder structure. Every file is a module, and its path is the path of the file from one of the roots of the [module path](#module-path), without the extension *(i.e. `src/company/utils/strings.lks` is `company/utils/strings`)*.

Paths are never relative: a module is always imported with the same path, no matter which file imports it. There is no `./` or `../`:

```
// In src/main.lks, src/company/app.lks or src/company/utils/other.lks, always:
use company/utils/strings::strPad;
```

## Module path

Like Java's classpath, the module path is the list of places where `use` looks for modules. `use a/b/c;` looks for `a/b/c.lks` in each of them:

1. The project's `src/` folder. For a standalone script, the folder the script is in. If the script is run through a link *(i.e. `/usr/local/bin/deploy` pointing to `~/scripts/deploy.lks`)*, it's the folder of the real file, so the files it uses are found wherever the link is.
2. The dependencies of the project, added by `ferry` from the lock file, so a project always gets the same ones.
3. The folders in the `LKS_PATH` environment variable, separated by `:`, for standalone scripts and quick tests.
4. The standard library, built into `lks` under `std/` *(i.e. `use std/fs;`)*.

```
use std/fs;                 // the standard library
use helpers;                // helpers.lks, next to the script
use config/paths;           // src/config/paths.lks in a project
```

- A standalone script and the files it uses are moved together, like a project. `use helpers;` keeps working as long as `helpers.lks` stays next to the script.
- `std/` is reserved: no folder or dependency can provide a module whose path starts with `std/`, so the standard library can never be replaced.
- A module found in more than one root is an error that names both places. In Java the first one silently wins, which makes a program behave differently depending on the machine. In a project, `ferry` finds these before anything runs *(see [Packages](#packages))*.
- A program that embeds Lakescript decides what the module path has. Leaving `std/process` or `std/fs` out of it means scripts can't run commands or touch files.
- Modules are only looked for and loaded when a `use` that needs them runs.

### Packages

Every package has an ID, which can be a single name or a path *(i.e. `acme_http`, `github_io/folgue02/backup`)*. Its parts follow the same rules as names, so a dot becomes `_` *(i.e. `github_io`)*.

```
// The project github_io/folgue02/something depends on github_io/user/dependency
use config;                              // src/config.lks, the project's own module
use github_io/user/dependency/client;    // client.lks of the dependency
```

- A dependency is only reachable through its ID: a module belongs to the dependency whose ID starts its path.
- The project's own modules keep their short paths from `src/`. Only dependencies are written with their ID.
- `ferry` picks a single version of each package, so when two dependencies use the same package, they share it.
- Every clash is found by `ferry` while resolving the dependencies, never by `lks` while the script runs:
    - Two dependencies that need versions of a package that don't work together. The error names both of them and both versions.
    - A module of the project whose path starts with the ID of a dependency *(i.e. `src/github_io/user/dependency/client.lks`)*.
    - A package ID that starts another one *(i.e. `github_io/user` and `github_io/user/dependency`)*, so every path belongs to a single package.
- Sharing the first parts of an ID is fine *(i.e. the project and its dependencies can all start with `github_io/`)*.

### Without `use`

Like `java.lang` in Java, a few things can always be used without `use`:

- `out` and `outln`, to write to stdout, and `err` and `errln`, to write to stderr.
- `exit`.
- The built-in exceptions *(i.e. `ValueError`, see [errors.md](errors.md))*.

Everything else in the standard library needs a `use` *(see [stdlib/README.md](../stdlib/README.md))*.

## Importing a module

`use` followed by the path of the module. Its contents are then reached with `::` and the last part of the path:

```
use config/paths;

paths::load();
outln(paths::$defaultPath);
```

## Importing specific things

`::` after the path imports a single thing, which can then be used without the prefix. Several things go between curly braces:

```
use config/paths::load;
load();

use config/paths::{load, save, $defaultPath};
save(load($defaultPath));
```

- Constants keep their `$`, both when importing them and when using them *(i.e. `use config/paths::$defaultPath;`, `paths::$defaultPath`)*. The `$` is what tells a variable from a function.

## Aliases

`as` gives an imported module or thing another name:

```
use company/utils/strings as str;
str::pad('5', 3);

use config/paths::load as loadPaths;
loadPaths();
```

## Exports

Only what's marked with `pub` can be used from other modules. Everything else is private to its file:

```
// src/config/paths.lks
pub const $defaultPath = 'config.json';

pub string load(string $path) = _read($path).trim();

string _read(string $path) = readFile($path);   // only usable in this file
```

- `pub` goes before functions, constants declared with `const` and type aliases *(i.e. `pub type Predicate = (string) -> bool;`)*.
- `let` variables can't be `pub`, so another module can never change them. A module that wants to share changing state does it through functions.
- Importing something that isn't `pub` is an error *(i.e. `use config/paths::_read;`)*.

## Loading

- Every module is loaded once, like PHP's `require_once` or C's `#pragma once`. All the files that `use` it share the same module, and its constants are only computed once.
- A module is loaded the first time something in it is used, not when the `use` line is read.
- An imported module can only have declarations at its top level: functions, `const` and `type`. Importing it never prints, runs commands or touches files.
    - A `const` can still call a function *(i.e. `const $config = loadConfig();`)*. It runs once, when the module is loaded, only to compute the value.
    - Anything else at the top level of an imported module is an error *(i.e. `outln('loaded');`)*. Only the script being run can have statements at its top level.
- `use` lines go at the top of the file, before any other statement. A `use` inside a function, an `if` or a loop is an error, so all the modules a file needs can be seen at a glance.

## Rules

- `::` is a single token, so it's never confused with a symbol *(`:name`)*.
- The parts of a path can be reserved words, since they're names of folders and files *(i.e. `use text/case;`)*. A module with a reserved name needs an alias to be used *(i.e. `use text/case as c;`)*.
- Circular imports are an error: if `a` uses `b`, `b` can't use `a`, directly or through other modules. This keeps the rule that everything is defined before it's used, so modules can still be compiled in one pass.

## Still to be decided

- Whether a package ID also says where `ferry` downloads it from (like Go), or packages come from a registry (like Cargo). To be discussed with the tooling.
