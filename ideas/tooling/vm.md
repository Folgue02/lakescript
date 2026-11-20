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

## Where it lives

- `crates/lakescript/src/bytecode.rs`: the instructions and `Proto`, a compiled function.
- `crates/lakescript/src/compiler.rs`: the syntax tree to bytecode, after the type check.
- `crates/lakescript/src/interpreter.rs`: the VM. The methods of the built-in types and the standard library are the same as before.
