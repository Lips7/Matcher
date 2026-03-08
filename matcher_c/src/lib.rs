use std::{
    ffi::{CStr, CString, c_char},
    panic::{self, AssertUnwindSafe},
    ptr, str,
};

use matcher_rs::{
    ProcessType, SimpleMatcher, SimpleTableSerde as SimpleTable,
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
};

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
        let res_json = match sonic_rs::to_vec(&res) {
            Ok(json) => json,
            Err(_) => return ptr::null_mut(),
        };
        let res_cstring = match CString::new(res_json) {
            Ok(cs) => cs,
            Err(_) => return ptr::null_mut(),
        };
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

/// Processes text using the specified ProcessType bit.
///
/// # Safety
/// The caller must ensure `text` points to a valid null-terminated C string.
/// Returns a null pointer if an error occurs.
/// The caller must free the returned pointer using `drop_string`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn text_process(process_type: u8, text: *const c_char) -> *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if text.is_null() {
            return ptr::null_mut();
        }
        let text_bytes = CStr::from_ptr(text).to_bytes();
        let text_str = match str::from_utf8(text_bytes) {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };
        let process_type_bit = match ProcessType::from_bits(process_type) {
            Some(pt) => pt,
            None => return ptr::null_mut(),
        };
        let res = text_process_rs(process_type_bit, text_str);
        match CString::new(res.as_ref()) {
            Ok(cs) => cs.into_raw(),
            Err(_) => ptr::null_mut(),
        }
    }));
    result.unwrap_or(ptr::null_mut())
}

/// Applies a sequence of rules to text, returning all intermediate variants.
///
/// # Safety
/// The caller must ensure `text` points to a valid null-terminated C string.
/// The caller must free the returned struct using `drop_string_array`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn reduce_text_process(
    process_type: u8,
    text: *const c_char,
) -> *mut *mut c_char {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if text.is_null() {
            return ptr::null_mut();
        }
        let text_bytes = CStr::from_ptr(text).to_bytes();
        let text_str = match str::from_utf8(text_bytes) {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };
        let process_type_bits = match ProcessType::from_bits(process_type) {
            Some(pt) => pt,
            None => return ptr::null_mut(),
        };

        let processed_texts = reduce_text_process_rs(process_type_bits, text_str);

        let mut c_strings: Vec<*mut c_char> = Vec::with_capacity(processed_texts.len() + 1);
        for cow in processed_texts {
            if let Ok(cs) = CString::new(cow.as_ref()) {
                c_strings.push(cs.into_raw());
            }
        }

        // Add a NULL terminator to the end of the array
        c_strings.push(ptr::null_mut());

        c_strings.shrink_to_fit();
        let strings = c_strings.as_mut_ptr();
        std::mem::forget(c_strings);

        strings
    }));

    result.unwrap_or(ptr::null_mut())
}

/// Deallocates a `char**` array that was returned by `reduce_text_process`.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI.
/// The caller must pass a valid null-terminated array returned by `reduce_text_process`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_string_array(array: *mut *mut c_char) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !array.is_null() {
            // Reconstruct the vector by finding the null terminator
            let mut len = 0;
            while !(*array.add(len)).is_null() {
                len += 1;
            }
            // Include the null terminator in the length for deallocation
            let vec = Vec::from_raw_parts(array, len + 1, len + 1);
            for s in vec {
                if !s.is_null() {
                    drop(CString::from_raw(s));
                }
            }
        }
    }));
}
