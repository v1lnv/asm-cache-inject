//! Module facade for the block device I/O layer.

pub mod device;
pub mod device_info;
pub mod reader;
pub mod writer;

pub use device::BlockDevice;
pub use device_info::DeviceInfo;
pub use reader::{read_block, read_lba};
pub use writer::{sync_device, write_block, write_lba};
