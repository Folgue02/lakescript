# Errors

Errors are exceptions, like in Java. They can be caught with `try`/`catch`, or turned into a `result` with `try` used as an expression, like in Rust.

## Built-in exceptions

Every error the language or the standard library raises is one of these:

```
Exception
├── IOError
│   ├── FileNotFound
│   ├── PermissionDenied
│   └── NetworkError    // a request of std/http that got no response
├── ValueError          // '5a'.toNumber(), invalid JSON
├── TypeError           // .as() failed, `'a' + 5`, a wrong argument
│   └── NullError       // `$x!` on a `null`
├── IndexError          // replacing `$list[$i]` past the end, `.removeAt()` and `.insert()` outside a list (reading `$list[$i]` past the end is `null`)
├── ArithmeticError     // % or .div() by zero
├── CommandFailed       // a shell command returned an error
└── AssertionError      // an `assert` whose condition was false

Interrupted             // Ctrl+C: beside Exception, not under it (see Ctrl+C below)
```

- Custom exceptions are classes that extend one of these *(see [classes.md](classes.md))*:

```
class ConfigError extends ValueError {
    pub string @file;
}

class MissingKey extends ConfigError {
    pub string @key;

    pub new(@key, string $file) {
        @super(f"`{@key}` is missing", $file);
    }
}

throw ConfigError('the port must be a number', file: 'app.json');
```

- A custom exception gets the field `message` first, a `const string` that's `''` unless it's given. Its constructor takes it like any other field, and a `new` of its own gives it with `@super(message)`.
- It's thrown and caught like the built-in ones: `catch (ConfigError $e)` catches it and its subclasses, and `catch (ValueError $e)` too. It has their methods *(i.e. `$e.message()`, `$e.line()`)*, and its own fields and methods.
- Only exceptions can be thrown: an object of a class that doesn't extend one is an error.

## assert

`assert $condition;` throws an `AssertionError` when the condition is `false`. Its message has the code of the condition, and for a comparison the values of both sides *(i.e. `assert failed: $total == 10`, then `left: 9` and `right: 10`)*. It's what tests use, and any script can *(see [testing.md](../tooling/testing.md))*.

## Catching

```
try {
    const $text = readFile($path);
} catch (FileNotFound $e) {
    outln(f"Missing: {$e.message()}");
} catch (IOError $e) {          // any other IOError
    outln('Something else went wrong');
} catch ($e) {                  // anything else
    outln($e);
} finally {
    outln('Always runs');
}
```

- The `catch` blocks are checked in order, and the first one that matches runs. A type also catches the exceptions under it, like `.is()`.
- `catch ($e)` without a type catches every `Exception`, but not an `Interrupted` *(see Ctrl+C below)*.
- `finally` runs after the `try` or the `catch`, also when they `return`, `break` or throw again.
- A `try` block needs at least one `catch` or a `finally`.

## Ctrl+C

*Built.*

Ctrl+C throws an `Interrupted` wherever the script is, like Python's `KeyboardInterrupt`. So a script can clean up before it stops *(i.e. remove a temporary folder in a `finally`)*.

```
try {
    for ($page in $pages) {
        scrape($page);
    }
} catch (Interrupted $e) {
    errln(f"Stopped after {$done} pages");
}
```

- **It's beside `Exception`, not under it**, so `catch ($e)` and `catch (Exception $e)` don't catch it. Only `catch (Interrupted $e)` does. Otherwise a loop that asks again after any error could never be stopped, and that's why Python's `KeyboardInterrupt` isn't an `Exception` either.
- `finally` blocks run, like with `exit()`.
- Not caught, the script stops with the code 130, like shells do, without the message of an error.
- `try` as an expression and `test::throws` don't catch it either.
- **How**: the signal of Ctrl+C *(SIGINT)* only sets a flag, and the VM checks it at the start of each loop's turn and each call, which costs a check of a bool. A read of stdin that's waiting, or a command that runs, ends and then it's thrown. A command gets the Ctrl+C itself too, since the terminal sends it to every process in front.
- `lks` and `ferry` set the flag from a handler of `ctrlc`. A second Ctrl+C before the VM sees the first one *(i.e. while a long call of Rust runs)* stops the script at once, with 130. Engines that embed Lakescript give their own flag with `Engine::set_interrupt_flag`.
- The VM checks it at backward jumps, at each turn of a `for` and at each call. Tests are in `crates/lakescript/tests/interrupt.rs`.
- In `std/terminal`'s prompts the terminal is in raw mode, where Ctrl+C is a key: they put the terminal back and throw it themselves *(see [terminal.md](../stdlib/terminal.md))*.

## Throwing

```
throw ValueError(f"'{$input}' isn't a valid port");
```

