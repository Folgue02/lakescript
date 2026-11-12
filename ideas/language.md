# Syntax

- C-like syntax, uses curly braces and semicolons.
- `$` to prefix variables, like PHP or Perl
- use of `let` and `const` to declare variables, don't allow redeclarations.
- use of operators such as `<<` to push things into collections.
- coalesce operator (`<<`), ternary operator (`... ? ... : ...`), nullable operator like in JS (`$nullableThing?.method()`)
- conditional operators like in bash/powershell:
    - `-f` to check if the path points to a file.
    - `-d` to check if it's a directory.
    - `-x` to check if the path points to an executable.
    - `-r` to check if it's readable.
    - `-w` to check if it's writable.
    - `-like` to check if a string matches a pattern.
    - `-in` to check if the element its in a collection.

- Just like in PHP, it has optional typing (but it should also have generics, i.e. `array<string>`)
- String interpolation, with a prefix for the string just like in Python i.e. `f"Hello {$name}"`
- Ranges like in Python `1..5` (this involves slicing arrays by ranges)
- Negative indexing for arrays like in python.
- try catch with exceptions like Java's, but with the option of getting a result type like Rust's. if function `readFile($filePath)` could throw an exception, use `#readFile($filePath)` and this should return a hypothetical `Result<string, IOException>`

# Types

- There should be primitive types, `string`, `number`, `bool`
- Collections:
    - `List`, like in PHP, defined with `[1,2,3,4]`.
    - `Map`, like in PHP, defined with `{'Folgue' => 14, 'Daniel' => 24}`
- Symbols, just like in Ruby, `:symbol`
- `Self` should be a placeholder for the name of the current interface/class/enum in where its placed. *(i.e. `Self createFromString(string $name) {...}`)*

## OOP and functions

### Functions

- Should allow parameter types
- Should allow default values
- Should allow varargs
- Should allow calling them while naming parameters
- Should allow specifying return types
- Should allow one line definitions
- Should allow being called with parenthesis if its not ambiguous

Examples:
```
// ---------------------
// Definition
string greet(string $name) = f"Hello {$name}";

// Execution
greet "Folgue";

// ---------------------
// Definition
string greet(string $name = "user") {
    return f"Hello {$name}""
}

// Call
greet;
greet "Folgue";

// ---------------------
// Definition
string 
```

### Classes

- Single inheritance.
- Accepts generic types.
- Non-static methods/variables should be accessed with `@` as a prefix, i.e. `@readFile(@pathName)`, same as `this.readFile(this.pathName)`

### Interfaces

- Should allow defining static methods.
- Should allow defining methods with body.
- Should allow defining without method.

Example:
```
interface Worker {
    bool working?();
    string name() {
    
    }
}
```

## Enums

- Pretty similar to Java's.
- Should have methods such as `ordinal()` to get the position in the variant list, `values()`
```
enum Colors {
    RED,
    GREEN,
    BLUE
}

enum Vehicles {
    MOTORCYCLE 2,
    CAR 4,
    TRICYCLE 3;

    public int @wheels;

    private new(int $wheels : @) {
    }
}
```

# Collections

The following collections are defined as interfaces, which can be implemented by other classes.

But all of them have a default implementation to be used when initialized.

## List

Defined just like in PHP
```
let $fruits = ['Oranges', 'Apples', 'Tomatoes'];
```

There should also be two 

