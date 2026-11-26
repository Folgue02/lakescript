# std/test

What tests need besides `assert`. `use std/test;`, then `test::throws(...)`. How tests are written and run is in [testing.md](../tooling/testing.md).

```
use std/test;

void testInvalidSizes() {
    const $e = test::throws(() -> units::bytes('lots'));
    assert $e.typeName() == 'ValueError';
    assert $e.message().contains('lots');
}
```

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `throws(() -> any $code)` | `exception` | Runs `$code` and returns the exception it threw, to check its type and message. If it throws nothing, it throws an `AssertionError`, so the test fails |
| `tempDir()` | `string` | A new empty folder, removed with what's in it when the test ends, also when it fails *(i.e. `const $dir = test::tempDir();`)*. Outside a test, it's removed when the script ends. Its path is a real one, without links |
