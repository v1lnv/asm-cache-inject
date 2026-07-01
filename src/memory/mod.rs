//! Module facade — exposes the page-aligned allocator, the fixed-block memory
//! pool, and the lightweight block-buffer handle.

pub mod aligned_allocator;
pub mod block_buffer;
pub mod pool;

// Re-export the most commonly used types for ergonomic imports.
pub use aligned_allocator::{AlignedBuffer, PAGE_ALIGNMENT};
pub use block_buffer::BlockBuffer;
pub use pool::MemoryPool;
