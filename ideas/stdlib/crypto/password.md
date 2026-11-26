# std/crypto/password

Keeping passwords so they can be checked, but never read back. `use std/crypto/password;`, then `password::hash($password)`.

*Built in phase 13 (see [roadmap.md](../../roadmap.md)).*

```
use std/crypto/password;

const $stored = password::hash('correct horse');   // '$argon2id$v=19$m=19456,t=2,p=1$...'
outln(password::verify('correct horse', $stored));  // true
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `hash(string $password)` | `string` | The password hashed with Argon2id and a new random salt, as one string that also says how it was made, to keep in a file or a database. The same password gives a different string each time |
| `verify(string $password, string $hash)` | `bool` | Whether the password is the one of a string from `hash()`. Throws a `ValueError` if `$hash` isn't one |

- Argon2id with the settings that RFC 9106 and OWASP recommend, which take some milliseconds on purpose, so trying many passwords is slow.
