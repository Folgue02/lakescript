# Flow control

## Conditionals

- Curly braces are optional if there is only one line as body.
- The condition must be a `bool` *(i.e. `if ($list)` is an error, use `if ($list.empty?())`)*.

Examples:
```
if ($name.upper?()) {

} else if ($name.blank?()) {

} else {

}

if ($age > 18 && $age < 23) {

}

if (-f $file)
    outln("File exists.");
```

## If as an expression

Like `switch`, an `if` can return a value. Then it must have an `else`, so there's always a value:

```
const $label = if ($count > 1) 'items' else 'item';

const $size = if ($bytes > 1_000_000) {
    const $mb = $bytes / 1_000_000;
    yield f"{$mb} MB";
} else if ($bytes > 1_000) 'KB' else 'bytes';
```

- A body with a single expression doesn't end with `;`, since the whole `if` is part of a statement that already ends with one.
- When the body is a block, `yield` gives its value, like in `switch`.
- For short cases, the ternary operator does the same: `$count > 1 ? 'items' : 'item'`.

## For loops

- `break` and `continue` can be used. There are no labelled breaks.
- Curly braces are optional if there is only one line as body.

```
for ($i in 1..5) {

}

for ($fruit in $fruits) {
    outln("Something");
}

// With the index
for ($i, $fruit in $fruits)
    outln(f"{$i}: {$fruit}");

// Maps, with the key and the value
for ($name => $score in $scores)
    outln(f"{$name}: {$score}");

// Maps, only the keys
for ($name in $scores)
    outln($name);

// With the index, and the element destructured, like `for ([$name, $age] in $rows)`
for ($i, [$name, $age] in $rows)
    outln(f"{$i}: {$name} is {$age}");

// The same for the values of a map
for ($city => [$lat, $lon] in $places)
    outln(f"{$city}: {$lat}, {$lon}");

// With the type of the element, checked against the elements
for (IGenerator $g in $generators)
    $g.next();
```

- After the index or the key, a `[` destructures the element, with the same pattern as `for ([$a, $b] in $rows)` and `let [$a, ...$rest] = ...`, `...$rest` included *(i.e. `for ($i, [$first, ...$others] in $rows)`)*. The checker types it the same way: the index is a number, the key has the type of the map's keys, and each name has the type of the elements of the inner lists. Decided after cinder.
- The variable of the element can have its type written *(i.e. `for (IGenerator $g in $list)`, `for ($i, string $line in $lines)`)*. The checker checks that the elements have it, like a declaration: a `list<Wolf>` can be gone through as `IAnimal`, and a `list<any>` as anything, like a declaration from an `any` value. A key of a map can't have one, and neither can destructuring. Without it, the type is the one of the elements.

## While loops

- `break` and `continue` can be used. There are no labelled breaks, and there is no `do-while`.
- Curly braces are optional if there is only one line as body.

```
while ($iterations < 18) {
    $iterations++;
}
```


## Switch

```
switch ($variable) {
    case 1 -> {
        // ...
    }
    case 2 -> outln('test');
    case 1..20 -> {
        // ...
    }
    default -> {
        // ...
    }
}
```

- The first case that matches runs, and only that one: there is no fall-through, so there is no `break`.
- A case can be:
    - A value, compared with `==` *(i.e. `case 1`, `case 'Daniel'`, `case :admin`)*.
    - A range, which matches the values inside it *(i.e. `case 1..20`)*.
    - Several values or ranges separated by commas, which matches if any of them does *(i.e. `case 1, 2, 3`, `case :admin, :editor`)*.
    - A type, which matches like `.is()`. Inside the case, the value has that type. It takes the same types as `.is()`, so function types can't be used:

```
switch ($value) {
    case string -> outln($value.upper());
    case number -> outln($value * 2);
    case list<any> -> outln(f"{$value.length()} items");
    default -> outln('Something else');
}
```
- The body is a block in curly braces, or a single statement or expression, which must end with `;`.

## Switch as an expression

A switch can return a value. Then it must be exhaustive: every possible value must match a case, which in practice means it needs a `default` *(except for a `bool`, if both `true` and `false` have a case)*.

```
const $label = switch ($age) {
    case 0..13 -> 'child';
    case 13..18 -> 'teenager';
    default -> 'adult';
};
```

When the body is a block, `yield` gives its value. `return` can't be used for this, since it would return from the function the switch is in:

```
const $price = switch ($product) {
    case :apple -> 1;
    default -> {
        const $base = lookupPrice($product);
        yield $base * 1.21;
    }
};
```
