//! C FFI bindings for the [`matcher_rs`] pattern-matching engine.
//!
//! # Lifecycle
//!
//! 1. Call [`init_simple_matcher`] (JSON) or use the builder API to get a `*mut
//!    SimpleMatcher`.
//! 2. Pass the pointer to query functions ([`simple_matcher_is_match`],
//!    [`simple_matcher_process`], [`simple_matcher_find_match`]).
//! 3. Free results with the corresponding `drop_*` function
//!    ([`drop_simple_result`], [`drop_simple_result_list`], [`drop_string`],
//!    [`drop_string_array`]).
//! 4. Call [`drop_simple_matcher`] when done.
//!
//! # Memory ownership
//!
//! Every pointer returned by this library has a corresponding `drop_*`
//! function. The caller is responsible for calling it exactly once.
//!
//! # String encoding
//!
//! All strings are null-terminated UTF-8 (`*const c_char`).
//!
//! # Panic safety
//!
//! All public functions wrap their body in
//! [`catch_unwind`](std::panic::catch_unwind) and return null / `false` on
//! panic.

use std::{
    ffi::{CStr, CString, c_char},
    panic::{self, AssertUnwindSafe},
    ptr, str,
};

use matcher_rs::{
    ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleTableSerde as SimpleTable,
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
};

// ---------------------------------------------------------------------------
// FFI helpers
// ---------------------------------------------------------------------------

/// Wraps an FFI function body in [`std::panic::catch_unwind`] with a default
/// return value on panic.
macro_rules! ffi_fn {
    ($name:expr, $default:expr, $body:expr) => {{
        let result = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| $body));
        result.unwrap_or_else(|_| {
            eprintln!(concat!($name, " panicked"));
            $default
        })
    }};
}

/// Null-checks a C string pointer, then decodes it as UTF-8.
///
/// # Safety
///
/// `ptr` must be a valid null-terminated C string pointer or null.
unsafe fn decode_c_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(ptr) }.to_bytes();
    match str::from_utf8(bytes) {
        Ok(s) => Some(s),
        Err(_) => {
            eprintln!("Input is not a valid utf-8 string");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Opaque builder for constructing a [`SimpleMatcher`] from C without JSON.
pub struct CSimpleMatcherBuilder {
    words: Vec<(u8, u32, String)>,
}

/// Creates a new empty builder. The caller must either call
/// [`simple_matcher_builder_build`] (which consumes it) or
/// [`drop_simple_matcher_builder`] to free it.
#[unsafe(no_mangle)]
pub extern "C" fn init_simple_matcher_builder() -> *mut CSimpleMatcherBuilder {
    Box::into_raw(Box::new(CSimpleMatcherBuilder { words: Vec::new() }))
}

/// Adds a word pattern to the builder. The `word` string is copied; the
/// caller retains ownership. Returns `true` on success.
///
/// # Safety
///
/// `builder` must be a valid pointer from [`init_simple_matcher_builder`].
/// `word` must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_builder_add_word(
    builder: *mut CSimpleMatcherBuilder,
    process_type: u8,
    word_id: u32,
    word: *const c_char,
) -> bool {
    ffi_fn!("simple_matcher_builder_add_word", false, unsafe {
        let Some(b) = builder.as_mut() else {
            return false;
        };
        let Some(word_str) = decode_c_str(word) else {
            return false;
        };
        b.words.push((process_type, word_id, word_str.to_owned()));
        true
    })
}

/// Consumes the builder and produces a [`SimpleMatcher`]. The builder is
/// **always** freed by this call (even on error). Returns null on error.
///
/// # Safety
///
/// `builder` must be a valid pointer from [`init_simple_matcher_builder`]
/// and must not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_builder_build(
    builder: *mut CSimpleMatcherBuilder,
) -> *mut SimpleMatcher {
    ffi_fn!("simple_matcher_builder_build", ptr::null_mut(), unsafe {
        if builder.is_null() {
            return ptr::null_mut();
        }
        let builder = Box::from_raw(builder);
        let mut rs_builder = SimpleMatcherBuilder::new();
        for &(bits, id, ref word) in &builder.words {
            rs_builder =
                rs_builder.add_word(ProcessType::from_bits_retain(bits), id, word.as_str());
        }
        match rs_builder.build() {
            Ok(matcher) => Box::into_raw(Box::new(matcher)),
            Err(e) => {
                eprintln!("SimpleMatcherBuilder build failed: {e}");
                ptr::null_mut()
            }
        }
    })
}

