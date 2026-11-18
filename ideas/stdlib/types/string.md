# string

Text, made of characters. Strings never change: every method returns a new string *(see Strings in [builtin_types.md](../../language/builtin_types.md))*. Indexes count characters and start at 0.

## Information

| Method | Returns | Description |
|--------|---------|-------------|
| `length()` | `number` | The number of characters *(i.e. `'año'.length()` is `3`)* |
| `empty?()` | `bool` | `true` if it has no characters *(i.e. `''.empty?()`)* |
| `blank?()` | `bool` | `true` if it's empty or only has whitespace *(i.e. `'  \n'.blank?()`)* |
| `contains(string $text)` | `bool` | `true` if `$text` appears anywhere *(i.e. `'backup.tar.gz'.contains('.tar')`)* |
| `startsWith(string $prefix)` | `bool` | `true` if it starts with `$prefix` *(i.e. `'/usr/bin'.startsWith('/usr')`)* |
| `endsWith(string $suffix)` | `bool` | `true` if it ends with `$suffix` *(i.e. `'notes.txt'.endsWith('.txt')`)* |
| `hasPrefix(string $prefix)` | `bool` | Same as `startsWith` |
| `hasSuffix(string $suffix)` | `bool` | Same as `endsWith` |
| `count(string $text)` | `number` | How many times `$text` appears, without overlapping *(i.e. `'banana'.count('an')` is `2`)* |
| `equalsIgnoreCase(string $other)` | `bool` | Compares ignoring upper and lower case *(i.e. `'YES'.equalsIgnoreCase('yes')`)* |
| `upper?()` | `bool` | `true` if it has letters and all of them are upper case *(i.e. `'ABC1'.upper?()`)* |
| `lower?()` | `bool` | `true` if it has letters and all of them are lower case |
| `numeric?()` | `bool` | `true` if `toNumber()` would work on it *(i.e. `'-2.5'.numeric?()`, `'1e6'.numeric?()`)* |
| `digits?()` | `bool` | `true` if it's not empty and only has the digits 0 to 9 *(i.e. `'0042'.digits?()`)* |
| `alpha?()` | `bool` | `true` if it's not empty and only has letters |
| `alphanumeric?()` | `bool` | `true` if it's not empty and only has letters and digits |

## Searching

All the `indexOf` methods return `null` when the text isn't found.

| Method | Returns | Description |
|--------|---------|-------------|
| `indexOf(string $text, number $from = 0)` | `?number` | Position of the first `$text`, starting the search at `$from` *(i.e. `'a-b-c'.indexOf('-')` is `1`)* |
| `indexOfFirst(string $text)` | `?number` | Same as `indexOf` with no `$from` |
| `indexOfLast(string $text)` | `?number` | Position of the last `$text` *(i.e. `'a-b-c'.indexOfLast('-')` is `3`)* |
| `indexesOf(string $text)` | `list<number>` | Positions of every `$text` *(i.e. `'a-b-c'.indexesOf('-')` is `[1, 3]`)* |
| `before(string $text)` | `?string` | The part before the first `$text` *(i.e. `'key=value'.before('=')` is `'key'`)* |
| `after(string $text)` | `?string` | The part after the first `$text` *(i.e. `'key=value'.after('=')` is `'value'`)* |
| `beforeLast(string $text)` | `?string` | The part before the last `$text` *(i.e. `'archive.tar.gz'.beforeLast('.')` is `'archive.tar'`)* |
| `afterLast(string $text)` | `?string` | The part after the last `$text` *(i.e. `'/home/me/notes.txt'.afterLast('/')` is `'notes.txt'`)* |
| `match(regex $pattern)` | `?list<string>` | The first match: the whole match, then each group. `null` if it doesn't match *(i.e. `'v1.2'.match(%r"(\d+)\.(\d+)")` is `['1.2', '1', '2']`)* |
| `matchAll(regex $pattern)` | `list<list<string>>` | Every match, each one like `match()` *(i.e. `'a1 b2'.matchAll(%r"\w(\d)")` is `[['a1', '1'], ['b2', '2']]`)* |

## Case

| Method | Returns | Description |
|--------|---------|-------------|
| `upper()` | `string` | All letters in upper case *(i.e. `'hello'.upper()` is `'HELLO'`)* |
| `lower()` | `string` | All letters in lower case |
| `capitalize()` | `string` | The first character in upper case, the rest as it was *(i.e. `'hello world'.capitalize()` is `'Hello world'`)* |
| `capitalizeWords()` | `string` | The first letter of every word in upper case *(i.e. `'hello world'.capitalizeWords()` is `'Hello World'`)* |
| `uncapitalize()` | `string` | The first character in lower case *(i.e. `'Hello'.uncapitalize()` is `'hello'`)* |
| `swapCase()` | `string` | Upper case letters become lower case and the other way around |
| `camelCase()` | `string` | *(i.e. `'user name'.camelCase()` and `'user_name'.camelCase()` are `'userName'`)* |
| `pascalCase()` | `string` | *(i.e. `'user name'.pascalCase()` is `'UserName'`)* |
| `snakeCase()` | `string` | *(i.e. `'userName'.snakeCase()` is `'user_name'`)* |
| `kebabCase()` | `string` | *(i.e. `'userName'.kebabCase()` is `'user-name'`)* |
| `constantCase()` | `string` | *(i.e. `'userName'.constantCase()` is `'USER_NAME'`)* |

