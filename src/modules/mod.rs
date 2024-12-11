//! Core filesystem modules.
//! 
//! This module provides the main components of the filesystem:
//! 
//! - `constants`: Filesystem constants and default values
//! - `mount`: Filesystem mounting and management
//! - `namespace`: Namespace and binding operations
//! - `proto`: 9P protocol implementation
//! - `daemon`: Unix daemon process management and control
//! - `session`: Session management and daemon communication

pub mod constants;
pub mod mount;
/// Namespace management and binding operations implementation.
pub mod namespace;
pub mod proto;
/// Unix daemon process management implementation.
/// 
/// This module provides functionality for running processes in the background
/// as Unix daemons, handling process detachment, file descriptor management,
/// and PID file handling.
pub mod daemon;
/// Session management implementation.
/// 
/// This module provides the `Session` struct which manages filesystem sessions
/// and handles communication with the background daemon process. It supports:
/// - Session creation and initialization
/// - Mount and unmount operations
/// - Clean session shutdown
/// - Signal handling
pub mod session;
