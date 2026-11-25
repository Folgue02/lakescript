# The compiler and the VM

From phase 5, a script is run in four steps: it's parsed into a syntax tree, the tree is type checked, the tree is compiled to bytecode, and a VM runs the bytecode. The tree-walking interpreter of phases 1 to 4 is gone: there is only one way to run a script, so there's only one to keep correct.

## Why

Measured at the end of phase 4 *(release build, macOS)*:

| | Time |
|---|---|
| Starting `lks examples/hello.lks` | 2.2 ms *(bash: 1.6 ms, Python: 13 ms)* |
| A loop with a `%` test, 3 million times | 525 ms *(Python: 126 ms)* |
| `fib(27)` | 109 ms *(Python: 25 ms)* |

Starting was already fast, but running was about 4 times slower than Python, because every variable was looked up by name through a chain of scopes, and every pass of a loop created a new scope. The goal of phase 5 was to be at least as fast as Python on these.

With the VM *(Python 3.14 for comparison, timed without its 13 ms of startup)*:

| | Tree-walker | VM | Python |
|---|---|---|---|
| The loop with a `%` test, 3 million times | 525 ms | 60 ms | 110 ms |
| Calling a one-line function 3 million times in a loop | | 90 ms | 100 ms |
| `fib(32)`: 7 million calls, each doing little | | 270 ms | 110 ms |

Loops and ordinary calls are faster than Python. Deep recursion of tiny functions is still about 2.5 times slower: CPython 3.14 is very optimized for exactly that, and catching up needs bigger changes *(a register VM, or specialized instructions)* that aren't worth it for scripts yet.

## Decisions

- **Variables are resolved when compiling.** Each local variable gets a numbered slot in its function's frame, so running a script never looks up a name.
- **Closures capture variables, not values**, like the notes say *(see [functions.md](../language/functions.md))*. A variable that a closure uses lives in a shared cell *(like Lua's upvalues)*. Variables no closure uses stay plain slots, so they cost nothing extra.
    - A variable declared inside a loop is a new variable on every pass, so closures created in different passes don't share it.
- **Calls to named functions are resolved when compiling**, to a slot, a cell, an upvalue, a global or a built-in.
    - A name that isn't declared yet gets a global slot, and using it checks that it was declared by then. So without the type check, a function can still call one declared after it, as long as it exists when the call runs.
- **Errors that the tree-walker found while running are still found while running**, at the same line: declaring a variable twice, changing a constant, calling a function that doesn't exist. The compiler writes an instruction that throws there. Only `break`, `continue`, `return` and `yield` in the wrong place stop the script before it starts.
- **Conditions are compiled with their comparison**: `if ($n < 2)` is one instruction that compares and jumps.
- **Call frames live on the heap**, not on Rust's stack. Recursion is limited by a maximum depth instead *(from the engine's stack budget: one call for every 64 bytes, from 256 up to 100,000 calls)*. Going past it throws the same "too many nested calls" exception as before.
    - Rust's stack is still used when Rust code calls a script's function *(i.e. `.map()` calling its function)*, and those calls still check the stack budget.
- **Exceptions** use a table of handlers in each function: throwing finds the closest `try` around the current instruction, in this function or in the ones that called it.
- **The bytecode is private to the library.** It can change between versions, so nothing outside the crate depends on it. `lks --bytecode` prints it, for debugging.
- **No cache yet**: see the [README](../README.md).

## The review after v0.10.0

The code was measured against Python 3.14 and profiled, and what could be fixed without giving anything up was fixed. The same release build before and after, on the same machine *(ms)*:

