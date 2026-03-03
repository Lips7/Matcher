use std::ptr;
use vectorscan_rs_sys as hs;

/// Trait defining the core interface for any Vectorscan database implementation.
pub trait VectorscanDatabase: Send + Sync + std::fmt::Debug {
    /// Returns the raw pointer to the compiled Vectorscan database.
    fn as_ptr(&self) -> *mut hs::hs_database_t;
}

/// A database implementation specifically optimized for multiple literal patterns.
#[derive(Debug)]
pub struct LiteralDatabase {
    db: *mut hs::hs_database_t,
}

unsafe impl Send for LiteralDatabase {}
unsafe impl Sync for LiteralDatabase {}

impl LiteralDatabase {
    pub fn new(patterns: &[&str]) -> Self {
        let patterns_ptr: Vec<*const i8> =
            patterns.iter().map(|s| s.as_ptr() as *const i8).collect();
        let patterns_len: Vec<usize> = patterns.iter().map(|s| s.len()).collect();
        let ids: Vec<u32> = (0..patterns.len() as u32).collect();
        let flags: Vec<u32> = vec![0; patterns.len()];

        let mut db: *mut hs::hs_database_t = ptr::null_mut();
        let mut compile_error: *mut hs::hs_compile_error_t = ptr::null_mut();

        unsafe {
            let status = hs::hs_compile_lit_multi(
                patterns_ptr.as_ptr(),
                flags.as_ptr(),
                ids.as_ptr(),
                patterns_len.as_ptr(),
                patterns.len() as u32,
                hs::HS_MODE_BLOCK,
                ptr::null_mut(),
                &mut db,
                &mut compile_error,
            );
            if status != hs::HS_SUCCESS as i32 {
                if !compile_error.is_null() {
                    hs::hs_free_compile_error(compile_error);
                }
                panic!("Failed to compile vectorscan literal database: {}", status);
            }
        }

        LiteralDatabase { db }
    }
}

impl VectorscanDatabase for LiteralDatabase {
    fn as_ptr(&self) -> *mut hs::hs_database_t {
        self.db
    }
}

impl Drop for LiteralDatabase {
    fn drop(&mut self) {
        unsafe {
            hs::hs_free_database(self.db);
        }
    }
}
