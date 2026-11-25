# std/resources

The files a program needs that aren't code, like Java's resources: templates, default configurations, help texts, levels of a game. They're in the folder `resources/` of the project, and a bundle has them inside it *(see [bundles.md](../tooling/bundles.md))*. `use std/resources;`, then `resources::read('templates/page.html')`.

```
use std/resources;

const $help = resources::read('help.txt');
const $logo = resources::readBytes('images/logo.png');
for ($level in resources::list('levels'))
    outln(resources::read(f"levels/{$level}"));
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `read(string $name)` | `string` | The text of a resource *(i.e. `resources::read('templates/page.html')`)*. Throws a `FileNotFound` if there's none with that name, and a `ValueError` if it isn't text: `readBytes()` reads any file |
| `readBytes(string $name)` | `bytes` | The bytes of a resource, for what isn't text *(i.e. an image, see [bytes](types/bytes.md))*. Throws a `FileNotFound` if there's none with that name |
| `exists?(string $name)` | `bool` | `true` if there's a resource or a folder of them with that name. `''` isn't one, so it's `false` |
| `list(string $folder = '')` | `list<string>` | The names of what's directly inside a folder of `resources/`, files and folders, sorted *(i.e. `resources::list('levels')` is `['1.txt', '2.txt']`)*. Throws a `FileNotFound` if there's no such folder. A project without `resources/` has none, so `list()` is `[]` |

- **Hidden files aren't resources**: a name with a part that starts with `.` *(i.e. `.DS_Store`, `.git/config`)* is never found, and `list()` leaves them out, also when they're read from the folder, so a folder and a bundle give the same, since bundles leave them out.
- **A name is a path inside `resources/`**, with `/` on every system *(i.e. `'templates/page.html'`)*. A name that would leave it *(i.e. `'../x'`, `'/etc/passwd'`)* is a `ValueError`.
- **The same names everywhere**: they're read from `resources/` while a project is developed and run with `ferry run`, and from inside the `.lkb` once it's shipped, so the code doesn't change.
- **Where `resources/` is**:
    - In a project, its own folder *(next to `src/` and `bin/`)*, for `ferry run`, `ferry test` and the commands `ferry install` writes.
    - In a bundle, inside the file.
    - For a script run with `lks` outside a project, the folder `resources/` next to the script *(the real file's, through a link)*, or the one `lks --resources <folder>` gives.
- **Only reading**: a bundle can't change, so there's no writing. A program that needs to write a resource out *(i.e. a default configuration)* reads it and writes it with `std/fs` *(i.e. into `dirs::App('myapp').config()`, see [dirs.md](dirs.md))*.
- The resources are the project's own: a dependency's `resources/` isn't in the bundle of a project that uses it.

## How it's made

- In Rust, `crates/lakescript/src/stdlib/resources.rs`, over where the script's state says they are *(`host::Resources`)*: a folder, or the files of a bundle, read into memory when it's opened.