| | Before | After | Python |
|---|---|---|---|
| `fib(32)` | 290 | 251 | 158 |
| A loop with a `%` test, 3 million times | 75 | 59 | 152 |
| 2 million calls of a method | 183 | 119 | 124 |
| 3 million reads of a field | 118 | 74 | 58 |
| `.map()` and `.filter()` with lambdas, 1 million items | 160 | 115 | 92 |
| Building and splitting 300,000 strings | 139 | 99 | 92 |
| A map with string keys, 300,000 updates | 126 | 94 | 71 |
| 500,000 calls of `path::name()` | 113 | 80 | 103 |
| 1 million `outln` into a pipe | 545 | 69 | 167 |
| `$text[$i]` over a string of 200,000 characters | 1800 | 15 | 24 |
| Starting `lks hello.lks` | 3.4 | 2.2 | 13 |

What changed:

- **Output waits in a buffer when it isn't a terminal**, like Python's: a million lines were a million writes. On a terminal each line is still written at once. It's written out before the script reads stdin, asks something with `std/terminal` or runs a command, and when it ends. stderr never waits, so into the same pipe its lines can come before earlier ones of stdout, as in Python and C.
- **A place that reads a field or calls a method remembers what it found**, for objects of the class it saw last *(`InlineCache` in `bytecode.rs`)*: the field's or the method's number, so the next object of that class skips the search by name. A class never changes once it's made, so it stays right. The class is remembered through a weak reference, so its address isn't given to another class, and nothing is kept alive.
- **Regexes written in the code are compiled once for their place**, instead of looked up by their text every time they run.
- **Calls into Rust make fewer copies**: a method of a built-in type doesn't copy its name, the standard library shares its string arguments with the script, `module::function` makes no string to find the function, and the function of `.map()` gets its values straight in its frame.
- **Numbers, bools and `null` are copied and dropped without a call**: a fifth of `fib` was those calls.
- **The release profile**: one codegen unit, LTO and `panic = "abort"`, which is 5 to 15% faster and makes `lks` 6 MB instead of 9.
- **Starting**: the signatures of the built-in methods *(`prelude.txt`)* are read only for the types and modules a script uses, and once for every engine of a thread. Ctrl+C is a small handler of our own *(`lakescript::on_ctrl_c`)* instead of the `ctrlc` crate, which linked Foundation on macOS and started a thread.
- **An engine frees what its scripts declared when it's dropped.** A function keeps its top level and the top level keeps the function, so nothing was ever freed, which matters for a program that makes many engines.
- **`.length()` and indexing of strings are instant**: the strings being indexed are remembered, with where each character starts for the ones that aren't only ASCII *(see Strings in [language/builtin_types.md](../language/builtin_types.md))*. A loop over a long string was quadratic.
- `.length()` of a range is arithmetic, not a count.

What's left, and why:

- **`fib` is still 1.6 times Python's**: each call copies two shared pointers and makes a frame. Closing it needs a register VM or specialized instructions.
- **Data that refers to itself is never freed** while a script runs *(i.e. two objects that point at each other)*: values are reference counted, and only a garbage collector would free them. For scripts that end, it doesn't matter.
- **The VM keeps checking types while it runs**, also after the checker passed: types never change what the code does, and the check can be turned off.

## The review after phase 11

On the branch `optimize-interpreter`. The benchmarks are in `tools/bench`, with the same programs in Python *(`python3 tools/bench/run.py`, and `--python`)*. The same release build before and after, on an Apple M5, with Python 3.14 *(ms)*:

| | Before | After | Python |
|---|---|---|---|
| `fib(32)` | 197 | 202 | 126 |
| A loop with a `%` test, 3 million times | 49 | 47 | 118 |
| 2 million calls of a method | 98 | 71 | 76 |
| 3 million reads of a field | 63 | 59 | 103 |
| `.map()` and `.filter()` with lambdas, 1 million items | 102 | 72 | 75 |
| Building and splitting 300,000 strings | 102 | 47 | 73 |
| A map with string keys, 300,000 updates | 45 | 34 | 48 |
| 1 million small objects, made and compared with `==` | 496 | 203 | 232 |
| 2 million pushes to a list, and reading them by index | 145 | 117 | 158 |
| 2 million calls of a closure | 67 | 68 | 98 |
| Collatz in `while` loops, 6 million steps | 354 | 197 | 222 |
| A grid of 100 by 100 in a class, 200 passes | 298 | 202 | 191 |
| cinder's bot, 5 games *(turns a second)* | 669 | 868 | |

