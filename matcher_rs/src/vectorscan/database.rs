use std::ffi::c_char;
use std::ptr;

use vectorscan_rs_sys as hs;

use crate::vectorscan::error::{AsResult, Error, extract_compile_error};

/// Safe wrapper for a compiled Vectorscan pattern database.
///
/// A `Database` holds the immutable automaton produced by one of Vectorscan's compiler
/// functions. It carries no per-scan state; all temporary state lives in a [`Scratch`]
/// space allocated separately for each thread.
///
/// # Thread Safety
///
/// `Send + Sync`: the compiled automaton is strictly read-only and can be shared across
/// threads without synchronization (typically via `Arc<Database>`).
///
/// # Memory Management
///
/// The internal buffer is allocated by Vectorscan's compiler and freed via
/// `hs_free_database` when this struct is dropped.
///
/// [`Scratch`]: crate::vectorscan::scratch::Scratch
#[derive(Debug)]
pub struct Database {
    db: *mut hs::hs_database_t,
}

// SAFETY: A compiled Vectorscan database is strictly immutable and inherently thread-safe.
// It can safely be sent across threads or accessed concurrently by multiple threads.
unsafe impl Send for Database {}
unsafe impl Sync for Database {}

impl Database {
    /// Compiles a literal pattern database via `hs_compile_lit_multi`.
    ///
    /// Patterns are treated as exact byte literals — no regex syntax. Each pattern is
    /// compiled in `HS_MODE_BLOCK` (stateless, non-streaming) and assigned a zero-based
    /// integer ID equal to its index in `patterns`.
    ///
    /// # Arguments
    /// * `patterns` — literal byte strings to compile.
    /// * `flags` — per-pattern Hyperscan flags (e.g. `HS_FLAG_CASELESS`). Must be the
    ///   same length as `patterns`.
    ///
    /// # Panics
    /// In debug builds, panics if `patterns.len() != flags.len()`.
    ///
    /// # Errors
    /// Returns [`Error::VectorscanCompile`] if any pattern fails to compile (includes the
    /// diagnostic message and the zero-based pattern index). Returns [`Error::Vectorscan`]
    /// on unexpected API failures.
    pub fn new_literal(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        debug_assert_eq!(patterns.len(), flags.len());

        let patterns_ptr: Vec<*const c_char> = patterns
            .iter()
            .map(|s| s.as_ptr() as *const c_char)
            .collect();
        let pattern_lengths: Vec<usize> = patterns.iter().map(|s| s.len()).collect();
        let ids: Vec<u32> = (0..patterns.len() as u32).collect();

        let mut db: *mut hs::hs_database_t = ptr::null_mut();
        let mut compile_error: *mut hs::hs_compile_error_t = ptr::null_mut();

        unsafe {
            let status = hs::hs_compile_lit_multi(
                patterns_ptr.as_ptr(),
                flags.as_ptr(),
                ids.as_ptr(),
                pattern_lengths.as_ptr(),
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
    /// Required when calling Vectorscan FFI functions directly (e.g. `hs_scan`,
    /// `hs_alloc_scratch`). The pointer is valid for the lifetime of this `Database`.
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
