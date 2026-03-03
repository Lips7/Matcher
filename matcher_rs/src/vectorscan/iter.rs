use std::collections::VecDeque;
use std::ffi::c_void;
use vectorscan_rs_sys as hs;
use crate::vectorscan::database::VectorscanDatabase;
use crate::vectorscan::scratch::Scratch;

/// A simple, deferred iterator that yields pattern IDs.
pub struct VMatches<'a, 'h> {
    db: &'a dyn VectorscanDatabase,
    scratch: &'a mut Scratch,
    haystack: &'h [u8],
    buffer: VecDeque<usize>,
    scanned: bool,
}

impl<'a, 'h> VMatches<'a, 'h> {
    pub fn new(db: &'a dyn VectorscanDatabase, scratch: &'a mut Scratch, haystack: &'h [u8]) -> Self {
        Self {
            db,
            scratch,
            haystack,
            buffer: VecDeque::new(),
            scanned: false,
        }
    }
}

impl Iterator for VMatches<'_, '_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.scanned {
            unsafe {
                hs::hs_scan(
                    self.db.as_ptr(),
                    self.haystack.as_ptr() as *const i8,
                    self.haystack.len() as u32,
                    0,
                    self.scratch.as_ptr(),
                    Some(on_match),
                    &mut self.buffer as *mut VecDeque<usize> as *mut c_void,
                );
            }
            self.scanned = true;
        }
        self.buffer.pop_front()
    }
}

extern "C" fn on_match(id: u32, _from: u64, _to: u64, _flags: u32, ctx: *mut c_void) -> i32 {
    let buffer = unsafe { &mut *(ctx as *mut VecDeque<usize>) };
    buffer.push_back(id as usize);
    0
}
