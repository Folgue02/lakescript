# std/bytes

Bytes from the ways text writes them: hex and base64. `use std/bytes;`, then `bytes::fromHex('ff00')`. The type itself, and its methods, are in [types/bytes.md](types/bytes.md).

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `fromHex(string $text)` | `bytes` | Bytes written in hex, in any case, with or without spaces *(i.e. `bytes::fromHex('89 50 4E 47')`)*. Throws a `ValueError` if it isn't hex |
| `fromBase64(string $text)` | `bytes` | Bytes written in base64, standard or for URLs *(with `-` and `_`)*, with or without the `=` at the end. Line breaks are ignored. Throws a `ValueError` if it isn't base64 |

- `$b.hex()` and `$b.base64()` write them back.

## How it's made

- In Rust, in `crates/lakescript/src/stdlib/bytes.rs`, with the hex and base64 of `crates/lakescript/src/bytes.rs`: small enough that a crate isn't needed.
