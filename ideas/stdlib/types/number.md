# number

Any number, whole or with decimals *(see Number in [builtin_types.md](../../language/builtin_types.md))*. Numbers never change: every method returns a new value.

## Information

| Method | Returns | Description |
|--------|---------|-------------|
| `even?()` | `bool` | `true` for whole numbers divisible by 2 *(i.e. `4.even?()`)* |
| `odd?()` | `bool` | `true` for whole numbers not divisible by 2 |
| `whole?()` | `bool` | `true` if it has no decimals *(i.e. `4.0.whole?()` is `true`, `4.5.whole?()` is `false`)* |
| `positive?()` | `bool` | `true` if it's bigger than 0 |
| `negative?()` | `bool` | `true` if it's smaller than 0 |
| `zero?()` | `bool` | `true` if it's 0 |
| `nan?()` | `bool` | `true` if it's `NaN` *(i.e. `(0 / 0).nan?()`)* |
| `infinite?()` | `bool` | `true` if it's `Infinity` or `-Infinity` |
| `finite?()` | `bool` | `true` if it's neither `NaN` nor infinite |
| `between(number $min, number $max)` | `bool` | `true` if it's from `$min` to `$max`, both included *(i.e. `5.between(1, 10)`)* |

## Rounding

| Method | Returns | Description |
|--------|---------|-------------|
| `round(number $decimals = 0)` | `number` | Rounds to the nearest, halves away from zero *(i.e. `2.5.round()` is `3`, `3.14159.round(2)` is `3.14`)* |
| `floor()` | `number` | Rounds down *(i.e. `2.7.floor()` is `2`, `(-2.1).floor()` is `-3`)* |
| `ceil()` | `number` | Rounds up *(i.e. `2.1.ceil()` is `3`)* |
| `truncate()` | `number` | Removes the decimals *(i.e. `(-2.7).truncate()` is `-2`)* |

## Math

| Method | Returns | Description |
|--------|---------|-------------|
| `abs()` | `number` | The value without its sign *(i.e. `(-5).abs()` is `5`)* |
| `sign()` | `number` | `-1`, `0` or `1` |
| `min(number $other)` | `number` | The smaller of the two *(i.e. `5.min(3)` is `3`)* |
| `max(number $other)` | `number` | The bigger of the two |
| `clamp(number $min, number $max)` | `number` | Keeps it between `$min` and `$max` *(i.e. `150.clamp(0, 100)` is `100`)* |
| `div(number $by)` | `number` | Division without decimals, truncated like Java's *(i.e. `7.div(2)` is `3`)*. Throws an `ArithmeticError` if `$by` is 0 |
| `mod(number $by)` | `number` | Remainder with the sign of `$by`, unlike `%` *(i.e. `(-1).mod(5)` is `4`, `-1 % 5` is `-1`)*. Useful for wrapping around. Throws an `ArithmeticError` if `$by` is 0 |
| `sqrt()` | `number` | Square root. Negative numbers give `NaN` |
| `log(?number $base = null)` | `number` | Logarithm, natural without a `$base` *(i.e. `100.log(10)` is `2`)* |

## Formatting

| Method | Returns | Description |
|--------|---------|-------------|
| `toString(number $base = 10)` | `string` | *(i.e. `255.toString()` is `'255'`, `255.toString(16)` is `'ff'`, `5.toString(2)` is `'101'`)*. Bases other than 10 only work with whole numbers |
| `toFixed(number $decimals)` | `string` | Always shows `$decimals` decimals *(i.e. `2.5.toFixed(2)` is `'2.50'`)* |
| `withSeparators(string $separator = ',')` | `string` | Groups thousands *(i.e. `1234567.withSeparators()` is `'1,234,567'`)* |
| `formatBytes()` | `string` | A size in bytes, readable *(i.e. `1536.formatBytes()` is `'1.5 KB'`, `3_000_000_000.formatBytes()` is `'2.8 GB'`)* |
| `toChar()` | `string` | The character with this Unicode code *(i.e. `65.toChar()` is `'A'`)* |

## Repeating

| Method | Returns | Description |
|--------|---------|-------------|
| `times((number) -> void $fn)` | `void` | Calls `$fn` this many times, with the count from 0 *(i.e. `3.times(($i) = outln($i))`)* |
