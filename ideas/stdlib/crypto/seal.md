# std/crypto/seal

Encrypting data with a key, so only who has the key can read it, and nobody can change it without it being noticed. `use std/crypto/seal;`, then `seal::seal($data, $key)`.

*Built in phase 13 (see [roadmap.md](../../roadmap.md)).*

```
use std/crypto/seal;
use std/fs;

const $key = seal::key();                       // 32 random bytes, kept secret
fs::writeBytes('notes.sealed', seal::seal('my notes', $key));
outln(seal::open(fs::readBytes('notes.sealed'), $key).text());
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `key()` | `bytes` | A new key: 32 random bytes from the system's secure source. Keep it like a password *(i.e. `$key.base64()` in a variable of the environment)* |
| `seal(any $data, bytes $key)` | `bytes` | A string *(as UTF-8)* or `bytes`, sealed with the key. Sealing the same data twice gives different bytes |
| `open(bytes $sealed, bytes $key)` | `bytes` | The data again, as `bytes` *(i.e. `.text()` for a string)*. Throws a `ValueError` if the key is another one, or the bytes were changed after they were sealed |

- **Nothing to choose, so nothing to get wrong**: the cipher is XChaCha20-Poly1305, and its nonce is a new random one each time, kept in what `seal()` gives. Scripts never pick a cipher, a mode or a nonce, which is where mistakes are made, like libsodium's secretbox.
- What `seal()` gives starts with a byte for the version of the format, then the nonce, then the sealed data, so the format can change later and still open what's sealed now.
- A key is always 32 bytes. Turning a password into a key isn't here yet.
