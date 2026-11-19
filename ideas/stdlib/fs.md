# std/fs

Files and folders on disk. `use std/fs;`, then `fs::read($path)`. Paths are strings, relative to the current folder unless they're absolute *(see [path.md](path.md) to build them)*.

Every function throws when the disk says no: `FileNotFound` if the path doesn't exist, `PermissionDenied` if it can't be read or written, and `IOError` for anything else.

## Reading

| Function | Returns | Description |
|----------|---------|-------------|
| `read(string $path)` | `string` | The whole file as text. Throws a `ValueError` if it isn't valid UTF-8 *(i.e. `fs::read('notes.txt')`)* |
| `readLines(string $path)` | `list<string>` | The lines of the file, without their new lines, like `read($path).lines()` |

## Writing

| Function | Returns | Description |
|----------|---------|-------------|
| `write(string $path, string $text)` | `void` | Replaces the file with `$text`, creating it if needed. The folder must exist |
| `append(string $path, string $text)` | `void` | Adds `$text` at the end of the file, creating it if needed |
| `writeLines(string $path, list<string> $lines)` | `void` | Writes the lines with a new line after each one |

## Information

| Function | Returns | Description |
|----------|---------|-------------|
| `exists?(string $path)` | `bool` | `true` if something is there: a file, a folder or a link. Never throws |
| `file?(string $path)` | `bool` | Same as `-f $path` |
| `dir?(string $path)` | `bool` | Same as `-d $path` |
| `link?(string $path)` | `bool` | `true` if it's a symbolic link, without following it |
| `size(string $path)` | `number` | The size in bytes *(i.e. `fs::size($path).formatBytes()`)* |
| `modified(string $path)` | `number` | When it was last changed, in seconds since 1970. It becomes a date once there's `std/time` |
| `mode(string $path)` | `number` | The Unix permissions *(i.e. `0o644`)*. On Windows, `0o444` or `0o666` |

## Folders

| Function | Returns | Description |
|----------|---------|-------------|
| `list(string $path = '.')` | `list<string>` | The names of what's inside the folder, sorted, without `.` and `..` *(i.e. `['a.txt', 'docs']`)* |
| `walk(string $path = '.')` | `list<string>` | The paths of every file inside the folder and its subfolders, sorted, starting with `$path` *(i.e. `['src/main.lks', 'src/util/text.lks']`)* |
| `glob(string $pattern)` | `list<string>` | The paths that match a pattern, sorted. `*` and `?` work like in `-like` without crossing a `/`, and `**` matches any number of folders *(i.e. `fs::glob('src/**/*.lks')`)*. Like in the shell, names starting with `.` only match when the pattern starts with `.` |
| `makeDir(string $path)` | `void` | Creates the folder and the folders above it that are missing. Does nothing if it already exists |
| `tempDir()` | `string` | Creates a new empty folder in the system's temporary folder and returns its path |
| `tempFile(string $extension = '')` | `string` | Creates a new empty file in the system's temporary folder and returns its path *(i.e. `fs::tempFile('.json')`)* |

## Copying, moving and removing

| Function | Returns | Description |
|----------|---------|-------------|
| `copy(string $from, string $to)` | `void` | Copies a file, replacing `$to` if it exists. A folder is copied with everything inside it |
| `move(string $from, string $to)` | `void` | Moves or renames a file or a folder, replacing a file at `$to` |
| `remove(string $path)` | `void` | Removes a file, a link or an empty folder. Does nothing if it doesn't exist |
| `removeAll(string $path)` | `void` | Removes a folder with everything inside it. Does nothing if it doesn't exist |
| `setMode(string $path, number $mode)` | `void` | Changes the Unix permissions *(i.e. `fs::setMode($script, 0o755)`)*. On Windows it only changes whether the file is read-only |
| `link(string $target, string $path)` | `void` | Creates a symbolic link at `$path` pointing to `$target` |
