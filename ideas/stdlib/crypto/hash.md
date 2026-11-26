# std/crypto/hash

Hashes and HMAC. `use std/crypto/hash;`, then `hash::sha256($text)`.

*Built in phase 13 (see [roadmap.md](../../roadmap.md)). All of `std/crypto` is in pure Rust, on the RustCrypto crates, and is the `crypto` feature of the crate.*

```
use std/crypto/hash;

outln(hash::sha256('hello'));               // 2cf24dba5fb0a30e26e83b2ac5b9e29e...
outln(hash::file('release.tar.gz') == $expected);

// A webhook: the sender signs the body with a secret both know
if (!hash::verifyHmac($secret, $body, $headers['X-Signature'])) exit(1);
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `sha256(any $data)` | `string` | The SHA-256 of a string *(as UTF-8)* or of `bytes`, in lowercase hexadecimal *(i.e. `hash::sha256('')` is `'e3b0c442...'`)*. `bytes::fromHex()` gives its bytes |
| `sha512(any $data)` | `string` | The SHA-512, in hexadecimal |
| `sha1(any $data)` | `string` | The SHA-1, in hexadecimal. Only to check what's already given with it *(i.e. git's commits)*: it isn't safe against someone who makes two things with the same one |
| `md5(any $data)` | `string` | The MD5, in hexadecimal. Like SHA-1, only to check what's already given with it |
| `crc32(any $data)` | `string` | The CRC-32, 8 hexadecimal digits *(i.e. `'cbf43926'` for `'123456789'`)*. A checksum, not a hash for security |
| `file(string $path, string $algorithm = 'sha256')` | `string` | The hash of a file, read in pieces so a big one never has to fit in memory. `$algorithm` is `'sha256'`, `'sha512'`, `'sha1'`, `'md5'` or `'crc32'` |
| `hmac(any $key, any $data, string $algorithm = 'sha256')` | `string` | The HMAC of `$data` with `$key`, in hexadecimal, with `'sha256'` or `'sha512'`, to sign requests and webhooks |
| `verifyHmac(any $key, any $data, string $signature, string $algorithm = 'sha256')` | `bool` | Whether `$signature` is the HMAC of `$data` with `$key`. It takes it in hexadecimal, in any case, and with a `sha256=` before it, the way GitHub sends it. It's compared in the same time whatever matches, so the time it takes says nothing about the right one |

- A hash is text, to print and to compare with the one given with a file. It's never `bytes`, since the type of what a function gives can't depend on an argument *(the `raw: true` that was the idea)*: `bytes::fromHex($hash)` gives them.
