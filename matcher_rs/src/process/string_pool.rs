//! Thread-local string pool for the transformation pipeline.
//!
//! The string pool ([`STRING_POOL`]) reduces allocation churn by recycling `String`
//! buffers across matcher calls within each thread.
//!
//! # Safety model
//!
//! Thread-local statics use `UnsafeCell` with `#[thread_local]` (a nightly feature)
//! to avoid the closure overhead of the `thread_local!` macro. Safety relies on two
//! invariants:
//!
//! 1. `#[thread_local]` guarantees single-threaded access — no data races.
//! 2. No public function in this module is re-entrant: the borrow from `UnsafeCell::get()`
//!    is always dropped before any call that could re-enter the same pool.

use std::cell::UnsafeCell;

/// Maximum number of [`String`] buffers retained in the pool between calls; excess are dropped.
const STRING_POOL_MAX: usize = 128;

/// Pool of reusable [`String`] buffers, one per thread.
///
/// # Safety
///
/// Uses `#[thread_local]` + `UnsafeCell` to eliminate the `thread_local!` macro's
/// `.with()` closure overhead. Single-threaded access is guaranteed by the
/// `#[thread_local]` attribute. No function in this module is re-entrant while the
/// mutable reference from `UnsafeCell::get()` is live.
#[thread_local]
pub(crate) static STRING_POOL: UnsafeCell<Vec<String>> = UnsafeCell::new(Vec::new());

/// Pops a reusable [`String`] from the thread-local pool, or allocates a new one.
///
/// Returns a cleared `String` with capacity ≥ `capacity`. If a buffer is
/// available in the pool, it is reused: `clear()` is called to reset its
/// length, and `reserve` grows it if its existing capacity falls short.
/// If the pool is empty, a fresh `String::with_capacity(capacity)` is allocated.
///
/// Callers should pass the expected output length as `capacity` to avoid
/// mid-write reallocations. Passing 0 is valid and simply pops an arbitrary-
/// capacity buffer from the pool.
///
/// # Safety
///
/// Accesses [`STRING_POOL`] via `UnsafeCell::get()`. Safe because
/// `#[thread_local]` guarantees single-threaded ownership and this function
/// is not re-entrant while the returned `String` is live.
pub(crate) fn get_string_from_pool(capacity: usize) -> String {
    // SAFETY: #[thread_local] guarantees single-threaded access; non-re-entrant.
    let pool = unsafe { &mut *STRING_POOL.get() };
    if let Some(mut s) = pool.pop() {
        s.clear();
        if s.capacity() < capacity {
            s.reserve(capacity - s.capacity());
        }
        s
    } else {
        String::with_capacity(capacity)
    }
}

/// Returns a [`String`] to the thread-local pool for future reuse.
///
/// If the pool already holds [`STRING_POOL_MAX`] buffers, the string is
/// dropped immediately rather than growing the pool without bound. This
/// caps per-thread memory usage while still amortizing allocation costs
/// across the most common working sets.
///
/// Callers should only return buffers that were obtained from
/// [`get_string_from_pool`]; returning arbitrary strings is safe but may
/// leave oversized buffers in the pool.
///
/// # Safety
///
/// Accesses [`STRING_POOL`] via `UnsafeCell::get()`. Safe because
/// `#[thread_local]` guarantees single-threaded ownership and this function
/// is not re-entrant.
pub(crate) fn return_string_to_pool(s: String) {
    // SAFETY: #[thread_local] guarantees single-threaded access; non-re-entrant.
    let pool = unsafe { &mut *STRING_POOL.get() };
    if pool.len() < STRING_POOL_MAX {
        pool.push(s);
    }
}
