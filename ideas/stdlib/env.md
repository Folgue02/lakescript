# std/env

Environment variables. `use std/env;`, then `env::get('HOME')`.

Changes only affect the script and the commands it runs, never the shell that started it.

| Function | Returns | Description |
|----------|---------|-------------|
| `get(string $name)` | `?string` | The value, or `null` if it isn't set *(i.e. `env::get('EDITOR') ?? 'vi'`)* |
| `require(string $name)` | `string` | The value. Throws a `ValueError` that names the variable if it isn't set or is empty *(i.e. `const $token = env::require('API_TOKEN');`)* |
| `has(string $name)` | `bool` | `true` if it's set, even if it's empty |
| `set(string $name, string $value)` | `void` | Sets it for the script and the commands it runs |
| `remove(string $name)` | `void` | Unsets it. Does nothing if it isn't set |
| `all()` | `map<string, string>` | Every variable, sorted by name |
| `load(string $path = '.env')` | `void` | Sets the variables of a `.env` file, but never one that's already set, like dotenv: the file only gives defaults *(i.e. `env::load();` at the start of a script)*. Throws a `FileNotFound` if it isn't there, and a `ValueError` for a line that isn't `NAME=value` |
| `parse(string $text)` | `map<string, string>` | The variables of the text of a `.env` file, in order, without setting them |

## .env files

`env::load()` and `env::parse()` read the usual format *(decided for phase 13, see [roadmap.md](../roadmap.md))*:

```
# A comment
export API_URL=https://example.com
NAME = Ana López        # after a space, `#` starts a comment
GREETING="Hello\nthere"  # in "...", \n, \t, \" and \\ work
RAW='$HOME stays as it is'
```

- A line is `NAME=value`, with spaces around them left out, and maybe `export` before it.
- Values in `'...'` are as they're written. There's no `$VAR` in values: a variable is never made from another.
