//! C API for bink.

use std::{os::raw::c_char, ffi::CString};

pub mod cstory;

const BINKC_OK: u32 = 0;
const BINKC_FAIL: u32 = 1;
const BINKC_FAIL_NULL_POINTER: u32 = 2;

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn binkc_cstring_free(cstring: *mut c_char) {
    unsafe {
        if !cstring.is_null() {
            drop(CString::from_raw(cstring));
        }
    }
}
