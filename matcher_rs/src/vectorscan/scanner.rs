use std::ffi::{c_char, c_void};
use std::sync::Arc;

use vectorscan_rs_sys as hs;

use crate::vectorscan::database::{LiteralDatabase, RegexDatabase, VectorscanDatabase};
use crate::vectorscan::error::{AsResult, Error};
use crate::vectorscan::init_allocator;
use crate::vectorscan::scratch::Scratch;

/// High-level Vectorscan scanner.
///
/// This scanner manages a compiled database and the necessary scratch space
/// for scanning operations. It is designed to be thread-safe by cloning
/// the scratch space for each individual scan call.
#[derive(Debug, Clone)]
pub struct VectorscanScanner {
    db: Arc<dyn VectorscanDatabase>,
    scratch: Scratch,
}

impl VectorscanScanner {
    /// Creates a new scanner from a pre-built database.
    ///
    /// This function initializes the memory allocator and prepares the template
    /// scratch space required for scanning.
    ///
    /// # Arguments
    /// * `db` - An [`Arc`] containing a compiled Vectorscan database.
    ///
    /// # Returns
    /// A [`Result<Self, Error>`] containing the initialized scanner.
    pub fn new(db: Arc<dyn VectorscanDatabase>) -> Result<Self, Error> {
        #[cfg(target_os = "macos")]
        init_allocator();
        let scratch = unsafe { Scratch::new(db.as_ptr())? };
        Ok(Self { db, scratch })
    }

    /// Convenience: compiles a literal database and returns a ready scanner.
    ///
    /// This function provides a simple way to create a scanner from a list of
    /// literal patterns without needing to manually build the database first.
    ///
    /// # Arguments
    /// * `patterns` - Literal byte patterns to be matched.
    /// * `flags` - Per-pattern flags.
    ///
    /// # Returns
    /// A [`Result<Self, Error>`] containing the initialized scanner.
    pub fn new_literal(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        let db = Arc::new(LiteralDatabase::new(patterns, flags)?);
        Self::new(db)
    }

    /// Convenience: compiles a regex database and returns a ready scanner.
    ///
    /// This function provides a simple way to create a scanner from a list of
    /// regular expressions without needing to manually build the database first.
    ///
    /// # Arguments
    /// * `patterns` - Regex expressions to be matched.
    /// * `flags` - Per-pattern flags.
    ///
    /// # Returns
    /// A [`Result<Self, Error>`] containing the initialized scanner.
    pub fn new_regex(patterns: &[&str], flags: &[u32]) -> Result<Self, Error> {
        let db = Arc::new(RegexDatabase::new(patterns, flags)?);
        Self::new(db)
    }

    /// Scans the given haystack and invokes the callback for every match.
    ///
    /// This function clones the internal scratch space and performs a block-mode
    /// scan of the provided data. For each match found, the `on_match` closure
    /// is called with the identifier of the matched pattern.
    ///
    /// # Arguments
    /// * `haystack` - A byte slice representing the data to be scanned.
    /// * `on_match` - A closure that is called for each match, receiving the pattern ID.
    ///
    /// # Returns
    /// A [`Result<(), Error>`] indicating the success or failure of the scan operation.
    ///
    /// # Errors
    /// Returns an error if scratch cloning or the scan itself fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::vectorscan::VectorscanScanner;
    /// use std::sync::Arc;
    ///
    /// let scanner = VectorscanScanner::new_literal(&["apple", "banana"], &[0, 0]).unwrap();
    /// let mut matches = Vec::new();
    /// scanner.scan(b"I have an apple and a banana", |id| matches.push(id)).unwrap();
    /// assert_eq!(matches.len(), 2);
    /// ```
    pub fn scan<F>(&self, haystack: &[u8], mut on_match: F) -> Result<(), Error>
    where
        F: FnMut(usize),
    {
        let scratch = self.scratch.try_clone()?;

        // We pass a raw pointer to `on_match` through the FFI context parameter.
        let ctx = &mut on_match as *mut F as *mut c_void;

        unsafe {
            let status = hs::hs_scan(
                self.db.as_ptr(),
                haystack.as_ptr() as *const c_char,
                haystack.len() as u32,
                0,
                scratch.as_ptr(),
                Some(match_callback::<F>),
                ctx,
            );

            // HS_SCAN_TERMINATED is returned when the callback returns non-zero,
            // which we never do, so treat it as an error here.
            status.ok()
        }
    }
}

/// FFI callback invoked by the Vectorscan scan function for each match found.
///
/// This callback takes the context pointer, which is expected to be a closure,
/// and calls it with the pattern identifier.
///
/// # Arguments
/// * `id` - The identifier assigned to the matching pattern at compile time.
/// * `_from` - The starting position of the match (unused).
/// * `_to` - The ending position of the match (unused).
/// * `_flags` - Matching flags (unused).
/// * `ctx` - A raw pointer to the user-provided closure.
///
/// # Returns
/// Always returns 0 to continue scanning.
extern "C" fn match_callback<F>(id: u32, _from: u64, _to: u64, _flags: u32, ctx: *mut c_void) -> i32
where
    F: FnMut(usize),
{
    let on_match = unsafe { &mut *(ctx as *mut F) };
    on_match(id as usize);
    0
}
