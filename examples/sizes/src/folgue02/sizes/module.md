Two commands to read and write sizes in bytes the way people do: `1536` is
`1.5 KB`, and `1.5 KB` is `1536`.

The module `units` does both, so other projects can use it too:

```
use folgue02/sizes/units;

outln(units::human(1048576));   // 1 MB
outln(units::bytes('2 mb'));     // 2097152
```

- Units go up by 1024, from `B` to `TB`.
- Text is read in any case, and a number without a unit is bytes.