- The built-in exceptions are created by calling them with a message *(i.e. `ValueError('...')`)*.
- Only exceptions can be thrown: `throw 'oops';` is an error.
- `throw` is also an expression, so it can go where a value is expected. It never gives a value, so the checker accepts it in place of any type:

```
const $name = $config['name'] ?? throw ValueError('the config needs a name');

string describe(any $v) = switch ($v) {
    case string -> $v;
    case bool -> throw TypeError('bools have no description');
    default -> 'something else';
};
```

- Like `try`, `throw` applies to the whole expression after it.

## `try` as an expression

`try` followed by anything but `{` is an expression that returns a `result<T, Exception>` instead of throwing *(see `result` in [builtin_types.md](builtin_types.md))*:

```
const $config = (try readFile('config.txt')).orElse('');

const $r = try readFile($path);
if ($r.isError() && $r.error().is(FileNotFound))
    outln('No config, using defaults');
```

- `try` applies to the whole expression after it, so parentheses are needed to call methods on the result.

## Exception values

| Method | Returns | Description |
|--------|---------|-------------|
| `message()` | `string` | The message it was created with |
| `typeName()` | `string` | Its type *(i.e. `'FileNotFound'`)* |
| `line()` | `number` | The line where it was thrown |
| `trace()` | `list<string>` | The calls it was thrown through, one per line, as an uncaught one shows them *(see Tracebacks below)* |
| `toString()` | `string` | The type and the message *(i.e. `'ValueError: bad port'`)* |

- `$e.is(IOError)` checks the type, including the exceptions under it.

## Uncaught exceptions

An exception nobody catches stops the script. `lks` prints it with the file, line and column, and exits with code 1 *(see [diagnostics.md](../tooling/diagnostics.md))*. A program that embeds Lakescript gets it back as an error.

## Tracebacks

An uncaught exception shows the calls that led to it, not only the line where it was thrown, so a helper called from many places says which call failed. The error itself looks as it always has *(see [diagnostics.md](../tooling/diagnostics.md))*: its message, the arrow with the full path of the file, and its line of code. The calls go above it, one line each like Java's traceback, so the error stays the last thing on the screen:

```
  at the script (/home/ana/shop/main.lks:2:1)
  at report::build (/home/ana/shop/report.lks:2:19)
  at a lambda in report::build (/home/ana/shop/report.lks:2:39)
error: FileNotFound: can't read 'b.csv': it doesn't exist
 --> /home/ana/shop/report.lks:4:34
  |
4 | list<any> load(string $file) = csv::read($file);
  |                                ^^^^^^^^^^^^^^^^
```

- **Each line** is a call that hadn't finished: the function it was in, and the file, line and column it was at, in the order they were called, the first one at the top. The last function, where the error was thrown, is the error below them. A function of a module has its name *(i.e. `report::build`)*, and a method its class *(i.e. `Point.length`)*.
- Paths are full, like the arrow's, so a terminal or an editor can open them.
- Modules count, and so do functions called by methods *(i.e. the lambda of `.map()`)*.
- **The standard library is left out**: the calls inside the modules written in Lakescript *(i.e. `<std>/time.lks`)* aren't lines, and an error thrown inside one is shown where the script called it, with its line of code. Their code isn't the script's, and their files can't be opened. A function of the script that they call *(i.e. the `$check` of `terminal::ask`)* is still a line. `$e.line()` and `$e.trace()` say the same.
- **A function that calls itself** many times is one line *(i.e. `at fact (/home/ana/math.lks:3:12), 998 times`)*.
- **An error without calls**, like the checker's or one at the top of the script, has no lines above it, and looks the same as now.
- **Only an uncaught exception prints its trace.** A caught one prints nothing on its own: **`$e.trace()`** gives its lines as a `list<string>`, which the script joins and prints if it wants to *(i.e. `errln($e.trace().join('\n'))`)*. Its last line is where it was thrown *(i.e. `at load (/home/ana/shop/report.lks:4:34)`)*, since there's no error below it.
- The trace is the one of where the exception was **first thrown**: throwing it again from a `catch` keeps it. An exception that was created but not thrown yet has an empty one.
- The test runner shows it with a failure, so an `assert` inside a helper says which test called it *(see [testing.md](../tooling/testing.md))*.
- **Its cost**: it's taken when something is thrown, from the frames the VM already has. Code that doesn't throw pays nothing, and code that throws in a loop pays about a microsecond for each call in the trace.
- **How it's made**: the VM takes the trace in `unwind`, the first time an exception goes through it, while every frame is still there. Each frame is at the call it made, saved before every call, and the last one is where it was thrown. The compiler gives each function the name tracebacks show *(`Proto::shown`)*: `report::` in front of the functions of a module, `Class.method`, `a lambda in ...` for lambdas, `the script`, `the module report` and `the template`. Throwing costs about 0.06 µs more with 11 calls in the trace.
