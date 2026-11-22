# std/terminal

*Built: layers 1 and 2. `form` and `std/tui` come later.*

Talking with the person who runs the script: asking them things, colors and styles, the size of the terminal, progress, and ready-made interactive pieces like a fuzzy finder or a pager. `use std/terminal;`, then `terminal::ask('Your name?')`.

It has two layers:

1. **The basics**: prompts, styles, what the terminal is like, single keys, progress.
2. **Ready-made interactive pieces**, like `gum` and `fzf`: behavior of a TUI from one call, without programming one.

Full TUIs, with a screen the script draws and an event loop, are a third layer, in `std/tui`, and they're in the backlog *(see [planned.md](planned.md))*.

## Prompts

```
use std/terminal;

const $name = terminal::ask('Your name?', default: 'Ana');
const $pass = terminal::password('Password:');
if (terminal::confirm('Delete 3 files?', default: false)) {
    ...
}
const $env = terminal::choose('Deploy to', ['staging', 'production']);
const $tags = terminal::chooseMany('Tags', ['bug', 'docs', 'ui']);
const $port = terminal::ask('Port?', check: ($s) -> $s.digits?() ? null : 'a number, please');
```

| Function | Returns | Description |
|----------|---------|-------------|
| `ask(string $question, ?string $default = null, ?(string) -> ?string $check = null)` | `string` | A line the person writes. `$check` gives an error message, or `null` when the answer is fine, and the question is asked again until it is |
| `password(string $question)` | `string` | The same, without showing what's written |
| `confirm(string $question, ?bool $default = null)` | `bool` | Yes or no |
| `choose(string $question, list<string> $options)` | `string` | One of the options, picked with the arrow keys |
| `chooseMany(string $question, list<string> $options)` | `list<string>` | Several, ticked with the space bar |

- One `ask` with `check:` instead of `askNumber`, `askEmail` and the like.
- **Prompts are written to stderr**, so what the script writes to stdout can still be piped or captured *(i.e. `name=$(lks script.lks)`)*, like `gum` and `fzf` do.
- **Without a person at the terminal** *(stdin piped, CI)*, prompts read their answer from stdin, a line each, so `echo y | script.lks` works. `choose` takes a line with one of the options, and `chooseMany` one with options separated by commas. When stdin has nothing more, the default is the answer, and without a default it's an error.
    - A `$check` that doesn't like an answer from stdin throws a `ValueError` with its message, since there's no person to ask again.
    - **Except `password`**, which never reads from a pipe: without a terminal it throws an `IOError`, so a secret never comes from a file or a command by accident.
- **Ctrl+C** throws an `Interrupted`, like it does anywhere in a script *(see Ctrl+C in [errors.md](../language/errors.md))*. While a prompt or a ready-made piece waits, the terminal is in raw mode, so Ctrl+C arrives as a key: the terminal is put back as it was first, then it's thrown. Escape gives up a prompt the same way, except in `filter`, where it gives `null`.
- On a terminal, what `$check` says is shown under the question, after a `✗`, and the question is asked again.

## Styles

One function, with the styles as the values of an enum, `Style`. Colors, backgrounds and effects are all values of it, given in the same place. It's a type module of its own, `std/terminal/Style`, so `use` gives `Style` itself *(see type modules in [modules.md](../language/modules.md))*:

```
use std/terminal;
use std/terminal/Style;

errln(terminal::style('error:', Style::RED, Style::BOLD) + ' the file is missing');
outln(terminal::style(' OK ', Style::BLACK, Style::ON_GREEN));
```

The values of `Style` *(as many as terminals can show, with plain names)*:

- **Colors**: `BLACK`, `RED`, `GREEN`, `YELLOW`, `BLUE`, `MAGENTA`, `CYAN`, `WHITE`, and their bright versions `BRIGHT_RED` and so on. Also `GRAY`, `ORANGE`, `PINK`, `PURPLE` and `BROWN`, from the 256 colors most terminals have.
- **Backgrounds**: the same colors after `ON_` *(i.e. `ON_RED`, `ON_BRIGHT_BLUE`, `ON_GRAY`)*.
- **Effects**: `BOLD`, `DIM`, `ITALIC`, `UNDERLINE`, `DOUBLE_UNDERLINE`, `CURLY_UNDERLINE`, `OVERLINE`, `STRIKE`, `BLINK`, `INVERSE` and `HIDDEN`. A terminal that doesn't have one ignores it.

| Function | Returns | Description |
|----------|---------|-------------|
| `style(string $text, ...Style $styles)` | `string` | The text with the codes of those styles around it |
| `strip(string $text)` | `string` | The text without the codes of styles |
| `visibleWidth(string $text)` | `number` | How many columns the text takes, without the codes and with characters that take two columns *(i.e. emoji)* counted twice |

