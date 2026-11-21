# std/html

Reading HTML pages and finding their parts with CSS selectors, to scrape them. `use std/html;`, then `html::parse($text)`. It's written in Lakescript, in `crates/lakescript/src/stdlib/html.lks`, with its Rust part in `html.rs` *(see How modules are made in [README.md](README.md))*. Pages are parsed by `html5ever`, the parser of Servo, through the `scraper` crate, so broken HTML is repaired the way browsers do it.

```
use std/http;
use std/html;

const $page = html::parse(http::get('https://news.ycombinator.com').body);
for ($link in $page.select('.titleline > a')) {
    outln(f"{$link.text()} -> {$link.attr('href')}");
}
outln($page.title());
```

- It doesn't download anything: pages come from `std/http`, a file or anywhere else.
- `select` and `first` take any CSS selector browsers know *(i.e. `li:not(.sold) > a`, `a[href^="https"]`, `tr:nth-child(2) td`)*. One that isn't valid throws a `ValueError`.

## Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `parse(string $text)` | `Document` | Reads a page. It never fails: broken HTML is repaired, like in browsers |
| `escape(string $text)` | `string` | The text with `&`, `<`, `>`, `"` and `'` as entities, to put it in HTML |
| `unescape(string $text)` | `string` | The text with its entities as characters *(i.e. `&amp;` is `&`, `&copy;` is `©`)* |

## Document

A page, from `parse()`.

| Method | Returns | Description |
|--------|---------|-------------|
| `select(string $css)` | `list<Element>` | Every element that matches the selector, in the order of the page |
| `first(string $css)` | `?Element` | The first one, or `null` |
| `title()` | `?string` | The text of its `<title>`, or `null` |
| `text()` | `string` | The text of the page as a browser shows it |
| `html()` | `string` | The whole page as HTML, repaired |

## Element

An element of a page *(i.e. `<a href="...">`)*.

| Field | Type | Description |
|-------|------|-------------|
| `tag` | `string` | The name of its tag, in lowercase *(i.e. `a`)* |

| Method | Returns | Description |
|--------|---------|-------------|
| `select(string $css)` | `list<Element>` | Every element inside it that matches the selector. The element itself isn't one, as with `querySelectorAll` in browsers |
| `first(string $css)` | `?Element` | The first one, or `null` |
| `text()` | `string` | Its text as a browser shows it *(i.e. `Hello world` for `<p>Hello <b>world</b></p>`)* |
| `attr(string $name)` | `?string` | An attribute, or `null` *(i.e. `$link.attr('href')`)* |
| `attrs()` | `map<string, string>` | Every attribute, in the order of the page |
| `html()` | `string` | The HTML inside it |
| `outerHtml()` | `string` | Its HTML, with its own tag |
| `children()` | `list<Element>` | The elements right inside it |
| `parent()` | `?Element` | The element it's inside, or `null` at the top |

## Text

`text()` gives the text the way a browser shows it, not the way it's written:

- Spaces and new lines are collapsed into one space, and the ends are trimmed.
- Elements that start a line of their own *(i.e. `<p>`, `<div>`, `<li>`, `<br>`, `<td>`)* are separated by a space, so `<p>a</p><p>b</p>` is `a b`, not `ab`.
- The text of `<script>`, `<style>`, `<template>`, `<noscript>` and `<head>` isn't shown, unless it's the element whose `text()` is asked for. So `$page.first('script[type="application/ld+json"]')?.text()` gives the JSON in it.

## How it's made

- An element is a `Document` or `Element` object whose hidden field holds the page in Rust *(a `Value::Handle`)*, which scripts never see. The page is freed when no element of it is used anymore, so scraping many pages doesn't keep them in memory. Printing an element leaves that field out *(i.e. `Element(tag: 'li')`)*.
- `std/html/native` is its Rust part, which only `html.lks` can use.

## Backlog

- Changing a page *(i.e. removing elements)*, which `scraper` doesn't do.
- XPath. CSS selectors do what scripts need.
