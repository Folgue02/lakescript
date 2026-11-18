# any

Methods that every value has, whatever its type.

## Types

| Method | Returns | Description |
|--------|---------|-------------|
| `as(type)` | the type | Tells the checker the value has that type, and throws a `TypeError` while running if it doesn't *(i.e. `$data['name'].as(string)`)*. See [builtin_types.md](../../language/builtin_types.md) |
| `is(type)` | `bool` | `true` if the value has that type. Inside an `if` that uses it, the value is treated as that type *(i.e. `$value.is(string)`)* |
| `typeName()` | `string` | The name of the value's type, for messages *(i.e. `5.typeName()` is `'number'`, `[1].typeName()` is `'list'`)* |

## Comparing and printing

| Method | Returns | Description |
|--------|---------|-------------|
| `equals(any $other)` | `bool` | Compares the contents, also for lists and maps *(i.e. `[1, 2].equals([1, 2])` is `true`)* |
| `toString()` | `string` | The value as text, the same that `outln` prints *(i.e. `[1, 2].toString()` is `'[1, 2]'`)* |
