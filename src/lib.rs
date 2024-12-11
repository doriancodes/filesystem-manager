#![doc(html_root_url = "https://docs.rs/froggr/0.1.1")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(rustdoc::missing_crate_level_docs)]

//! froggr: A simple file system implementation using the 9P protocol
//! 
//! A simple file system implementation using the 9P protocol.
//! 
//! ## Features
//! 
//! - Flexible namespace management through bind operations
//! - Multiple binding modes (Replace, Before, After, Create)
//! - Union directories
//! - Custom environments
//! 
//! ## Quick Start
//! 
//! ```rust,no_run
//! use froggr::{FilesystemManager, NineP, BindMode};
//! use std::path::PathBuf;
//! 
//! # fn main() -> anyhow::Result<()> {
//! // Create a new filesystem
//! let fs = NineP::new(PathBuf::from("/tmp/test"))?;
//! let manager = FilesystemManager::new(fs);
//! 
//! // Bind a directory
//! manager.bind(
//!     "/source/path".as_ref(),
//!     "/target/path".as_ref(),
//!     BindMode::Replace
//! )?;
//! # Ok(())
//! # }
//! ```
//! 
//! ## Bind Modes
//! 
//! - `Replace`: Replaces existing content at the mountpoint
//! - `Before`: Adds content with higher priority
//! - `After`: Adds content with lower priority
//! - `Create`: Creates mountpoint if needed

pub mod modules;

pub use modules::mount::FilesystemManager;
pub use modules::proto::NineP;

// Re-export commonly used types
pub use modules::namespace::BindMode;
