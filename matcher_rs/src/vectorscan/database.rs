use std::ffi::CString;
use std::ptr;

use vectorscan_rs_sys as hs;

use crate::vectorscan::error::{AsResult, Error, extract_compile_error};

/// Trait defining the core interface for any Vectorscan database implementation.
///
/// This trait ensures that any database type can provide a raw pointer to its
/// underlying Vectorscan database for use in scanning operations.
pub trait VectorscanDatabase: Send + Sync + std::fmt::Debug {
    /// Returns the raw pointer to the compiled Vectorscan database.
    ///
    /// # Returns
    /// A raw pointer to the underlying [`hs::hs_database_t`].
    fn as_ptr(&self) -> *mut hs::hs_database_t;
}

// ---------------------------------------------------------------------------
// LiteralDatabase — hs_compile_lit_multi
// ---------------------------------------------------------------------------

/// A database compiled from multiple literal (non-regex) patterns.
///
/// Uses `hs_compile_lit_multi` which treats every byte literally,
/// including NUL bytes (lengths are provided explicitly).
#[derive(Debug)]
pub struct LiteralDatabase {
    db: *mut hs::hs_database_t,
}

unsafe impl Send for LiteralDatabase {}
unsafe impl Sync for LiteralDatabase {}

impl LiteralDatabase {
    /// Compiles a literal database from the given patterns and per-pattern flags.
    ///
    /// This function takes a slice of literal strings and corresponding flags,
    /// and compiles them into a Vectorscan database optimized for literal matching.
    ///
    /// # Arguments
    /// * `patterns` - Literal byte patterns (no regex interpretation).
    /// * `flags` - Per-pattern flags (e.g. `HS_FLAG_CASELESS`, `HS_FLAG_SINGLEMATCH`).
    ///   Must have the same length as `patterns`.
    ///
    /// # Returns
    /// A [`Result<Self, Error>`] containing the compiled literal database.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::vectorscan::database::LiteralDatabase;
    ///
    /// let patterns = vec!["apple", "banana"];
    /// let flags = vec![0, 0]; // No special flags
    /// let db = LiteralDatabase::new(&patterns, &flags).unwrap();
    /// ```
    pub fn new(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        debug_assert_eq!(patterns.len(), flags.len());

        let patterns_ptr: Vec<*const i8> =
            patterns.iter().map(|s| s.as_ptr() as *const i8).collect();
        let patterns_len: Vec<usize> = patterns.iter().map(|s| s.len()).collect();
        let ids: Vec<u32> = (0..patterns.len() as u32).collect();

        let mut db: *mut hs::hs_database_t = ptr::null_mut();
        let mut compile_error: *mut hs::hs_compile_error_t = ptr::null_mut();

        unsafe {
            let status = hs::hs_compile_lit_multi(
                patterns_ptr.as_ptr(),
                flags.as_ptr(),
                ids.as_ptr(),
                patterns_len.as_ptr(),
                patterns.len() as u32,
                hs::HS_MODE_BLOCK,
                ptr::null_mut(),
                &mut db,
                &mut compile_error,
            );

            if status != hs::HS_SUCCESS as i32 {
                return if !compile_error.is_null() {
                    Err(extract_compile_error(compile_error))
                } else {
                    status.ok().map(|_| unreachable!())
                };
            }
        }

        Ok(LiteralDatabase { db })
    }
}

impl VectorscanDatabase for LiteralDatabase {
    /// Returns the raw pointer to the compiled Vectorscan database.
    ///
    /// # Returns
    /// A raw pointer to the underlying [`hs::hs_database_t`].
    fn as_ptr(&self) -> *mut hs::hs_database_t {
        self.db
    }
}

impl Drop for LiteralDatabase {
    fn drop(&mut self) {
        unsafe {
            hs::hs_free_database(self.db);
        }
    }
}

// ---------------------------------------------------------------------------
// RegexDatabase — hs_compile_multi
// ---------------------------------------------------------------------------

/// A database compiled from multiple regular-expression patterns.
///
/// Uses `hs_compile_multi`. Patterns are standard Vectorscan/Hyperscan
/// regex expressions (PCRE-like subset). Each pattern must be a valid
/// UTF-8 string (null-terminated internally via `CString`).
#[derive(Debug)]
pub struct RegexDatabase {
    db: *mut hs::hs_database_t,
}

unsafe impl Send for RegexDatabase {}
unsafe impl Sync for RegexDatabase {}

impl RegexDatabase {
    /// Compiles a regex database from the given patterns and per-pattern flags.
    ///
    /// This function takes a slice of regex strings and corresponding flags,
    /// and compiles them into a Vectorscan database optimized for regex matching.
    ///
    /// # Arguments
    /// * `patterns` - Regex expressions (PCRE-like subset understood by Vectorscan).
    /// * `flags` - Per-pattern flags (e.g. `HS_FLAG_CASELESS | HS_FLAG_UTF8`).
    ///   Must have the same length as `patterns`.
    ///
    /// # Returns
    /// A [`Result<Self, Error>`] containing the compiled regex database.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::vectorscan::database::RegexDatabase;
    ///
    /// let patterns = vec!["apple.*", "banana.+"];
    /// let flags = vec![0, 0]; // No special flags
    /// let db = RegexDatabase::new(&patterns, &flags).unwrap();
    /// ```
    pub fn new(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        debug_assert_eq!(patterns.len(), flags.len());

        // hs_compile_multi requires null-terminated C strings.
        let c_patterns: Vec<CString> = patterns
            .iter()
            .map(|s| CString::new(*s).expect("pattern must not contain NUL bytes"))
            .collect();
        let c_pattern_ptrs: Vec<*const i8> = c_patterns.iter().map(|cs| cs.as_ptr()).collect();
        let ids: Vec<u32> = (0..patterns.len() as u32).collect();

        let mut db: *mut hs::hs_database_t = ptr::null_mut();
        let mut compile_error: *mut hs::hs_compile_error_t = ptr::null_mut();

        unsafe {
            let status = hs::hs_compile_multi(
                c_pattern_ptrs.as_ptr(),
                flags.as_ptr(),
                ids.as_ptr(),
                patterns.len() as u32,
                hs::HS_MODE_BLOCK,
                ptr::null_mut(),
                &mut db,
                &mut compile_error,
            );

            if status != hs::HS_SUCCESS as i32 {
                return if !compile_error.is_null() {
                    Err(extract_compile_error(compile_error))
                } else {
                    status.ok().map(|_| unreachable!())
                };
            }
        }

        Ok(RegexDatabase { db })
    }
}

impl VectorscanDatabase for RegexDatabase {
    /// Returns the raw pointer to the compiled Vectorscan database.
    ///
    /// # Returns
    /// A raw pointer to the underlying [`hs::hs_database_t`].
    fn as_ptr(&self) -> *mut hs::hs_database_t {
        self.db
    }
}

impl Drop for RegexDatabase {
    fn drop(&mut self) {
        unsafe {
            hs::hs_free_database(self.db);
        }
    }
}
