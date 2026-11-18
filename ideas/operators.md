# Operators

## Standard operators

These are the same operators Java has for numbers and booleans. Since `bool` and `number` can't be used as each other, every operator only accepts the types listed for it *(i.e. `true + 1` and `1 && 0` are errors)*.

### Arithmetic operators

Operands: `number`. Result: `number`.

| Operator | Name           | Example   | Result |
|----------|----------------|-----------|--------|
| `+`      | Addition       | `5 + 2`   | `7`    |
| `-`      | Subtraction    | `5 - 2`   | `3`    |
| `*`      | Multiplication | `5 * 2`   | `10`   |
| `/`      | Division       | `5 / 2`   | `2.5`  |
| `%`      | Remainder      | `5 % 2`   | `1`    |
| `+`      | Unary plus     | `+$x`     | `$x`   |
| `-`      | Unary minus    | `-$x`     | `-$x`  |
| `**`     | Power          | `2 ** 10` | `1024` |

- The result of `%` has the sign of the left side, like in Java: `-5 % 3` is `-2` and `5 % -3` is `2`.
- `**` groups from the right: `2 ** 3 ** 2` is `2 ** 9`, which is `512`.
- `**` binds tighter than a `-` on its left, like in Python: `-2 ** 2` is `-(2 ** 2)`, which is `-4`. A `-` on its right works as expected: `2 ** -1` is `0.5`.
- Two integers give an integer, unless the exponent is negative *(i.e. `2 ** -1` is a float)*.

**NOTE**: Since there is only one `number` type, `/` can't truncate like Java's integer division (`5 / 2` would be `2` in Java). Truncating division should be a method instead *(i.e. `5.div(2)` → `2`)*.

#### Division by zero

`/` always gives a float, so it follows Java's rules for `double`. `%` and `.div()` always throw, so the result never depends on how the number is stored *(see Number in [builtin_types.md](builtin_types.md))*:

| Expression    | Result |
|---------------|--------|
| `1 / 0`       | `Infinity` |
| `-1 / 0`      | `-Infinity` |
| `0 / 0`       | `NaN` |
| `5 % 0`       | throws an `Exception` |
| `5.5 % 0`     | throws an `Exception` |
| `5.div(0)`    | throws an `Exception` |

- `NaN` is never equal to anything, not even to itself: `NaN == NaN` is `false`. Use `$n.nan?()` to check for it.

### Increment and decrement operators

Operands: a `number` variable declared with `let` (not with `const`).

| Operator | Name           | Example | Behaviour                                         |
|----------|----------------|---------|---------------------------------------------------|
| `++`     | Pre-increment  | `++$x`  | Adds 1 to `$x`, then returns the new value        |
| `++`     | Post-increment | `$x++`  | Returns the current value of `$x`, then adds 1    |
| `--`     | Pre-decrement  | `--$x`  | Subtracts 1 from `$x`, then returns the new value |
| `--`     | Post-decrement | `$x--`  | Returns the current value of `$x`, then subtracts 1 |

### Relational operators

Operands: `number`. Result: `bool`.

| Operator | Name                  | Example  | Result  |
|----------|-----------------------|----------|---------|
| `<`      | Less than             | `1 < 2`  | `true`  |
| `>`      | Greater than          | `1 > 2`  | `false` |
| `<=`     | Less than or equal    | `2 <= 2` | `true`  |
| `>=`     | Greater than or equal | `1 >= 2` | `false` |

### Equality operators

Operands: any two values. Result: `bool`.

| Operator | Name      | Example  | Result  |
|----------|-----------|----------|---------|
| `==`     | Equal     | `1 == 1` | `true`  |
| `!=`     | Not equal | `1 != 1` | `false` |

Like in Java, `==` checks if both sides are the same object, except for the types that are compared by their contents:

| Compared by contents                                      | Compared by reference (same object) |
|-----------------------------------------------------------|-------------------------------------|
| `number`, `bool`, `symbol`, `null`, `string`, `range`     | lists, maps, functions, `regex`, `result`, `Exception` |

```
'a' + 'b' == 'ab'           // true, strings compare their contents
1..5 == 1..5                // true, ranges compare their contents
[1, 2] == [1, 2]            // false, two different lists
[1, 2].equals([1, 2])       // true, `.equals()` compares the contents
```

