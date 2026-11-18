# Modules

Modules work like Java's packages: they're namespaces, and they match the folder structure of the project. Every file is a module, and its path is the path of the file from the project's `src/` folder, without the extension *(i.e. `src/company/utils/strings.lks` is `company/utils/strings`)*.

Paths are never relative: a module is always imported with the same path, no matter which file imports it. There is no `./` or `../`:

```
// In src/main.lks, src/company/app.lks or src/company/utils/other.lks, always:
use company/utils/strings::strPad;
```

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

## Rules

- `::` is a single token, so it's never confused with a symbol *(`:name`)*.
- Circular imports are an error: if `a` uses `b`, `b` can't use `a`, directly or through other modules. This keeps the rule that everything is defined before it's used, so modules can still be compiled in one pass.

## Still to be decided

- Whether an imported file can have code at the top level, or only declarations (functions, constants, `type`).
- How the standard library and dependencies are named, so they don't clash with the project's own modules *(i.e. `use std/fs;`)*. To be discussed with the tooling and the project structure.
- Whether a standalone script (outside a project) can use modules, and where their paths would start.
