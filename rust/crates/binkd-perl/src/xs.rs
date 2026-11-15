//! The XS helpers of the port, the port of the `XS()` functions of
//! `perlhooks.c`.
//!
//! The C shim extracts the arguments of Perl and calls these functions. The
//! functions use the hooks of the call of Perl on this thread, the port of the
//! `current_config` and of the session `STATE` of the C code.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

use crate::hooks::with_current;

/// Returns the text of a C string, or an empty string for a NULL pointer.
fn string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        // SAFETY: the caller passes a NUL-terminated string of the call.
        unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
    }
}

/// Returns the texts of an array of C strings.
fn strings(count: usize, ptr: *const *const c_char) -> Vec<String> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let value = unsafe { *ptr.add(i) };
        out.push(string(value));
    }
    out
}

/// Returns the texts of an array of C strings, with `None` for a NULL pointer.
fn optional_strings(count: usize, ptr: *const *const c_char) -> Vec<Option<String>> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let value = unsafe { *ptr.add(i) };
        out.push(if value.is_null() {
            None
        } else {
            Some(string(value))
        });
    }
    out
}

/// Logs a line, the port of `perl_Log()` of `perlhooks.c`.
#[no_mangle]
pub extern "C" fn binkd_perl_xs_log(level: c_int, text: *const c_char) {
    let text = string(text);
    with_current(|hooks| hooks.log(level, &text));
}

/// Compares an address with a list, the port of `perl_aeq()`.
///
/// Answers 1 for a match, 0 for no match, and -1 for the undefined result,
/// the port of `XSRETURN_UNDEF`.
#[no_mangle]
pub extern "C" fn binkd_perl_xs_aeq(
    first: *const c_char,
    count: usize,
    others: *const *const c_char,
) -> c_int {
    let first = string(first);
    let others = strings(count, others);
    let others: Vec<&str> = others.iter().map(String::as_str).collect();
    match with_current(|hooks| hooks.aeq(&first, &others)) {
        Some(Some(true)) => 1,
        Some(Some(false)) => 0,
        _ => -1,
    }
}

/// Removes addresses from an array, the port of `perl_arm()`.
///
/// Answers the number of removed elements, or -1 for the undefined result.
/// Writes one flag for every element to `keep`: 1 keeps the element and 0
/// removes it.
#[no_mangle]
pub extern "C" fn binkd_perl_xs_arm(
    count: usize,
    elements: *const *const c_char,
    arg_count: usize,
    args: *const *const c_char,
    keep: *mut c_int,
) -> c_int {
    let elements = optional_strings(count, elements);
    let elements: Vec<Option<&str>> = elements.iter().map(|e| e.as_deref()).collect();
    let args = strings(arg_count, args);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match with_current(|hooks| hooks.arm(&elements, &args)) {
        Some(flags) => {
            let mut removed = 0;
            for (i, retain) in flags.iter().enumerate() {
                if !*retain {
                    removed += 1;
                }
                if !keep.is_null() {
                    // SAFETY: the caller supplies an array of `count` flags.
                    unsafe { *keep.add(i) = if *retain { 1 } else { 0 } };
                }
            }
            removed as c_int
        }
        None => -1,
    }
}

/// Queues a message, the port of `perl_msg_send()`.
#[no_mangle]
pub extern "C" fn binkd_perl_xs_msg_send(kind: c_int, text: *const c_char) {
    let text = string(text);
    with_current(|hooks| hooks.msg_send(kind as u8, &text));
}

/// Logs the lines of a Perl warning or error, the port of `perl_warn()`.
#[no_mangle]
pub extern "C" fn binkd_perl_xs_warn(text: *const c_char) {
    let text = string(text);
    with_current(|hooks| hooks.warn(&text));
}
