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
├── IndexError          // replacing `$list[$i]` past the end, `.removeAt()` and `.insert()` outside a list (reading `$list[$i]` past the end is `null`)
├── ArithmeticError     // % or .div() by zero
└── CommandFailed       // a shell command returned an error
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
- `catch ($e)` without a type catches everything.
- `finally` runs after the `try` or the `catch`, also when they `return`, `break` or throw again.
- A `try` block needs at least one `catch` or a `finally`.

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
| `toString()` | `string` | The type and the message *(i.e. `'ValueError: bad port'`)* |

- `$e.is(IOError)` checks the type, including the exceptions under it.

## Uncaught exceptions

An exception nobody catches stops the script. `lks` prints it with the file, line and column, and exits with code 1 *(see [diagnostics.md](../tooling/diagnostics.md))*. A program that embeds Lakescript gets it back as an error.
