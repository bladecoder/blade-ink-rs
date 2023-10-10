use std::{ffi::CString, os::raw::c_char, rc::Rc};

use bladeink::choice::Choice;

use crate::{BINKC_FAIL, BINKC_FAIL_NULL_POINTER, BINKC_OK};

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn binkc_choices_get_text(
    choices: *const Vec<Rc<Choice>>,
    idx: usize,
    text: *mut *mut c_char,
) -> u32 {
    if choices.is_null() {
        return BINKC_FAIL_NULL_POINTER;
    }

    let choices: &Vec<Rc<Choice>> = unsafe { &*choices };

    let choice = choices.get(idx);

    match choice {
        Some(choice) => unsafe {
            *text = CString::new(choice.text.as_str()).unwrap().into_raw();
        },
        None => {
            return BINKC_FAIL;
        }
    }

    BINKC_OK
}
