# std/format/csv

Reading and writing CSV, the format of spreadsheets. `use std/format/csv;`, then `csv::parse($text)`.

```
use std/format/csv;

// Every row as a list of strings
for ($row in csv::read('scores.csv')) {
    outln($row.join(' | '));
}

// The first line names the columns, and the rows are maps by those names
const $people = csv::readMaps('people.csv');
outln($people.map(($p) -> $p['name']).join(', '));

csv::write('adults.csv', $people.filter(($p) -> ($p['age']?.toNumber() ?? 0) >= 18));
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `parse(string $text, string $separator = ',')` | `list<list<string>>` | The rows of the text, every value a string *(i.e. `csv::parse("a,b\n1,2")` is `[['a', 'b'], ['1', '2']]`)* |
| `parseMaps(string $text, string $separator = ',')` | `list<map<string, string>>` | The rows after the first one, as maps by the names in the first one *(i.e. `csv::parseMaps("name,age\nAna,30")` is `[{'name' => 'Ana', 'age' => '30'}]`)* |
| `stringify(list<any> $rows, string $separator = ',')` | `string` | The rows as CSV text. Rows are lists of values, or maps, whose keys are written in a first line |
| `read(string $path, string $separator = ',')` | `list<list<string>>` | Reads and parses a file, like `csv::parse(fs::read($path))` |
| `readMaps(string $path, string $separator = ',')` | `list<map<string, string>>` | Reads a file and parses it with `parseMaps` |
| `write(string $path, list<any> $rows, string $separator = ',')` | `void` | Writes the rows to a file, like `fs::write($path, csv::stringify($rows))` |

## Reading

- Every value is a string, even the ones that look like numbers, so nothing is guessed. `.toNumber()` turns them into numbers *(i.e. `$row['age'].toNumber()`)*.
- Values in double quotes can have the separator, new lines and quotes written twice *(i.e. `"Smith, John"` and `"say ""hi"""`)*, as in RFC 4180.
- Rows can have different numbers of values. With `parseMaps`, a row with fewer values than the first line gets `''` for the rest, and one with more throws a `ValueError`. So does a name that's twice in the first line.
- `$separator` is one character: `'\t'` reads TSV, `';'` the CSV of spreadsheets in languages that write decimals with a comma.
- Invalid CSV throws a `ValueError` with the line.

## Writing

- Values can be strings, numbers, bools and symbols, written as `outln` writes them. `null` is an empty value. Anything else throws a `TypeError`.
- Values that have the separator, new lines or quotes are put in double quotes.
- Rows that are maps write their keys in a first line, in the order of the first map. A later map without a key gets an empty value for it, and one with a key the first doesn't have throws a `ValueError`.
- Every line ends with a new line, also the last one.
