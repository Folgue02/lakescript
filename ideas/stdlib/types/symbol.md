# symbol

A fixed name, like `:active` *(see Symbols in [builtin_types.md](../../language/builtin_types.md))*.

## Converting

| Method | Returns | Description |
|--------|---------|-------------|
| `name()` | `string` | The name without the `:` *(i.e. `:active.name()` is `'active'`)* |
| `toString()` | `string` | Same as `name()` |
