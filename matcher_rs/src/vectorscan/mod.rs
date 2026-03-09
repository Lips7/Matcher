pub mod database;
pub mod error;
pub mod scanner;
pub mod scratch;

pub use crate::vectorscan::scanner::VectorscanScanner;
pub use crate::vectorscan::scratch::Scratch;

#[cfg(target_os = "macos")]
mod allocator;
