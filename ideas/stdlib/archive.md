# std/archive

Archives: zip, tar and tar.gz, read and written the same way. `use std/archive;`, then `archive::open('site.zip')` or `archive::create('backup.tar.gz')`. The format comes from the name of the file.

```
use std/archive;

const $zip = archive::open('site.zip');
for ($entry in $zip.entries()) {
    outln(f"{$entry.name}  {$entry.size} bytes");
}
outln($zip.read('docs/notes.txt'));
$zip.extract('out');

const $backup = archive::create('backup.tar.gz');
$backup.add('photos/', as: 'pictures');
$backup.addText('README.txt', 'My photos');
$backup.close();
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `open(string $path)` | `Archive` | An archive to read. Throws a `FileNotFound` if it doesn't exist, an `IOError` if it can't be read as its format, and a `ValueError` if its name doesn't say a format |
| `create(string $path)` | `Archive` | A new archive, written by `close()` with what was added to it, over a file that may be there. Throws a `ValueError` if its name doesn't say a format |

- The formats, by the end of the name, in any case: `.zip`, `.tar`, and `.tar.gz` or `.tgz` for a tar compressed with gzip.

## Archive

From `open()` to read it, or from `create()` to write a new one.

| Field | Type | Description |
|-------|------|-------------|
| `path` | `string` | The path of its file, as it was given |

| Method | Returns | Description |
|--------|---------|-------------|
| `entries()` | `list<Entry>` | Its files and folders, in the order they're in it |
| `read(string $name)` | `string` | The text of a file in it *(i.e. `$zip.read('docs/notes.txt')`)*. Throws a `FileNotFound` if it isn't there, and a `ValueError` if it isn't text: `readBytes()` reads any file |
| `readBytes(string $name)` | `bytes` | The bytes of a file in it, for what isn't text *(i.e. an image, see [bytes](types/bytes.md))*. Throws a `FileNotFound` if it isn't there |
| `extract(string $dir = '.')` | `void` | Writes everything in it into a folder, which is created if needed. Throws a `ValueError` for an entry that would be written outside it *(i.e. `../x` or `/etc/x`)*, before writing that entry |
| `add(string $path, ?string $as = null)` | `void` | Adds a file, or a folder with everything in it, under the name `$as`, or its own name *(i.e. `photos` for `/home/ana/photos/`)*. `.` adds what's inside the current folder. Throws a `FileNotFound` if it doesn't exist |
| `addText(string $name, string $text)` | `void` | Adds a file with that text |
| `addBytes(string $name, bytes $bytes)` | `void` | Adds a file with those bytes |
| `close()` | `void` | Writes a new archive with what was added, in that order. An archive that's read needs no `close()` |

- Reading methods on an archive from `create()`, adding to one from `open()`, or adding after `close()`, throw a `ValueError` that says which to use.
- Names use `/` and have no `/` at the end, also for folders *(i.e. `pictures/2026`)*. A name with `..` can't be written, so archives made here can always be extracted.
- Folders are added with what's in them sorted, so the same files give the same archive.
- Files keep when they were changed and, on Linux and macOS, their permissions, so scripts in an archive stay runnable.

## Entry

A file or a folder in an archive, from `entries()`.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Its path inside the archive *(i.e. `docs/notes.txt`)* |
| `size` | `number` | Its size in bytes, uncompressed. 0 for a folder |
| `modified` | `?Date` | When it was last changed, as a date in the local zone *(see [time.md](time.md))*, or `null` when the archive doesn't say. Zip archives keep it without a zone, in the local one, like `zip` and `unzip` do |

| Method | Returns | Description |
|--------|---------|-------------|
| `dir?()` | `bool` | `true` for a folder |

## How it's made

- Written in Lakescript, in `crates/lakescript/src/stdlib/archive.lks`, with its Rust part, `std/archive/native`, in `archive.rs` *(see How modules are made in [README.md](README.md))*.
- On the crates `zip` (MIT), `tar` and `flate2` (MIT or Apache-2.0). It's the feature `archive` of the crate, which needs `dates` for the dates of the entries.
- An `Archive` only keeps its path: each method opens the file again, and a new one keeps what's added until `close()` writes it all. So nothing stays open, and a script that forgets `close()` leaves no half-written file.
- One API for every format, with objects, rather than plain functions *(i.e. `archive::extract($file, $dir)`)*: an open archive can do several things, and adding files in a loop is natural.

## Backlog

- More formats *(i.e. `.tar.xz`, `.tar.zst`)*, if scripts need them.
