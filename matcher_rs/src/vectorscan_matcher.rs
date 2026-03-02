use std::ffi::c_void;
use std::ptr;

use parking_lot::Mutex;
use vectorscan_rs_sys as hs;

#[derive(Debug)]
pub struct VectorscanMatcher {
    db: *mut hs::hs_database_t,
    scratch: Mutex<*mut hs::hs_scratch_t>,
}

unsafe impl Send for VectorscanMatcher {}
unsafe impl Sync for VectorscanMatcher {}

impl VectorscanMatcher {
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

        let mut scratch: *mut hs::hs_scratch_t = ptr::null_mut();
        unsafe {
            let status = hs::hs_alloc_scratch(db, &mut scratch);
            if status != hs::HS_SUCCESS as i32 {
                panic!("Failed to allocate vectorscan scratch space: {}", status);
            }
        }

        VectorscanMatcher {
            db,
            scratch: Mutex::new(scratch),
        }
    }

    pub fn find_overlapping_iter(&self, text: &str) -> Vec<usize> {
        let mut results = Vec::new();
        let scratch = self.scratch.lock();

        extern "C" fn on_match(
            id: u32,
            _from: u64,
            _to: u64,
            _flags: u32,
            context: *mut c_void,
        ) -> i32 {
            let results = unsafe { &mut *(context as *mut Vec<usize>) };
            results.push(id as usize);
            0
        }

        unsafe {
            hs::hs_scan(
                self.db,
                text.as_ptr() as *const i8,
                text.len() as u32,
                0,
                *scratch,
                Some(on_match),
                &mut results as *mut Vec<_> as *mut c_void,
            );
        }

        results
    }
}

impl Drop for VectorscanMatcher {
    fn drop(&mut self) {
        unsafe {
            hs::hs_free_scratch(*self.scratch.lock());
            hs::hs_free_database(self.db);
        }
    }
}

impl Clone for VectorscanMatcher {
    fn clone(&self) -> Self {
        let mut scratch: *mut hs::hs_scratch_t = ptr::null_mut();
        unsafe {
            let status = hs::hs_alloc_scratch(self.db, &mut scratch);
            if status != hs::HS_SUCCESS as i32 {
                panic!("Failed to allocate vectorscan scratch space: {}", status);
            }
        }
        VectorscanMatcher {
            db: self.db,
            scratch: Mutex::new(scratch),
        }
    }
}
