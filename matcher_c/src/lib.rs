use std::{
    ffi::{c_char, CStr, CString},
    str,
};

use matcher_rs::{
    MatchTableMapSerde as MatchTableMap, Matcher, SimpleMatcher, SimpleTableSerde as SimpleTable,
    TextMatcherTrait,
};

/// Initializes a `Matcher` from a serialized `MatchTableMap` in MessagePack format.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `match_table_map_bytes` points to a valid null-terminated C string containing a
/// serialized `MatchTableMap`, and that the string remains valid for the duration of the call.
///
/// # Parameters
/// - `match_table_map_bytes`: A pointer to a C string containing the serialized `MatchTableMap`.
///
/// # Returns
/// A raw pointer to the newly created `Matcher`. The caller is responsible for managing the
/// lifetime of this pointer and must eventually call `drop` on it to free the memory.
///
/// # Panics
/// This function will panic if the input data cannot be deserialized into a `MatchTableMap`.
#[no_mangle]
pub unsafe extern "C" fn init_matcher(match_table_map_bytes: *const c_char) -> *mut Matcher {
    unsafe {
        let match_table_map: MatchTableMap = match sonic_rs::from_slice(
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

/// Checks if the given text matches any pattern in the Matcher.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `matcher` points to a valid `Matcher` instance and that `text` points to a valid
/// null-terminated C string. Both the `matcher` and the `text` must remain valid for the
/// duration of the call.
///
/// # Parameters
/// - `matcher`: A pointer to the `Matcher` instance.
/// - `text`: A pointer to a C string containing the text to be checked for matches.
///
/// # Returns
/// - `true` if the text matches any pattern in the `Matcher`.
/// - `false` otherwise.
///
/// # Panics
/// This function will panic if the input `text` is not a valid UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn matcher_is_match(matcher: *mut Matcher, text: *const c_char) -> bool {
    unsafe {
        let text = str::from_utf8(CStr::from_ptr(text).to_bytes());
        match text {
            Ok(text) => matcher.as_ref().unwrap().is_match(text),
            Err(_) => {
                panic!("Input is not a valid utf-8 string");
            }
        }
    }
}

/// Processes the input text through the Matcher and returns the result as a C string.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `matcher` points to a valid `Matcher` instance and that `text` points to a valid
/// null-terminated C string. Both the `matcher` and the `text` must remain valid for the
/// duration of the call.
///
/// # Parameters
/// - `matcher`: A pointer to the `Matcher` instance.
/// - `text`: A pointer to a C string containing the text to be processed.
///
/// # Returns
/// A pointer to a newly allocated C string containing the processing result. The caller is
/// responsible for managing the lifetime of this pointer and must eventually call `drop_string`
/// on it to free the memory.
///
/// # Panics
/// This function will panic if the input `text` is not a valid UTF-8 string or if the
/// serialization of the result fails.
#[no_mangle]
pub unsafe extern "C" fn matcher_process_as_string(
    matcher: *mut Matcher,
    text: *const c_char,
) -> *mut c_char {
    unsafe {
        let text = str::from_utf8(CStr::from_ptr(text).to_bytes());
        let res = match text {
            Ok(text) => matcher.as_ref().unwrap().process(text),
            Err(_) => {
                panic!("Input is not a valid utf-8 string");
            }
        };
        let res_cstring = CString::new(sonic_rs::to_vec(&res).unwrap_unchecked()).unwrap();
        res_cstring.into_raw()
    }
}

/// Processes the input text through the `Matcher` and returns the word match result as a C string.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `matcher` points to a valid `Matcher` instance and that `text` points to a valid
/// null-terminated C string. Both the `matcher` and the `text` must remain valid for the
/// duration of the call.
///
/// # Parameters
/// - `matcher`: A pointer to the `Matcher` instance.
/// - `text`: A pointer to a C string containing the text to be processed.
///
/// # Returns
/// A pointer to a newly allocated C string containing the word match processing result.
/// The caller is responsible for managing the lifetime of this pointer and must eventually
/// call `drop_string` on it to free the memory.
///
/// # Panics
/// This function will panic if the input `text` is not a valid UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn matcher_word_match_as_string(
    matcher: *mut Matcher,
    text: *const c_char,
) -> *mut c_char {
    unsafe {
        let text = str::from_utf8(CStr::from_ptr(text).to_bytes());
        let res = match text {
            Ok(text) => {
                sonic_rs::to_string(&matcher.as_ref().unwrap().word_match(text)).unwrap_unchecked()
            }
            Err(_) => {
                panic!("Input is not a valid utf-8 string");
            }
        };
        let res_cstring = CString::new(res).unwrap();
        res_cstring.into_raw()
    }
}

/// Frees the memory allocated for the `Matcher` instance.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `matcher` points to a valid `Matcher` instance. This function transfers ownership
/// of the raw pointer and deallocates the memory, so the caller must not use the `matcher`
/// pointer after calling this function.
///
/// # Parameters
/// - `matcher`: A pointer to the `Matcher` instance to be deallocated.
#[no_mangle]
pub unsafe extern "C" fn drop_matcher(matcher: *mut Matcher) {
    unsafe { drop(Box::from_raw(matcher)) }
}

/// Initializes a `SimpleMatcher` instance from serialized table bytes.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `simple_table_bytes` points to a valid null-terminated C string. The returned
/// `SimpleMatcher` pointer must be properly managed and eventually deallocated by calling
/// `drop_simple_matcher`.
///
/// # Parameters
/// - `simple_table_bytes`: A pointer to a C string containing the serialized table bytes.
///
/// # Returns
/// A pointer to a newly allocated `SimpleMatcher` instance. The caller is responsible for managing
/// the lifetime of this pointer and must eventually call `drop_simple_matcher` to free the memory.
///
/// # Panics
/// This function will panic if the deserialization of `simple_table_bytes` fails.
#[no_mangle]
pub unsafe extern "C" fn init_simple_matcher(
    simple_table_bytes: *const c_char,
) -> *mut SimpleMatcher {
    unsafe {
        let simple_table: SimpleTable =
            match sonic_rs::from_slice(CStr::from_ptr(simple_table_bytes).to_bytes()) {
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

/// Determines if the input text matches using the `SimpleMatcher`.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `simple_matcher` points to a valid `SimpleMatcher` instance and that `text` points to a
/// valid null-terminated C string. Both the `simple_matcher` and the `text` must remain valid for
/// the duration of the call.
///
/// # Parameters
/// - `simple_matcher`: A pointer to the `SimpleMatcher` instance.
/// - `text`: A pointer to a C string containing the text to be processed.
///
/// # Returns
/// A boolean indicating whether the text matches based on the `SimpleMatcher`.
///
/// # Panics
/// This function will panic if the input `text` is not a valid UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn simple_matcher_is_match(
    simple_matcher: *mut SimpleMatcher,
    text: *const c_char,
) -> bool {
    unsafe {
        let text = str::from_utf8(CStr::from_ptr(text).to_bytes());
        match text {
            Ok(text) => simple_matcher.as_ref().unwrap().is_match(text),
            Err(_) => {
                panic!("Input is not a valid utf-8 string");
            }
        }
    }
}

/// Processes the input text using the `SimpleMatcher` and returns the result as a C string.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `simple_matcher` points to a valid `SimpleMatcher` instance and that `text` points to a
/// valid null-terminated C string. Both `simple_matcher` and `text` must remain valid for the
/// duration of the call.
///
/// # Parameters
/// - `simple_matcher`: A pointer to the `SimpleMatcher` instance.
/// - `text`: A pointer to a C string containing the text to be processed.
///
/// # Returns
/// A pointer to a newly allocated C string containing the processing result. The caller is
/// responsible for managing the lifetime of this pointer and must eventually call
/// `drop_string` on it to free the memory.
///
/// # Panics
/// This function will panic if the input `text` is not a valid UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn simple_matcher_process_as_string(
    simple_matcher: *mut SimpleMatcher,
    text: *const c_char,
) -> *mut c_char {
    unsafe {
        let text = str::from_utf8(CStr::from_ptr(text).to_bytes());
        let res = match text {
            Ok(text) => simple_matcher.as_ref().unwrap().process(text),
            Err(_) => {
                panic!("Input is not a valid utf-8 string");
            }
        };
        let res_cstring = CString::new(sonic_rs::to_vec(&res).unwrap_unchecked()).unwrap();
        res_cstring.into_raw()
    }
}

/// Deallocates a `SimpleMatcher` instance.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `simple_matcher` points to a valid `SimpleMatcher` instance that was previously allocated
/// by `init_simple_matcher`. After calling this function, the `simple_matcher` pointer must not be
/// used again as it points to deallocated memory.
///
/// # Parameters
/// - `simple_matcher`: A pointer to the `SimpleMatcher` instance to be deallocated.
#[no_mangle]
pub unsafe extern "C" fn drop_simple_matcher(simple_matcher: *mut SimpleMatcher) {
    unsafe { drop(Box::from_raw(simple_matcher)) }
}

/// Deallocates a C string that was previously allocated by the Rust code and passed to C.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `ptr` points to a valid C string that was previously allocated by Rust code using
/// `CString::into_raw` or a similar method. After calling this function, the `ptr` pointer must
/// not be used again as it points to deallocated memory.
///
/// # Parameters
/// - `ptr`: A pointer to the C string to be deallocated.
#[no_mangle]
pub unsafe extern "C" fn drop_string(ptr: *mut c_char) {
    unsafe { drop(CString::from_raw(ptr)) }
}
