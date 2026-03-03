use std::ptr;

use vectorscan_rs_sys as hs;

use crate::vectorscan::error::{AsResult, Error};

/// Safe wrapper for Vectorscan's internal scratch space.
///
/// Scratch space is required by `hs_scan` and must not be shared
/// across concurrent scan calls. Clone the scratch to obtain an
/// independent copy for each thread or scan invocation.
#[derive(Debug)]
pub struct Scratch {
    scratch: *mut hs::hs_scratch_t,
}

unsafe impl Send for Scratch {}
// SAFETY: The template scratch stored in VectorscanScanner is never mutated
// concurrently — each scan() call clones its own independent copy.
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
        Ok(Scratch { scratch })
    }

    /// Returns the raw scratch pointer for FFI calls.
    pub fn as_ptr(&self) -> *mut hs::hs_scratch_t {
        self.scratch
    }

    /// Fallible clone — creates an independent copy of this scratch space.
    pub fn try_clone(&self) -> Result<Self, Error> {
        let mut scratch: *mut hs::hs_scratch_t = ptr::null_mut();
        unsafe {
            hs::hs_clone_scratch(self.scratch, &mut scratch).ok()?;
        }
        Ok(Scratch { scratch })
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