`!=` is always the opposite of `==`.

`==` never converts types, and there is no `===`:

- If the checker knows both types and they can never be equal, it's an error before the script runs *(i.e. `1 == '1'`)*.
- If one side is `any`, values of different types are never equal, so the result is `false`.
- To compare values of different types, convert one of them first *(i.e. `$input.toNumber() == 1`)*.
- Integers and floats are both `number`, so `1 == 1.0` is `true`.

### Bitwise operators

Operands: `number`, which must be a whole number *(i.e. `1.5 & 1` is an error)*. They work on 64 bits. Result: `number`.

| Operator | Name                 | Example     | Result |
|----------|----------------------|-------------|--------|
| `&`      | Bitwise AND          | `6 & 3`     | `2`    |
| `\|`     | Bitwise OR           | `6 \| 3`    | `7`    |
| `^`      | Bitwise XOR          | `6 ^ 3`     | `5`    |
| `~`      | Bitwise NOT          | `~6`        | `-7`   |
| `<<`     | Left shift           | `1 << 3`    | `8`    |
| `>>`     | Right shift (signed) | `-16 >> 2`  | `-4`   |
| `>>>`    | Right shift (unsigned) | `-16 >>> 60` | `15` |

**NOTE**: `<<` is also the push operator for collections *(i.e. `$list << 4`)*. The left operand decides which one it is: a number shifts, a collection pushes.

### Boolean (logical) operators

Operands: `bool`. Result: `bool`.

| Operator | Name                       | Example          | Result  |
|----------|----------------------------|------------------|---------|
| `&&`     | AND (short-circuit)        | `true && false`  | `false` |
| `\|\|`   | OR (short-circuit)         | `true \|\| false`| `true`  |
| `!`      | NOT                        | `!true`          | `false` |
| `&`      | AND (always evaluates both)| `true & false`   | `false` |
| `\|`     | OR (always evaluates both) | `true \| false`  | `true`  |
| `^`      | XOR                        | `true ^ true`    | `false` |

- `&&` and `||` stop as soon as the result is known: in `false && check()`, `check()` is never called.
- `&` and `|` on booleans evaluate both sides, same as in Java. The operand type decides whether `&`, `|` and `^` are bitwise (`number`) or logical (`bool`).

### Ternary operator

```
let $label = $count > 1 ? "items" : "item";
```

The condition must be a `bool`.

**NOTE**: Function and method names can end with `?` (see [functions.md](functions.md)), so a `?` written right after a name and followed by `(` is part of the name. `$n.even?(1) : 2` calls a method named `even?`. Put a space before the ternary `?`: `$n.even ? (1) : 2`.

### Assignment operators

Operands: a variable declared with `let`. `$x op= $y` is the same as `$x = $x op $y`.

| Operator | Example      | Same as          |
|----------|--------------|------------------|
| `=`      | `$x = 5`     |                  |
| `+=`     | `$x += 5`    | `$x = $x + 5`    |
| `-=`     | `$x -= 5`    | `$x = $x - 5`    |
| `*=`     | `$x *= 5`    | `$x = $x * 5`    |
| `/=`     | `$x /= 5`    | `$x = $x / 5`    |
| `%=`     | `$x %= 5`    | `$x = $x % 5`    |
| `**=`    | `$x **= 2`   | `$x = $x ** 2`   |
| `&=`     | `$x &= 5`    | `$x = $x & 5`    |
| `\|=`    | `$x \|= 5`   | `$x = $x \| 5`   |
| `^=`     | `$x ^= 5`    | `$x = $x ^ 5`    |
| `<<=`    | `$x <<= 2`   | `$x = $x << 2`   |
| `>>=`    | `$x >>= 2`   | `$x = $x >> 2`   |
| `>>>=`   | `$x >>>= 2`  | `$x = $x >>> 2`  |

## Other operators

### String concatenation

`+` with two strings joins them: `'Hello ' + 'world'` is `'Hello world'`. Both sides must be strings *(see Strings in [builtin_types.md](builtin_types.md))*.

### Null operators

