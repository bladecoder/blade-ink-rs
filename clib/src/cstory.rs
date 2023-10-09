use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
};

use bink::story::Story;

use crate::{BINKC_FAIL, BINKC_FAIL_NULL_POINTER, BINKC_OK};

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn binkc_story_new(
    story: *mut *mut Story,
    json_string: *const c_char,
    err_msg: *mut *mut c_char,
) -> u32 {
    if story.is_null() || err_msg.is_null() {
        return BINKC_FAIL_NULL_POINTER;
    }

    unsafe {
        *story = std::ptr::null_mut();
        *err_msg = std::ptr::null_mut();
    }

    let c_str: &CStr = unsafe { CStr::from_ptr(json_string) };
    let str_slice: &str = c_str.to_str().unwrap();

    let result = Story::new(str_slice);

    match result {
        Ok(s) => unsafe {
            *story = Box::into_raw(Box::new(s));
            BINKC_OK
        },
        Err(e) => unsafe {
            *err_msg = CString::new(e.to_string()).unwrap().into_raw();
            BINKC_FAIL
        },
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn binkc_story_free(story: *mut Story) {
    if !story.is_null() {
        unsafe {
            drop(Box::from_raw(story));
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn binkc_story_can_continue(story: *mut Story, can_continue: *mut bool) -> u32 {
    if story.is_null() {
        return BINKC_FAIL_NULL_POINTER;
    }

    let story: &mut Story = unsafe { &mut *story };

    unsafe {
        *can_continue = story.can_continue();
    }

    BINKC_OK
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn binkc_story_cont(
    story: *mut Story,
    line: *mut *mut c_char,
    err_msg: *mut *mut c_char,
) -> u32 {
    if story.is_null() {
        return BINKC_FAIL_NULL_POINTER;
    }

    let story: &mut Story = unsafe { &mut *story };

    let result = story.cont();

    match result {
        Ok(l) => unsafe {
            *line = CString::new(l).unwrap().into_raw();
            BINKC_OK
        },
        Err(e) => unsafe {
            *err_msg = CString::new(e.to_string()).unwrap().into_raw();
            BINKC_FAIL
        },
    }
}
