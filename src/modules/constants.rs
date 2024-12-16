#![allow(warnings)]

//! Constant values used throughout the filesystem implementation.
//! 
//! This module defines various constants used for filesystem operations,
//! permissions, and default values.

use std::time::Duration;

/// Time-to-live for filesystem entries
pub const TTL: Duration = Duration::from_secs(1);

/// Size of filesystem blocks
pub const BLOCK_SIZE: u64 = 512;

/// Default permission mode for new files
pub const DEFAULT_PERMISSION: u16 = 0o755;

/// Root directory inode number
pub const ROOT_INODE: u64 = 1;

/// Starting inode number for new files
pub const INITIAL_INODE: u64 = 2;

/// Default user ID for filesystem operations
pub const DEFAULT_UID: u32 = 501;

/// Default group ID for filesystem operations
pub const DEFAULT_GID: u32 = 20;
