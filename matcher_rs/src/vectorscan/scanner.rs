use std::ffi::{c_char, c_void};
use std::sync::Arc;

use vectorscan_rs_sys as hs;

#[cfg(target_os = "macos")]
use crate::vectorscan::allocator;
use crate::vectorscan::database::Database;
use crate::vectorscan::error::{AsResult, Error};
use crate::vectorscan::scratch::Scratch;

/// High-level wrapper for performing scans with Vectorscan.
///
/// `VectorscanScanner` binds a compiled, thread-safe `Database` (wrapped in an `Arc`)
/// to scanning logic. It acts as the primary entry point for executing matches against
/// an input haystack.
#[derive(Debug, Clone)]
pub struct VectorscanScanner {
    db: Arc<Database>,
}

impl VectorscanScanner {
    /// Creates a scanner wrapping an existing compiled `Database`.
    ///
    /// `db` is wrapped in an `Arc` so the scanner can be cheaply cloned and shared
    /// across threads without copying the compiled automaton. On macOS, also initializes
    /// the mimalloc allocator for Vectorscan the first time this is called.
    ///
    /// # Errors
    /// Returns [`Error`] if the macOS allocator initialization fails (rare; typically
    /// indicates a Vectorscan API incompatibility).
    pub fn new(db: Arc<Database>) -> Result<Self, Error> {
        #[cfg(target_os = "macos")]
        allocator::init_allocator();
        Ok(Self { db })
    }

    /// Compiles `patterns` into a new literal [`Database`] and returns a ready scanner.
    ///
    /// Shorthand for `Database::new_literal(patterns, flags)` followed by `Self::new(Arc::new(db))`.
    ///
    /// # Arguments
    /// * `patterns` — literal byte patterns to match; no regex syntax.
    /// * `flags` — per-pattern Hyperscan flags (e.g. `HS_FLAG_CASELESS`). Must be the same
    ///   length as `patterns`.
    ///
    /// # Errors
    /// Returns [`Error::VectorscanCompile`] if any pattern fails to compile, or
    /// [`Error::Vectorscan`] on unexpected API failures.
    pub fn new_literal(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        let db = Arc::new(Database::new_literal(patterns, flags)?);
        Self::new(db)
    }

    /// Returns the raw pointer to the compiled Vectorscan database.
    ///
    /// Used to pass to [`Scratch::new`](crate::vectorscan::scratch::Scratch::new) and
    /// [`Scratch::update`](crate::vectorscan::scratch::Scratch::update) for scratch
    /// allocation/validation.
    #[inline(always)]
    pub fn as_db_ptr(&self) -> *mut hs::hs_database_t {
        self.db.as_ptr()
    }

    /// Scans `haystack` and invokes `on_match` for every matching pattern.
    ///
    /// Allocates a fresh [`Scratch`] space on every call. For hot-paths use
    /// [`scan_with_scratch`](Self::scan_with_scratch) with a pre-allocated and reused
    /// scratch space instead.
    ///
    /// # Errors
    /// Returns [`Error::Vectorscan`] if the scan itself fails (e.g. `HS_NOMEM`).
    pub fn scan<F>(&self, haystack: &[u8], on_match: F) -> Result<(), Error>
    where
        F: FnMut(usize) -> bool,
    {
        let mut scratch = unsafe { Scratch::new(self.db.as_ptr())? };
        self.scan_with_scratch(haystack, &mut scratch, on_match)
    }

    /// Scans `haystack` using the provided `scratch` space, invoking `on_match` for every hit.
    ///
    /// `scratch` must have been allocated or updated for this scanner's database (via
    /// [`Scratch::new`](crate::vectorscan::scratch::Scratch::new) or
    /// [`Scratch::update`](crate::vectorscan::scratch::Scratch::update)).
    ///
    /// Returning `false` from `on_match` terminates the scan early (useful for
    /// [`is_match`](crate::SimpleMatcher::is_match)-style short-circuiting).
    ///
    /// # Errors
    /// Returns [`Error::Vectorscan`] on unexpected API failures. Early termination via a
    /// `false` return from `on_match` is **not** reported as an error.
    pub fn scan_with_scratch<F>(
        &self,
        haystack: &[u8],
        scratch: &mut Scratch,
        mut on_match: F,
    ) -> Result<(), Error>
    where
        F: FnMut(usize) -> bool,
    {
        let ctx = &mut on_match as *mut F as *mut c_void;

        unsafe {
            let status = hs::hs_scan(
                self.db.as_ptr(),
                haystack.as_ptr() as *const c_char,
                haystack.len() as u32,
                0,
                scratch.as_ptr(),
                Some(match_callback::<F>),
                ctx,
            );

            match status {
                // HS_SCAN_TERMINATED means our closure returned true (abort early), which is expected.
                s if s == hs::HS_SUCCESS as i32 || s == hs::HS_SCAN_TERMINATED => Ok(()),
                _ => status.ok(),
            }
        }
    }
}

/// FFI callback invoked by Vectorscan for each match during a scan.
///
/// Bridges the Vectorscan C ABI to the Rust closure stored in `ctx`. The closure receives
/// the zero-based pattern index (`id`) and returns `true` to continue scanning or `false`
/// to abort early. Vectorscan uses the opposite convention: returning `0` (success)
/// continues the scan and returning non-zero terminates it, hence the inversion below.
extern "C" fn match_callback<F>(id: u32, _from: u64, _to: u64, _flags: u32, ctx: *mut c_void) -> i32
where
    F: FnMut(usize) -> bool,
{
    let on_match = unsafe { &mut *(ctx as *mut F) };
    // Closure returns true → continue (0 = HS_SUCCESS); false → stop (1 = HS_SCAN_TERMINATED).
    if on_match(id as usize) { 0 } else { 1 }
}