/// Frees a builder that was NOT consumed by [`simple_matcher_builder_build`].
///
/// # Safety
///
/// `builder` must be a valid pointer or null. Must not be called after
/// [`simple_matcher_builder_build`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_simple_matcher_builder(builder: *mut CSimpleMatcherBuilder) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !builder.is_null() {
            drop(Box::from_raw(builder));
        }
    }));
}

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

/// Returns the library version as a static null-terminated string.
///
/// The returned pointer is valid for the lifetime of the process and must NOT
/// be freed.
#[unsafe(no_mangle)]
pub extern "C" fn matcher_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

// ---------------------------------------------------------------------------
// SimpleMatcher lifecycle
// ---------------------------------------------------------------------------

/// Initializes a [`SimpleMatcher`] from JSON bytes. Returns null on error.
///
/// # Safety
///
/// `simple_table_bytes` must be a valid null-terminated C string containing
/// UTF-8 JSON. The returned pointer must be freed with [`drop_simple_matcher`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn init_simple_matcher(
    simple_table_bytes: *const c_char,
) -> *mut SimpleMatcher {
    ffi_fn!("init_simple_matcher", ptr::null_mut(), unsafe {
        if simple_table_bytes.is_null() {
            return ptr::null_mut();
        }
        let simple_table: SimpleTable =
            match sonic_rs::from_slice(CStr::from_ptr(simple_table_bytes).to_bytes()) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Deserialize simple_table_bytes failed: {e}");
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
    })
}

/// Deallocates a [`SimpleMatcher`].
///
/// # Safety
///
/// `simple_matcher` must have been returned by [`init_simple_matcher`] or
/// [`simple_matcher_builder_build`] and must not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_simple_matcher(simple_matcher: *mut SimpleMatcher) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !simple_matcher.is_null() {
            drop(Box::from_raw(simple_matcher))
        }
    }));
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

/// Returns whether any rule matches `text`. Returns `false` on null or error.
///
/// # Safety
///
/// `simple_matcher` must be a valid matcher pointer. `text` must be a valid
/// null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_is_match(
    simple_matcher: *const SimpleMatcher,
    text: *const c_char,
) -> bool {
    ffi_fn!("simple_matcher_is_match", false, unsafe {
        let Some(text_str) = decode_c_str(text) else {
            return false;
        };
        simple_matcher
            .as_ref()
            .is_some_and(|m| m.is_match(text_str))
    })
}

/// Returns all matches as a [`CSimpleResultList`], or null on error.
///
/// # Safety
///
/// `simple_matcher` must be a valid matcher pointer. `text` must be a valid
/// null-terminated C string. Free the result with [`drop_simple_result_list`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_process(
    simple_matcher: *const SimpleMatcher,
    text: *const c_char,
) -> *mut CSimpleResultList {
    ffi_fn!("simple_matcher_process", ptr::null_mut(), unsafe {
        let Some(text_str) = decode_c_str(text) else {
            return ptr::null_mut();
        };
        let Some(m) = simple_matcher.as_ref() else {
            return ptr::null_mut();
        };
        let results = m.process(text_str);
        let mut items: Vec<CSimpleResult> = Vec::with_capacity(results.len());
        for r in &results {
            let Ok(word) = CString::new(r.word.as_ref()) else {
                continue;
            };
            items.push(CSimpleResult {
                word_id: r.word_id,
                word: word.into_raw(),
            });
        }
        let len = items.len();
        let items_ptr = Box::into_raw(items.into_boxed_slice()) as *mut CSimpleResult;
        Box::into_raw(Box::new(CSimpleResultList {
            len,
            items: items_ptr,
        }))
    })
}

