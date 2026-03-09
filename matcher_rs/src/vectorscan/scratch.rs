use std::ptr;

use vectorscan_rs_sys as hs;

use crate::vectorscan::error::{AsResult, Error};

/// Safe wrapper for Vectorscan's internal scratch space.
///
/// Scratch space is used by Vectorscan to store temporary state during a scan.
/// Because scratch space cannot be shared concurrently across multiple scans,
/// it must be instantiated per-thread or per-scan.
///
/// **Caching Mechanism**:
/// This struct stores `last_db_ptr`, the database pointer it was last sized for.
/// This acts purely as an ultra-fast cache key. In scenarios where multiple `SimpleMatcher`
/// instances run on the same thread (sharing a `thread_local!` Scratch space), this check
/// ensures that when the matcher switches to a new database, the scratch space is safely
/// resized. If the database hasn't changed, the `update()` call becomes a zero-cost operation,
/// avoiding an expensive FFI boundary crossing.
#[derive(Debug)]
pub struct Scratch {
    scratch: *mut hs::hs_scratch_t,
    /// An opaque cache key to determine if a reallocation FFI call is necessary.
    last_db_ptr: *mut hs::hs_database_t,
}

unsafe impl Send for Scratch {}
// SAFETY: Scratch space must not be mutated concurrently. However, it can be shared across
// threads as long as exclusive access (`&mut`) is guaranteed during scanning (e.g., via thread_local!).
unsafe impl Sync for Scratch {}

impl Scratch {
    /// Allocates a new scratch space sized for the given database.
    ///
    /// # Safety
    /// The caller must ensure that `db` is a valid pointer to a compiled Vectorscan database.
    pub unsafe fn new(db: *mut hs::hs_database_t) -> Result<Self, Error> {
        let mut scratch: *mut hs::hs_scratch_t = ptr::null_mut();
        unsafe {
            hs::hs_alloc_scratch(db, &mut scratch).ok()?;
        }
        Ok(Scratch {
            scratch,
            last_db_ptr: db,
        })
    }

    /// Returns the raw scratch pointer required by Vectorscan FFI calls such as `hs_scan`.
    #[inline(always)]
    pub fn as_ptr(&self) -> *mut hs::hs_scratch_t {
        self.scratch
    }

    /// Updates the scratch space to ensure it is correctly sized for the given database.
    ///
    /// If the provided database matches the `last_db_ptr` cache key, this function returns
    /// instantly without crossing the FFI boundary, resulting in a zero-cost operation.
    ///
    /// # Safety
    /// The caller must ensure that `db` is a valid pointer to a compiled Vectorscan database.
    #[inline(always)]
    pub unsafe fn update(&mut self, db: *mut hs::hs_database_t) -> Result<(), Error> {
        // Fast-path: The scratch space is already sized for this exact database.
        if self.last_db_ptr == db {
            return Ok(());
        }

        // Slow-path: We switched to a new database (e.g., another matcher on the same thread).
        // Reallocate/resize the scratch space for the new database constraints.
        unsafe {
            hs::hs_alloc_scratch(db, &mut self.scratch).ok()?;
        }
        self.last_db_ptr = db;
        Ok(())
    }

    /// Creates an independent copy of this scratch space via `hs_clone_scratch`.
    ///
    /// Useful when a `Scratch` must be used on multiple threads concurrently: clone it
    /// once per thread rather than sharing a single instance.
    ///
    /// # Errors
    /// Returns [`Error::Vectorscan`] if `hs_clone_scratch` fails (e.g. `HS_NOMEM`).
    pub fn try_clone(&self) -> Result<Self, Error> {
        let mut scratch: *mut hs::hs_scratch_t = ptr::null_mut();
        unsafe {
            hs::hs_clone_scratch(self.scratch, &mut scratch).ok()?;
        }
        Ok(Scratch {
            scratch,
            last_db_ptr: self.last_db_ptr,
        })
    }
}

impl Clone for Scratch {
    fn clone(&self) -> Self {
        self.try_clone()
            .expect("failed to clone vectorscan scratch")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        unsafe {
            hs::hs_free_scratch(self.scratch);
        }
    }
}
