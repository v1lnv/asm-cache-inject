//! Library root — re-exports all public modules so that the crate can be used
//! both as a CLI binary (`main.rs`) and as a library dependency.

pub mod asm;
pub mod bench;
pub mod cache;
pub mod cli;
pub mod engine;
pub mod error;
pub mod flush;
pub mod io;
pub mod memory;
