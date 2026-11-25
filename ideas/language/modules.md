# Modules

Modules work like Java's packages: they're namespaces, and they match the folder structure. Every file is a module, and its path is the path of the file from one of the roots of the [module path](#module-path), without the extension *(i.e. `src/company/utils/strings.lks` is `company/utils/strings`)*.

Modules and folders are named in lowercase, with words separated by `_` *(i.e. `text_utils.lks`)*. A file that holds a single class, interface or enum is named like it, in PascalCase *(i.e. `ChildClass.lks`, see [classes.md](classes.md))*.

Paths are never relative: a module is always imported with the same path, no matter which file imports it. There is no `./` or `../`:

```
// In bin/app.lks, src/company/app.lks or src/company/utils/other.lks, always:
use company/utils/strings::strPad;
```

## Module path

Like Java's classpath, the module path is the list of places where `use` looks for modules. `use a/b/c;` looks for `a/b/c.lks` in each of them:

1. The project's `src/` folder *(the scripts in `bin/` use the same module path, see [ferry.md](../tooling/ferry.md))*. For a standalone script, the folder the script is in. If the script is run through a link *(i.e. `/usr/local/bin/deploy` pointing to `~/scripts/deploy.lks`)*, it's the folder of the real file, so the files it uses are found wherever the link is.
2. The dependencies of the project, added by `ferry` from the lock file, so a project always gets the same ones.
3. The folders in the `LKS_PATH` environment variable, separated by `:`, for standalone scripts and quick tests.
4. The standard library, built into `lks` under `std/` *(i.e. `use std/fs;`)*.

```
use std/fs;                 // the standard library
use helpers;                // helpers.lks, next to the script
use folgue02/backup/config; // src/folgue02/backup/config.lks in a project
```

- A standalone script and the files it uses are moved together, like a project. `use helpers;` keeps working as long as `helpers.lks` stays next to the script.
- A script only reaches modules in its own folder and below it, never above it: `use` paths have no `..`, and no tool searches the parent folders. A module in a subfolder *(i.e. `shop/store/Customer.lks` with `use store/Order;`)* is written for the module path of the script that uses it *(`shop/`)*, so on its own it doesn't find its neighbours, and that's expected.
- `std/` is reserved: no folder or dependency can provide a module whose path starts with `std/`, so the standard library can never be replaced.
- A module found in more than one root is an error that names both places. In Java the first one silently wins, which makes a program behave differently depending on the machine. In a project, `ferry` finds these before anything runs *(see [Packages](#packages))*.
- A program that embeds Lakescript decides what the module path has. Leaving `std/process` or `std/fs` out of it means scripts can't run commands or touch files.
- Modules are only looked for and loaded when a `use` that needs them runs.

### Packages

Every package has an ID, `author/name` *(i.e. `folgue02/backup`)*. Both parts follow the same rules as names. How projects, repositories and versions work is in [ferry.md](../tooling/ferry.md).

```
// In the project folgue02/something, which depends on acme/http
use folgue02/something/config;     // src/folgue02/something/config.lks, the project's own module
use acme/http/client;              // client.lks of the dependency
```

- A project's modules live under `src/author/name/`, so every module has the same full path in its own project and in the projects that use it, and every path belongs to a single package.
- Versions follow [semver](https://semver.org): `MAJOR.MINOR.PATCH` *(i.e. `1.4.2`)*. A new major version may break code that uses the package, a minor one only adds, and a patch only fixes.
    - For now a dependency asks for an exact version. Ranges written like Cargo's *(i.e. `^1.4`, `~1.4`)* can come later.
- `ferry` picks a single version of each package, so when two dependencies use the same package, they share it.
- Every clash is found by `ferry` while resolving the dependencies, never by `lks` while the script runs:
    - Two dependencies that need different versions of a package. The error names both of them and both versions.
    - Two packages with the same ID from different repositories *(i.e. `github:acme/http` and `work:acme/http`)*.

### Without `use`

Like `java.lang` in Java, a few things can always be used without `use`:

- `out` and `outln`, to write to stdout, and `err` and `errln`, to write to stderr.
- `exit`.
- The built-in exceptions *(i.e. `ValueError`, see [errors.md](errors.md))*.

Everything else in the standard library needs a `use` *(see [stdlib/README.md](../stdlib/README.md))*.

### Types in the same folder

Like the classes of a package in Java, a type whose module is in the same folder can be used without `use`. In `src/folgue02/shop/Order.lks`, `Customer` is `folgue02/shop/Customer`, the `Customer.lks` next to it:

```
// src/folgue02/shop/Order.lks, without `use folgue02/shop/Customer;`
pub class Order {
    pub Customer @customer;
    pub list<Item> @items = [];      // folgue02/shop/Item
}
```

- **Only type modules**, the ones named in PascalCase *(see Importing a type in [classes.md](classes.md))*. Lowercase modules still need their `use`, so `name::` always says where it comes from.
- **The folder is the one of the module's own path**, looked for through the module path like any `use`. A standalone script's folder is a root, so the types next to it can be used without `use` too.
- **It's the last place a name is looked for.** A type declared in the file, a type parameter, a `use` *(also of another type with the same name)* and the built-in types *(i.e. `ValueError`)* all come first. So adding a file to a folder never changes what code that already worked means, and a `use` wins, like a single-type import in Java.
- A `use` of a type in the same folder still works, it just isn't needed.
- **It costs nothing to code that doesn't need it.** The folder is only looked in for a type name that nothing else gives, once per name, when the file is checked or loaded. The module is loaded the first time it's used, like with a `use`.
- Editors don't write a `use` for them when completing *(see Importing types from completion in [tooling/lsp.md](../tooling/lsp.md))*.

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
pub const string $defaultPath = 'config.json';

pub string load(string $path) = _read($path).trim();

string _read(string $path) = readFile($path);   // only usable in this file
```

- `pub` goes before functions, constants declared with `const`, classes, interfaces and enums.
- A `pub const` always has its type written *(i.e. `pub const string $defaultPath = 'config.json';`)*, like a function always has its return type. So everything another module can use has its type in its declaration, and the checker can read it without checking the code around it *(see [classes.md](classes.md#circular-imports))*.
- `let` variables can't be `pub`, so another module can never change them. A module that wants to share changing state does it through functions.
- Importing something that isn't `pub` is an error *(i.e. `use config/paths::_read;`)*.

## Loading

- Every module is loaded once, like PHP's `require_once` or C's `#pragma once`. All the files that `use` it share the same module, and its constants are only computed once.
- A module is loaded the first time something in it is used, not when the `use` line is read.
- An imported module can only have declarations at its top level: functions, `const`, `let`, classes, interfaces and enums. Importing it never prints, runs commands or touches files.
    - A `let` at the top level of a module is its own state *(i.e. the position of a parser, or a cache)*. It can't be `pub`: other modules can't read or change it, only the module's functions. To share it, a `pub` function returns it.
    - A `const` can still call a function *(i.e. `const $config = loadConfig();`)*. It runs once, when the module is loaded, only to compute the value.
    - Anything else at the top level of an imported module is an error *(i.e. `outln('loaded');`)*. Only the script being run can have statements at its top level.
- `use` lines go at the top of the file, before any other statement. A `use` inside a function, an `if` or a loop is an error, so all the modules a file needs can be seen at a glance.

## Documenting folders

A module documents itself with the `///` comments of its declarations *(see Doc comments in [functions.md](functions.md))*. A folder of modules is documented by a `module.md` in it, in Markdown: what the folder holds, or is meant to hold.

```
src/folgue02/web/
├── module.md            // what folgue02/web is: the front page of the library
├── server.lks
└── api/
    ├── module.md        // what goes in api/: the endpoints, one module each
    ├── users.lks
    └── orders.lks
```

- `module.md` rather than `README.md`: the name says what it's for, and it doesn't mix with the `README.md` files a repository has for the people who read it. GitHub and GitLab don't show it on their own, which is the cost.
- `ferry doc` gives each folder a page with its `module.md` first and then its modules. The one in `src/<author>/<name>/` is the front page of the library *(see phase 9 in [roadmap.md](../roadmap.md))*.
- Editors show its first paragraph on hover over a part of a `use` path *(i.e. `api` in `use folgue02/web/api/users;`)*, and its first line next to each folder while a `use` path is completed. A module shows its `//!` text on hover, and the first line of it next to its name while the path is completed.
- `ferry` already leaves the files of `src/` that aren't `.lks` alone, so a `module.md` there changes nothing for it.

## Rules

- The parts of a path can be reserved words, since they're names of folders and files *(i.e. `use text/case;`)*. A module with a reserved name needs an alias to be used *(i.e. `use text/case as c;`)*.
- Modules can import each other, directly or through other modules *(i.e. `Order` and `Customer`, each in its own file)*. This works because a module's top level only has declarations, and everything `pub` has its type written. How it works is in [classes.md](classes.md#circular-imports).
    - The checker reads the signatures of a module that's in the middle of being checked from its declarations, without checking its code, since everything `pub` has its type written.
    - When the script runs, a module that's still loading is given as far as it got to the modules it uses, and the names they import from it get their values when it finishes. So functions and classes can call each other freely.
    - Only a constant that needs the other module while it's still loading fails, with an error that says so *(i.e. `$x` of `a.lks` isn't ready yet: that module is still loading, because it uses this one)*.
