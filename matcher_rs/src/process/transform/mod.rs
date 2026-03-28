//! Low-level text transformation engines.
//!
//! Provides the building blocks used by the step registry and pipeline executor:
//! pre-compiled data tables ([`constants`]), charwise lookup engines
//! ([`charwise`]), delete handling ([`delete`]), normalization
//! ([`normalize`]), and SIMD-accelerated character-skip utilities ([`simd`]).
pub(crate) mod charwise;
pub(crate) mod constants;
pub(crate) mod delete;
pub(crate) mod normalize;
pub(crate) mod simd;
