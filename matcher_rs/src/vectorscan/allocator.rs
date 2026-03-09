use std::ffi::c_void;
use std::sync::Once;

use vectorscan_rs_sys as hs;

unsafe extern "C" {
    pub fn mi_malloc(size: usize) -> *mut c_void;
    pub unsafe fn mi_free(ptr: *mut c_void);
}

static INIT_ALLOCATOR: Once = Once::new();

/// Configures Vectorscan to use mimalloc for all internal memory allocations.
///
/// This function sets the internal memory allocator for Vectorscan to use `mimalloc`,
/// which is used throughout this library. It is safe to call multiple times, as
/// the initialization is protected by a `Once` synchronization primitive.
///
/// This is called automatically by [`VectorscanScanner::new`].
pub fn init_allocator() {
    INIT_ALLOCATOR.call_once(|| unsafe {
        let status = hs::hs_set_allocator(Some(mi_malloc), Some(mi_free));
        assert_eq!(
            status,
            hs::HS_SUCCESS as hs::hs_error_t,
            "failed to set vectorscan allocator to mimalloc"
        );
    });
}
