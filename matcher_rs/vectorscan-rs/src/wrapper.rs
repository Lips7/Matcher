use crate::error::{AsResult, Error};
use bitflags::bitflags;
use foreign_types::{foreign_type, ForeignType};
use std::{
    str,
    mem::{self, MaybeUninit},
    ptr,
};
use vectorscan_rs_sys as hs;

foreign_type! {
    unsafe type CompileError {
        type CType = hs::hs_compile_error_t;
        fn drop = compile_error_drop;
    }

    pub unsafe type Database: Send + Sync {
        type CType = hs::hs_database_t;
        fn drop = database_drop;
    }

    pub unsafe type Scratch {
        type CType = hs::hs_scratch_t;
        fn drop = scratch_drop;
    }
}

unsafe fn database_drop(v: *mut hs::hs_database_t) {
    let res = hs::hs_free_database(v);
    if res != hs::HS_SUCCESS as hs::hs_error_t {
        panic!("hs_free_database failed: {res}");
    }
}

unsafe fn scratch_drop(v: *mut hs::hs_scratch_t) {
    let res = hs::hs_free_scratch(v);
    if res != hs::HS_SUCCESS as hs::hs_error_t {
        panic!("hs_free_scratch failed: {res}");
    }
}

unsafe fn compile_error_drop(v: *mut hs::hs_compile_error_t) {
    let res = hs::hs_free_compile_error(v);
    if res != hs::HS_SUCCESS as hs::hs_error_t {
        panic!("hs_free_compile_error failed: {res}");
    }
}

bitflags! {
    #[derive(Default, Clone, Copy)]
    pub struct Flag: u32 {
        const CASELESS = hs::HS_FLAG_CASELESS;
        const DOTALL = hs::HS_FLAG_DOTALL;
        const MULTILINE = hs::HS_FLAG_MULTILINE;
        const SINGLEMATCH = hs::HS_FLAG_SINGLEMATCH;
        const ALLOWEMPTY = hs::HS_FLAG_ALLOWEMPTY;
        const UTF8 = hs::HS_FLAG_UTF8;
        const UCP = hs::HS_FLAG_UCP;
        const PREFILTER = hs::HS_FLAG_PREFILTER;
        const SOM_LEFTMOST = hs::HS_FLAG_SOM_LEFTMOST;
        const COMBINATION = hs::HS_FLAG_COMBINATION;
        const QUIET = hs::HS_FLAG_QUIET;
    }
}

pub struct Pattern<'a> {
    expression: &'a [u8],
    flags: Flag,
    id: u32,
    len: usize,
}

impl<'a> Pattern<'a> {
    pub fn new(expression: &'a [u8], flags: Flag, id: u32) -> Self {
        let len = expression.len();
        Self {
            expression,
            flags,
            id,
            len,
        }
    }
}

impl Database {
    pub fn new(patterns: Vec<Pattern>, mode: ScanMode, is_literal: bool) -> Result<Self, Error> {
        let mut c_exprs = Vec::with_capacity(patterns.len());
        let mut c_flags = Vec::with_capacity(patterns.len());
        let mut c_ids = Vec::with_capacity(patterns.len());
        let mut c_lens = Vec::with_capacity(patterns.len());
        for Pattern {
            expression,
            flags,
            id,
            len,
        } in patterns
        {
            // have to keep the original strings until the db is created
            let c_expr = unsafe { mem::transmute::<*const u8, *const i8>(expression.as_ptr()) };
            c_exprs.push(c_expr);
            c_flags.push(flags.bits());
            c_ids.push(id);
            c_lens.push(len);
        }

        let mut db = MaybeUninit::uninit();
        let mut err = MaybeUninit::uninit();
        if is_literal {
            unsafe {
                hs::hs_compile_lit_multi(
                    c_exprs.as_ptr(),
                    c_flags.as_ptr(),
                    c_ids.as_ptr(),
                    c_lens.as_ptr(),
                    c_exprs.len() as u32,
                    mode.bits(),
                    ptr::null(),
                    db.as_mut_ptr(),
                    err.as_mut_ptr(),
                )
                .ok()
                .map_err(|_| {
                    let err = CompileError::from_ptr(err.assume_init());
                    Error::HyperscanCompile(err.message(), err.expression())
                })?;
                Ok(Database::from_ptr(db.assume_init()))
            }
        } else {
            unsafe {
                hs::hs_compile_multi(
                    c_exprs.as_ptr(),
                    c_flags.as_ptr(),
                    c_ids.as_ptr(),
                    c_exprs.len() as u32,
                    mode.bits(),
                    ptr::null(),
                    db.as_mut_ptr(),
                    err.as_mut_ptr(),
                )
                .ok()
                .map_err(|_| {
                    let err = CompileError::from_ptr(err.assume_init());
                    Error::HyperscanCompile(err.message(), err.expression())
                })?;
                Ok(Database::from_ptr(db.assume_init()))
            }
        }
    }
}

impl Scratch {
    pub fn new(database: &Database) -> Result<Self, Error> {
        let mut scratch = MaybeUninit::zeroed();
        unsafe {
            hs::hs_alloc_scratch(database.as_ptr(), scratch.as_mut_ptr())
                .ok()
                .map(|_| Scratch::from_ptr(scratch.assume_init()))
        }
    }
}

impl CompileError {
    fn message(&self) -> String {
        unsafe {
            let err = self.0.as_ptr();

            std::ffi::CStr::from_ptr((*err).message)
                .to_str()
                .unwrap()
                .into()
        }
    }
    fn expression(&self) -> i32 {
        unsafe { (*self.0.as_ptr()).expression }
    }
}

bitflags! {
    pub struct ScanMode: u32 {
        const BLOCK = hs::HS_MODE_BLOCK;
        const VECTORED = hs::HS_MODE_VECTORED;
        const STREAM = hs::HS_MODE_STREAM;
        const SOM_SMALL = hs::HS_MODE_SOM_HORIZON_SMALL;
        const SOM_MEDIUM = hs::HS_MODE_SOM_HORIZON_MEDIUM;
        const SOM_LARGE = hs::HS_MODE_SOM_HORIZON_LARGE;
    }
}
