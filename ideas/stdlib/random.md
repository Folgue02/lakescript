# std/random

Random numbers, `true` or `false`, UUIDs, and a seed to get the same ones on every run. `use std/random;`, then `random::between(1, 6)`.

```
use std/random;

outln(random::between(1, 6));              // a die: 1 to 6
if (random::number() < 0.1) outln('lucky');   // 1 time in 10
const $id = random::uuid();                // 'f47ac10b-58cc-4372-a567-0e02b2c3d479'

random::seed(42);                          // the same numbers on every run from here,
outln(['a', 'b', 'c'].random());           // also for choosing and shuffling lists
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `number()` | `number` | From 0 to 1, without 1, like Python's `random.random()` and Java's `Math.random()` |
| `between(number $min, number $max)` | `number` | A whole number from `$min` to `$max`, both included, like Python's `randint` *(i.e. `random::between(1, 6)` for a die)*. Throws a `ValueError` if `$min` is bigger than `$max`, and a `TypeError` if one isn't a whole number |
| `bool()` | `bool` | `true` or `false`, half the times each |
| `uuid()` | `string` | A random UUID, version 4, in lowercase *(i.e. `'f47ac10b-58cc-4372-a567-0e02b2c3d479'`)*. Its bits come from the system, so `seed()` doesn't change them, and two runs never give the same ones |
| `seed(number $seed)` | `void` | Starts the generator again from a whole number, so the numbers that follow are the same on every run, for tests and for things that must be repeatable *(i.e. a level of a game)*. It's also the generator of `$list.random()`, `$list.shuffle()` and `$list.shuffled()` |

| `state()` | `string` | The state of the generator, to put it back later with `restore()`, like Python's `random.getstate()` *(i.e. to save a game and load it)*. It's a string, so it can be written to JSON: a number can't hold the 64 bits |
| `restore(string $state)` | `void` | Puts back a state from `state()`, like Python's `random.setstate()`: the numbers that follow are the ones that followed the `state()` call. Throws a `ValueError` for a string that isn't one |

- Choosing and shuffling are methods of lists, not functions here: `$list.random()` gives an element, and `$list.shuffle()` and `$list.shuffled()` put them in random order *(see [list.md](types/list.md))*.
- Without `seed()`, the numbers are different on every run.
- It's good enough for scripts, games and tests, not for secrets: passwords and keys need a generator made for them.

## Random

A generator of its own, for a stream of numbers that nothing else in the script moves forward, like Python's `random.Random(seed)` and Java's `new Random(seed)` *(i.e. the levels of a game from their seed, while the rest of the game uses the shared one)*. `random::Random(42)` starts it from a seed, and `random::Random()` from the system, like a script without `seed()`.

```
const $level = random::Random(7);
outln($level.between(1, 6));           // the same on every run, whatever else uses random
const $saved = $level.state();
```

| Method | Returns | Description |
|--------|---------|-------------|
| `number()` | `number` | Like `random::number()`, from this generator |
| `between(number $min, number $max)` | `number` | Like `random::between()`, from this generator |
| `bool()` | `bool` | Like `random::bool()`, from this generator |
| `seed(number $seed)` | `void` | Starts it again from a whole number, like `random::seed()` |
| `state()` | `string` | Its state, like `random::state()` |
| `restore(string $state)` | `void` | Puts back a state, like `random::restore()`. A state of one generator can go into another, and into the shared one |

- It has no `uuid()`: the bits of a UUID never come from a generator *(see `uuid()`)*, so it would be the same as `random::uuid()`.
- `$list.random()`, `$list.shuffle()` and `$list.shuffled()` keep using the shared generator.

## How it's made

- `random.lks` has `Random` and the functions, over `std/random/native` *(`crates/lakescript/src/stdlib/random.rs`)*, on the generator every script already has for lists *(xorshift64\*, started from the time and the process)*. A `Random` keeps its own state in Rust data, a `Value::Handle`. A state is its 64 bits in hexadecimal. `uuid()` gets its bits from the keys the system gives Rust's hash maps instead.
