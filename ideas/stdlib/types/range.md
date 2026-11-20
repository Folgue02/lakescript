# range

Whole numbers from a start to an end, like `1..5` or `1..=5` *(see Ranges in [builtin_types.md](../../language/builtin_types.md))*. Ranges never change: every method returns a new value.

## Information

| Method | Returns | Description |
|--------|---------|-------------|
| `start()` | `number` | *(i.e. `(1..5).start()` is `1`)* |
| `end()` | `number` | *(i.e. `(1..5).end()` is `5`, even though 5 isn't included)* |
| `inclusive?()` | `bool` | `true` if it was written with `..=` |
| `length()` | `number` | How many numbers it has *(i.e. `(1..5).length()` is `4`, `(0..10).step(5).length()` is `2`)* |
| `empty?()` | `bool` | `true` if it has no numbers *(i.e. `(3..3).empty?()`)* |
| `contains(number $n)` | `bool` | `true` if `$n` is one of its numbers. Same as `-in` |
| `first()` | `?number` | The first number, or `null` if it's empty |
| `last()` | `?number` | The last number, or `null` if it's empty *(i.e. `(1..5).last()` is `4`)* |

## Changing

| Method | Returns | Description |
|--------|---------|-------------|
| `step(number $step)` | `range` | A range that skips by `$step`. The step is always positive *(i.e. `(1..10).step(2)` is 1, 3, 5, 7, 9)* |
| `reverse()` | `range` | The same numbers in the other direction *(i.e. `(1..=5).reverse()` is `5..=1`)* |

## Using the numbers

| Method | Returns | Description |
|--------|---------|-------------|
| `toList()` | `list<number>` | *(i.e. `(1..=3).toList()` is `[1, 2, 3]`)* |
| `map((number) -> U $fn)` | `list<U>` | `$fn` applied to every number *(i.e. `(1..=3).map(($n) -> $n * $n)` is `[1, 4, 9]`)* |
| `filter((number) -> bool $fn)` | `list<number>` | The numbers `$fn` returns `true` for |
| `sum()` | `number` | The sum of its numbers |
| `random()` | `?number` | A random number from it, or `null` if it's empty *(i.e. `(1..=6).random()`)* |
| `each((number) -> void $fn)` | `void` | Calls `$fn` with every number. Usually a `for` loop reads better |
