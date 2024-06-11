use std::{
    ffi::{CStr, CString},
    str::from_utf8_unchecked,
};

use matcher_rs::{MatchTableMap, Matcher, SimpleMatchTypeWordMap, SimpleMatcher, TextMatcherTrait};

#[no_mangle]
pub extern "C" fn init_matcher(match_table_map_bytes: *const i8) -> *mut Matcher {
    unsafe {
        let match_table_map: MatchTableMap = match rmp_serde::from_slice(
            CStr::from_ptr(match_table_map_bytes).to_bytes(),
        ) {
            Ok(match_table_map) => match_table_map,
            Err(e) => {
                panic!("Deserialize match_table_map_bytes failed, Please check the input data.\nErr: {}", e.to_string())
            }
        };

        Box::into_raw(Box::new(Matcher::new(&match_table_map)))
    }
}

#[no_mangle]
pub extern "C" fn matcher_is_match(matcher: *mut Matcher, text: *const i8) -> bool {
    unsafe {
        matcher
            .as_ref()
            .unwrap()
            .is_match(from_utf8_unchecked(CStr::from_ptr(text).to_bytes()))
    }
}

#[no_mangle]
pub extern "C" fn matcher_word_match(matcher: *mut Matcher, text: *const i8) -> *mut i8 {
    let res = unsafe {
        CString::new(
            serde_json::to_string(
                &matcher
                    .as_ref()
                    .unwrap()
                    .word_match(from_utf8_unchecked(CStr::from_ptr(text).to_bytes())),
            )
            .unwrap(),
        )
        .unwrap()
    };

    res.into_raw()
}

#[no_mangle]
pub extern "C" fn drop_matcher(matcher: *mut Matcher) {
    unsafe { drop(Box::from_raw(matcher)) }
}

#[no_mangle]
pub extern "C" fn init_simple_matcher(
    simple_match_type_word_map_bytes: *const i8,
) -> *mut SimpleMatcher {
    unsafe {
        let simple_match_type_word_map: SimpleMatchTypeWordMap = match rmp_serde::from_slice(
            CStr::from_ptr(simple_match_type_word_map_bytes).to_bytes(),
        ) {
            Ok(simple_match_type_word_map) => simple_match_type_word_map,
            Err(e) => {
                panic!(
                    "Deserialize simple_match_type_word_map_bytes failed, Please check the input data.\nErr: {}", e.to_string(),
                )
            }
        };

        Box::into_raw(Box::new(SimpleMatcher::new(&simple_match_type_word_map)))
    }
}

#[no_mangle]
pub extern "C" fn simple_matcher_is_match(
    simple_matcher: *mut SimpleMatcher,
    text: *const i8,
) -> bool {
    unsafe {
        simple_matcher
            .as_ref()
            .unwrap()
            .is_match(from_utf8_unchecked(CStr::from_ptr(text).to_bytes()))
    }
}

#[no_mangle]
pub extern "C" fn simple_matcher_process(
    simple_matcher: *mut SimpleMatcher,
    text: *const i8,
) -> *mut i8 {
    let res = unsafe {
        CString::new(
            serde_json::to_string(
                &simple_matcher
                    .as_ref()
                    .unwrap()
                    .process(from_utf8_unchecked(CStr::from_ptr(text).to_bytes())),
            )
            .unwrap(),
        )
        .unwrap()
    };

    res.into_raw()
}

#[no_mangle]
pub extern "C" fn drop_simple_matcher(simple_matcher: *mut SimpleMatcher) {
    unsafe { drop(Box::from_raw(simple_matcher)) }
}

#[no_mangle]
pub extern "C" fn drop_string(ptr: *mut i8) {
    unsafe { drop(CString::from_raw(ptr)) }
}