Wall times move by a few percent with the load of the machine, so changes were judged by CPU cycles and instructions too, which `run.py` shows.

What changed:

- **A small allocator, `lakescript::SmallAlloc`**, for `lks` and `ferry`. On macOS every `free()` of the system's allocator reads the clock, which was a fifth of the time of the strings benchmark. It keeps up to 1,024 freed blocks of each size up to 256 bytes, for each thread, like Python's allocator for small objects, and gives the rest to the system. It's pure Rust, without a dependency, so the reason the notes gave for not doing one *(a C library)* doesn't hold. It's opt-in: the library never uses it by itself, and a program that embeds Lakescript can with `#[global_allocator]`. A thread that ends keeps its blocks, 2 MB at the most, since a cache with a destructor could allocate while being used. Code that doesn't allocate got 2 to 3% slower from how the dispatch loop is compiled, for 30 to 55% on code that does.
- **An exact division stays a whole number** *(i.e. `6 / 3`)*, so `%` on what comes from it needs no `fmod`, which was 29% of the `while` benchmark. Scripts can't tell: `number` is one type, and `0 / -5` stays `-0`. `%` of whole floats is done as integers too.
- **`==` and the other operators on objects** find their method in a table of the class, made once when the class is made, instead of looking up `IEquals` and `equals?` by name every time, and the object is put before the argument without moving it.
- **The constructor that takes every field**, given all of them in order and nothing to compute first, uses the arguments as the fields: two allocations instead of four.
- **A variable gets a cell only when a closure can use it.** A nested function's own parameters no longer count, so a loop variable `$x` isn't a cell because a method has a parameter `$x`. Its other variables still do, since a body can use the one around it before declaring its own, which is an error only when it gets there.
- **Fields of a local object** *(`@x`, `$p.x`)* are read where the object is, and `@x = ...` set there, without copying the object onto the stack and dropping it again *(`GetLocalField`, `SetLocalField`)*.
- **`&&` and `||` in conditions are jumps**: no bool is made and checked, and the comparisons on their sides compare and jump in one instruction. A side that isn't a bool gives the same error, at the same place.
- **A loop's element goes straight into its variable**, from `ForNext` *(the kind `STORE`)*.
- **Calls and returns borrow the code** of the running function instead of cloning two shared pointers, a call takes the function from its place on the stack, and a frame's slots are made and dropped without a call for each number. Numbers are copied without a call where lists, cells, fields and constants are read.

What's left, and why:

- **`fib` is still 1.6 times Python's**, and calls in general cost the most: a frame, the function's reference count, and the `RefCell` of the globals. The instructions it runs went down 10%, but the time didn't follow: the dispatch loop already runs about 6 instructions a cycle, and what's left is work that waits on itself. Closing it needs a register VM or a bigger change to how calls work.
- **The dispatch loop is sensitive to its own size**: a new instruction, or a bigger arm of the `match`, can make the compiler lay out the rest worse *(i.e. a separate instruction for the loop's element made `while` loops 6% slower, so it became a kind of `ForNext`)*. Changes there are measured on cinder before they're kept.
- **A built-in method's arguments are a `Vec`**, one allocation for each call with arguments *(i.e. `$list.push($x)`)*. `SmallAlloc` makes it cheap, so it was left.

## Where it lives

- `crates/lakescript/src/bytecode.rs`: the instructions and `Proto`, a compiled function.
- `crates/lakescript/src/compiler.rs`: the syntax tree to bytecode, after the type check.
- `crates/lakescript/src/interpreter.rs`: the VM. The methods of the built-in types and the standard library are the same as before.
