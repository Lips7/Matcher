use std::ffi::c_void;
use std::sync::Arc;

use vectorscan_rs_sys as hs;

use crate::vectorscan::database::{LiteralDatabase, RegexDatabase, VectorscanDatabase};
use crate::vectorscan::error::{AsResult, Error};
use crate::vectorscan::init_allocator;
use crate::vectorscan::scratch::Scratch;

/// High-level Vectorscan scanner.
///
/// Owns a compiled database and a template scratch space.
/// Each [`scan`] call clones the scratch internally, so the
/// scanner is safe to share across threads without a mutex.
#[derive(Debug, Clone)]
pub struct VectorscanScanner {
    db: Arc<dyn VectorscanDatabase>,
    scratch: Scratch,
}

impl VectorscanScanner {
    /// Creates a scanner from a pre-built database.
    pub fn new(db: Arc<dyn VectorscanDatabase>) -> Result<Self, Error> {
        init_allocator();
        let scratch = unsafe { Scratch::new(db.as_ptr())? };
        Ok(Self { db, scratch })
    }

    /// Convenience: compiles a literal database and returns a ready scanner.
    pub fn new_literal(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        let db = Arc::new(LiteralDatabase::new(patterns, flags)?);
        Self::new(db)
    }

    /// Convenience: compiles a regex database and returns a ready scanner.
    pub fn new_regex(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        let db = Arc::new(RegexDatabase::new(patterns, flags)?);
        Self::new(db)
    }

    /// Scans `haystack` and calls `on_match(pattern_id)` for every match.
    ///
    /// A fresh scratch clone is used per call — no locking required.
    /// The callback receives the pattern id (as assigned at compile time).
    ///
    /// # Errors
    /// Returns an error if scratch cloning or scanning fails.
    pub fn scan<F>(&self, haystack: &[u8], mut on_match: F) -> Result<(), Error>
    where
        F: FnMut(usize),
    {
        let scratch = self.scratch.try_clone()?;

        // We pass a raw pointer to `on_match` through the FFI context parameter.
        let ctx = &mut on_match as *mut F as *mut c_void;

        unsafe {
            let status = hs::hs_scan(
                self.db.as_ptr(),
                haystack.as_ptr() as *const i8,
                haystack.len() as u32,
                0,
                scratch.as_ptr(),
                Some(match_callback::<F>),
                ctx,
            );

            // HS_SCAN_TERMINATED is returned when the callback returns non-zero,
            // which we never do, so treat it as an error here.
            status.ok()
        }
    }
}

/// FFI callback invoked by `hs_scan` for each match.
///
/// Casts the context pointer back to the user-provided closure
/// and calls it with the pattern id.
extern "C" fn match_callback<F>(id: u32, _from: u64, _to: u64, _flags: u32, ctx: *mut c_void) -> i32
where
    F: FnMut(usize),
{
    let on_match = unsafe { &mut *(ctx as *mut F) };
    on_match(id as usize);
    0
}
