//! Safe Rust wrappers around the Vectorscan (Hyperscan) SIMD pattern-matching library.
//!
//! Only available with the `vectorscan` feature flag. Requires Boost at build time and is
//! not supported on Windows or ARM64.
//!
//! The three primary types are:
//! - [`Database`](database::Database) — a compiled, immutable pattern database (thread-safe, shareable).
//! - [`Scratch`] — per-scan temporary workspace (one per thread, reusable).
//! - [`VectorscanScanner`] — combines a `Database` with scanning logic; the main API entry point.
//!
//! For error handling, see the [`error`] module.

pub mod database;
pub mod error;
pub mod scanner;
pub mod scratch;

pub use crate::vectorscan::scanner::VectorscanScanner;
pub use crate::vectorscan::scratch::Scratch;

#[cfg(target_os = "macos")]
mod allocator;
