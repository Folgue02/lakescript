# std/path

Working with paths as text. `use std/path;`, then `path::join($dir, 'notes.txt')`. These functions never touch the disk, so they never throw because of it *(see [fs.md](fs.md) for that)*.

Paths use `/` on every system. On Windows, `\` is accepted too.

## Building

| Function | Returns | Description |
|----------|---------|-------------|
| `join(...string $parts)` | `string` | Joins the parts with `/`, without doubling it. A part that is absolute starts again from it *(i.e. `path::join('/var', 'log', 'web.log')` is `'/var/log/web.log'`)* |
| `withExtension(string $path, string $extension)` | `string` | Replaces the extension, or adds it *(i.e. `path::withExtension('a.txt', 'md')` is `'a.md'`)*. An empty `$extension` removes it |
| `withName(string $path, string $name)` | `string` | Replaces the last part *(i.e. `path::withName('/a/b.txt', 'c.txt')` is `'/a/c.txt'`)* |

## Parts

| Function | Returns | Description |
|----------|---------|-------------|
| `parent(string $path)` | `?string` | The folder that holds it, or `null` for `/` or a single name *(i.e. `path::parent('/a/b/c.txt')` is `'/a/b'`)* |
| `name(string $path)` | `string` | The last part *(i.e. `path::name('/a/b/c.txt')` is `'c.txt'`)* |
| `stem(string $path)` | `string` | The last part without its extension *(i.e. `path::stem('/a/archive.tar.gz')` is `'archive.tar'`)* |
| `extension(string $path)` | `?string` | The extension without the dot, or `null` if there's none *(i.e. `path::extension('a.tar.gz')` is `'gz'`)*. A name that starts with a dot, like `.bashrc`, has no extension |
| `parts(string $path)` | `list<string>` | Every part *(i.e. `path::parts('/a/b')` is `['/', 'a', 'b']`)* |

## Resolving

| Function | Returns | Description |
|----------|---------|-------------|
| `absolute?(string $path)` | `bool` | `true` if it starts at the root *(i.e. `'/etc'`, `'C:/Users'`)* |
| `absolute(string $path)` | `string` | The path from the root, starting from the current folder if it's relative, and normalized |
| `normalize(string $path)` | `string` | Removes `.`, doubled `/`, and `..` where possible *(i.e. `path::normalize('a/./b/../c')` is `'a/c'`)*. Links aren't followed |
| `relative(string $path, string $from = '.')` | `string` | The path from `$from` to `$path` *(i.e. `path::relative('/a/b/c', '/a')` is `'b/c'`)* |
| `expandHome(string $path)` | `string` | Replaces a `~` at the start with the home folder *(i.e. `'~/notes'` is `'/home/me/notes'`)* |
| `home()` | `string` | The home folder of the user |

## Constants

| Constant | Type | Description |
|----------|------|-------------|
| `$separator` | `string` | The separator the system itself uses: `'/'`, or `'\'` on Windows |
