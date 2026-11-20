# std/time

The current time. `use std/time;`, then `time::now()`.

For now it only gives the time as a number. Dates, formatting and durations come later, once there are classes.

| Function | Returns | Description |
|----------|---------|-------------|
| `now()` | `number` | Milliseconds since 1970-01-01 00:00 UTC, like Java's `System.currentTimeMillis()` *(i.e. `const $start = time::now(); ... outln(f"took {time::now() - $start} ms");`)*. It follows the system clock, so it can jump if the clock is changed |
