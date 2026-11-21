# std/template

Templates in the style of Ruby's ERB: a text with tags in double curly braces, and a `map<string, any>` of the values it uses, give a string. `use std/template;`, then `template::render($text, $values)`.

```
<h1>{{ $title }}</h1>
{{ // a comment, which writes nothing }}
{{ if ($items.empty?()) }}
  <p>Nothing here.</p>
{{ else }}
  <ul>
  {{ for ($item in $items) }}
    <li>{{ $item['name'] }}: {{ $item['price'] }} €</li>
  {{ endfor }}
  </ul>
{{ endif }}
{{ const $total = $items.map(($i) -> $i['price']).sum() }}
<p>Total: {{ $total }} €</p>
```

```
use std/template;

const $page = template::renderFile('page.html.elks', {'title' => 'Shop', 'items' => $items});
const $text = template::render('Hello {{ $name.upper() }}!', {'name' => 'Ana'});
```

## Files

**Templates end in `.elks`**, for "embedded Lakescript", like Ruby's `.erb`: `page.html.elks` is a template that gives HTML, `notes.md.elks` one that gives Markdown. So editors know which files have `{{ }}` tags with Lakescript inside: farsub highlights them as HTML with Lakescript in its tags. `renderFile` takes any file, with this extension or not.

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `render(string $text, map<string, any> $values = {}, bool $html = false)` | `string` | The template `$text` with `$values`. With `$html`, the values it writes are escaped for HTML |
| `renderFile(string $path, map<string, any> $values = {}, ?bool $html = null)` | `string` | The template in the file `$path`. It's escaped for HTML when the file ends in `.html` or `.htm`, also before `.elks` *(i.e. `page.html.elks`)*, unless `$html` says otherwise |

## Tags

- **`{{ expression }}` writes its value**, as `outln` writes it. `null` writes nothing, so `{{ $user['nickname'] }}` leaves nothing on the page when there's no nickname.
- **Blocks:** `for`, `if`, `else if`, `else` and `while` open a block without `{`, with what's inside written as in Lakescript *(i.e. `{{ for ($item in $items) }}`, `{{ if ($n > 1) }}`)*. Each block closes with its own end: `{{ endfor }}`, `{{ endif }}`, `{{ endwhile }}`. So nested blocks don't end in a row of `{{ } }}` like PHP's `<?php } ?>`, and an end in the wrong place says which block it expected.
- **What changes a variable writes nothing**: `const` and `let` *(i.e. `{{ const $total = $prices.sum() }}`)*, assignments *(i.e. `{{ $i += 1 }}`)*, `break` and `continue`. `{{ // ... }}` is a comment, and ends at the first `}}`.
- **An `if` that gives a value** is an expression, not a block: `{{ if ($n > 1) 'items' else 'item' }}` writes one of them. A tag is a block when the condition in parentheses is all there is after the `if`.
- **A line with only a block tag, a definition or a comment disappears**, also its new line, so a template of a config file or of Markdown doesn't fill up with empty lines.
- **A `{{` in the text** is written as `{{ '{{' }}`.

## What a template sees

- The keys of the map, as constants *(i.e. `'title'` is `$title`)*. A key that isn't a valid name throws a `ValueError`.
- The built-in functions and the methods of the values.
- Not the variables of the script that renders it, nor its modules. A function is passed in the map *(i.e. `{'money' => ($n) -> f"{$n} €"}`, then `{{ $money($price) }}`)*.

## HTML

- With HTML escaping on, `<`, `>`, `&`, `"` and `'` in the values that are written become entities, so a value a user typed can't add HTML to the page. The text of the template itself is never changed.
- `{{ raw($x) }}` writes a value without escaping it.

## Errors

A template becomes Lakescript code that builds the string, and runs as a script of its own whose variables are the values given. Its errors point at the line and column of the template, also the ones of the language *(i.e. ``page.html.elks:12:9: error: `$nmae` isn't declared``)*, and `$e.line()` is that line. A template made with `render` is `<template>` in its errors.

- A name that isn't declared is found when its tag runs, not before, since the values only come with the call.
- A block that isn't closed, an end of another block *(i.e. `{{ endif }}` closing a `{{ for }}`)*, an `{{ else }}` outside an `{{ if }}` and a `{{` without `}}` are errors before anything runs.
- A template can't catch errors: a `try` is a block, which tags don't have. The script that renders it can.

## How it's made

It's in `crates/lakescript/src/stdlib/template.rs`. The template is turned into the code of a script: its text is written by `__text(n)`, `{{ $x }}` becomes `__write($x)`, `{{ raw($x) }}` becomes `__raw($x)`, and a block becomes the block of the language with `{` and `}`. `render` and `renderFile` are functions that run Lakescript code, so they get the VM *(a `Function::Vm`)*, while the other functions of the standard library only get their arguments. Where each part of the code came from is kept, to move errors back to the template.

- **Nothing is cached:** every call turns the template into code and compiles it again. A small template takes about 13 µs per render in a release build, against about 0.6 µs for the same string built by hand, which scripts don't notice. A cache is left out until a script needs it.
- **A template is code:** it can do anything a script can. Templates are written by the author of the script, so a template that came from a user must never be rendered, as with ERB and Jinja.

## Backlog

- Templates that include others *(i.e. `{{ include('header.html') }}`)*.
- `use` inside templates.
- A cache of compiled templates, by their text and the names of their values, if rendering the same template many times gets slow.
