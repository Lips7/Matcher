//! Error types for the Vectorscan module.
//!
//! Provides a unified [`Error`] enum, a typed [`VectorscanErrorCode`] mapping
//! from raw `hs_error_t` values, and the [`AsResult`] extension trait for
//! converting raw FFI return codes into idiomatic `Result` values.

use std::ffi::CStr;
use std::fmt;

use thiserror::Error;
use vectorscan_rs_sys as hs;

/// Unified error type for all Vectorscan operations.
///
/// Covers both runtime scan/API errors and pattern compilation failures.
#[derive(Debug, Error)]
pub enum Error {
    /// A runtime error originating from a Vectorscan API call.
    #[error("Error originating from Vectorscan API: {0} (code {1})")]
    Vectorscan(VectorscanErrorCode, i32),

    /// A pattern compilation error with the diagnostic message and the
    /// zero-based index of the pattern that caused the failure.
    #[error("Pattern compilation failed: {0} (index {1})")]
    VectorscanCompile(String, i32),
}

/// Typed representation of Vectorscan/Hyperscan error codes.
///
/// Each variant represents a specific error condition that can be returned
/// by the Vectorscan C API.
#[derive(Debug, PartialEq, Eq)]
pub enum VectorscanErrorCode {
    /// A parameter passed to the function was invalid.
    Invalid,
    /// A memory allocation failed.
    Nomem,
    /// The scan was aborted early because the match callback returned non-zero.
    ///
    /// This is **not** an error in normal usage — [`VectorscanScanner::scan_with_scratch`]
    /// treats `HS_SCAN_TERMINATED` as a successful early-exit signal and returns `Ok(())`.
    ///
    /// [`VectorscanScanner::scan_with_scratch`]: crate::vectorscan::scanner::VectorscanScanner::scan_with_scratch
    ScanTerminated,
    /// The pattern compiler failed.
    CompileError,
    /// The database was built for a different version of Vectorscan.
    DbVersionError,
    /// The database was built for a different platform.
    DbPlatformError,
    /// The database was built for a different mode.
    DbModeError,
    /// A parameter was not correctly aligned.
    BadAlign,
    /// The memory allocator returned misaligned memory.
    BadAlloc,
    /// The scratch space was already in use by another scan call.
    ScratchInUse,
    /// The current CPU architecture is unsupported.
    ArchError,
    /// Provided buffer was too small.
    InsufficientSpace,
    /// An unknown internal error occurred.
    UnknownError,
    /// The raw code does not match any known constant.
    UnknownErrorCode,
}

impl fmt::Display for VectorscanErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => write!(f, "invalid parameter"),
            Self::Nomem => write!(f, "out of memory"),
            Self::ScanTerminated => write!(f, "scan terminated by callback"),
            Self::CompileError => write!(f, "compilation error"),
            Self::DbVersionError => write!(f, "database version mismatch"),
            Self::DbPlatformError => write!(f, "database platform mismatch"),
            Self::DbModeError => write!(f, "database mode mismatch"),
            Self::BadAlign => write!(f, "bad alignment"),
            Self::BadAlloc => write!(f, "allocator returned misaligned memory"),
            Self::ScratchInUse => write!(f, "scratch space in use"),
            Self::ArchError => write!(f, "unsupported CPU architecture"),
            Self::InsufficientSpace => write!(f, "insufficient space"),
            Self::UnknownError => write!(f, "unknown internal error"),
            Self::UnknownErrorCode => write!(f, "unrecognized error code"),
        }
    }
}

/// Converts a raw `hs_error_t` integer into a typed [`VectorscanErrorCode`].
///
/// Unknown codes that do not match any `HS_*` constant are mapped to
/// [`VectorscanErrorCode::UnknownErrorCode`].
impl From<hs::hs_error_t> for VectorscanErrorCode {
    fn from(value: hs::hs_error_t) -> Self {
        match value {
            hs::HS_INVALID => Self::Invalid,
            hs::HS_NOMEM => Self::Nomem,
            hs::HS_SCAN_TERMINATED => Self::ScanTerminated,
            hs::HS_COMPILER_ERROR => Self::CompileError,
            hs::HS_DB_VERSION_ERROR => Self::DbVersionError,
            hs::HS_DB_PLATFORM_ERROR => Self::DbPlatformError,
            hs::HS_DB_MODE_ERROR => Self::DbModeError,
            hs::HS_BAD_ALIGN => Self::BadAlign,
            hs::HS_BAD_ALLOC => Self::BadAlloc,
            hs::HS_SCRATCH_IN_USE => Self::ScratchInUse,
            hs::HS_ARCH_ERROR => Self::ArchError,
            hs::HS_INSUFFICIENT_SPACE => Self::InsufficientSpace,
            hs::HS_UNKNOWN_ERROR => Self::UnknownError,
            _ => Self::UnknownErrorCode,
        }
    }
}

impl From<hs::hs_error_t> for Error {
    fn from(value: hs::hs_error_t) -> Self {
        Self::Vectorscan(value.into(), value)
    }
}

/// Extension trait for converting a raw `hs_error_t` return code into a `Result`.
///
/// This trait provides a convenient way to check the return status of
/// Vectorscan FFI calls and convert them into idiomatic Rust [`Result`] values.
pub trait AsResult: Sized {
    /// Converts this value into a `Result`.
    ///
    /// # Returns
    /// * `Ok(())` - If the value represents `HS_SUCCESS`.
    /// * `Err(Error)` - For any non-success error code.
    fn ok(self) -> Result<(), Error>;
}

impl AsResult for hs::hs_error_t {
    fn ok(self) -> Result<(), Error> {
        if self == hs::HS_SUCCESS as hs::hs_error_t {
            Ok(())
        } else {
            Err(self.into())
        }
    }
}

/// Extracts the compile error message and expression index from an
/// `hs_compile_error_t` pointer.
///
/// This function converts the raw FFI error into a structured [`Error::VectorscanCompile`],
/// and ensures that the memory allocated by Vectorscan for the error message is freed.
///
/// # Arguments
/// * `compile_error` - A non-null pointer to the compile error returned
///   by a Vectorscan compiler function.
///
/// # Returns
/// An [`Error::VectorscanCompile`] containing the diagnostic message and
/// the zero-based index of the pattern that failed.
///
/// # Safety
/// * `compile_error` must be a valid, non-null pointer returned by a
///   Vectorscan compiler call (`hs_compile_multi`, `hs_compile_lit_multi`, etc.).
/// * This function frees the compile error; the pointer must not be used afterwards.
pub(crate) unsafe fn extract_compile_error(compile_error: *mut hs::hs_compile_error_t) -> Error {
    unsafe {
        let message = if (*compile_error).message.is_null() {
            "unknown compile error".to_string()
        } else {
            CStr::from_ptr((*compile_error).message)
                .to_string_lossy()
                .into_owned()
        };
        let expression = (*compile_error).expression;
        hs::hs_free_compile_error(compile_error);
        Error::VectorscanCompile(message, expression)
    }
}
