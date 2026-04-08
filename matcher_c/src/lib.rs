use std::{
    ffi::{CStr, CString, c_char},
    panic::{self, AssertUnwindSafe},
    ptr, str,
};

/// Returns the library version as a static null-terminated string.
///
/// The returned pointer is valid for the lifetime of the process and must NOT
/// be freed.
#[unsafe(no_mangle)]
pub extern "C" fn matcher_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

use matcher_rs::{
    ProcessType, SimpleMatcher, SimpleTableSerde as SimpleTable,
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
};

/// A single match result returned across the FFI boundary.
///
/// `word` is a heap-allocated null-terminated UTF-8 string owned by this
/// struct. Free with [`drop_simple_result`] (single) or
/// [`drop_simple_result_list`] (when part of a list).
#[repr(C)]
pub struct CSimpleResult {
    pub word_id: u32,
    pub word: *mut c_char,
}

/// A list of match results returned by [`simple_matcher_process`].
///
/// `items` points to a heap-allocated array of `len` [`CSimpleResult`]
/// elements. Free the entire list with [`drop_simple_result_list`].
#[repr(C)]
pub struct CSimpleResultList {
    pub len: usize,
    pub items: *mut CSimpleResult,
}

/// Initializes a [`SimpleMatcher`] instance from serialized table bytes.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The
/// caller must ensure that `simple_table_bytes` points to a valid
/// null-terminated C string. The returned [`SimpleMatcher`] pointer must be
/// properly managed and eventually deallocated by calling
/// `drop_simple_matcher`.
///
/// # Arguments
/// - `simple_table_bytes`: A pointer to a C string containing the serialized
///   table bytes.
///
/// # Returns
/// A pointer to a newly allocated [`SimpleMatcher`] instance, or null on error.
/// The caller is responsible for managing the lifetime of this pointer and must
/// eventually call [`drop_simple_matcher`] to free the memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn init_simple_matcher(
    simple_table_bytes: *const c_char,
) -> *mut SimpleMatcher {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if simple_table_bytes.is_null() {
            return ptr::null_mut();
        }
        let simple_table: SimpleTable =
            match sonic_rs::from_slice(CStr::from_ptr(simple_table_bytes).to_bytes()) {
                Ok(simple_table) => simple_table,
                Err(e) => {
                    eprintln!("Deserialize simple_table_bytes failed: {}", e);
                    return ptr::null_mut();
                }
            };

        match SimpleMatcher::new(&simple_table) {
            Ok(matcher) => Box::into_raw(Box::new(matcher)),
            Err(e) => {
                eprintln!("SimpleMatcher build failed: {e}");
                ptr::null_mut()
            }
        }
    }));

    result.unwrap_or_else(|_| {
        eprintln!("init_simple_matcher panicked");
        ptr::null_mut()
    })
}

/// Determines if the input text matches using the [`SimpleMatcher`].
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The
/// caller must ensure that `simple_matcher` points to a valid [`SimpleMatcher`]
/// instance and that `text` points to a valid null-terminated C string. Both
/// the `simple_matcher` and the `text` must remain valid for the duration of
/// the call.
///
/// # Arguments
/// - `simple_matcher`: A pointer to the [`SimpleMatcher`] instance.
/// - `text`: A pointer to a C string containing the text to be processed.
///
/// # Returns
/// A boolean indicating whether the text matches, or `false` on any error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_is_match(
    simple_matcher: *const SimpleMatcher,
    text: *const c_char,
) -> bool {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if text.is_null() {
            return false;
        }
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

/// Returns all matches for the input text as a [`CSimpleResultList`].
///
/// # Safety
/// The caller must ensure that `simple_matcher` points to a valid
/// [`SimpleMatcher`] instance and that `text` points to a valid
/// null-terminated C string. The caller must free the returned list with
/// [`drop_simple_result_list`].
///
/// # Returns
/// A pointer to a heap-allocated [`CSimpleResultList`], or null on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_process(
    simple_matcher: *const SimpleMatcher,
    text: *const c_char,
) -> *mut CSimpleResultList {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if text.is_null() {
            return ptr::null_mut();
        }
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
        let results = m.process(text_str);
        let mut items: Vec<CSimpleResult> = Vec::with_capacity(results.len());
        for r in &results {
            let word = match CString::new(r.word.as_ref()) {
                Ok(cs) => cs.into_raw(),
                Err(_) => continue,
            };
            items.push(CSimpleResult {
                word_id: r.word_id,
                word,
            });
        }
        let len = items.len();
        let items_ptr = Box::into_raw(items.into_boxed_slice()) as *mut CSimpleResult;
        Box::into_raw(Box::new(CSimpleResultList {
            len,
            items: items_ptr,
        }))
    }));

    result.unwrap_or_else(|_| {
        eprintln!("simple_matcher_process panicked");
        ptr::null_mut()
    })
}

