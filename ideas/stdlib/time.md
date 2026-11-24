# std/time

The current time, and dates: reading them, writing them in any format, and in any time zone. `use std/time;`, then `time::now()` or `time::date()`.

```
use std/time;
use std/time/zone;

const $start = time::now();                       // milliseconds, to time code

const $d = time::date();                          // now, in the local zone
outln($d.format('yyyy-MM-dd HH:mm'));             // 2026-11-22 00:41
outln($d.format());                               // 2026-11-22T00:41:07+01:00
outln(f"{$d.day}/{$d.month}/{$d.year}, day {$d.weekday} of the week");

const $p = time::parse('22/11/2026', 'dd/MM/yyyy');
outln($p.format('EEEE d MMMM'));                  // Sunday 22 November
const $api = time::parse('2026-11-22T10:00:00Z'); // ISO 8601, without a format
outln($api.inZone(zone::named('America/Lima')).format('HH:mm VV'));   // 05:00 America/Lima
if ($api < $d) { ... }
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `now()` | `number` | Milliseconds since 1970-01-01 00:00 UTC, like Java's `System.currentTimeMillis()` *(i.e. `const $start = time::now(); ... outln(f"took {time::now() - $start} ms");`)*. It follows the system clock, so it can jump if the clock is changed |
| `date(?number $millis = null, ?Zone $zone = null)` | `Date` | The date at that instant, in milliseconds since 1970 like `now()`, or now. In the local zone, unless one is given |
| `of(number $year, number $month, number $day, number $hour = 0, number $minute = 0, number $second = 0, number $millisecond = 0, ?Zone $zone = null)` | `Date` | The date with those parts, in the local zone unless one is given *(i.e. `time::of(2026, 12, 24, 20)`)*. A date that doesn't exist *(i.e. February 30)* throws a `ValueError` |
| `parse(string $text, ?string $format = null, ?Zone $zone = null)` | `Date` | Reads a date written in that format *(see Formats)*. Without one, ISO 8601, the format of JSON and APIs *(i.e. `2026-11-22`, `2026-11-22T10:00:00Z`, `2026-11-22T10:00:00+01:00`)*. A text without a zone or an offset is in `$zone`, or in the local zone. One that doesn't match throws a `ValueError` |

- A date without a time is midnight.
- A date written with an offset but no zone *(i.e. `+01:00`)* has a zone that's that offset, like Java's `ZonedDateTime.parse`.
- A time that happens twice or never in a zone, when the clocks change, is read like Java and most libraries do: the first of the two, or moved forward by the gap.

## Date

A date and time, in a zone. From `date()`, `of()` or `parse()`. Like Java's `ZonedDateTime`.

| Field | Type | Description |
|-------|------|-------------|
| `year` | `number` | *(i.e. `2026`)* |
| `month` | `number` | From 1, January, to 12 |
| `day` | `number` | The day of the month, from 1 |
| `hour` | `number` | From 0 to 23 |
| `minute` | `number` | From 0 to 59 |
| `second` | `number` | From 0 to 59 |
| `millisecond` | `number` | From 0 to 999 |
| `weekday` | `number` | From 1, Monday, to 7, Sunday, like ISO 8601 and Java |
| `zone` | `Zone` | Its zone *(see [Zones](#zones))* |

| Method | Returns | Description |
|--------|---------|-------------|
| `format(?string $format = null)` | `string` | The date written in that format *(see Formats)*. Without one, ISO 8601 with its offset *(i.e. `2026-11-22T00:41:07+01:00`)*, which `parse()` reads back |
| `millis()` | `number` | The instant, in milliseconds since 1970, like `now()` |
| `inZone(Zone $zone)` | `Date` | The same instant in another zone *(i.e. `$d.inZone(zone::utc())`)*. Not `in`, which is a keyword |

- `==` and `<` compare the instants, through `IEquals` and `IOrdered`, so dates in different zones compare right, and lists of dates can be sorted. Like Python, and like Java's `isEqual` and `isBefore`.
- Dates are values that don't change: `inZone()` gives a new one.

## Formats

The letters of Java's `DateTimeFormatter`, the most used ones:

| Letters | Gives | Example |
|---------|-------|---------|
| `yyyy` | The year | `2026` |
| `yy` | The year, in two digits | `26` |
| `M`, `MM` | The month, as a number, without and with a zero | `1`, `01` |
| `MMM`, `MMMM` | The name of the month, short and full | `Nov`, `November` |
| `d`, `dd` | The day of the month | `5`, `05` |
| `EEE`, `EEEE` | The name of the day of the week, short and full | `Sun`, `Sunday` |
| `H`, `HH` | The hour, from 0 to 23 | `7`, `07` |
| `h`, `hh` | The hour, from 1 to 12 | `7`, `07` |
| `a` | AM or PM | `PM` |
| `m`, `mm` | The minute | `4`, `04` |
| `s`, `ss` | The second | `9`, `09` |
| `S`, `SS`, `SSS` | The fraction of the second, in that many digits | `123` |
| `Z` | The offset | `+0100` |
| `XXX` | The offset, or `Z` for UTC | `+01:00`, `Z` |
| `xxx` | The offset, with `+00:00` for UTC | `+01:00` |
| `z` | The abbreviation of the zone, only to write it | `CET` |
| `VV` | The name of the zone | `Europe/Madrid` |
| `'...'` | Text as it is. `''` is a quote | `'at' HH:mm` gives `at 10:00` |

- Other characters, like `-`, `/`, `:`, spaces and `T`... are written as they are, except letters: every letter is reserved, like in Java, so text goes between quotes *(i.e. `yyyy-MM-dd'T'HH:mm`)*. A letter that isn't in the table throws a `ValueError`.
- Names of months and days are in English. Reading them ignores case.
- `z` can't be read: abbreviations mean different zones in different places *(i.e. `CST`)*. `VV` or an offset can.

## Zones

`use std/time/zone;`. A zone is a class of its own, `Zone`, like Java's `ZoneId` and Python's `zoneinfo`, so a misspelled zone fails where it's named, and `std/time` stays about dates.

| Function | Returns | Description |
|----------|---------|-------------|
| `utc()` | `Zone` | UTC. A function, like `local()`, since constants of modules are variables *(i.e. `path::$separator`)* |
| `named(string $name)` | `Zone` | A zone by its IANA name *(i.e. `Europe/Madrid`, `America/Lima`)*, `UTC`, or an offset *(i.e. `+05:30`)*. One that doesn't exist throws a `ValueError` |
| `local()` | `Zone` | The zone of the system, or the one in `TZ` |
| `all()` | `list<string>` | The names of every zone the system knows, sorted |

## Zone

A time zone, from `std/time/zone`. Two zones with the same name are equal.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Its IANA name *(i.e. `Europe/Madrid`)*, `UTC`, or an offset *(i.e. `+01:00`)* |

## How it's made

- `std/time` and `std/time/zone` are written in Lakescript, in `crates/lakescript/src/stdlib/time.lks` and `time/zone.lks`, with their Rust part, `std/time/native`, in `time.rs` *(see How modules are made in [README.md](README.md))*.
- On `jiff`, which reads the zones of the system *(`/usr/share/zoneinfo`)*, and has its own copy of them on Windows. It's MIT or Unlicense.
- A `Date` keeps its instant in milliseconds and the name of its zone, and its fields are worked out from them in Rust when it's made.
- Formats are turned into the ones of `strftime` that `jiff` reads and writes *(i.e. `yyyy-MM-dd` into `%Y-%m-%d`)*.

## Backlog

- Durations, and adding them to dates *(i.e. `$d.plus(days: 3)`)*. Until then, `time::date($d.millis() + 3 * 86400000)` adds 3 days of 24 hours.
- Names of months and days in other languages.
