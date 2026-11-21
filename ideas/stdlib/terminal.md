# std/terminal

*Decided, not built yet.*

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
    - **Except `password`**, which never reads from a pipe: without a terminal it throws an `IOError`, so a secret never comes from a file or a command by accident.
- **Ctrl+C** while a prompt or a ready-made piece waits throws an `Interrupted`, a new built-in exception, after putting the terminal back as it was. The script can catch it *(i.e. to clean up, or to take it as a "no")*. Not caught, the script stops with the code 130, like shells do, without the message of an error.

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
| `width()` | `number` | Its width, in columns |
| `height()` | `number` | Its height, in lines |
| `readKey()` | `string` | One key, without waiting for enter *(i.e. `'a'`, `'enter'`, `'up'`, `'ctrl+c'`)* |
| `clear()` | `void` | Clears the screen |

## Progress

```
const $bar = terminal::progress($files.length(), 'Copying');
for ($f in $files) {
    copy($f);
    $bar.advance();
}
$bar.finish();

terminal::spinner('Downloading', () -> http::download($url, 'x.tgz'));
```

| Function | Returns | Description |
|----------|---------|-------------|
| `progress(?number $total, string $label = '')` | `Progress` | A bar that fills up to `$total`. With `null`, it only shows that something is happening, without a percentage |
| `spinner(string $label, () -> T $code)` | `T` | Runs `$code` with a spinner next to `$label`, and gives what it returned |

A `Progress` has `advance(number $n = 1)`, which moves it forward, and `finish()`, which leaves it full and goes to the next line.

- A spinner turns while the code runs. Lakescript has no concurrency, so a thread in Rust draws it while the VM runs the code.

## Ready-made pieces

| Function | Returns | Description |
|----------|---------|-------------|
| `filter(list<string> $items)` | `?string` | Typing filters the list, fuzzily, like `fzf`. The item picked, or `null` when the person gives up |
| `pager(string $text)` | `void` | Shows long text to scroll through, like `less` |
| `table(list<any> $rows, ?list<string> $header = null)` | `void` | Writes the rows with their columns lined up, or lets the person scroll through them when they don't fit. Rows are lists, or maps whose keys are the header, like the rows of [format/csv.md](format/csv.md) |
| `edit(string $text)` | `string` | Opens `$EDITOR` on the text and gives what was saved, like `git commit` |

## How it's made

- On `crossterm`, which also works on Windows, a supported platform. It's MIT.
- The prompts may come from `inquire` (MIT), or be written on `crossterm`, so they look like the rest.

## Not doing

- A rendering engine of our own, or an API like curses.
- Widgets as hierarchies of classes.
- The mouse and images in the terminal, at first.
- Async or concurrency: the spinner's thread is inside the module.

## Later

- `form`, several prompts on one screen. Each field needs a kind, a question and a default, which is a class of its own *(i.e. `Field`)* or maps the checker can't help with. Until people want it, `ask`, `confirm` and `choose` one after the other do the same.
