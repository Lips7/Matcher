#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::{
    ffi::{CStr, CString},
    str::from_utf8_unchecked,
};

use matcher_rs::{MatchTableMap, Matcher, SimpleMatchTypeWordMap, SimpleMatcher, TextMatcherTrait};

/// # Safety
/// This function is unsafe because it assumes that the provided pointer is valid and points to a null-terminated
/// byte string that can be deserialized into a `MatchTableMap`.
///
/// # Arguments
/// * `match_table_map_bytes` - A pointer to a null-terminated byte string that represents a serialized
///   `MatchTableMap`.
///
/// # Returns
/// * A raw pointer to a new `Matcher` instance that is created using the deserialized `MatchTableMap`.
///
/// # Panics
/// This function will panic if the deserialization of `match_table_map_bytes` fails.
///
/// # Description
/// This function initializes a `Matcher` instance from the provided serialized `MatchTableMap` byte string.
/// It performs deserialization of the byte string into a `MatchTableMap`, and then uses it to create a new `Matcher`.
/// The newly created `Matcher` instance is then wrapped in a `Box` and converted into a raw pointer before being returned.
#[no_mangle]
pub extern "C" fn init_matcher(match_table_map_bytes: *const i8) -> *mut Matcher {
    unsafe {
        // Convert the raw pointer passed as `match_table_map_bytes` to a CStr and then to a byte slice.
        // Deserialize the byte slice into a `MatchTableMap` instance.
        let match_table_map: MatchTableMap = match rmp_serde::from_slice(
            CStr::from_ptr(match_table_map_bytes).to_bytes(),
        ) {
            // If deserialization is successful, assign the `MatchTableMap` to `match_table_map`.
            Ok(match_table_map) => match_table_map,
            // If deserialization fails, panic with an error message containing the deserialization error.
            Err(e) => {
                panic!("Deserialize match_table_map_bytes failed, Please check the input data.\nErr: {}", e)
            }
        };

        // Create a new `Matcher` instance using the deserialized `MatchTableMap` and wrap it in a Box.
        // Convert the Box into a raw pointer using `Box::into_raw` before returning it.
        Box::into_raw(Box::new(Matcher::new(match_table_map)))
    }
}

/// # Safety
/// This function is unsafe because it assumes that the provided `matcher` and `text` pointers are valid.
/// The `matcher` pointer should point to a valid `Matcher` instance, and the `text` pointer should point to a null-terminated byte string.
///
/// # Arguments
/// * `matcher` - A raw pointer to a `Matcher` instance.
/// * `text` - A pointer to a null-terminated byte string that represents the text to be matched.
///
/// # Returns
/// * A boolean value indicating whether the text matches the pattern defined by the `Matcher` instance.
///
/// # Panics
/// This function will panic if the `matcher` pointer is null.
#[no_mangle]
pub extern "C" fn matcher_is_match(matcher: *mut Matcher, text: *const i8) -> bool {
    unsafe {
        // Dereference the matcher pointer and convert it to a reference.
        // Unwrap the Option to get the underlying Matcher reference.
        matcher
            .as_ref()
            .unwrap()
            // Call the is_match method on the Matcher reference.
            // Convert the text pointer from a C string to a byte slice, and then to a UTF-8 string.
            .is_match(from_utf8_unchecked(CStr::from_ptr(text).to_bytes()))
    }
}

/// # Safety
/// This function is unsafe because it assumes that the provided `matcher` and `text` pointers are valid.
/// The `matcher` pointer should point to a valid `Matcher` instance, and the `text` pointer should point to a null-terminated byte string.
///
/// # Arguments
/// * `matcher` - A raw pointer to a `Matcher` instance.
/// * `text` - A pointer to a null-terminated byte string that represents the text to be matched.
///
/// # Returns
/// * A raw pointer to an i8 holding a JSON-encoded string indicating the result of the `word_match` function called on the `Matcher` instance.
///
/// # Panics
/// This function will panic if any of the following occur:
/// * The `matcher` pointer is null.
/// * The byte slice pointed to by `text` is not valid UTF-8.
/// * Creating a `CString` from the JSON string fails.
#[no_mangle]
pub extern "C" fn matcher_word_match(matcher: *mut Matcher, text: *const i8) -> *mut i8 {
    // Unsafe block to perform operations requiring manual memory management and direct pointer manipulation.
    let res = unsafe {
        // Create a new CString from a JSON string, ensuring it is null-terminated and safe for C-interoperability.
        CString::new(
            // Serialize the result of the word_match function to a JSON string.
            sonic_rs::to_string(
                &matcher
                    // Convert the raw matcher pointer to a reference. If null, unwrap will cause a panic.
                    .as_ref()
                    .unwrap()
                    // Perform the word_match operation, converting the text pointer from C string to byte slice, and then to UTF-8 string.
                    .word_match(from_utf8_unchecked(CStr::from_ptr(text).to_bytes())),
            )
            // Unwrap the Result to obtain the JSON string. Panics if serialization fails.
            .unwrap(),
        )
        // Unwrap the Result to obtain the CString. Panics if the string contains an interior null byte.
        .unwrap()
    };

    // Convert the CString into a raw pointer and return it.
    res.into_raw()
}

