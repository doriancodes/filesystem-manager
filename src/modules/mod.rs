//! Core filesystem modules.
//! 
//! This module provides the main components of the filesystem:
//! 
//! - `constants`: Filesystem constants and default values
//! - `mount`: Filesystem mounting and management
//! - `namespace`: Namespace and binding operations
//! - `proto`: 9P protocol implementation

pub mod constants;
pub mod mount;
/// Namespace management and binding operations implementation.
pub mod namespace;
pub mod proto;
