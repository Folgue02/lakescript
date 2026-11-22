//! Ctrl+C for the programs that run scripts, like `lks` and `ferry` *(see
//! Ctrl+C in `ideas/language/errors.md`)*. The library never handles signals
//! on its own: a program asks for it with `on_ctrl_c`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// The flag the handler sets. It's read from a signal handler, which can run
/// at any moment, so it's never freed.
static FLAG: AtomicPtr<AtomicBool> = AtomicPtr::new(std::ptr::null_mut());

/// Makes Ctrl+C set `flag`, the one of an engine *(see
/// `Engine::interrupt_flag`)*, so the script throws an `Interrupted` where it
/// is. A second Ctrl+C before the script got to the first stops the program
/// with the code 130, for a script stuck waiting *(i.e. reading stdin)*.
/// Calling it again changes the flag.
pub fn on_ctrl_c(flag: Arc<AtomicBool>) {
    // The flag before this one stays alive: the handler may be reading it
    FLAG.store(Arc::into_raw(flag) as *mut AtomicBool, Ordering::SeqCst);
    install();
}

/// What the handler does. Only what's allowed inside a signal handler: it
/// reads and writes an atomic, and stops the process.
fn pressed() {
    let flag = FLAG.load(Ordering::SeqCst);
    if flag.is_null() {
        return;
    }
    // SAFETY: it comes from `Arc::into_raw` in `on_ctrl_c`, and is never freed
    if unsafe { &*flag }.swap(true, Ordering::SeqCst) {
        stop();
    }
}

#[cfg(unix)]
fn stop() -> ! {
    // SAFETY: `_exit` is one of the functions a signal handler may call.
    // `std::process::exit` isn't: it runs code that takes locks.
    unsafe { libc::_exit(130) }
}

#[cfg(unix)]
fn install() {
    extern "C" fn handle(_: libc::c_int) {
        pressed();
    }
    // SAFETY: `action` is filled in before it's used, and the handler only
    // does what a signal handler may do *(see `pressed`)*.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handle as extern "C" fn(libc::c_int) as usize;
        // What was waiting *(i.e. a read of stdin)* goes on after the handler
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut());
    }
}

#[cfg(windows)]
fn stop() -> ! {
    // On Windows the handler runs on a thread of its own, so this is fine
    std::process::exit(130)
}

#[cfg(windows)]
fn install() {
    // From kernel32, which every program on Windows has
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
    }
    const CTRL_C_EVENT: u32 = 0;
    unsafe extern "system" fn handle(event: u32) -> i32 {
        if event != CTRL_C_EVENT {
            return 0;
        }
        pressed();
        1
    }
    // SAFETY: `handle` has the signature Windows asks for, and lives forever
    unsafe {
        SetConsoleCtrlHandler(Some(handle), 1);
    }
}

#[cfg(not(any(unix, windows)))]
fn stop() -> ! {
    std::process::exit(130)
}

#[cfg(not(any(unix, windows)))]
fn install() {}
