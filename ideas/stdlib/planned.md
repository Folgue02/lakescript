# Planned modules

What the standard library is getting next *(see [README.md](README.md))*. Terminal modules are discussed apart.

## Decided

- **`std/http`**, to make requests *(see [http.md](http.md))*.
  - A response with a 4xx or 5xx status isn't an exception: it's a response like any other, and the script checks its status.
  - When there's no response at all *(no network, a name that doesn't resolve, a refused connection, a timeout)*, it throws a `NetworkError`, a new built-in exception under `IOError`.
- **Formats grouped under `std/format/`**: `std/format/json`, `std/format/csv`, later `std/format/toml` and maybe `std/format/xml`. `std/json` moves there without a period of both, since only its author uses the language while it's in development *(see [roadmap.md](../roadmap.md))*. After `use std/format/json;` the module is still called `json`.
  - The formats share their names where they fit: `parse($text)` and `stringify($value)` for text, `read($path)` and `write($path, $value)` for files.
- **CSV** in `std/format/csv`, read and written as lists of rows, or as maps by the names of the columns.
- **`std/random`**: numbers, choosing, shuffling and UUIDs.
- **Standard library modules can be written in Lakescript**, embedded in the interpreter, with a part in Rust *(see How modules are made in [README.md](README.md))*. So they can declare classes like `http::Response`, checked and shown in editors like any other code.
- **Templates like Ruby's ERB**, built in `std/template`: a template and a `map<string, any>` of the values it uses give a string.
  - The tags use double curly braces, and what's inside is Lakescript *(i.e. `{{ $name.upper() }}`)*. The keys of the map become variables.
  - `for`, `if` and the like get a shorter form in templates, so nested blocks don't end in a row of `{{ } }}` like PHP's `<?php } ?>`.
  - Blocks open without `{` and close with their own end: `{{ endfor }}`, `{{ endif }}`, `{{ endwhile }}`.
  - `renderFile` escapes HTML for `.html` and `.htm` files, `render` only with `html: true`, and `{{ raw($x) }}` writes a value without escaping. `null` writes nothing. The rest is in [template.md](template.md).
- **One `std/archive` for every archive format, with objects.** `archive::open($path)` gives an `archive::Archive` to read, and `archive::create($path)` one to write, which is finished with `close()`. An archive has `entries()`, `read($name)`, `extract($dir)`, `add($path, as:)` and `addText($name, $text)`, and its entries are `archive::Entry` values with `name`, `size`, `dir?()` and `modified`. The format comes from the name: `.zip`, `.tar`, `.tar.gz` or `.tgz`. Plain functions *(i.e. `archive::extract($file, $dir)`)* were the other option, but an open archive can do several things without being opened again, and adding files in a loop is natural. The names of the methods can still change when it's built.
  ```
  const $zip = archive::open('site.zip');
  for ($entry in $zip.entries()) {
      outln(f"{$entry.name}  {$entry.size} bytes");
  }
  $zip.extract('out');

  const $backup = archive::create('backup.tar.gz');
  $backup.add('photos/', as: 'pictures/');
  $backup.close();
  ```
- **Testing**, built: tests are functions named `test...`, checked with `assert`, a statement of the language that shows both sides of a comparison that failed. `std/test` has only what `assert` can't do *(i.e. `test::throws($fn)`)*. Tests see the `pub` items of the project's modules. `ferry test` runs the files in `tests/`, and `lks --test` the tests of a script *(see [testing.md](../tooling/testing.md))*.
- **Crates instead of writing our own, when their licenses allow it.** The project is Apache-2.0, so MIT, Apache-2.0, BSD, ISC, Zlib and Unlicense are fine. MPL-2.0 is fine too, as long as its files aren't changed: it's copyleft only for its own files. Checked so far:
  - `ureq` (HTTP) and everything it uses are MIT, Apache-2.0, ISC, BSD or Zlib. Its certificates come from `webpki-roots`, under CDLA-Permissive-2.0, a permissive license for data.
  - `csv` is MIT or Unlicense.
  - `scraper` (HTML and CSS selectors) is ISC. It uses `html5ever` (MIT or Apache-2.0), and `selectors` and `cssparser` from Servo, which are MPL-2.0.

## Being discussed

- **`std/html`**, for scraping: a page is parsed with `html5ever`, the parser of Servo, which repairs broken HTML the way browsers do. Its elements are found with CSS selectors *(i.e. `html::parse($page).select('a.title')`)*, then their text and attributes are read. With the `scraper` crate rather than a selector engine of our own.

## Left out for now

Servers and sockets, databases, email and XML parsing beyond what `std/format` gets. Each one is big, and none is common in scripts.