/// Returns the first match for the input text as a [`CSimpleResult`].
///
/// # Safety
/// The caller must ensure that `simple_matcher` points to a valid
/// [`SimpleMatcher`] instance and that `text` points to a valid
/// null-terminated C string. The caller must free the returned result
/// with [`drop_simple_result`].
///
/// # Returns
/// A pointer to a heap-allocated [`CSimpleResult`], or null if no match
/// is found or an error occurs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_find_match(
    simple_matcher: *const SimpleMatcher,
    text: *const c_char,
) -> *mut CSimpleResult {
    let result = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if text.is_null() {
            return ptr::null_mut();
        }
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
        let r = match m.find_match(text_str) {
            Some(r) => r,
            None => return ptr::null_mut(),
        };
        let word = match CString::new(r.word.as_ref()) {
            Ok(cs) => cs.into_raw(),
            Err(_) => return ptr::null_mut(),
        };
        Box::into_raw(Box::new(CSimpleResult {
            word_id: r.word_id,
            word,
        }))
    }));

    result.unwrap_or_else(|_| {
        eprintln!("simple_matcher_find_match panicked");
        ptr::null_mut()
    })
}

/// Frees a single [`CSimpleResult`] returned by [`simple_matcher_find_match`].
///
/// # Safety
/// The pointer must have been returned by [`simple_matcher_find_match`] and
/// must not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_simple_result(result: *mut CSimpleResult) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !result.is_null() {
            let r = Box::from_raw(result);
            if !r.word.is_null() {
                drop(CString::from_raw(r.word));
            }
        }
    }));
}

/// Frees a [`CSimpleResultList`] returned by [`simple_matcher_process`].
///
/// # Safety
/// The pointer must have been returned by [`simple_matcher_process`] and
/// must not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_simple_result_list(list: *mut CSimpleResultList) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !list.is_null() {
            let l = Box::from_raw(list);
            if !l.items.is_null() && l.len > 0 {
                let items = Box::from_raw(ptr::slice_from_raw_parts_mut(l.items, l.len));
                for item in items.iter() {
                    if !item.word.is_null() {
                        drop(CString::from_raw(item.word));
                    }
                }
            }
        }
    }));
}

/// Deallocates a [`SimpleMatcher`] instance.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The
/// caller must ensure that `simple_matcher` points to a valid [`SimpleMatcher`]
/// instance that was previously allocated by [`init_simple_matcher`]. After
/// calling this function, the `simple_matcher` pointer must not be used again
/// as it points to deallocated memory.
///
/// # Arguments
/// - `simple_matcher`: A pointer to the [`SimpleMatcher`] instance to be
///   deallocated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_simple_matcher(simple_matcher: *mut SimpleMatcher) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !simple_matcher.is_null() {
            drop(Box::from_raw(simple_matcher))
        }
    }));
}

/// Deallocates a C string that was previously allocated by the Rust code and
/// passed to C.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI. The
/// caller must ensure that `ptr` points to a valid C string that was previously
/// allocated by Rust code using [`CString::into_raw`] or a similar method.
/// After calling this function, the `ptr` pointer must not be used again as it
/// points to deallocated memory.
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
        let process_type_bit = ProcessType::from_bits_retain(process_type);
        let res = text_process_rs(process_type_bit, text_str);
        match CString::new(res.as_ref()) {
            Ok(cs) => cs.into_raw(),
            Err(_) => ptr::null_mut(),
        }
    }));
    result.unwrap_or_else(|_| {
        eprintln!("text_process panicked");
        ptr::null_mut()
    })
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
        let process_type_bits = ProcessType::from_bits_retain(process_type);

        let processed_texts = reduce_text_process_rs(process_type_bits, text_str);

        let mut c_strings: Vec<*mut c_char> = Vec::with_capacity(processed_texts.len() + 1);
        for cow in processed_texts {
            if let Ok(cs) = CString::new(cow.as_ref()) {
                c_strings.push(cs.into_raw());
            }
        }

        // Add a NULL terminator to the end of the array
        c_strings.push(ptr::null_mut());

        // into_boxed_slice guarantees capacity == len, avoiding UB in drop_string_array
        Box::into_raw(c_strings.into_boxed_slice()) as *mut *mut c_char
    }));

    result.unwrap_or_else(|_| {
        eprintln!("reduce_text_process panicked");
        ptr::null_mut()
    })
}

/// Deallocates a `char**` array that was returned by `reduce_text_process`.
///
/// # Safety
/// This function is unsafe because it relies on raw pointers and FFI.
/// The caller must pass a valid null-terminated array returned by
/// `reduce_text_process`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_string_array(array: *mut *mut c_char) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !array.is_null() {
            // Walk to find length (null terminator not included in count), freeing each
            // string
            let mut len = 0;
            while !(*array.add(len)).is_null() {
                drop(CString::from_raw(*array.add(len)));
                len += 1;
            }
            // Reconstruct the boxed slice (len + 1 includes the null terminator) and drop
            // it
            drop(Box::from_raw(ptr::slice_from_raw_parts_mut(array, len + 1)));
        }
    }));
}
