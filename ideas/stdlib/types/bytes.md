# bytes

Binary data: what files, HTTP bodies and archives hold when it isn't text *(i.e. an image)*. Like a string, a value that doesn't change, compared by its contents with `==` *(see Bytes in [builtin_types.md](../../language/builtin_types.md))*.

```
const $png = %b"\x89PNG\r\n\x1a\n";
const $file = fs::readBytes('logo.png');
if ($file.startsWith($png)) outln(f"A PNG of {$file.length().formatBytes()}");

const $b = 'héllo'.toBytes();      // UTF-8
outln($b);                         // <bytes 6 bytes>
outln($b[0]);                      // 104, a number from 0 to 255
outln($b[1..3].hex());             // 'c3a9'
outln($b.text());                  // 'héllo'
```

- Made with the literal `%b"..."`, where characters are written as UTF-8 and `\xFF` is one byte, with `$text.toBytes()`, `$numbers.toBytes()`, `bytes::fromHex()` and `bytes::fromBase64()` *(see [bytes.md](../bytes.md))*, and by `fs::readBytes()`, `$response.bytes()` and `$archive.readBytes()`.
- `$b[$i]` is the byte at `$i`, a number from 0 to 255, or `null` past the end. Negative indexes count from the end. `$b[1..3]` gives bytes.
- `$a + $b` joins them. `for ($n in $b)` loops over the numbers.
- They print as how many there are *(i.e. `<bytes 1.2 KB>`)*, since they aren't text: `.text()` reads them as text.
- They can't be map keys, and `std/format/json` can't write them: `.base64()` can.

## Information

| Method | Returns | Description |
|--------|---------|-------------|
| `length()` | `number` | How many bytes there are |
| `empty?()` | `bool` | `true` if there are none |
| `startsWith(bytes $prefix)` | `bool` | `true` if they start with those bytes *(i.e. the header of a file)* |
| `endsWith(bytes $suffix)` | `bool` | `true` if they end with those bytes |
| `indexOf(bytes $part)` | `?number` | Where those bytes first are, or `null` |

## Converting

| Method | Returns | Description |
|--------|---------|-------------|
| `text()` | `string` | The bytes read as UTF-8 text. Throws a `ValueError` that says where if they aren't valid UTF-8 |
| `hex()` | `string` | In hex, two lowercase digits per byte *(i.e. `'89504e47'`)* |
| `base64()` | `string` | In standard base64, with `=` at the end, like the `base64` command and most APIs |
| `toList()` | `list<number>` | The bytes as numbers from 0 to 255 |
