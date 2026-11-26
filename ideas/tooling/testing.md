# Testing

Tests are plain functions, checked with `assert`, like Python's pytest: nothing to register and no classes to extend. `ferry test` runs the tests of a project, and `lks --test` the ones in a script.

```
// tests/units.lks
use folgue02/sizes/units;
use std/test;

void testBytesToText() {
    assert units::human(1536) == '1.5 KB';
    assert units::human(0) == '0 B';
}

void testTextToBytes() {
    assert units::bytes('1.5 KB') == 1536;
}

void testInvalidSizes() {
    const $e = test::throws(() -> units::bytes('lots'));
    assert $e.typeName() == 'ValueError';
}
```

## Tests

- **A test is a function** at the top level of a test file, whose name starts with `test` and that takes no arguments *(i.e. `void testBytesToText()`)*. Other functions are helpers, also the ones named `test...` that take arguments.
- The file runs first, like any script: its `use` lines, its constants and whatever its top level does. If that fails, or the file has an error the checker finds, none of its tests run.
- The tests of a file run in the order they're written. They share the file's top-level values, and each file is loaded on its own, so files don't see each other's.
- A test passes when it returns, and fails when it throws anything: an `assert` that failed, or any other exception. Calling `exit()` fails it too.
- **Tests only see the `pub` items** of the project's modules, like any other code that uses them. Nothing changes in how modules work.

## assert

`assert` is a statement of the language, so every script can use it, not only tests *(see [errors.md](../language/errors.md))*.

```
assert $count > 0;
assert units::bytes('1.5 KB') == 1536;
```

- When the condition is `false`, it throws an `AssertionError`, a built-in exception, whose message has the code of the condition.
- When the condition is a comparison *(`==`, `!=`, `<`, `<=`, `>`, `>=`)*, the message also has the values of both sides, since they're what tells what went wrong:

```
assert failed: units::bytes('1.5 KB') == 1536
  left:  1500
  right: 1536
```

- A value of several lines goes on under its first line, after its `'`, so both sides line up *(since `v0.12.2`)*.

- `$a.equals($b)` shows both sides too, since it's how lists and maps are compared by their values.
- `==` compares lists and maps by reference *(see [operators.md](../language/operators.md))*, so `assert $list == [1, 2]` fails even when `$list` has those values. Then the message says so, and that `.equals()` compares the values.
- The condition must be a `bool`, like the one of an `if`. After the `assert`, the checker knows it holds, like after an `if` that returns *(i.e. `assert $name != null;` and then `$name.upper()`)*.
- Its condition runs once: both sides of a comparison are kept, and shown from there.

## Before and after each test

A file of tests can have `void beforeEach()` and `void afterEach()`, found by their names like the tests. They run before and after each of its tests, with what the test prints:

```
use std/test;

let string $dir = '';

void beforeEach() {
    $dir = test::tempDir();
}

void afterEach() {
    outln(f"done with {$dir}");
}
```

- `afterEach` runs also when the test or `beforeEach` failed, so it can clean up. When `beforeEach` fails, the test doesn't run, and fails with its error.
- An error in `afterEach` makes a test that passed fail.
- The folders of `test::tempDir()` are removed after `afterEach`.

## std/test

What `assert` can't do *(see [stdlib/test.md](../stdlib/test.md))*:

| Function | Returns | Description |
|----------|---------|-------------|
| `throws(() -> any $code)` | `exception` | Runs `$code` and returns the exception it threw. If it throws nothing, the test fails |

## Running them

**In a project**, the tests are the files in `tests/`, next to `src/` and `bin/` *(see [ferry.md](ferry.md))*:

```
sizes/
├── ferry.toml
├── src/folgue02/sizes/units.lks
├── bin/human.lks
└── tests/
    └── units.lks
```

- `ferry test` runs every file in `tests/`, with the module path of `ferry run`: the project's `src/`, its dependencies and the standard library. So tests use the project's modules like the scripts in `bin/` *(i.e. `use folgue02/sizes/units;`)*.
- `ferry test <filter>` runs only the tests whose name has `<filter>` in it *(i.e. `ferry test bytes`)*.
- `ferry check` checks the files in `tests/` too.

**For a script** outside a project, `lks --test file.lks` runs the tests in that file *(see [lks.md](lks.md))*.

**What it says**, like the rest of ferry *(see What ferry says in [ferry.md](ferry.md))*:

```
     Testing folgue02/sizes
tests/units.lks
          ok testBytesToText
        FAIL testTextToBytes
             tests/units.lks:10:5: assert failed: units::bytes('1.5 KB') == 1536
               left:  1500
               right: 1536
          ok testInvalidSizes
    Finished 2 passed, 1 failed in 0.02 s
```

- A test that fails doesn't stop the others.
- **The files run at once**, each in its own thread and its own engine, as many as the machine has cores *(built in phase 13)*. What each file gives is printed whole, in the order of the files, as soon as the ones before it are done, so the output is the same as one at a time. `ferry test --jobs 1` runs them one at a time, and `--jobs 4` four at once. The tests of one file still run one after another, since they share its top level.
- The exit code is `1` when a test failed or a file couldn't run, and `0` when every one passed.
- `ferry test` says all of it on stderr, like the rest of what ferry says. `lks --test` writes the tests on stdout, and an error that stops the file on stderr.
- A file whose tests all are left out by the filter isn't shown. A filter that no test has in its name gets a note, since it picks tests by their names, not by their files. Whether it should also pick files isn't decided.
- What a test prints is only shown when it fails, like cargo.

## How it's made

- `assert` is a statement *(`StmtKind::Assert`)*. The lexer keeps the code of its condition, and the compiler turns it into `if (!(cond)) __assertFailed('code');`, or for a comparison into a block that keeps both sides in variables first. `__assertFailed` is a built-in function that throws the `AssertionError` with the message.
- `Engine::run_tests` runs a file and then each test, keeping what it prints. `render_result` and `render_summary` show them, the same for `lks` and `ferry` *(in `crates/lakescript/src/testing.rs`)*.
- `test::throws` is a function that runs Lakescript code, like `template::render` *(see [template.md](../stdlib/template.md))*.

## Later

- Running a test from an editor, through `lks-lsp`.