- An enum instead of symbols or a function for each color: there are few names to learn, and the checker catches a misspelled style and completes them.
- Styles turn themselves off when `NO_COLOR` is set or the output isn't a terminal, so the codes never end up in a file or a pipe.

## The terminal

| Function | Returns | Description |
|----------|---------|-------------|
| `interactive?()` | `bool` | `true` when there's a person at a terminal: stdin and stderr are terminals |
| `width()` | `number` | Its width, in columns, or 80 when it isn't a terminal |
| `height()` | `number` | Its height, in lines, or 24 when it isn't a terminal |
| `readKey()` | `string` | One key, without waiting for enter *(i.e. `'a'`, `'enter'`, `'up'`, `'space'`, `'escape'`, `'ctrl+d'`, `'f1'`)*. Ctrl+C throws an `Interrupted`, and without a terminal it throws an `IOError` |
| `clear()` | `void` | Clears the screen |

- A person is at a terminal when stdin and stderr are terminals, and the script isn't run by a program that captures what it writes *(i.e. the tests of the engine)*.

## Progress

```
const $bar = terminal::progress($files.length(), 'Copying');
for ($f in $files) {
    copy($f);
    $bar.advance();
}
$bar.finish();

// Without a total, it's a spinner that turns with each `advance()`
const $pages = terminal::progress(null, 'Reading pages');
while ($next != null) {
    $next = scrape($next);
    $pages.advance();
}
$pages.finish();
```

| Function | Returns | Description |
|----------|---------|-------------|
| `progress(?number $total, string $label = '')` | `Progress` | A bar that fills up to `$total`. With `null`, a spinner that shows that something is happening, without a percentage |

A `Progress` has `advance(number $n = 1)`, which moves it forward, and `finish()`, which leaves it full and goes to the next line. Its fields `total`, `label` and `done` can be read.

- It's drawn on stderr, and only when stderr is a terminal, so a log or a pipe never gets it. It's drawn at most every 40 ms, so a fast loop doesn't slow down for it.

- **No spinner that turns on its own** while the script's code runs *(i.e. `spinner('Downloading', () -> ...)`)*. It would need a thread drawing while the VM runs, which collides with what the script prints, with the commands it runs and with Ctrl+C: a small concurrency system hidden in one function, in a language without concurrency. `progress(null)` turns with each `advance()` instead, which covers loops. For one long call, a line like `Downloading...` is enough. It can come back if the language gets concurrency.

## Ready-made pieces

| Function | Returns | Description |
|----------|---------|-------------|
| `filter(list<string> $items, string $question = 'Search')` | `?string` | Typing filters the list, fuzzily, like `fzf`. The item picked, or `null` when the person gives up with Escape. Without a terminal, a line of stdin that's one of the items, or `null` |
| `pager(string $text)` | `void` | Shows long text to scroll through, with `$PAGER` or `less -R`. When stdout isn't a terminal, it writes the text |
| `table(list<any> $rows, ?list<string> $header = null)` | `void` | Writes the rows with their columns lined up, or lets the person scroll through them when they don't fit. Rows are lists, or maps whose keys are the header, like the rows of [format/csv.md](format/csv.md). Without a header, maps give it with the keys of the first one, and `null` is an empty cell |
| `edit(string $text)` | `string` | Opens `$VISUAL` or `$EDITOR` *(or `vi`)* on the text in a temporary file, and gives what was saved, like `git commit`. Without a terminal, it throws an `IOError` |

## How it's made

- It's written in Lakescript, in `crates/lakescript/src/stdlib/terminal.lks`, with `Style` in `terminal/Style.lks` and its Rust part, `std/terminal/native`, in `terminal.rs` *(see How modules are made in [README.md](README.md))*.
- On `crossterm` 0.29, which also works on Windows. The prompts are `inquire` 0.9, with its `fuzzy` feature for `filter`, drawn on stderr. Widths come from `unicode-width`. All MIT, or MIT and Apache-2.0.
- `style` asks `Style` values for their names *(`.name()`)* and the Rust part turns them into codes: 30–37 and 90–97 for colors, 40–47 and 100–107 for backgrounds, `38;5;n` and `48;5;n` for the 256 colors, and the codes of the effects, all in one `ESC [ ... m` with `ESC [0m` after the text.
- Ctrl+C in a prompt is `inquire`'s `OperationInterrupted`, turned into an `Interrupted`. In `readKey`, it's a key in raw mode, which is put back first.
- The tests are in `crates/lakescript/tests/std_terminal.rs`, without a terminal. The prompts on one were tried in a pseudo-terminal.

## Not doing

- A rendering engine of our own, or an API like curses.
- Widgets as hierarchies of classes.
- The mouse and images in the terminal, at first.
- Async or concurrency, and so a spinner that turns on its own.

## Later

- `form`, several prompts on one screen. Each field needs a kind, a question and a default, which is a class of its own *(i.e. `Field`)* or maps the checker can't help with. Until people want it, `ask`, `confirm` and `choose` one after the other do the same.
