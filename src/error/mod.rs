//! Module facade — re-exports the public error type and result alias so that
//! dependents only need `use crate::error::*;`.

mod types;

pub use types::CacheError;
pub use types::CacheResult;