/// Returns the first match as a [`CSimpleResult`], or null if none.
///
/// # Safety
///
/// `simple_matcher` must be a valid matcher pointer. `text` must be a valid
/// null-terminated C string. Free a non-null result with
/// [`drop_simple_result`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_find_match(
    simple_matcher: *const SimpleMatcher,
    text: *const c_char,
) -> *mut CSimpleResult {
    ffi_fn!("simple_matcher_find_match", ptr::null_mut(), unsafe {
        let Some(text_str) = decode_c_str(text) else {
            return ptr::null_mut();
        };
        let Some(m) = simple_matcher.as_ref() else {
            return ptr::null_mut();
        };
        let Some(r) = m.find_match(text_str) else {
            return ptr::null_mut();
        };
        let Ok(word) = CString::new(r.word.as_ref()) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(CSimpleResult {
            word_id: r.word_id,
            word: word.into_raw(),
        }))
    })
}

/// Approximate heap memory in bytes used by the matcher. Returns 0 on null.
///
/// # Safety
///
/// `simple_matcher` must be a valid matcher pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn simple_matcher_heap_bytes(simple_matcher: *const SimpleMatcher) -> usize {
    ffi_fn!("simple_matcher_heap_bytes", 0, unsafe {
        simple_matcher.as_ref().map_or(0, |m| m.heap_bytes())
    })
}

// ---------------------------------------------------------------------------
// Result deallocation
// ---------------------------------------------------------------------------

/// Frees a single [`CSimpleResult`] returned by [`simple_matcher_find_match`].
///
/// # Safety
///
/// `result` must have been returned by [`simple_matcher_find_match`].
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
///
/// `list` must have been returned by [`simple_matcher_process`].
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

// ---------------------------------------------------------------------------
// Text processing
// ---------------------------------------------------------------------------

/// Applies the text transformation pipeline. Returns null on error.
///
/// # Safety
///
/// `text` must be a valid null-terminated C string. Free the result with
/// [`drop_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn text_process(process_type: u8, text: *const c_char) -> *mut c_char {
    ffi_fn!("text_process", ptr::null_mut(), unsafe {
        let Some(text_str) = decode_c_str(text) else {
            return ptr::null_mut();
        };
        let res = text_process_rs(ProcessType::from_bits_retain(process_type), text_str);
        CString::new(res.as_ref())
            .map(CString::into_raw)
            .unwrap_or(ptr::null_mut())
    })
}

/// Applies the transformation pipeline, returning a null-terminated array of
/// all intermediate variants. Returns null on error.
///
/// # Safety
///
/// `text` must be a valid null-terminated C string. Free the result with
/// [`drop_string_array`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn reduce_text_process(
    process_type: u8,
    text: *const c_char,
) -> *mut *mut c_char {
    ffi_fn!("reduce_text_process", ptr::null_mut(), unsafe {
        let Some(text_str) = decode_c_str(text) else {
            return ptr::null_mut();
        };
        let variants =
            reduce_text_process_rs(ProcessType::from_bits_retain(process_type), text_str);
        let mut c_strings: Vec<*mut c_char> = Vec::with_capacity(variants.len() + 1);
        for cow in variants {
            if let Ok(cs) = CString::new(cow.as_ref()) {
                c_strings.push(cs.into_raw());
            }
        }
        c_strings.push(ptr::null_mut());
        Box::into_raw(c_strings.into_boxed_slice()) as *mut *mut c_char
    })
}

// ---------------------------------------------------------------------------
// String deallocation
// ---------------------------------------------------------------------------

/// Frees a C string returned by [`text_process`].
///
/// # Safety
///
/// `ptr` must have been returned by a function in this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_string(ptr: *mut c_char) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !ptr.is_null() {
            drop(CString::from_raw(ptr))
        }
    }));
}

/// Frees a null-terminated `char**` array returned by
/// [`reduce_text_process`].
///
/// # Safety
///
/// `array` must have been returned by [`reduce_text_process`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_string_array(array: *mut *mut c_char) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| unsafe {
        if !array.is_null() {
            let mut len = 0;
            while !(*array.add(len)).is_null() {
                drop(CString::from_raw(*array.add(len)));
                len += 1;
            }
            drop(Box::from_raw(ptr::slice_from_raw_parts_mut(array, len + 1)));
        }
    }));
}