## Trimming and padding

| Method | Returns | Description |
|--------|---------|-------------|
| `trim(string $characters = whitespace)` | `string` | Removes the whitespace, or the given characters, from both ends *(i.e. `'  hi \n'.trim()` is `'hi'`, `'--hi--'.trim('-')` is `'hi'`)* |
| `trimStart(string $characters = whitespace)` | `string` | Same as `trim`, only at the start |
| `trimEnd(string $characters = whitespace)` | `string` | Same as `trim`, only at the end |
| `removePrefix(string $prefix)` | `string` | Removes `$prefix` once if it starts with it, otherwise returns it as it was *(i.e. `'v1.2'.removePrefix('v')` is `'1.2'`)* |
| `removeSuffix(string $suffix)` | `string` | Removes `$suffix` once if it ends with it *(i.e. `'web.log'.removeSuffix('.log')` is `'web'`)* |
| `padStart(number $length, string $fill = ' ')` | `string` | Adds `$fill` at the start until it's `$length` characters long *(i.e. `'7'.padStart(3, '0')` is `'007'`)* |
| `padEnd(number $length, string $fill = ' ')` | `string` | Adds `$fill` at the end until it's `$length` characters long, useful for columns |
| `center(number $length, string $fill = ' ')` | `string` | Adds `$fill` on both sides until it's `$length` characters long *(i.e. `'hi'.center(6, '*')` is `'**hi**'`)* |
| `truncate(number $length, string $ending = '...')` | `string` | Cuts it to `$length` characters, `$ending` included, if it's longer *(i.e. `'Hello world'.truncate(8)` is `'Hello...'`)* |

## Changing

| Method | Returns | Description |
|--------|---------|-------------|
| `replace(string \| regex $find, string $with)` | `string` | Replaces every `$find` *(i.e. `'a-b-c'.replace('-', '+')` is `'a+b+c'`)*. With a regex, `$1`, `$2`... in `$with` are the groups |
| `replaceFirst(string \| regex $find, string $with)` | `string` | Replaces only the first `$find` |
| `replaceLast(string \| regex $find, string $with)` | `string` | Replaces only the last `$find` |
| `remove(string \| regex $find)` | `string` | Removes every `$find` *(i.e. `'1,000,000'.remove(',')` is `'1000000'`)* |
| `insert(number $index, string $text)` | `string` | Inserts `$text` at `$index` *(i.e. `'helo'.insert(3, 'l')` is `'hello'`)*. Throws an `IndexError` if `$index` is past the end |
| `repeat(number $times)` | `string` | *(i.e. `'-'.repeat(20)`)* |
| `reverse()` | `string` | The characters in reverse order *(i.e. `'abc'.reverse()` is `'cba'`)* |
| `collapseWhitespace()` | `string` | Trims it and turns every run of whitespace into one space *(i.e. `' a   b\n c '.collapseWhitespace()` is `'a b c'`)* |
| `indent(number $spaces = 4)` | `string` | Adds `$spaces` spaces at the start of every line that isn't empty |
| `dedent()` | `string` | Removes the indentation all the lines share, like multi-line strings do |
| `wrap(number $width)` | `string` | Breaks lines between words so none is longer than `$width`, when possible |

## Splitting

| Method | Returns | Description |
|--------|---------|-------------|
| `split(string \| regex $separator, ?number $limit = null)` | `list<string>` | Splits at every `$separator`. With `$limit`, makes at most that many parts *(i.e. `'a,b,c'.split(',')` is `['a', 'b', 'c']`, `'a,b,c'.split(',', 2)` is `['a', 'b,c']`)* |
| `splitOnce(string $separator)` | `?list<string>` | The part before and the part after the first `$separator`, or `null` if it's not there *(i.e. `'key=a=b'.splitOnce('=')` is `['key', 'a=b']`)* |
| `lines()` | `list<string>` | Splits at new lines, `\n` or `\r\n`, without them. A last empty line is ignored *(i.e. `"a\nb\n".lines()` is `['a', 'b']`)* |
| `words()` | `list<string>` | Splits at any whitespace, ignoring empty parts *(i.e. `' a  b\tc '.words()` is `['a', 'b', 'c']`)* |
| `chars()` | `list<string>` | Every character as a one-character string *(i.e. `'abc'.chars()` is `['a', 'b', 'c']`)* |

## Converting

| Method | Returns | Description |
|--------|---------|-------------|
| `toNumber()` | `number` | Reads a number in any of the literal forms *(i.e. `'42'`, `'-2.5'`, `'0xFF'`, `'1_000'`)*. Whitespace around it is ignored. Throws a `ValueError` otherwise |
| `toBool()` | `bool` | `'true'` and `'false'`, ignoring case. Throws a `ValueError` otherwise |
| `toSymbol()` | `symbol` | *(i.e. `'active'.toSymbol()` is `:active`)*. Throws a `ValueError` if it isn't a valid name |
| `codePoint(number $index = 0)` | `number` | The Unicode code of the character at `$index` *(i.e. `'A'.codePoint()` is `65`)* |
| `escapeRegex()` | `string` | Escapes the characters that mean something in a regex, so it can be matched literally *(i.e. `'1+1'.escapeRegex()` is `'1\+1'`)* |
| `toString()` | `string` | The same string |