/// # Safety
/// This function is unsafe because it assumes that the provided `matcher` pointer is valid and was previously allocated using `Box::into_raw`.
/// It also assumes that the lifetime of the `matcher` pointer is over and it is safe to drop the data.
///
/// # Arguments
/// * `matcher` - A raw pointer to a `Matcher` instance that needs to be freed.
///
/// # Panics
/// This function will panic if the `matcher` pointer is null.
/// It is the caller's responsibility to ensure that the pointer is valid and that no other references to the `Matcher` instance exist.
///
/// # Description
/// This function converts the raw pointer back into a `Box` and then drops it, effectively freeing the memory that the `Matcher` instance occupied.
/// After calling this function, the `matcher` pointer must not be used again.
#[no_mangle]
pub extern "C" fn drop_matcher(matcher: *mut Matcher) {
    unsafe { drop(Box::from_raw(matcher)) }
}

/// # Safety
/// This function is unsafe because it assumes that the provided pointer is valid and points to a null-terminated
/// byte string that can be deserialized into a `SimpleMatchTypeWordMap`.
///
/// # Arguments
/// * `simple_match_type_word_map_bytes` - A pointer to a null-terminated byte string that represents a serialized
///   `SimpleMatchTypeWordMap`.
///
/// # Returns
/// * A raw pointer to a new `SimpleMatcher` instance that is created using the deserialized `SimpleMatchTypeWordMap`.
///
/// # Panics
/// This function will panic if the deserialization of `simple_match_type_word_map_bytes` fails.
///
/// # Description
/// This function initializes a `SimpleMatcher` instance from the provided serialized `SimpleMatchTypeWordMap` byte string.
/// It performs deserialization of the byte string, transforms it into a `SimpleMatchTypeWordMap`, and then uses it to
/// create a new `SimpleMatcher`. The newly created `SimpleMatcher` instance is then wrapped in a `Box` and converted
/// into a raw pointer before being returned.
#[no_mangle]
pub extern "C" fn init_simple_matcher(
    simple_match_type_word_map_bytes: *const i8,
) -> *mut SimpleMatcher {
    unsafe {
        // Convert the raw pointer passed as `simple_match_type_word_map_bytes` to a CStr and then to a byte slice.
        // Deserialize the byte slice into a `SimpleMatchTypeWordMap` instance.
        let simple_match_type_word_map: SimpleMatchTypeWordMap = match rmp_serde::from_slice(
            CStr::from_ptr(simple_match_type_word_map_bytes).to_bytes(),
        ) {
            // If deserialization is successful, assign the `SimpleMatchTypeWordMap` to `simple_match_type_word_map`.
            Ok(simple_match_type_word_map) => simple_match_type_word_map,
            // If deserialization fails, panic with an error message containing the deserialization error.
            Err(e) => {
                panic!(
                    "Deserialize simple_match_type_word_map_bytes failed, Please check the input data.\nErr: {}", e,
                )
            }
        };

        // Create a new `SimpleMatcher` instance using the deserialized `SimpleMatchTypeWordMap` and wrap it in a Box.
        // Convert the Box into a raw pointer using `Box::into_raw` before returning it.
        Box::into_raw(Box::new(SimpleMatcher::new(simple_match_type_word_map)))
    }
}

/// # Safety
/// This function is unsafe because it assumes that the provided `simple_matcher` and `text` pointers are valid.
/// The `simple_matcher` pointer should point to a valid `SimpleMatcher` instance, and the `text` pointer should point to a null-terminated byte string.
///
/// # Arguments
/// * `simple_matcher` - A raw pointer to a `SimpleMatcher` instance.
/// * `text` - A pointer to a null-terminated byte string that represents the text to be matched.
///
/// # Returns
/// * A boolean value indicating whether the text matches the pattern defined by the `SimpleMatcher` instance.
///
/// # Panics
/// This function will panic if the `simple_matcher` pointer is null.
///
/// # Description
/// This function calls the `is_match` method on a `SimpleMatcher` instance. It converts the raw pointers to their
/// respective Rust types, performs the `is_match` operation, and returns a boolean indicating the match result.
/// The conversion assumes that the `text` pointer points to a valid UTF-8 encoded, null-terminated C string, and
/// that the `simple_matcher` pointer is valid and non-null.
#[no_mangle]
pub extern "C" fn simple_matcher_is_match(
    simple_matcher: *mut SimpleMatcher,
    text: *const i8,
) -> bool {
    // Unsafe block to perform operations that involve manual memory management and direct pointer manipulation
    unsafe {
        // Attempt to convert the raw pointer 'simple_matcher' to a reference.
        // If 'simple_matcher' is a null pointer, 'unwrap()' will panic.
        simple_matcher
            .as_ref()
            // Dereference the Option to get the underlying 'SimpleMatcher' reference.
            .unwrap()
            // Call the 'is_match' method on the 'SimpleMatcher' instance.
            // Convert the 'text' pointer from a C string to a byte slice, and then to a UTF-8 string.
            .is_match(from_utf8_unchecked(CStr::from_ptr(text).to_bytes()))
    }
}

