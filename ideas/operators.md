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

**NOTE**: Since there is only one `number` type, `/` can't truncate like Java's integer division (`5 / 2` would be `2` in Java). Truncating division should be a method instead *(i.e. `5.div(2)` → `2`)*.

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
| `number`, `bool`, `symbol`, `null`, `string`, `range`     | lists, maps, functions              |

```
'a' + 'b' == 'ab'           // true, strings compare their contents
1..5 == 1..5                // true, ranges compare their contents
[1, 2] == [1, 2]            // false, two different lists
[1, 2].equals([1, 2])       // true, `.equals()` compares the contents
```

`!=` is always the opposite of `==`.

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
| `-in`    | any value, collection  | `'Daniel' -in $names`        | The value is in the list, is a key of the map, or is inside the range |

- The path tests (`-f`, `-d`, `-x`, `-r`, `-w`) take everything up to the comparison operators as their path, so the path can be built in place: `-f $dir + '/config.txt'` checks the joined path.
- **Lexer rule**: a `-` directly followed by a letter is a test operator. Subtraction of a function call needs a space: `$a - foo()`, not `$a -foo()`.

## Precedence

From highest to lowest. The Java operators keep Java's order. Operators on the same row have the same precedence.

| Level | Operators                                   | Associativity |
|-------|---------------------------------------------|---------------|
| 1     | `.` `?.` `()` `[]` `$x++` `$x--`            | Left          |
| 2     | `++$x` `--$x` `+$x` `-$x` `~` `!`           | Right         |
| 3     | `*` `/` `%`                                 | Left          |
| 4     | `+` `-`                                     | Left          |
| 5     | `<<` `>>` `>>>`                             | Left          |
| 6     | `..` `..=`                                  | None          |
| 7     | `<` `>` `<=` `>=` `-like` `-match` `-in`, and the prefix `-f` `-d` `-x` `-r` `-w` | None |
| 8     | `==` `!=`                                   | Left          |
| 9     | `&`                                         | Left          |
| 10    | `^`                                         | Left          |
| 11    | `\|`                                        | Left          |
| 12    | `&&`                                        | Left          |
| 13    | `\|\|`                                      | Left          |
| 14    | `??`                                        | Right         |
| 15    | `? :`                                       | Right         |
| 16    | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` `>>>=` | Right |

- "None" means they can't be chained: `1..5..10` and `$a < $b < $c` are errors.
- The test operators are at the same level as `<`, so `$file -like '*.txt' && -f $file` works without parentheses.
