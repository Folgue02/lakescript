# regex

A regular expression, written `%r"..."` *(see Regex in [builtin_types.md](../../language/builtin_types.md))*. Matching is done with `-match` and with the string methods `match()`, `matchAll()`, `replaceMatches()`, `replaceFirstMatch()`, `replaceLastMatch()`, `removeMatches()` and `splitMatches()` *(see [string.md](string.md))*.

## Information

| Method | Returns | Description |
|--------|---------|-------------|
| `pattern()` | `string` | The pattern as it was written *(i.e. `%r"^\d+$"i.pattern()` is `'^\d+$'`)* |
| `flags()` | `string` | The flags *(i.e. `%r"^\d+$"i.flags()` is `'i'`)* |
| `toString()` | `string` | The regex as it's written in code *(i.e. `'%r"^\d+$"i'`)* |
