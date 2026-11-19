//! Measuring how much stack is used, so deeply nested code or endless
//! recursion stops with an error instead of crashing the program.

/// Where the stack is now. Only the difference between two calls means anything.
#[inline(always)]
pub fn position() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// How much stack the parser may use by default. It's safe on threads with a
/// stack of 2 MB or more. The engine gives the parser its own stack budget.
pub const DEFAULT_BUDGET: usize = 1024 * 1024;

/// How deep interpolated strings can be nested inside each other.
pub const MAX_INTERPOLATION_DEPTH: usize = 32;
