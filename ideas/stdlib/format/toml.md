# std/format/toml

Reading and writing TOML, the format of configuration files like `ferry.toml`. `use std/format/toml;`, then `toml::read('config.toml')`.

*Built in phase 13 (see [roadmap.md](../../roadmap.md)).*

```
use std/format/toml;

class Server {
    pub string @host = 'localhost';
    pub number @port = 8080;
}

class Config {
    pub string @name;
    pub Server @server = Server();
}

const $config = toml::parse("name = 'site'\n\n[server]\nport = 3000\n", Config);
outln($config.server.port);   // 3000
outln(toml::stringify($config));
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `parse<T>(string $text, ?type<T> $type = null)` | `T` | The table the TOML text holds, as a `map<string, any>`. Throws a `ValueError` with the line and column if it isn't valid. With a type, the value read into it, like `json::parse()` *(see Into types in [json.md](json.md))* |
| `stringify(any $value)` | `string` | The value as TOML text. It must be a table: a map, or an object, whose fields are written like `json::stringify()` writes them |
| `read<T>(string $path, ?type<T> $type = null)` | `T` | Reads and parses a file, like `toml::parse(fs::read($path), $type)` |
| `write(string $path, any $value)` | `void` | Writes the value to a file, like `toml::stringify()` |

## Reading

| TOML | Lakescript |
|------|------------|
| table | `map<string, any>`, keeping the order of the keys |
| array | `list<any>` |
| string | `string` |
| integer, float | `number` |
| boolean | `bool` |
| date and time | `string`, as TOML writes it *(i.e. `'1979-05-27T07:32:00Z'`)*, which `std/time` can read |

- Reading into a type works like JSON's: a class gets its fields from the keys of a table, a missing key leaves the default of its field, and a value of the wrong type is a `TypeError` that says where *(i.e. `` `$.server.port` is a string, not a number ``)*.

## Writing

| Lakescript | TOML |
|------------|------|
| `map`, object | table. An object gets every field, and a value of an enum its name |
| `list`, `range` | array |
| `string`, `number`, `bool` | the same |

- TOML has no `null`, so a `null` is a `TypeError` that says where it is *(i.e. `` `$.server.host` is `null`, which TOML can't have ``)*. A field that can be `null` is left out by giving the map without it.
- Tables inside tables are written as their own sections *(i.e. `[server]`)*.
