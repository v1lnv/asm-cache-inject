//! Module facade for the core cache engine.

pub mod cache_engine;
pub mod config;
pub mod read_path;
pub mod write_path;

pub use cache_engine::CacheEngine;
pub use config::CacheConfig;
pub use read_path::ReadResult;
pub use write_path::WriteResult;