/// # Safety
/// This function is unsafe because it assumes that the provided `simple_matcher` and `text` pointers are valid.
/// The `simple_matcher` pointer should point to a valid `SimpleMatcher` instance, and the `text` pointer should point to a null-terminated byte string.
///
/// # Arguments
/// * `simple_matcher` - A raw pointer to a `SimpleMatcher` instance.
/// * `text` - A pointer to a null-terminated byte string that represents the text to be processed.
///
/// # Returns
/// * A raw pointer to an i8 holding a JSON-encoded string indicating the result of the `process` function called on the `SimpleMatcher` instance.
///
/// # Panics
/// This function will panic if any of the following occur:
/// * The `simple_matcher` pointer is null.
/// * The byte slice pointed to by `text` is not valid UTF-8.
/// * Creating a `CString` from the JSON string fails.
///
/// # Description
/// This function calls the `process` method on a `SimpleMatcher` instance, converting the result to a JSON string.
/// It converts the raw pointers to their respective Rust types, performs the `process` operation,
/// serializes the result as a JSON string, and then converts this string to a C-compatible CString.
/// The resulting CString is then converted into a raw pointer before being returned.
#[no_mangle]
pub extern "C" fn simple_matcher_process(
    simple_matcher: *mut SimpleMatcher,
    text: *const i8,
) -> *mut i8 {
    // Begin unsafe block to allow for manual memory management and pointer manipulation.
    let res = unsafe {
        // Create a new CString, which ensures it is null-terminated and safe for C-interoperability.
        CString::new(
            // Serialize the result of the process method to a JSON string.
            sonic_rs::to_string(
                &simple_matcher
                    // Convert the raw simple_matcher pointer to a reference. If null, unwrap will cause a panic.
                    .as_ref()
                    .unwrap()
                    // Perform the process operation, converting the text pointer from C string to byte slice, and then to UTF-8 string.
                    .process(from_utf8_unchecked(CStr::from_ptr(text).to_bytes())),
            )
            // Unwrap the Result to obtain the JSON string. Panics if serialization fails.
            .unwrap(),
        )
        // Unwrap the Result to obtain the CString. Panics if the string contains an interior null byte.
        .unwrap()
    };

    // Convert the CString into a raw pointer and return it.
    res.into_raw()
}

/// # Safety
/// This function is unsafe because it assumes that the provided `simple_matcher` pointer is valid and was previously allocated using `Box::into_raw`.
/// It also assumes that the lifetime of the `simple_matcher` pointer is over and it is safe to drop the data.
///
/// # Arguments
/// * `simple_matcher` - A raw pointer to a `SimpleMatcher` instance that needs to be freed.
///
/// # Panics
/// This function will panic if the `simple_matcher` pointer is null.
/// It is the caller's responsibility to ensure that the pointer is valid and that no other references to the `SimpleMatcher` instance exist.
///
/// # Description
/// This function converts the raw pointer back into a `Box` and then drops it, effectively freeing the memory that the `SimpleMatcher` instance occupied.
/// After calling this function, the `simple_matcher` pointer must not be used again.
#[no_mangle]
pub extern "C" fn drop_simple_matcher(simple_matcher: *mut SimpleMatcher) {
    unsafe { drop(Box::from_raw(simple_matcher)) }
}

/// # Safety
/// This function is unsafe because it assumes that the provided pointer is valid and was previously allocated using `CString::into_raw`.
/// It also assumes that the lifetime of the pointer is over and it is safe to drop the data.
///
/// # Arguments
/// * `ptr` - A raw pointer to a null-terminated byte string that needs to be freed.
///
/// # Panics
/// This function will panic if the `ptr` pointer is null.
/// It is the caller's responsibility to ensure that the pointer is valid and that no other references to the CString data exist.
///
/// # Description
/// This function converts the raw pointer back into a `CString` and then drops it, effectively freeing the memory that the CString instance occupied.
/// After calling this function, the `ptr` pointer must not be used again.
#[no_mangle]
pub extern "C" fn drop_string(ptr: *mut i8) {
    unsafe { drop(CString::from_raw(ptr)) }
}