| Operator | Name           | Example             | Behaviour |
|----------|----------------|---------------------|-----------|
| `?.`     | Nullable call  | `$user?.name()`     | If `$user` is `null`, the whole expression is `null` and `name()` isn't called |
| `??`     | Coalesce       | `$nickname ?? $name`| The left side, or the right side if the left side is `null`. The right side is only evaluated when needed |
| `?[]`    | Nullable index | `$data?['user']?['name']` | If the left side is `null`, the whole expression is `null` and the index isn't used |

**NOTE**: Like with `?(` in method names, `?[` written without a space is the nullable index. The ternary operator needs a space before a list: `$ok ? ['a'] : ['b']`, not `$ok ?['a'] : ['b']`.

### Push

`$list << $value` adds `$value` at the end of `$list` and returns the list, so pushes can be chained: `$list << 1 << 2;`. When the left side is a `number`, `<<` is the left shift instead.

### Ranges

`$start..$end` doesn't include the end, `$start..=$end` includes it *(see Ranges in [builtin_types.md](builtin_types.md))*.

### Spread

`...$list` passes the elements of a list as separate arguments *(i.e. `greet(...$names)`)*. It can only be used in function calls.

### Test operators

Like in bash and PowerShell *(see [language.md](language.md))*. Result: `bool`.

| Operator | Operands               | Example                      | Checks |
|----------|------------------------|------------------------------|--------|
| `-f`     | path (`string`)        | `-f $path`                   | The path points to a file |
| `-d`     | path (`string`)        | `-d $path`                   | The path points to a directory |
| `-x`     | path (`string`)        | `-x $path`                   | The path points to an executable |
| `-r`     | path (`string`)        | `-r $path`                   | The path is readable |
| `-w`     | path (`string`)        | `-w $path`                   | The path is writable |
| `-like`  | `string`, `string`     | `$file -like '*.txt'`        | The string matches a wildcard pattern |
| `-match` | `string`, `regex` or `string` | `$name -match %r"^\d+$"` | The string matches a regex |
| `-in`    | any value, collection  | `'Daniel' -in $names`        | The value is in the list, is a key of the map, or is inside the range. It compares contents, like `.equals()` *(i.e. `[1, 2] -in [[1, 2]]` is `true`)* |

- The path tests (`-f`, `-d`, `-x`, `-r`, `-w`) take everything up to the comparison operators as their path, so the path can be built in place: `-f $dir + '/config.txt'` checks the joined path.
- They can be negated with `!`: `!-f $path` is `true` when the path isn't a file. `!-f $dir + '/config.txt'` negates the check of the joined path, the same as `!(-f ($dir + '/config.txt'))`.
- **Lexer rule**: a `-` directly followed by a letter is a test operator. Subtraction of a function call needs a space: `$a - foo()`, not `$a -foo()`.

## Precedence

From highest to lowest. The Java operators keep Java's order. Operators on the same row have the same precedence.

| Level | Operators                                   | Associativity |
|-------|---------------------------------------------|---------------|
| 1     | `.` `?.` `()` `[]` `?[]` `$x++` `$x--`      | Left          |
| 2     | `**`                                        | Right         |
| 3     | `++$x` `--$x` `+$x` `-$x` `~` `!`           | Right         |
| 4     | `*` `/` `%`                                 | Left          |
| 5     | `+` `-`                                     | Left          |
| 6     | `<<` `>>` `>>>`                             | Left          |
| 7     | `..` `..=`                                  | None          |
| 8     | `<` `>` `<=` `>=` `-like` `-match` `-in`, and the prefix `-f` `-d` `-x` `-r` `-w` | None |
| 9     | `==` `!=`                                   | Left          |
| 10    | `&`                                         | Left          |
| 11    | `^`                                         | Left          |
| 12    | `\|`                                        | Left          |
| 13    | `&&`                                        | Left          |
| 14    | `\|\|`                                      | Left          |
| 15    | `??`                                        | Right         |
| 16    | `? :`                                       | Right         |
| 17    | `=` `+=` `-=` `*=` `/=` `%=` `**=` `&=` `\|=` `^=` `<<=` `>>=` `>>>=` | Right |

- The right side of `**` can start with a unary operator *(i.e. `2 ** -1`)*.

- "None" means they can't be chained: `1..5..10` and `$a < $b < $c` are errors.
- The test operators are at the same level as `<`, so `$file -like '*.txt' && -f $file` works without parentheses.
