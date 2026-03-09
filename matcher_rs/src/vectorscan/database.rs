use std::ffi::c_char;
use std::ptr;

use vectorscan_rs_sys as hs;

use crate::vectorscan::error::{AsResult, Error, extract_compile_error};

/// Safe wrapper for a compiled Vectorscan database.
///
/// A Vectorscan database is the compiled representation of one or more regular expressions
/// or literal patterns. It represents the *immutable, thread-safe automaton* needed for matching.
///
/// **Thread Safety & Lifecycle**:
/// The database is fully thread-safe and is designed to be shared concurrently across
/// multiple threads (typically wrapped in an `Arc`). It does not store matching state;
/// temporary state during a scan is stored in a separate `Scratch` space.
///
/// **Memory Management**:
/// The internal memory is allocated by Vectorscan's compiler and must be explicitly freed
/// via `hs_free_database`. This struct ensures that the database is safely freed when
/// it goes out of scope.
#[derive(Debug)]
pub struct Database {
    db: *mut hs::hs_database_t,
}

// SAFETY: A compiled Vectorscan database is strictly immutable and inherently thread-safe.
// It can safely be sent across threads or accessed concurrently by multiple threads.
unsafe impl Send for Database {}
unsafe impl Sync for Database {}

impl Database {
    /// Compiles a literal database from the given patterns and per-pattern flags.
    ///
    /// This function takes a slice of literal strings and corresponding flags,
    /// and compiles them into a Vectorscan database optimized for literal matching
    /// (using `hs_compile_lit_multi`).
    ///
    /// # Arguments
    /// * `patterns` - Literal byte patterns to compile.
    /// * `flags` - Per-pattern Hyperscan flags (e.g., `HS_FLAG_CASELESS`, `HS_FLAG_SINGLEMATCH`).
    ///   Must have the exact same length as `patterns`.
    ///
    /// # Returns
    /// A [`Result<Self, Error>`] containing the compiled literal database.
    pub fn new_literal(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        debug_assert_eq!(patterns.len(), flags.len());

        let patterns_ptr: Vec<*const c_char> = patterns
            .iter()
            .map(|s| s.as_ptr() as *const c_char)
            .collect();
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

        Ok(Database { db })
    }

    /// Returns the raw pointer to the compiled Vectorscan database.
    ///
    /// This pointer is required for executing scan operations and for allocating
    /// or sizing compatible `Scratch` spaces.
    #[inline(always)]
    pub fn as_ptr(&self) -> *mut hs::hs_database_t {
        self.db
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        unsafe {
            hs::hs_free_database(self.db);
        }
    }
}
