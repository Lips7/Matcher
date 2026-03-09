use std::ffi::c_void;
use std::sync::Once;

use vectorscan_rs_sys as hs;

unsafe extern "C" {
    pub fn mi_malloc(size: usize) -> *mut c_void;
    pub unsafe fn mi_free(ptr: *mut c_void);
}

static INIT_ALLOCATOR: Once = Once::new();

/// Registers mimalloc as Vectorscan's internal allocator (macOS only).
///
/// Must be called before any Vectorscan `hs_scan_*` or `hs_alloc_scratch` calls so that
/// Vectorscan's internal allocations use the same allocator as the rest of the library.
/// Protected by a [`Once`] guard; safe to call multiple times — only the first call has
/// any effect.
///
/// Called automatically by [`crate::vectorscan::scanner::VectorscanScanner::new`].
///
/// # Panics
/// Panics if `hs_set_allocator` returns a non-success status, which would indicate a
/// Vectorscan API incompatibility.
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
