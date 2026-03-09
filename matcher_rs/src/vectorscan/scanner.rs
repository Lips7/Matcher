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
    /// Creates a new scanner using an existing, pre-built `Database`.
    ///
    /// The database is expected to be wrapped in an `Arc` to allow cheap cloning
    /// of the scanner across multiple threads.
    pub fn new(db: Arc<Database>) -> Result<Self, Error> {
        #[cfg(target_os = "macos")]
        allocator::init_allocator();
        Ok(Self { db })
    }

    /// Convenience method: compiles a new literal `Database` and returns a ready scanner.
    pub fn new_literal(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        let db = Arc::new(Database::new_literal(patterns, flags)?);
        Self::new(db)
    }

    /// Returns the raw pointer to the underlying Vectorscan database.
    #[inline(always)]
    pub fn as_db_ptr(&self) -> *mut hs::hs_database_t {
        self.db.as_ptr()
    }

    /// Scans the given haystack and invokes the callback for every match.
    ///
    /// **Performance Note**: This convenience method allocates a fresh `Scratch` space
    /// on every call. For hot-paths (like in `SimpleMatcher`), it is highly recommended
    /// to manage and reuse `Scratch` spaces externally and call `scan_with_scratch`.
    pub fn scan<F>(&self, haystack: &[u8], on_match: F) -> Result<(), Error>
    where
        F: FnMut(usize) -> bool,
    {
        let mut scratch = unsafe { Scratch::new(self.db.as_ptr())? };
        self.scan_with_scratch(haystack, &mut scratch, on_match)
    }

    /// Scans the given haystack using a provided, pre-allocated `Scratch` space.
    ///
    /// The `Scratch` space should be properly sized for this scanner's database prior
    /// to calling this method. It invokes the `on_match` closure for every matching
    /// pattern found. Returning `true` from the closure will immediately terminate the scan.
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

/// FFI callback invoked by Vectorscan's internal scan loop for each match found.
extern "C" fn match_callback<F>(id: u32, _from: u64, _to: u64, _flags: u32, ctx: *mut c_void) -> i32
where
    F: FnMut(usize) -> bool,
{
    let on_match = unsafe { &mut *(ctx as *mut F) };
    // If closure returns false, it signals to stop the scan early.
    // We return 1 (non-zero) to terminate the hyperscan loop, and 0 (HS_SUCCESS) to continue.
    if on_match(id as usize) { 0 } else { 1 }
}
