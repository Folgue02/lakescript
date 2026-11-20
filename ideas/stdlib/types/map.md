# map

`map<K, V>`: keys and their values, in the order they were added *(see Map in [builtin_types.md](../../language/builtin_types.md))*. Verbs change the map itself, past participles return a copy *(i.e. `merge()` and `merged()`)*. Functions passed to the methods get the key and the value.

## Information

| Method | Returns | Description |
|--------|---------|-------------|
| `length()` | `number` | The number of keys |
| `empty?()` | `bool` | `true` if it has no keys |
| `has(K $key)` | `bool` | `true` if `$key` is in the map, even if its value is `null`. Same as `-in` |
| `hasValue(V $value)` | `bool` | `true` if some key has `$value`, comparing contents |
| `keys()` | `list<K>` | The keys, in order |
| `values()` | `list<V>` | The values, in order |
| `keyOf(V $value)` | `?K` | The first key with `$value`, or `null` |

## Getting

| Method | Returns | Description |
|--------|---------|-------------|
| `getOrDefault(K $key, V $default)` | `V` | The value of `$key`, or `$default` if it's not there *(i.e. `$config.getOrDefault('port', 8080)`)*. `$map[$key]` gives a `?V`, which is `null` when the key isn't there |
| `getOrSet(K $key, V $default)` | `V` | The value of `$key`. If it's not there, it's set to `$default` first *(i.e. `$groups.getOrSet($ext, []) << $file;`)* |
| `first()` | `?K` | The first key, or `null` if the map is empty |
| `last()` | `?K` | The last key, or `null` if the map is empty |

## Changing

These change the map.

| Method | Returns | Description |
|--------|---------|-------------|
| `set(K $key, V $value)` | `map<K, V>` | Same as `$map[$key] = $value`. Returns the map |
| `update(K $key, V $default, (V) -> V $fn)` | `V` | Sets `$key` to what `$fn` returns for its value, or for `$default` if it's not there. Returns the new value *(i.e. `$counts.update($word, 0, ($n) -> $n + 1)`)* |
| `remove(K $key)` | `?V` | Removes `$key` and returns its value, or `null` if it wasn't there |
| `removeWhere((K, V) -> bool $fn)` | `number` | Removes the keys `$fn` returns `true` for, and returns how many there were |
| `merge(map<K, V> $other)` | `map<K, V>` | Adds the keys of `$other`, replacing the ones that already exist. Returns the map |
| `clear()` | `void` | Removes everything |
| `sortByKey()` | `map<K, V>` | Reorders the keys from smallest to biggest. Returns the map |
| `sortBy<S>((K, V) -> S $key)` | `map<K, V>` | Reorders the keys by what `$key` returns *(i.e. `$scores.sortBy(($name, $score) -> -$score)`)* |

## Transforming

These always return a new map.

| Method | Returns | Description |
|--------|---------|-------------|
| `merged(map<K, V> $other)` | `map<K, V>` | A copy with the keys of `$other` added, replacing the ones that already exist *(i.e. `$defaults.merged($userConfig)`)* |
| `filter((K, V) -> bool $fn)` | `map<K, V>` | The keys `$fn` returns `true` for |
| `reject((K, V) -> bool $fn)` | `map<K, V>` | The keys `$fn` returns `false` for |
| `mapValues((V) -> U $fn)` | `map<K, U>` | The same keys, with `$fn` applied to each value *(i.e. `$sizes.mapValues(($s) -> $s.formatBytes())`)* |
| `mapKeys((K) -> J $fn)` | `map<J, V>` | `$fn` applied to each key. If two give the same key, the last one is kept |
| `map((K, V) -> U $fn)` | `list<U>` | A list with what `$fn` returns for each key and value *(i.e. `$env.map(($k, $v) -> f"{$k}={$v}")`)* |
| `invert()` | `map<V, K>` | Values become keys and keys become values |
| `sortedByKey()` | `map<K, V>` | A copy with the keys from smallest to biggest |
| `sortedBy<S>((K, V) -> S $key)` | `map<K, V>` | A copy ordered by what `$key` returns |
| `copy()` | `map<K, V>` | A copy of the map. The values themselves aren't copied |

## Searching

| Method | Returns | Description |
|--------|---------|-------------|
| `find((K, V) -> bool $fn)` | `?K` | The first key `$fn` returns `true` for, or `null` |
| `any((K, V) -> bool $fn)` | `bool` | `true` if `$fn` returns `true` for at least one key |
| `all((K, V) -> bool $fn)` | `bool` | `true` if `$fn` returns `true` for every key. `true` for an empty map |
| `none((K, V) -> bool $fn)` | `bool` | `true` if `$fn` returns `true` for no key |
| `countWhere((K, V) -> bool $fn)` | `number` | How many keys `$fn` returns `true` for |

## Looping

| Method | Returns | Description |
|--------|---------|-------------|
| `each((K, V) -> void $fn)` | `void` | Calls `$fn` with every key and value. Usually a `for` loop reads better |
