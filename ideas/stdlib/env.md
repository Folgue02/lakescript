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
