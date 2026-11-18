//! The signals a native crash raises, caught so the phone can show what happened.
//!
//! A Java `UncaughtExceptionHandler` never sees a SIGSEGV: the process is already past Java. The
//! handlers here write a short description to a file and to logcat, then let the default handler
//! finish the process as it would have.
//!
//! Nothing here allocates or locks: a handler runs on the thread that crashed, with its stack and
//! the allocator in any state at all.

use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::sync::atomic::{AtomicI32, Ordering};

/// The file the crash is described in, prepared before any signal can arrive: the handler only
/// writes these bytes.
static MESSAGE: std::sync::OnceLock<CString> = std::sync::OnceLock::new();
static MESSAGE_LEN: AtomicI32 = AtomicI32::new(0);
/// Where the report goes, NUL-terminated. The app's own files directory: no app may write
/// `/data/local/tmp`, which belongs to the shell user.
static REPORT_PATH: std::sync::OnceLock<CString> = std::sync::OnceLock::new();

/// The handlers the app installed before us, chained to so the platform still sees the crash
/// (its tombstone, the system's own report).
static mut OLD_SEGV: libc::sigaction = unsafe { std::mem::zeroed() };
static mut OLD_BUS: libc::sigaction = unsafe { std::mem::zeroed() };
static mut OLD_ABRT: libc::sigaction = unsafe { std::mem::zeroed() };
static mut OLD_FPE: libc::sigaction = unsafe { std::mem::zeroed() };
static mut OLD_ILL: libc::sigaction = unsafe { std::mem::zeroed() };

/// The signal's name, without a lookup table: what can be said from a name only.
unsafe fn write_all(fd: c_int, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let written = unsafe { libc::write(fd, bytes.as_ptr().cast::<c_void>(), bytes.len()) };
        if written <= 0 {
            return;
        }
        bytes = &bytes[written as usize..];
    }
}

unsafe fn logcat(level: c_int, tag: &[u8], message: &[u8]) {
    unsafe {
        let prio = if level == libc::SIGABRT { 6 } else { 5 };
        crate::android::__android_log_write(prio, tag.as_ptr().cast(), message.as_ptr().cast());
    }
}

type Handler = unsafe extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void);

unsafe extern "C" fn handler(signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    // SAFETY: this runs on the crashing thread; only plain writes, no allocation.
    unsafe {
        if let Some(message) = MESSAGE.get() {
            let bytes = message.as_bytes();
            let len = MESSAGE_LEN.load(Ordering::Relaxed).max(0) as usize;
            if len > 0
                && len <= bytes.len()
                && let Some(path) = REPORT_PATH.get()
            {
                let fd = libc::open(
                    path.as_ptr(),
                    libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
                    0o600,
                );
                if fd >= 0 {
                    write_all(fd, &bytes[..len]);
                    libc::close(fd);
                }
            }
            logcat(signal, b"starling-crash\0", &bytes[..len]);
        }
        // The platform's handler still runs: its tombstone, and the process's end, are its own.
        let old = match signal {
            libc::SIGSEGV => OLD_SEGV,
            libc::SIGBUS => OLD_BUS,
            libc::SIGABRT => OLD_ABRT,
            libc::SIGFPE => OLD_FPE,
            libc::SIGILL => OLD_ILL,
            _ => return,
        };
        if old.sa_sigaction == libc::SIG_DFL || old.sa_sigaction == 0 {
            libc::signal(signal, libc::SIG_DFL);
            libc::raise(signal);
            return;
        }
        if old.sa_flags & libc::SA_SIGINFO != 0 {
            let previous: Handler = std::mem::transmute(old.sa_sigaction);
            previous(signal, info, context);
        } else {
            let previous: unsafe extern "C" fn(c_int) = std::mem::transmute(old.sa_sigaction);
            previous(signal);
        }
    }
}

/// Installs the handlers. `where` names the build (the version, the GPU), which is all a crash
/// report can be given without asking the dying process.
pub fn install(dir: &std::path::Path, description: &str) {
    // The report's path is set before the signal can arrive, like the message itself.
    if let Some(path) = dir
        .join("engine_crash.txt")
        .to_str()
        .and_then(|p| CString::new(p).ok())
    {
        let _ = REPORT_PATH.set(path);
    }
    let mut text = description.to_owned();
    text.push('\n');
    let Ok(message) = CString::new(text) else {
        return;
    };
    MESSAGE_LEN.store((message.as_bytes().len()) as i32, Ordering::Relaxed);
    let _ = MESSAGE.set(message);
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handler as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGSEGV, &raw const action, &raw mut OLD_SEGV);
        libc::sigaction(libc::SIGBUS, &raw const action, &raw mut OLD_BUS);
        libc::sigaction(libc::SIGABRT, &raw const action, &raw mut OLD_ABRT);
        libc::sigaction(libc::SIGFPE, &raw const action, &raw mut OLD_FPE);
        libc::sigaction(libc::SIGILL, &raw const action, &raw mut OLD_ILL);
    }
}
