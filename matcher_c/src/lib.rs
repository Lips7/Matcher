use std::{
    ffi::{CStr, CString, c_char},
    panic::{self, AssertUnwindSafe},
    ptr, str,
};

use matcher_rs::{SimpleMatcher, SimpleTableSerde as SimpleTable};

/// Initializes a [`SimpleMatcher`] instance from serialized table bytes.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `simple_table_bytes` points to a valid null-terminated C string. The returned
/// [`SimpleMatcher`] pointer must be properly managed and eventually deallocated by calling
/// `drop_simple_matcher`.
///
/// # Arguments
/// - `simple_table_bytes`: A pointer to a C string containing the serialized table bytes.
///
/// # Returns
/// A pointer to a newly allocated [`SimpleMatcher`] instance. The caller is responsible for managing
/// the lifetime of this pointer and must eventually call [`drop_simple_matcher`] to free the memory.
///
/// # Panics
/// This function will panic if the deserialization of `simple_table_bytes` fails.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn init_simple_matcher(
    simple_table_bytes: *const c_char,
) -> *mut SimpleMatcher {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        let simple_table: SimpleTable =
            match sonic_rs::from_slice(CStr::from_ptr(simple_table_bytes).to_bytes()) {
                Ok(simple_table) => simple_table,
                Err(e) => {
                    eprintln!("Deserialize simple_table_bytes failed: {}", e);
                    return ptr::null_mut();
                }
            };

        Box::into_raw(Box::new(SimpleMatcher::new(&simple_table)))
    }));

    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            eprintln!("init_simple_matcher panicked");
            ptr::null_mut()
        }
    }
}

/// Determines if the input text matches using the [`SimpleMatcher`].
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `simple_matcher` points to a valid [`SimpleMatcher`] instance and that `text` points to a
/// valid null-terminated C string. Both the `simple_matcher` and the `text` must remain valid for
/// the duration of the call.
///
/// # Arguments
/// - `simple_matcher`: A pointer to the [`SimpleMatcher`] instance.
/// - `text`: A pointer to a C string containing the text to be processed.
///
/// # Returns
/// A boolean indicating whether the text matches based on the [`SimpleMatcher`].
///
/// # Panics
/// This function will panic if the input `text` is not a valid UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_is_match(
    simple_matcher: *mut SimpleMatcher,
    text: *const c_char,
) -> bool {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        let text_bytes = CStr::from_ptr(text).to_bytes();
        let text_str = match str::from_utf8(text_bytes) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("Input is not a valid utf-8 string");
                return false;
            }
        };
        simple_matcher
            .as_ref()
            .is_some_and(|m| m.is_match(text_str))
    }));

    result.unwrap_or_else(|_| {
        eprintln!("simple_matcher_is_match panicked");
        false
    })
}

/// Processes the input text using the [`SimpleMatcher`] and returns the result as a C string.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `simple_matcher` points to a valid [`SimpleMatcher`] instance and that `text` points to a
/// valid null-terminated C string. Both `simple_matcher` and `text` must remain valid for the
/// duration of the call.
///
/// # Arguments
/// - `simple_matcher`: A pointer to the [`SimpleMatcher`] instance.
/// - `text`: A pointer to a C string containing the text to be processed.
///
/// # Returns
/// A pointer to a newly allocated C string containing the processing result. The caller is
/// responsible for managing the lifetime of this pointer and must eventually call
/// [`drop_string`] on it to free the memory.
///
/// # Panics
/// This function will panic if the input `text` is not a valid UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_process_as_string(
    simple_matcher: *mut SimpleMatcher,
    text: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        let text_bytes = CStr::from_ptr(text).to_bytes();
        let text_str = match str::from_utf8(text_bytes) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("Input is not a valid utf-8 string");
                return ptr::null_mut();
            }
        };
        let m = match simple_matcher.as_ref() {
            Some(m) => m,
            None => return ptr::null_mut(),
        };
        let res = m.process(text_str);
        let res_cstring = CString::new(sonic_rs::to_vec(&res).unwrap_unchecked()).unwrap();
        res_cstring.into_raw()
    }));

    result.unwrap_or_else(|_| {
        eprintln!("simple_matcher_process_as_string panicked");
        ptr::null_mut()
    })
}

/// Deallocates a [`SimpleMatcher`] instance.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `simple_matcher` points to a valid [`SimpleMatcher`] instance that was previously allocated
/// by [`init_simple_matcher`]. After calling this function, the `simple_matcher` pointer must not be
/// used again as it points to deallocated memory.
///
/// # Arguments
/// - `simple_matcher`: A pointer to the [`SimpleMatcher`] instance to be deallocated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_simple_matcher(simple_matcher: *mut SimpleMatcher) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !simple_matcher.is_null() {
            drop(Box::from_raw(simple_matcher))
        }
    }));
}

/// Deallocates a C string that was previously allocated by the Rust code and passed to C.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The caller must ensure
/// that `ptr` points to a valid C string that was previously allocated by Rust code using
/// [`CString::into_raw`] or a similar method. After calling this function, the `ptr` pointer must
/// not be used again as it points to deallocated memory.
///
/// # Arguments
/// - `ptr`: A pointer to the C string to be deallocated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_string(ptr: *mut c_char) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !ptr.is_null() {
            drop(CString::from_raw(ptr))
        }
    }));
}
