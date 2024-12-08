use std::time::Duration;

// File system constants
pub const TTL: Duration = Duration::from_secs(1);
pub const BLOCK_SIZE: u64 = 512;
pub const DEFAULT_PERMISSION: u16 = 0o755;
pub const ROOT_INODE: u64 = 1;
pub const INITIAL_INODE: u64 = 2;

// User and group IDs
pub const DEFAULT_UID: u32 = 501;
pub const DEFAULT_GID: u32 = 20;
