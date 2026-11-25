//! `SmallAlloc`, a cache of small freed blocks in front of the system's
//! allocator, one for each thread, like Python's allocator for small objects
//! *(see the review after phase 11 in `ideas/tooling/vm.md`)*. Scripts make
//! and drop many small values *(strings, lists, objects, the arguments of a
//! call)*, and on macOS the system's `free` reads the clock every time.
//!
//! The library never uses it by itself: a program opts in, like `lks` and
//! `ferry` do:
//!
//! ```ignore
//! #[global_allocator]
//! static ALLOCATOR: lakescript::SmallAlloc = lakescript::SmallAlloc;
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ptr::null_mut;

/// Blocks of up to 256 bytes, in 16 classes of 16 bytes each.
const CLASSES: usize = 16;
const STEP: usize = 16;
/// The most blocks a class keeps, so what a script freed goes back to the
/// system after that: about 2 MB for each thread at the most.
const KEEP: usize = 1 << 10;

/// A global allocator that keeps the small blocks a thread frees, to give
/// them back to that thread's next allocations of the same size. Everything
/// else goes to the system's allocator.
///
/// The blocks a thread keeps when it ends aren't given back: the cache has
/// no destructor, which could make it allocate while it's being used. A
/// thread keeps 2 MB at the most.
pub struct SmallAlloc;

/// The freed blocks of one size, each holding the address of the next.
struct Class {
    head: Cell<*mut u8>,
    len: Cell<usize>,
}

thread_local! {
    // `const` and without a destructor: using it never allocates
    static FREE: [Class; CLASSES] = const { [const { Class { head: Cell::new(null_mut()), len: Cell::new(0) } }; CLASSES] };
}

/// The class of a size, or `None` for the ones the system gets.
#[inline(always)]
fn class_of(layout: Layout) -> Option<usize> {
    if layout.align() > STEP || layout.size() == 0 || layout.size() > CLASSES * STEP {
        return None;
    }
    Some((layout.size() - 1) / STEP)
}

/// What every block of a class is allocated with, whatever was asked.
#[inline(always)]
fn class_layout(class: usize) -> Layout {
    // Safety: the size is a multiple of 16, which is a power of two, and small
    unsafe { Layout::from_size_align_unchecked((class + 1) * STEP, STEP) }
}

// Safety: a block of a class is always allocated with `class_layout`, by the
// system or from the cache, and given back to the system with it. A block in
// a list is no longer used by anyone, and at least 16 bytes aligned to 16,
// so it can hold the address of the next one.
unsafe impl GlobalAlloc for SmallAlloc {
    #[inline(never)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some(class) = class_of(layout) else { return unsafe { System.alloc(layout) } };
        let taken = FREE.try_with(|free| {
            let c = &free[class];
            let head = c.head.get();
            if !head.is_null() {
                c.head.set(unsafe { *(head as *mut *mut u8) });
                c.len.set(c.len.get() - 1);
            }
            head
        });
        match taken {
            Ok(block) if !block.is_null() => block,
            _ => unsafe { System.alloc(class_layout(class)) },
        }
    }

    #[inline(never)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let Some(class) = class_of(layout) else { return unsafe { System.dealloc(ptr, layout) } };
        let kept = FREE.try_with(|free| {
            let c = &free[class];
            if c.len.get() >= KEEP {
                return false;
            }
            unsafe { *(ptr as *mut *mut u8) = c.head.get() };
            c.head.set(ptr);
            c.len.set(c.len.get() + 1);
            true
        });
        if kept != Ok(true) {
            unsafe { System.dealloc(ptr, class_layout(class)) };
        }
    }

    #[inline(never)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Safety: `realloc`'s contract says the new size, with the same alignment, is a valid layout
        let new = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        match (class_of(layout), class_of(new)) {
            // Both too big for the classes: the system, which can grow it in place
            (None, None) => unsafe { System.realloc(ptr, layout, new_size) },
            // The same class: the block already has room
            (Some(a), Some(b)) if a == b => ptr,
            _ => {
                let block = unsafe { self.alloc(new) };
                if !block.is_null() {
                    unsafe { std::ptr::copy_nonoverlapping(ptr, block, layout.size().min(new_size)) };
                    unsafe { self.dealloc(ptr, layout) };
                }
                block
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_given_back_and_used_again() {
        unsafe {
            let a = SmallAlloc;
            let layout = Layout::from_size_align(24, 8).unwrap();
            let first = a.alloc(layout);
            assert!(!first.is_null() && first as usize % 16 == 0);
            first.write_bytes(7, 24);
            a.dealloc(first, layout);
            // The next one of the same class is the block that was freed
            let again = a.alloc(Layout::from_size_align(32, 16).unwrap());
            assert_eq!(again, first);
            a.dealloc(again, Layout::from_size_align(32, 16).unwrap());
        }
    }

    #[test]
    fn sizes_outside_the_classes_go_to_the_system() {
        assert_eq!(class_of(Layout::from_size_align(0, 1).unwrap()), None);
        assert_eq!(class_of(Layout::from_size_align(257, 8).unwrap()), None);
        assert_eq!(class_of(Layout::from_size_align(64, 32).unwrap()), None);
        assert_eq!(class_of(Layout::from_size_align(1, 1).unwrap()), Some(0));
        assert_eq!(class_of(Layout::from_size_align(256, 16).unwrap()), Some(15));
        unsafe {
            let layout = Layout::from_size_align(4096, 64).unwrap();
            let big = SmallAlloc.alloc(layout);
            assert!(!big.is_null() && big as usize % 64 == 0);
            SmallAlloc.dealloc(big, layout);
        }
    }

    #[test]
    fn realloc_keeps_the_bytes_across_classes() {
        unsafe {
            let a = SmallAlloc;
            let small = Layout::from_size_align(10, 1).unwrap();
            let p = a.alloc(small);
            for i in 0..10 {
                *p.add(i) = i as u8;
            }
            // In the same class, from another class, and out to the system and back
            let p = a.realloc(p, small, 14);
            let p = a.realloc(p, Layout::from_size_align(14, 1).unwrap(), 200);
            let p = a.realloc(p, Layout::from_size_align(200, 1).unwrap(), 5000);
            let p = a.realloc(p, Layout::from_size_align(5000, 1).unwrap(), 10);
            for i in 0..10 {
                assert_eq!(*p.add(i), i as u8);
            }
            a.dealloc(p, Layout::from_size_align(10, 1).unwrap());
        }
    }

    #[test]
    fn a_class_keeps_a_limited_number_of_blocks() {
        unsafe {
            let layout = Layout::from_size_align(48, 8).unwrap();
            let blocks: Vec<*mut u8> = (0..KEEP + 10).map(|_| SmallAlloc.alloc(layout)).collect();
            for b in &blocks {
                SmallAlloc.dealloc(*b, layout);
            }
            let kept = FREE.with(|free| free[class_of(layout).unwrap()].len.get());
            assert_eq!(kept, KEEP);
        }
    }

    #[test]
    fn blocks_freed_on_another_thread() {
        let layout = Layout::from_size_align(40, 8).unwrap();
        let p = unsafe { SmallAlloc.alloc(layout) } as usize;
        std::thread::spawn(move || unsafe { SmallAlloc.dealloc(p as *mut u8, layout) }).join().unwrap();
    }
}
