//! Module facade for the integrated benchmark suite.

pub mod random;
pub mod report;
pub mod runner;
pub mod sequential;

pub use runner::{run_benchmarks, BenchConfig, BenchMode};
