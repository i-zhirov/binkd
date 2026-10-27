//! The hooks of the process and of the session, the port of `perlhooks.c`.

use std::ffi::CString;
use std::os::raw::c_char;

use crate::bridge::{self, Interpreter, Value, G_EVAL, G_SCALAR, ON_LOG};
use crate::ffi;

/// The hooks of a script, the port of the state of `perlhooks.c`.
///
/// The C code holds the interpreter and the set of the hooks that the script
/// answers in `BINKD_CONFIG`, the port of `cfg->perl` and of `cfg->perl_ok`. The
/// port holds the two in one value, so a caller may run one set of hooks beside
/// other code.
pub struct Hooks {
    /// The interpreter, the port of `cfg->perl`.
    perl: *mut Interpreter,
    /// The name of the script, the port of `cfg->perl_script`.
    pub script: String,
}

// The interpreter of one value of the port belongs to one thread of the caller,
// the port of the interpreter of one process of the C code.
unsafe impl Send for Hooks {}

impl Hooks {
    /// Loads a script, the port of `perl_init()` of `perlhooks.c`.
    ///
    /// The C code creates the interpreter, it reads the script, and it runs the
    /// body, so the script holds its subs. The port does the same, and it holds
    /// the name of the script.
    pub fn load(script: &str) -> Option<Hooks> {
        let path = CString::new(script).ok()?;
        let perl = unsafe { ffi::binkd_perl_alloc() };
        if perl.is_null() {
            return None;
        }
        unsafe {
            ffi::binkd_perl_construct(perl);
        }
        let mut args: Vec<*mut c_char> = vec![path.as_ptr() as *mut c_char, std::ptr::null_mut()];
        let rc = unsafe {
            ffi::binkd_perl_parse(
                perl,
                std::ptr::null_mut(),
                1,
                args.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if rc != 0 {
            unsafe { ffi::binkd_perl_free(perl) };
            return None;
        }
        if unsafe { ffi::binkd_perl_run(perl) } != 0 {
            unsafe { ffi::binkd_perl_free(perl) };
            return None;
        }
        Some(Hooks {
            perl,
            script: script.to_string(),
        })
    }

    /// Calls the hook of the log, the port of `perl_on_log()` of `perlhooks.c`.
    ///
    /// The C code sets `$_` to the message and `$lvl` to the level, it calls the
    /// sub, and a true return replaces the two. The port does the same and it
    /// answers true when the hook changed the message, the port of the `rc` of the
    /// C code.
    pub fn on_log(&self, message: &mut Vec<u8>, level: &mut i32) -> bool {
        let text = match CString::new(message.clone()) {
            Ok(text) => text,
            Err(_) => return true,
        };
        let name = match CString::new(bridge::SUBNAMES[ON_LOG]) {
            Ok(name) => name,
            Err(_) => return true,
        };
        let message_name = CString::new("_").ok();
        let level_name = CString::new("lvl").ok();
        let (Some(message_name), Some(level_name)) = (message_name, level_name) else {
            return true;
        };
        unsafe {
            // The check of the C code against the recursion of the hook.
            let value: *mut Value = ffi::binkd_perl_get_sv(self.perl, message_name.as_ptr(), 1);
            if value.is_null() {
                return true;
            }
            ffi::binkd_perl_set_string(self.perl, value, text.as_ptr());
            let lvl = ffi::binkd_perl_get_sv(self.perl, level_name.as_ptr(), 1);
            if !lvl.is_null() {
                ffi::binkd_perl_set_int(self.perl, lvl, *level as i64);
            }
            let result = ffi::binkd_perl_call(self.perl, name.as_ptr(), G_EVAL | G_SCALAR);
            if ffi::binkd_perl_error(self.perl) != 0 {
                return true;
            }
            if result.is_null() || ffi::binkd_perl_ok(self.perl, result) == 0 {
                return true;
            }
            if ffi::binkd_perl_int(self.perl, result) == 0 {
                return true;
            }
            let mut len: bridge::Length = 0;
            let text = ffi::binkd_perl_string(self.perl, value, &mut len);
            if text.is_null() || len == 0 {
                return true;
            }
            let bytes = std::slice::from_raw_parts(text as *const u8, len).to_vec();
            if bytes.is_empty() {
                return true;
            }
            *message = bytes;
            let lvl = ffi::binkd_perl_get_sv(self.perl, level_name.as_ptr(), 0);
            if !lvl.is_null() {
                *level = ffi::binkd_perl_int(self.perl, lvl) as i32;
            }
        }
        false
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe { ffi::binkd_perl_free(self.perl) };
    }
}
