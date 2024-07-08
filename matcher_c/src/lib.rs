use std::{
    ffi::{c_char, CStr, CString},
    str,
};

use matcher_rs::{MatchTableMap, Matcher, SimpleMatchTypeWordMap, SimpleMatcher, TextMatcherTrait};

/// # Safety
/// This function is unsafe because it assumes that the provided pointer is valid and points to a null-terminated
/// byte string that can be deserialized into a [MatchTableMap].
///
/// # Arguments
/// * `match_table_map_bytes` - A pointer to a null-terminated byte string that represents a serialized [MatchTableMap].
///
/// # Returns
/// * A raw pointer to a new [Matcher] instance that is created using the deserialized [MatchTableMap].
///
/// # Panics
/// This function will panic if the deserialization of `match_table_map_bytes` fails.
///
/// # Description
/// This function initializes a [Matcher] instance from the provided serialized [MatchTableMap] byte string.
/// It performs deserialization of the byte string, transforms it into a [MatchTableMap], and then uses it to
/// create a new [Matcher]. The newly created [Matcher] instance is then wrapped in a [Box] and converted
/// into a raw pointer before being returned.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use std::ffi::CString;
///
/// use matcher_c::*;
/// use matcher_rs::{MatchTable, MatchTableType, SimpleMatchType};
///
/// let mut match_table_map = HashMap::new();
/// match_table_map.insert(
///     1,
///     vec![
///         MatchTable {
///             table_id: 1,
///             match_table_type: MatchTableType::Simple { simple_match_type: SimpleMatchType::None },
///             word_list: vec!["hello", "world"],
///             exemption_simple_match_type: SimpleMatchType::None,
///             exemption_word_list: vec![],
///         }
///     ]
/// );
/// let match_table_map_bytes = CString::new(rmp_serde::to_vec_named(&match_table_map).unwrap()).unwrap();
///
/// let matcher_ptr = unsafe {init_matcher(match_table_map_bytes.as_ptr())};
/// unsafe {drop_matcher(matcher_ptr)};
/// ```
#[no_mangle]
pub unsafe extern "C" fn init_matcher(match_table_map_bytes: *const c_char) -> *mut Matcher {
    unsafe {
        println!("{:?}", CStr::from_ptr(match_table_map_bytes).to_bytes());
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

/// # Safety
/// This function is unsafe because it assumes that the provided `matcher` and `text` pointers are valid.
/// The `matcher` pointer should point to a valid [Matcher] instance, and the `text` pointer should point to a null-terminated byte string.
///
/// # Arguments
/// * `matcher` - A raw pointer to a [Matcher] instance.
/// * `text` - A pointer to a null-terminated byte string that represents the text to be matched.
///
/// # Returns
/// * A boolean value indicating whether the text matches the pattern defined by the [Matcher] instance.
///
/// # Panics
/// This function will panic if the `matcher` pointer is null.
///
/// # Description
/// This function calls the [is_match](matcher_rs::Matcher::is_match) method on a [Matcher] instance. It converts the raw pointers to their
/// respective Rust types, performs the [is_match](matcher_rs::Matcher::is_match) operation, and returns a boolean indicating the match result.
/// The conversion assumes that the `text` pointer points to a valid UTF-8 encoded, null-terminated C string, and
/// that the `matcher` pointer is valid and non-null.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use std::ffi::CString;
///
/// use matcher_c::*;
/// use matcher_rs::{MatchTable, MatchTableType, SimpleMatchType};
///
/// let mut match_table_map = HashMap::new();
/// match_table_map.insert(
///     1,
///     vec![
///         MatchTable {
///             table_id: 1,
///             match_table_type: MatchTableType::Simple { simple_match_type: SimpleMatchType::None },
///             word_list: vec!["hello", "world"],
///             exemption_simple_match_type: SimpleMatchType::None,
///             exemption_word_list: vec![],
///         }
///     ]
/// );
/// let match_table_map_bytes = CString::new(rmp_serde::to_vec_named(&match_table_map).unwrap()).unwrap();
///
/// let matcher_ptr = unsafe {init_matcher(match_table_map_bytes.as_ptr())};
///
/// let match_text_bytes = CString::new("hello world!").unwrap();
/// let not_match_text_bytes = CString::new("test").unwrap();
///
/// assert!(unsafe {matcher_is_match(matcher_ptr, match_text_bytes.as_ptr())});
/// assert!(!unsafe {matcher_is_match(matcher_ptr, not_match_text_bytes.as_ptr())});
///
/// unsafe {drop_matcher(matcher_ptr)};
/// ```
#[no_mangle]
pub unsafe extern "C" fn matcher_is_match(matcher: *mut Matcher, text: *const c_char) -> bool {
    unsafe {
        matcher
            .as_ref()
            .unwrap()
            .is_match(str::from_utf8_unchecked(CStr::from_ptr(text).to_bytes()))
    }
}

/// # Safety
/// This function is unsafe because it assumes that the provided `matcher` and `text` pointers are valid.
/// The `matcher` pointer should point to a valid [Matcher] instance, and the `text` pointer should point to a
/// null-terminated byte string.
///
/// # Arguments
/// * `matcher` - A raw pointer to a [Matcher] instance.
/// * `text` - A pointer to a null-terminated byte string that represents the text to be matched.
///
/// # Returns
/// * A raw pointer to an [c_char] holding the result of the [word_match_as_string](matcher_rs::Matcher::word_match_as_string) function called on the [Matcher] instance.
///
/// # Panics
/// This function will panic if the `matcher` pointer is null.
///
/// # Description
/// This function calls the [word_match_as_string](matcher_rs::Matcher::word_match_as_string) method on a [Matcher] instance, converting the result to a JSON string.
/// It converts the raw pointers to their respective Rust types, performs the [word_match](matcher_rs::Matcher::word_match_as_string) operation,
/// serializes the result as a JSON string, and then converts this string to a C-compatible CString.
/// The resulting CString is then returned as a raw pointer before being returned.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use std::ffi::{CStr, CString};
/// use std::str;
///
/// use matcher_c::*;
/// use matcher_rs::{MatchTable, MatchTableType, SimpleMatchType};
///
/// let mut match_table_map = HashMap::new();
/// match_table_map.insert(
///     1,
///     vec![
///         MatchTable {
///             table_id: 1,
///             match_table_type: MatchTableType::Simple { simple_match_type: SimpleMatchType::None },
///             word_list: vec!["hello", "world"],
///             exemption_simple_match_type: SimpleMatchType::None,
///             exemption_word_list: vec![],
///         }
///     ]
/// );
/// let match_table_map_bytes = CString::new(rmp_serde::to_vec_named(&match_table_map).unwrap()).unwrap();
///
/// let matcher_ptr = unsafe {init_matcher(match_table_map_bytes.as_ptr())};
///
/// let match_text_bytes = CString::new("hello world!").unwrap();
/// let not_match_text_bytes = CString::new("test").unwrap();
///
/// assert_eq!(
///     unsafe {
///         str::from_utf8_unchecked(
///             CStr::from_ptr(
///                 matcher_word_match(
///                     matcher_ptr,
///                     match_text_bytes.as_ptr()
///                 )
///             ).to_bytes()
///         )
///     },
///     r#"{"1":[{"match_id":1,"table_id":1,"word":"hello"},{"match_id":1,"table_id":1,"word":"world"}]}"#
/// );
/// assert_eq!(
///     unsafe {
///         str::from_utf8_unchecked(
///             CStr::from_ptr(
///                 matcher_word_match(
///                     matcher_ptr,
///                     not_match_text_bytes.as_ptr()
///                 )
///             ).to_bytes()
///         )
///     },
///     r#"{}"#
/// );
///
/// unsafe {drop_matcher(matcher_ptr)};
/// ```
#[no_mangle]
pub unsafe extern "C" fn matcher_word_match(
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

/// # Safety
/// This function is unsafe because it assumes that the provided `matcher` pointer is valid and was previously allocated using [Box::into_raw].
/// It also assumes that the lifetime of the `matcher` pointer is over and it is safe to drop the data.
///
/// # Arguments
/// * `matcher` - A raw pointer to a [Matcher] instance that needs to be freed.
///
/// # Panics
/// This function will panic if the `matcher` pointer is null.
/// It is the caller's responsibility to ensure that the pointer is valid and that no other references to the [Matcher] instance exist.
///
/// # Description
/// This function converts the raw pointer back into a [Box] and then drops it, effectively freeing the memory that the [Matcher] instance occupied.
/// After calling this function, the `matcher` pointer must not be used again.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use std::ffi::CString;
///
/// use matcher_c::*;
/// use matcher_rs::{MatchTable, MatchTableType, SimpleMatchType};
///
/// let mut match_table_map = HashMap::new();
/// match_table_map.insert(
///     1,
///     vec![
///         MatchTable {
///             table_id: 1,
///             match_table_type: MatchTableType::Simple {
///                 simple_match_type: SimpleMatchType::None,
///             },
///             word_list: vec!["hello", "world"],
///             exemption_simple_match_type: SimpleMatchType::None,
///             exemption_word_list: vec![],
///         }
///     ]
/// );
/// let match_table_map_bytes = CString::new(rmp_serde::to_vec_named(&match_table_map).unwrap()).unwrap();
///
/// let matcher_ptr = unsafe {init_matcher(match_table_map_bytes.as_ptr())};
/// unsafe {drop_matcher(matcher_ptr)};
/// ```
#[no_mangle]
pub unsafe extern "C" fn drop_matcher(matcher: *mut Matcher) {
    unsafe { drop(Box::from_raw(matcher)) }
}

/// # Safety
/// This function is unsafe because it assumes that the provided pointer is valid and points to a null-terminated
/// byte string that can be deserialized into a [SimpleMatchTypeWordMap].
///
/// # Arguments
/// * `simple_match_type_word_map_bytes` - A pointer to a null-terminated byte string that represents a serialized [SimpleMatchTypeWordMap].
///
/// # Returns
/// * A raw pointer to a new [SimpleMatcher] instance that is created using the deserialized [SimpleMatchTypeWordMap].
///
/// # Panics
/// This function will panic if the deserialization of `simple_match_type_word_map_bytes` fails.
///
/// # Description
/// This function initializes a [SimpleMatcher] instance from the provided serialized [SimpleMatchTypeWordMap] byte string.
/// It performs deserialization of the byte string, transforms it into a [SimpleMatchTypeWordMap], and then uses it to
/// create a new [SimpleMatcher]. The newly created [SimpleMatcher] instance is then wrapped in a [Box] and converted
/// into a raw pointer before being returned.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use std::ffi::CString;
///
/// use matcher_c::*;
/// use matcher_rs::{SimpleMatcher, SimpleMatchType};
///
/// let mut simple_match_type_word_map = HashMap::new();
/// let mut word_map = HashMap::new();
/// word_map.insert(1, "hello&world");
/// simple_match_type_word_map.insert(SimpleMatchType::None, word_map);
/// let simple_match_type_word_map_bytes = CString::new(rmp_serde::to_vec_named(&simple_match_type_word_map).unwrap()).unwrap();
///
/// let simple_matcher_ptr = unsafe {init_simple_matcher(simple_match_type_word_map_bytes.as_ptr())};
/// unsafe {drop_simple_matcher(simple_matcher_ptr)};
/// ```
#[no_mangle]
pub unsafe extern "C" fn init_simple_matcher(
    simple_match_type_word_map_bytes: *const c_char,
) -> *mut SimpleMatcher {
    unsafe {
        let simple_match_type_word_map: SimpleMatchTypeWordMap = match rmp_serde::from_slice(
            CStr::from_ptr(simple_match_type_word_map_bytes).to_bytes(),
        ) {
            Ok(simple_match_type_word_map) => simple_match_type_word_map,
            Err(e) => {
                panic!(
                    "Deserialize simple_match_type_word_map_bytes failed, Please check the input data.\nErr: {}", e,
                )
            }
        };

        Box::into_raw(Box::new(SimpleMatcher::new(&simple_match_type_word_map)))
    }
}

/// # Safety
/// This function is unsafe because it assumes that the provided `simple_matcher` and `text` pointers are valid.
/// The `simple_matcher` pointer should point to a valid [SimpleMatcher] instance, and the `text` pointer should
/// point to a null-terminated byte string that represents the text to be processed.
///
/// # Arguments
/// * `simple_matcher` - A raw pointer to a [SimpleMatcher] instance.
/// * `text` - A pointer to a null-terminated byte string that represents the text to be matched.
///
/// # Returns
/// * A boolean value indicating whether the text matches the pattern defined by the [SimpleMatcher] instance.
///
/// # Panics
/// This function will panic if the `simple_matcher` pointer is null.
///
/// # Description
/// This function calls the [is_match](matcher_rs::SimpleMatcher::is_match) method on a [SimpleMatcher] instance. It converts the raw pointers
/// to their respective Rust types, performs the [is_match](matcher_rs::SimpleMatcher::is_match) operation, and returns a boolean indicating the match result.
/// the match result. The conversion assumes that the `text` pointer points to a valid UTF-8 encoded,
/// null-terminated C string, and that the `simple_matcher` pointer is valid and non-null.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use std::ffi::CString;
///
/// use matcher_c::*;
/// use matcher_rs::{SimpleMatcher, SimpleMatchType};
///
/// let mut simple_match_type_word_map = HashMap::new();
/// let mut word_map = HashMap::new();
/// word_map.insert(1, "hello&world");
/// simple_match_type_word_map.insert(SimpleMatchType::None, word_map);
/// let simple_match_type_word_map_bytes = CString::new(rmp_serde::to_vec_named(&simple_match_type_word_map).unwrap()).unwrap();
///
/// let simple_matcher_ptr = unsafe {init_simple_matcher(simple_match_type_word_map_bytes.as_ptr())};
///
/// let match_text_bytes = CString::new("hello world!").unwrap();
/// let not_match_text_bytes = CString::new("test").unwrap();
///
/// assert!(unsafe {simple_matcher_is_match(simple_matcher_ptr, match_text_bytes.as_ptr())});
/// assert!(!unsafe{simple_matcher_is_match(simple_matcher_ptr, not_match_text_bytes.as_ptr())});
///
/// unsafe {drop_simple_matcher(simple_matcher_ptr)};
/// ```
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

/// # Safety
/// This function is unsafe because it assumes that the provided `simple_matcher` and `text` pointers are valid.
/// The `simple_matcher` pointer should point to a valid [SimpleMatcher] instance, and the `text` pointer should point to a null-terminated byte string.
///
/// # Arguments
/// * `simple_matcher` - A raw pointer to a [SimpleMatcher] instance.
/// * `text` - A pointer to a null-terminated byte string that represents the text to be processed.
///
/// # Returns
/// * A raw pointer to a [c_char] holding the result of the [process](matcher_rs::SimpleMatcher::process) function called on the [SimpleMatcher] instance. The result is serialized to a JSON string.
///
/// # Panics
/// This function will panic if the `simple_matcher` pointer is null.
///
/// # Description
/// This function calls the [process](matcher_rs::SimpleMatcher::process) method on a [SimpleMatcher] instance. It converts the raw pointers
/// to their respective Rust types, performs the [process](matcher_rs::SimpleMatcher::process) operation, serializes the result as a JSON string,
/// and then converts this string to a C-compatible CString. The resulting CString is then returned as a raw pointer before being returned.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use std::ffi::{CStr, CString};
/// use std::str;
///
/// use matcher_c::*;
/// use matcher_rs::{SimpleMatcher, SimpleMatchType};
///
/// let mut simple_match_type_word_map = HashMap::new();
/// let mut word_map = HashMap::new();
/// word_map.insert(1, "hello&world");
/// simple_match_type_word_map.insert(SimpleMatchType::None, word_map);
/// let simple_match_type_word_map_bytes = CString::new(rmp_serde::to_vec_named(&simple_match_type_word_map).unwrap()).unwrap();
///
/// let simple_matcher_ptr = unsafe {init_simple_matcher(simple_match_type_word_map_bytes.as_ptr())};
///
/// let match_text_bytes = CString::new("hello world!").unwrap();
/// let non_match_text_bytes = CString::new("test").unwrap();
///
/// assert_eq!(
///     unsafe {
///         str::from_utf8_unchecked(
///             CStr::from_ptr(
///                 simple_matcher_process(
///                     simple_matcher_ptr,
///                     match_text_bytes.as_ptr()
///                 )
///             ).to_bytes()
///         )
///     },
///     r#"[{"word_id":1,"word":"hello&world"}]"#
/// );
/// assert_eq!(
///     unsafe {
///         str::from_utf8_unchecked(
///             CStr::from_ptr(
///                 simple_matcher_process(
///                     simple_matcher_ptr,
///                     non_match_text_bytes.as_ptr()
///                 )
///             ).to_bytes()
///         )
///     },
///     r#"[]"#
/// );
///
/// unsafe {drop_simple_matcher(simple_matcher_ptr)};
/// ```
#[no_mangle]
pub unsafe extern "C" fn simple_matcher_process(
    simple_matcher: *mut SimpleMatcher,
    text: *const c_char,
) -> *mut c_char {
    let res = unsafe {
        CString::new(
            sonic_rs::to_string(
                &simple_matcher
                    .as_ref()
                    .unwrap()
                    .process(str::from_utf8_unchecked(CStr::from_ptr(text).to_bytes())),
            )
            .unwrap(),
        )
        .unwrap()
    };

    res.into_raw()
}

/// # Safety
/// This function is unsafe because it assumes that the provided `simple_matcher` pointer is valid and was previously allocated using [Box::into_raw].
/// It also assumes that the lifetime of the `simple_matcher` pointer is over and it is safe to drop the data.
///
/// # Arguments
/// * `simple_matcher` - A raw pointer to a [SimpleMatcher] instance that needs to be freed.
///
/// # Panics
/// This function will panic if the `simple_matcher` pointer is null.
/// It is the caller's responsibility to ensure that the pointer is valid and that no other references to the [SimpleMatcher] instance exist.
///
/// # Description
/// This function converts the raw pointer back into a [Box] and then drops it, effectively freeing the memory that the [SimpleMatcher] instance occupied.
/// After calling this function, the `simple_matcher` pointer must not be used again.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use std::ffi::CString;
///
/// use matcher_c::*;
/// use matcher_rs::{SimpleMatcher, SimpleMatchType};
///
/// let mut simple_match_type_word_map = HashMap::new();
/// let mut word_map = HashMap::new();
/// word_map.insert(1, "hello&world");
/// simple_match_type_word_map.insert(SimpleMatchType::None, word_map);
/// let simple_match_type_word_map_bytes = CString::new(rmp_serde::to_vec_named(&simple_match_type_word_map).unwrap()).unwrap();
///
/// let simple_matcher_ptr = unsafe {init_simple_matcher(simple_match_type_word_map_bytes.as_ptr())};
/// unsafe {drop_simple_matcher(simple_matcher_ptr)};
/// ```
#[no_mangle]
pub unsafe extern "C" fn drop_simple_matcher(simple_matcher: *mut SimpleMatcher) {
    unsafe { drop(Box::from_raw(simple_matcher)) }
}

/// # Safety
/// This function is unsafe because it assumes that the provided pointer is a valid and previously allocated
/// CString that needs to be freed. The function will take ownership of the pointer, which implies that no other
/// part of the code should attempt to use or free this pointer after this function is called.
///
/// # Arguments
/// * `ptr` - A raw pointer to a [c_char] that represents a CString to be freed.
///
/// # Panics
/// This function will panic if the `ptr` is null. It is the caller's responsibility to ensure that the pointer is
/// valid and that no other references to the CString exist.
///
/// # Description
/// This function takes a raw pointer to a [c_char], converts it back into a CString, and then drops it, effectively
/// freeing the memory that the CString occupied. After calling this function, the `ptr` must not be used again.
///
/// # Example
///
/// ```
/// use std::ffi::CString;
///
/// use matcher_c::*;
///
/// let c_string = CString::new("hello world!").unwrap();
/// let c_string_ptr = c_string.into_raw();
///
/// unsafe {drop_string(c_string_ptr)};
/// ```
#[no_mangle]
pub unsafe extern "C" fn drop_string(ptr: *mut c_char) {
    unsafe { drop(CString::from_raw(ptr)) }
}
