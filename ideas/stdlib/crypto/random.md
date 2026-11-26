# std/crypto/random

Random values for secrets: tokens, keys, passwords. They come from the system's own secure source, unlike `std/random`, whose numbers can be predicted and repeated with a seed. `use std/crypto/random;`, then `random::token()`.

*Built in phase 13 (see [roadmap.md](../../roadmap.md)).*

```
use std/crypto/random;

const $token = random::token();         // 'kE3x...', for a URL or a header
const $code = random::between(100000, 999999);
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `token(number $bytes = 32)` | `string` | That many random bytes in base64 for URLs, without `=` *(i.e. 43 characters for 32 bytes)*, so it can go in a URL, a header or a file as it is |
| `hex(number $bytes = 16)` | `string` | That many random bytes in hexadecimal *(i.e. 32 digits for 16 bytes)* |
| `bytes(number $length)` | `bytes` | That many random bytes, from 1 to 1024 |
| `between(number $min, number $max)` | `number` | A whole number from `$min` to `$max`, both included, all of them as likely |

- `std/crypto/random` and `std/random` are both `random` after `use`, so a script that needs both gives one another name *(i.e. `use std/crypto/random as secure;`)*.
