use std::sync::Arc;
use parking_lot::Mutex;
use crate::vectorscan::database::{VectorscanDatabase, LiteralDatabase};
use crate::vectorscan::scratch::Scratch;
use crate::vectorscan::iter::VMatches;

/// High-level orchestration for Vectorscan matching.
#[derive(Debug, Clone)]
pub struct VectorscanMatcher {
    database: Arc<dyn VectorscanDatabase>,
    scratch: Arc<Mutex<Scratch>>,
}

impl VectorscanMatcher {
    /// Initializes a new matcher with a literal database.
    pub fn new(patterns: &[&str]) -> Self {
        let database = Arc::new(LiteralDatabase::new(patterns));
        let scratch = Arc::new(Mutex::new(Scratch::new_with_ptr(database.as_ptr())));
        Self { database, scratch }
    }

    /// Returns a lazy iterator wrapper for searching the haystack.
    pub fn find_overlapping_iter<'a, 'h>(&'a self, haystack: &'h str) -> VMatchesWrapper<'a, 'h> {
        VMatchesWrapper {
            matcher: self,
            haystack: haystack.as_bytes(),
        }
    }
}

/// Helper wrapper to manage scratch space locking during iteration.
pub struct VMatchesWrapper<'a, 'h> {
    matcher: &'a VectorscanMatcher,
    haystack: &'h [u8],
}

impl VMatchesWrapper<'_, '_> {
    /// Consumes the iterator and executes a closure for each pattern ID found.
    pub fn for_each<F>(self, mut f: F)
    where
        F: FnMut(usize),
    {
        let mut scratch = self.matcher.scratch.lock();
        let iter = VMatches::new(&*self.matcher.database, &mut scratch, self.haystack);
        for id in iter {
            f(id);
        }
    }
}
