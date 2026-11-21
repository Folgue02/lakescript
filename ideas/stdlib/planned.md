# Planned modules

What the standard library is getting next *(see [README.md](README.md))*. Terminal modules are discussed apart.

## Decided

- **`std/http`**, to make requests. A response with a 4xx or 5xx status isn't an exception: it's a response like any other, and the script checks its status.
- **CSV**, read and written as lists of rows, or as maps by the names of the columns.
- **`std/random`**: numbers, choosing, shuffling and UUIDs.

## Being discussed

- **`std/html`**, for scraping: a page is parsed, and its elements are found with CSS selectors *(i.e. `html::parse($page).select('a.title')`)*, then their text and attributes are read.
- **Formats grouped under `std/format/`**: `std/format/json`, `std/format/csv`, `std/format/toml`, maybe `std/format/xml`. `std/json` would move there.
- **One `std/archive` for every archive format**, instead of a module for each *(i.e. `std/archive/zip`, `std/archive/tar`)*.
- **Templates like Ruby's ERB**: a template and a `map<string, any>` of the values it uses give a string.
- **Classes of the standard library**: a response, an element of a page or an archive are objects with methods. The standard library has no classes yet, only functions *(see the philosophy in [README.md](README.md))*, so the modules above need it to be able to declare its own.

## Left out for now

Servers and sockets, databases, email and XML parsing beyond what `std/format` gets. Each one is big, and none is common in scripts.
