# OOP - Still in development, ignore

## Classes

- Single inheritance.
- Accepts generic types.
- Non-static methods/variables should be accessed with `@` as a prefix, i.e. `@readFile(@pathName)`, same as `this.readFile(this.pathName)`
- Classes can have static methods, they are accessed with `::` from the outside, and from within the class itself, they can be called with `Self::method`.

Examples:
```
class Person {
    public string @name;
    public number @age;

    // Constructor
    public new(string $name, number $age) {
        @name = $name;
        @age = $age;
    }

    // Private method
    private void grow() {
        @age++;
    }

    // Public method
    public void celebrateBirthday() {
        @grow;
        puts f"Happy birthday {@name}!";
    }

    // Static method
    public static Self createFromString(string $data) {
        const ($name, $ageStr) = $data.split(':');
        const $age = parseInt($ageStr);
        return Self::new($name, $age);
    }
}

```

## Interfaces

- Should allow defining static methods.
- Should allow defining methods with body.
- Should allow defining without method.

Example:
```
interface Worker {
    bool working?();
    string name() {
        // Code
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
    MOTORCYCLE(2),
    CAR(4),
    TRICYCLE(3);

    public number @wheels;

    private new(number $wheels : @) {
    }
}
```

## Referencing types themselves

- There should be a type named `type`

