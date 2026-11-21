# list

`list<T>`: values in order, starting at index 0 *(see List in [builtin_types.md](../../language/builtin_types.md))*. Verbs change the list itself, past participles return a copy *(i.e. `sort()` and `sorted()`)*.

## Information

| Method | Returns | Description |
|--------|---------|-------------|
| `length()` | `number` | The number of elements |
| `empty?()` | `bool` | `true` if it has no elements |
| `contains(T $value)` | `bool` | `true` if `$value` is in the list, comparing contents. Same as `-in` |
| `indexOf(T $value)` | `?number` | Position of the first `$value`, or `null` *(i.e. `['a', 'b', 'a'].indexOf('a')` is `0`)* |
| `indexOfLast(T $value)` | `?number` | Position of the last `$value`, or `null` |
| `count(T $value)` | `number` | How many times `$value` is in the list |
| `countWhere((T) -> bool $fn)` | `number` | How many elements `$fn` returns `true` for *(i.e. `$ages.countWhere(($a) -> $a >= 18)`)* |

## Getting

`$list[$index]` is the element at `$index`, or `null` outside the list, so `$list[$index] ?? $default` gives a default.

| Method | Returns | Description |
|--------|---------|-------------|
| `first()` | `?T` | The first element, or `null` if the list is empty |
| `last()` | `?T` | The last element, or `null` if the list is empty |
| `random()` | `?T` | A random element, or `null` if the list is empty |

## Adding and removing

These change the list.

| Method | Returns | Description |
|--------|---------|-------------|
| `push(...T $values)` | `list<T>` | Adds the values at the end, same as `<<`. Returns the list *(i.e. `$list.push(1, 2)`)* |
| `prepend(...T $values)` | `list<T>` | Adds the values at the start. Returns the list |
| `insert(number $index, T $value)` | `list<T>` | Inserts `$value` at `$index`, moving the rest. Throws an `IndexError` if `$index` is past the end |
| `extend(list<T> $other)` | `list<T>` | Adds every element of `$other` at the end |
| `removeFirst()` | `?T` | Removes and returns the first element, or `null` if the list is empty |
| `removeLast()` | `?T` | Removes and returns the last element, or `null` if the list is empty |
| `removeAt(number $index)` | `T` | Removes and returns the element at `$index`. Throws an `IndexError` if it's outside the list |
| `remove(T $value)` | `bool` | Removes the first `$value`. `true` if it was there |
| `removeAll(T $value)` | `number` | Removes every `$value`, and returns how many there were |
| `removeWhere((T) -> bool $fn)` | `number` | Removes the elements `$fn` returns `true` for, and returns how many there were |
| `clear()` | `void` | Removes everything |

## Ordering

| Method | Returns | Description |
|--------|---------|-------------|
| `sort()` | `list<T>` | Sorts the list from smallest to biggest, using `<`. Returns the list |
| `sorted()` | `list<T>` | A sorted copy |
| `sortBy<K>((T) -> K $key)` | `list<T>` | Sorts the list by what `$key` returns *(i.e. `$files.sortBy(($f) -> $f.length())`)* |
| `sortedBy<K>((T) -> K $key)` | `list<T>` | A copy sorted by what `$key` returns |
| `sortWith((T, T) -> number $compare)` | `list<T>` | Sorts the list with a comparison function, which returns a negative number if the first value goes first, a positive one if the second does, and `0` if they're equal. For values `<` can't compare, like lists *(i.e. `$rows.sortWith(($a, $b) -> $a[0] - $b[0])`)*. The sort is stable, so equal values keep their order. Returns the list |
| `sortedWith((T, T) -> number $compare)` | `list<T>` | A copy sorted with a comparison function |
| `reverse()` | `list<T>` | Reverses the list. Returns the list |
| `reversed()` | `list<T>` | A reversed copy |
| `shuffle()` | `list<T>` | Puts the elements in random order. Returns the list |
| `shuffled()` | `list<T>` | A copy in random order |

## Transforming

These always return a new list.

