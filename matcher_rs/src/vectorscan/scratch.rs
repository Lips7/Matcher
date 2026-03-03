use std::ptr;
use vectorscan_rs_sys as hs;

/// Safe wrapper for Vectorscan's internal scratch space.
#[derive(Debug)]
pub struct Scratch {
    pub(crate) scratch: *mut hs::hs_scratch_t,
}

unsafe impl Send for Scratch {}

impl Scratch {
    /// Allocates new scratch space from a raw database pointer.
    pub fn new_with_ptr(db: *mut hs::hs_database_t) -> Self {
        let mut scratch: *mut hs::hs_scratch_t = ptr::null_mut();
        unsafe {
            let status = hs::hs_alloc_scratch(db, &mut scratch);
            if status != hs::HS_SUCCESS as i32 {
                panic!("Failed to allocate vectorscan scratch space: {}", status);
            }
        }
        Scratch { scratch }
    }

    /// Returns the raw scratch pointer.
    pub fn as_ptr(&self) -> *mut hs::hs_scratch_t {
        self.scratch
    }
}

impl Clone for Scratch {
    fn clone(&self) -> Self {
        let mut scratch: *mut hs::hs_scratch_t = ptr::null_mut();
        unsafe {
            let status = hs::hs_clone_scratch(self.scratch, &mut scratch);
            if status != hs::HS_SUCCESS as i32 {
                panic!("Failed to clone vectorscan scratch space: {}", status);
            }
        }
        Scratch { scratch }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        unsafe {
            hs::hs_free_scratch(self.scratch);
        }
    }
}
