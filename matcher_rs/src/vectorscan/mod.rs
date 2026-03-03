pub mod database;
pub mod error;
pub mod scanner;
pub mod scratch;

pub use crate::vectorscan::scanner::VectorscanScanner;

use std::ffi::c_void;
use std::sync::Once;

use vectorscan_rs_sys as hs;

unsafe extern "C" {
    fn mi_malloc(size: usize) -> *mut c_void;
    fn mi_free(ptr: *mut c_void);
}

static INIT_ALLOCATOR: Once = Once::new();

/// Configures Vectorscan to use mimalloc for all internal memory allocations.
///
/// Safe to call multiple times — only the first invocation takes effect.
/// Called automatically by [`VectorscanScanner::new`].
pub(crate) fn init_allocator() {
    INIT_ALLOCATOR.call_once(|| unsafe {
        let status = hs::hs_set_allocator(Some(mi_malloc), Some(mi_free));
        assert_eq!(
            status,
            hs::HS_SUCCESS as hs::hs_error_t,
            "failed to set vectorscan allocator to mimalloc"
        );
    });
}