| Method | Returns | Description |
|--------|---------|-------------|
| `map((T) -> U $fn)` | `list<U>` | `$fn` applied to every element *(i.e. `$names.map(($n) -> $n.upper())`)* |
| `filter((T) -> bool $fn)` | `list<T>` | The elements `$fn` returns `true` for |
| `reject((T) -> bool $fn)` | `list<T>` | The elements `$fn` returns `false` for |
| `flatMap((T) -> list<U> $fn)` | `list<U>` | Like `map`, joining the lists `$fn` returns *(i.e. `$lines.flatMap(($l) -> $l.words())`)* |
| `flatten()` | `list<U>` | For a `list<list<U>>`: joins the lists inside it *(i.e. `[[1, 2], [3]].flatten()` is `[1, 2, 3]`)*. Throws a `TypeError` if an element isn't a list |
| `unique()` | `list<T>` | Without repeated values, keeping the first of each *(i.e. `[1, 2, 1].unique()` is `[1, 2]`)* |
| `uniqueBy<K>((T) -> K $key)` | `list<T>` | Without elements with a repeated `$key` |
| `compact()` | `list<T>` | Without the `null` values. A `list<?T>` becomes a `list<T>` |
| `take(number $count)` | `list<T>` | The first `$count` elements, or all of them if there are fewer |
| `skip(number $count)` | `list<T>` | Everything but the first `$count` elements |
| `takeWhile((T) -> bool $fn)` | `list<T>` | The elements from the start, until `$fn` returns `false` |
| `skipWhile((T) -> bool $fn)` | `list<T>` | The elements after the ones at the start that `$fn` returns `true` for |
| `chunk(number $size)` | `list<list<T>>` | Splits it in lists of `$size` elements, the last one can be shorter *(i.e. `[1, 2, 3].chunk(2)` is `[[1, 2], [3]]`)* |
| `concat(list<T> $other)` | `list<T>` | This list followed by `$other` |
| `copy()` | `list<T>` | A copy of the list. The elements themselves aren't copied |

## Grouping

| Method | Returns | Description |
|--------|---------|-------------|
| `groupBy((T) -> K $key)` | `map<K, list<T>>` | The elements grouped by what `$key` returns *(i.e. `$files.groupBy(($f) -> $f.afterLast('.'))`)* |
| `countBy((T) -> K $key)` | `map<K, number>` | How many elements give each `$key` *(i.e. `$words.countBy(($w) -> $w.lower())`)* |
| `partition((T) -> bool $fn)` | `list<list<T>>` | Two lists: the elements `$fn` returns `true` for, and the rest *(i.e. `let [$adults, $kids] = $ages.partition(($a) -> $a >= 18);`)* |
| `toMap((T) -> K $key)` | `map<K, T>` | A map from what `$key` returns to each element. If two give the same key, the last one is kept |

## Searching

| Method | Returns | Description |
|--------|---------|-------------|
| `find((T) -> bool $fn)` | `?T` | The first element `$fn` returns `true` for, or `null` |
| `findLast((T) -> bool $fn)` | `?T` | The last element `$fn` returns `true` for, or `null` |
| `findIndex((T) -> bool $fn)` | `?number` | The position of the first element `$fn` returns `true` for, or `null` |
| `any((T) -> bool $fn)` | `bool` | `true` if `$fn` returns `true` for at least one element |
| `all((T) -> bool $fn)` | `bool` | `true` if `$fn` returns `true` for every element. `true` for an empty list |
| `none((T) -> bool $fn)` | `bool` | `true` if `$fn` returns `true` for no element |

## Combining

| Method | Returns | Description |
|--------|---------|-------------|
| `join(string $separator = '')` | `string` | The elements as text, with `$separator` between them *(i.e. `['a', 'b'].join(', ')` is `'a, b'`)* |
| `reduce((T, T) -> T $fn)` | `?T` | Combines the elements from left to right, starting with the first one. `null` if the list is empty *(i.e. `$parts.reduce(($a, $b) -> $a + '/' + $b)`)* |
| `fold(U $initial, (U, T) -> U $fn)` | `U` | Like `reduce`, starting from `$initial` |
| `sum()` | `number` | The sum, for a `list<number>`. `0` for an empty list |
| `product()` | `number` | The product, for a `list<number>`. `1` for an empty list |
| `average()` | `?number` | The average, for a `list<number>`. `null` for an empty list |
| `min()` | `?T` | The smallest element, using `<`, or `null` |
| `max()` | `?T` | The biggest element, using `<`, or `null` |
| `minBy<K>((T) -> K $key)` | `?T` | The element with the smallest `$key`, or `null` |
| `maxBy<K>((T) -> K $key)` | `?T` | The element with the biggest `$key`, or `null` *(i.e. `$files.maxBy(($f) -> $f.size())`)* |

## Looping

| Method | Returns | Description |
|--------|---------|-------------|
| `each((T) -> void $fn)` | `void` | Calls `$fn` with every element. Usually a `for` loop reads better |
| `eachWithIndex((T, number) -> void $fn)` | `void` | Calls `$fn` with every element and its position |
