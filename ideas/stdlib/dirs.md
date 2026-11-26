# std/dirs

The folders where a tool keeps its things, on every system: its configuration, its data, its cache. `use std/dirs;`, then `dirs::App('myapp')`, and the folders of the user like downloads.

```
use std/dirs;
use std/format/json;
use std/fs;
use std/path;

const $app = dirs::App('myapp');
const $file = path::join($app.config(), 'settings.json');
const $settings = if (fs::exists?($file)) json::read($file) else ({});

json::write(path::join($app.config(create: true), 'settings.json'), $settings);
outln(dirs::downloads() ?? 'no downloads folder');
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `config()` | `string` | The folder for configuration: `$XDG_CONFIG_HOME` or `~/.config` on Linux and macOS, `%APPDATA%` on Windows |
| `data()` | `string` | The folder for data: `$XDG_DATA_HOME` or `~/.local/share` on Linux and macOS, `%LOCALAPPDATA%` on Windows |
| `cache()` | `string` | The folder for caches: `$XDG_CACHE_HOME` or `~/.cache` on Linux and macOS, `%LOCALAPPDATA%` on Windows |
| `state()` | `string` | The folder for state, like history and logs: `$XDG_STATE_HOME` or `~/.local/state` on Linux and macOS, `%LOCALAPPDATA%` on Windows |
| `temp()` | `string` | The folder for temporary files: `$TMPDIR` or `/tmp` on Linux and macOS, `%TEMP%` on Windows |
| `runtime()` | `?string` | The folder for sockets and lock files, which is emptied when the user logs out: `$XDG_RUNTIME_DIR` *(i.e. `/run/user/1000`)*, or `null` when it isn't set, as on macOS and Windows |

- These give the path, and never create the folder. The home folder is `path::home()` *(see [path.md](path.md))*.
- Paths use `/`, like every path in Lakescript *(i.e. `C:/Users/ana/AppData/Roaming`)*.
- The variables are read as the script sees them, so `env::set('XDG_CONFIG_HOME', ...)` changes them *(i.e. in tests)*. An `XDG_` variable that isn't an absolute path is ignored, as the XDG spec says.

## The user's folders

| Function | Returns | Description |
|----------|---------|-------------|
| `desktop()` | `?string` | The desktop *(i.e. `~/Desktop`)* |
| `documents()` | `?string` | The documents *(i.e. `~/Documents`)* |
| `downloads()` | `?string` | The downloads *(i.e. `~/Downloads`)* |
| `music()` | `?string` | The music *(i.e. `~/Music`)* |
| `pictures()` | `?string` | The pictures *(i.e. `~/Pictures`)* |
| `videos()` | `?string` | The videos *(i.e. `~/Videos`, `~/Movies` on macOS)* |

- `null` when the user has no such folder.
- **Linux**: what `~/.config/user-dirs.dirs` says, the file of `xdg-user-dirs` that desktops write and translate *(i.e. `~/Descargas` in Spanish)*. A folder that isn't in it is the English name in the home folder if it exists. One set to the home folder itself is turned off, so it's `null`.
- **macOS**: the folder in the home folder, if it exists. Videos are `~/Movies`.
- **Windows**: the Known Folders of the system, so a folder moved elsewhere *(i.e. documents in OneDrive)* is found where it is.

## App

The folders of a tool, created as `dirs::App('myapp')` *(i.e. `dirs::App('myapp').config()` is `~/.config/myapp`)*. Its `new` throws a `ValueError` if the name is empty or has `/` or `\`. There's no `dirs::app()` function: the class's own `new` checks the name, so a function would only be a second way to write the same thing. Each method gives the path, and creates the folder and the ones above it with `create: true`, for when the tool is about to write in it. Reading a configuration that isn't there doesn't need the folder, so it isn't created without being asked.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | The name of the tool |

| Method | Returns | Description |
|--------|---------|-------------|
| `config(bool $create = false)` | `string` | Its configuration: `~/.config/myapp` on Linux and macOS, `%APPDATA%\myapp` on Windows |
| `data(bool $create = false)` | `string` | Its data: `~/.local/share/myapp` on Linux and macOS, `%LOCALAPPDATA%\myapp\data` on Windows |
| `cache(bool $create = false)` | `string` | Its cache, which can be deleted: `~/.cache/myapp` on Linux and macOS, `%LOCALAPPDATA%\myapp\cache` on Windows |
| `state(bool $create = false)` | `string` | Its state, which is kept but isn't worth copying to another machine *(i.e. history, logs)*: `~/.local/state/myapp` on Linux and macOS, `%LOCALAPPDATA%\myapp\state` on Windows |

- macOS uses the folders of Linux, not `~/Library/Application Support`, like most command-line tools *(i.e. git, gh, nvim, fish)*: the same paths on both, and easy to find and edit by hand.
- On Windows the configuration is in `%APPDATA%` (Roaming), which follows the user to the other machines of a domain, and the rest is in `%LOCALAPPDATA%`, which stays on the machine, so nothing big is copied around. It's what Microsoft says each one is for, and what gh and Node's env-paths do.

## How it's made

- `dirs.lks` has `App` and the functions, over `std/dirs/native` *(`stdlib/dirs.rs`)*, which finds the folders from the variables of the script.
- On Windows, the user's folders come from `SHGetKnownFolderPath`, through the `dirs` crate. It's only a dependency on Windows, so it isn't a feature.
