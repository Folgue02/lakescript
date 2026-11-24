# Planned modules

What the standard library is getting next *(see [README.md](README.md))*. Terminal modules are discussed apart.

## Decided

- **`std/http`**, to make requests *(see [http.md](http.md))*.
  - A response with a 4xx or 5xx status isn't an exception: it's a response like any other, and the script checks its status.
  - When there's no response at all *(no network, a name that doesn't resolve, a refused connection, a timeout)*, it throws a `NetworkError`, a new built-in exception under `IOError`.
- **Formats grouped under `std/format/`**: `std/format/json`, `std/format/csv`, later `std/format/toml` and maybe `std/format/xml`. `std/json` moves there without a period of both, since only its author uses the language while it's in development *(see [roadmap.md](../roadmap.md))*. After `use std/format/json;` the module is still called `json`.
  - The formats share their names where they fit: `parse($text)` and `stringify($value)` for text, `read($path)` and `write($path, $value)` for files.
- **CSV** in `std/format/csv`, read and written as lists of rows, or as maps by the names of the columns.
- **`std/random`**, built *(see [random.md](random.md))*: `number()`, `between($min, $max)`, `bool()`, `uuid()` and `seed($n)`. Choosing and shuffling stay on lists *(`$list.random()`, `$list.shuffle()`)*, and `seed()` makes them repeatable too.
- **Standard library modules can be written in Lakescript**, embedded in the interpreter, with a part in Rust *(see How modules are made in [README.md](README.md))*. So they can declare classes like `http::Response`, checked and shown in editors like any other code.
- **Templates like Ruby's ERB**, built in `std/template`: a template and a `map<string, any>` of the values it uses give a string.
  - The tags use double curly braces, and what's inside is Lakescript *(i.e. `{{ $name.upper() }}`)*. The keys of the map become variables.
  - `for`, `if` and the like get a shorter form in templates, so nested blocks don't end in a row of `{{ } }}` like PHP's `<?php } ?>`.
  - Blocks open without `{` and close with their own end: `{{ endfor }}`, `{{ endif }}`, `{{ endwhile }}`.
  - `renderFile` escapes HTML for `.html` and `.htm` files, `render` only with `html: true`, and `{{ raw($x) }}` writes a value without escaping. `null` writes nothing. The rest is in [template.md](template.md).
- **One `std/archive` for every archive format, with objects**, built *(see [archive.md](archive.md))*: `archive::open($path)` and `archive::create($path)` give an `Archive`, with `entries()`, `read()`, `extract()`, `add($path, as:)`, `addText()` and `close()`, for `.zip`, `.tar`, `.tar.gz` and `.tgz`.
- **`std/html`**, built *(see [html.md](html.md))*: `html::parse($text)` gives a `Document`, whose `select($css)` and `first($css)` give `Element`s, with `text()`, `attr()` and the like. On `scraper`, not a selector engine of our own.
- **`std/terminal`** *(see [terminal.md](terminal.md))*: prompts *(`ask`, `password`, `confirm`, `choose`)*, styles, the size of the terminal, single keys and progress, and ready-made interactive pieces like `filter` *(like `fzf`)*, a pager and `edit` *(`$EDITOR`)*. Styles are one function with the values of an enum, `terminal::style($text, Style::RED, Style::BOLD)`. Prompts write to stderr.
- **Testing**, built: tests are functions named `test...`, checked with `assert`, a statement of the language that shows both sides of a comparison that failed. `std/test` has only what `assert` can't do *(i.e. `test::throws($fn)`)*. Tests see the `pub` items of the project's modules. `ferry test` runs the files in `tests/`, and `lks --test` the tests of a script *(see [testing.md](../tooling/testing.md))*.
- **Crates instead of writing our own, when their licenses allow it.** The project is Apache-2.0, so MIT, Apache-2.0, BSD, ISC, Zlib and Unlicense are fine. MPL-2.0 is fine too, as long as its files aren't changed: it's copyleft only for its own files. Checked so far:
  - `ureq` (HTTP) and everything it uses are MIT, Apache-2.0, ISC, BSD or Zlib. Its certificates come from `webpki-roots`, under CDLA-Permissive-2.0, a permissive license for data.
  - `csv` is MIT or Unlicense.
  - `scraper` (HTML and CSS selectors) is ISC. It uses `html5ever` (MIT or Apache-2.0), and `selectors` and `cssparser` from Servo, which are MPL-2.0.

## Being discussed

Nothing right now.

## Backlog

- **`std/tui`**, for full TUIs: a screen the script draws and an event loop. The idea so far is the Elm architecture: a state, a function that gives the new state for each event, and one that gives what the screen shows, with the library running the loop and always putting the terminal back. Drawn with `ratatui` (MIT) on `crossterm`. It's an experiment, after `std/terminal`.

## Left out for now

Servers and sockets, databases, email and XML parsing beyond what `std/format` gets. Each one is big, and none is common in scripts.
