use std::{
    ffi::{c_char, CStr, CString},
    str,
};

use matcher_rs::{MatchTableMap, Matcher, SimpleMatcher, SimpleTable, TextMatcherTrait};

#[no_mangle]
pub unsafe extern "C" fn init_matcher(match_table_map_bytes: *const c_char) -> *mut Matcher {
    unsafe {
        let match_table_map: MatchTableMap = match rmp_serde::from_slice(
            CStr::from_ptr(match_table_map_bytes).to_bytes(),
        ) {
            Ok(match_table_map) => match_table_map,
            Err(e) => {
                panic!("Deserialize match_table_map_bytes failed, Please check the input data.\nErr: {}", e)
            }
        };

        Box::into_raw(Box::new(Matcher::new(&match_table_map)))
    }
}

#[no_mangle]
pub unsafe extern "C" fn matcher_is_match(matcher: *mut Matcher, text: *const c_char) -> bool {
    unsafe {
        matcher
            .as_ref()
            .unwrap()
            .is_match(str::from_utf8_unchecked(CStr::from_ptr(text).to_bytes()))
    }
}

#[no_mangle]
pub unsafe extern "C" fn matcher_process_as_string(
    matcher: *mut Matcher,
    text: *const c_char,
) -> *mut c_char {
    let res = unsafe {
        CString::new(
            sonic_rs::to_vec(
                &matcher
                    .as_ref()
                    .unwrap()
                    .process(str::from_utf8_unchecked(CStr::from_ptr(text).to_bytes())),
            )
            .unwrap_unchecked(),
        )
        .unwrap()
    };

    res.into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn matcher_word_match_as_string(
    matcher: *mut Matcher,
    text: *const c_char,
) -> *mut c_char {
    let res = unsafe {
        CString::new(
            matcher
                .as_ref()
                .unwrap()
                .word_match_as_string(str::from_utf8_unchecked(CStr::from_ptr(text).to_bytes())),
        )
        .unwrap()
    };

    res.into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn drop_matcher(matcher: *mut Matcher) {
    unsafe { drop(Box::from_raw(matcher)) }
}

#[no_mangle]
pub unsafe extern "C" fn init_simple_matcher(
    simple_table_bytes: *const c_char,
) -> *mut SimpleMatcher {
    unsafe {
        let simple_table: SimpleTable =
            match rmp_serde::from_slice(CStr::from_ptr(simple_table_bytes).to_bytes()) {
                Ok(simple_table) => simple_table,
                Err(e) => {
                    panic!(
                    "Deserialize simple_table_bytes failed, Please check the input data.\nErr: {}",
                    e,
                )
                }
            };

        Box::into_raw(Box::new(SimpleMatcher::new(&simple_table)))
    }
}

#[no_mangle]
pub unsafe extern "C" fn simple_matcher_is_match(
    simple_matcher: *mut SimpleMatcher,
    text: *const c_char,
) -> bool {
    unsafe {
        simple_matcher
            .as_ref()
            .unwrap()
            .is_match(str::from_utf8_unchecked(CStr::from_ptr(text).to_bytes()))
    }
}

#[no_mangle]
pub unsafe extern "C" fn simple_matcher_process_as_string(
    simple_matcher: *mut SimpleMatcher,
    text: *const c_char,
) -> *mut c_char {
    let res = unsafe {
        CString::new(
            sonic_rs::to_vec(
                &simple_matcher
                    .as_ref()
                    .unwrap()
                    .process(str::from_utf8_unchecked(CStr::from_ptr(text).to_bytes())),
            )
            .unwrap_unchecked(),
        )
        .unwrap()
    };

    res.into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn drop_simple_matcher(simple_matcher: *mut SimpleMatcher) {
    unsafe { drop(Box::from_raw(simple_matcher)) }
}

#[no_mangle]
pub unsafe extern "C" fn drop_string(ptr: *mut c_char) {
    unsafe { drop(CString::from_raw(ptr)) }
}
