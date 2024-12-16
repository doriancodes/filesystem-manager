//! Session management for filesystem operations.
//!
//! This module provides the `Session` type which manages filesystem sessions
//! and handles communication between the client and filesystem operations.
//! It provides a thread-safe way to perform mount, bind, and unmount operations.

use crate::FilesystemManager;
use anyhow::Result;
use log::{error, info};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Messages that can be sent to the session handler thread.
#[derive(Debug)]
enum SessionMessage {
    /// Mount a filesystem
    Mount {
        /// Source path to mount from
        source: PathBuf,
        /// Target path to mount to
        target: PathBuf,
        /// Node identifier for remote mounts
        node_id: String,
    },
    /// Bind a directory
    Bind {
        /// Source path to bind from
        source: PathBuf,
        /// Target path to bind to
        target: PathBuf,
        /// Binding mode
        mode: crate::modules::namespace::BindMode,
    },
    /// Unmount a filesystem
    Unmount {
        /// Path to unmount
        path: PathBuf,
    },
    /// Shutdown the session
    Shutdown,
}

/// A filesystem state manager that handles mounting and binding operations.
///
/// The Session type provides a high-level interface for managing filesystem
/// operations in a thread-safe manner. It maintains a dedicated thread for
/// processing filesystem operations and ensures proper cleanup on shutdown.
///
/// # Example
///
/// ```no_run
/// use froggr::Session;
/// use std::path::PathBuf;
///
/// # fn main() -> anyhow::Result<()> {
/// let session = Session::new(PathBuf::from("/tmp/test"))?;
///
/// // Mount a filesystem
/// session.mount(
///     &PathBuf::from("/source"),
///     &PathBuf::from("/target"),
///     "localhost"
/// )?;
///
/// // Shutdown cleanly
/// session.shutdown()?;
/// # Ok(())
/// # }
/// ```
pub struct Session {
    /// The filesystem manager instance
    pub fs_manager: FilesystemManager,
    /// Channel sender for session messages
    message_tx: Sender<SessionMessage>,
    /// Handle to the message processing thread
    message_thread: JoinHandle<()>,
    /// Flag indicating if the session is running
    is_running: Arc<AtomicBool>,
}

impl Session {
    /// Creates a new Session with the specified root directory.
    ///
    /// # Arguments
    ///
    /// * `root` - The root directory path for the filesystem
    ///
    /// # Returns
    ///
    /// A new `Session` instance
    ///
    /// # Errors
    ///
    /// Returns an error if the filesystem manager cannot be initialized
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let fs = crate::NineP::new(root.clone())?;
        let fs_manager = FilesystemManager::new(fs);

        let (tx, rx) = channel();
        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = is_running.clone();
        let fs_manager_clone = fs_manager.clone();

        let message_thread = thread::spawn(move || {
            Self::run_message_handler(rx, is_running_clone, fs_manager_clone);
        });

        info!("Session started in {}", root.display());

        Ok(Self {
            fs_manager,
            message_tx: tx,
            message_thread,
            is_running,
        })
    }

    /// Runs the message handling loop.
    ///
    /// This internal method processes incoming messages and performs the
    /// corresponding filesystem operations.
    ///
    /// # Arguments
    ///
    /// * `rx` - The receiving end of the message channel
    /// * `is_running` - Atomic flag indicating if the session should continue running
    /// * `fs_manager` - The filesystem manager instance
    fn run_message_handler(
        rx: Receiver<SessionMessage>,
        is_running: Arc<AtomicBool>,
        fs_manager: FilesystemManager,
    ) {
        while is_running.load(Ordering::SeqCst) {
            match rx.recv() {
                Ok(message) => match message {
                    SessionMessage::Mount {
                        source,
                        target,
                        node_id,
                    } => {
                        info!(
                            "Processing mount request: {:?} -> {:?} (node: {})",
                            source, target, node_id
                        );
                        if let Err(e) = fs_manager.mount(&source, &target, &node_id) {
                            error!("Mount failed: {}", e);
                        }
                    }
                    SessionMessage::Bind {
                        source,
                        target,
                        mode,
                    } => {
                        info!("Processing bind request: {:?} -> {:?}", source, target);
                        if let Err(e) = fs_manager.bind(&source, &target, mode) {
                            error!("Bind failed: {}", e);
                        }
                    }
                    SessionMessage::Unmount { path } => {
                        info!("Processing unmount request: {:?}", path);
                        if let Err(e) = fs_manager.unmount(&path, None) {
                            error!("Unmount failed: {}", e);
                        }
                    }
                    SessionMessage::Shutdown => {
                        info!("Received shutdown message");
                        break;
                    }
                },
                Err(e) => {
                    error!("Message channel error: {}", e);
                    break;
                }
            }
        }
    }

    /// Mount a filesystem at the specified path.
    ///
    /// # Arguments
    ///
    /// * `source` - The source path to mount from
    /// * `target` - The target path to mount to
    /// * `node_id` - The node identifier for remote mounts
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the mount request was successfully queued
    /// * `Err` if the request could not be sent
    pub fn mount(&self, source: &Path, target: &Path, node_id: &str) -> Result<()> {
        self.message_tx.send(SessionMessage::Mount {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            node_id: node_id.to_string(),
        })?;
        Ok(())
    }

    /// Bind a source path to a target path.
    ///
    /// # Arguments
    ///
    /// * `source` - The source path to bind from
    /// * `target` - The target path to bind to
    /// * `mode` - The binding mode to use
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the bind request was successfully queued
    /// * `Err` if the request could not be sent
    pub fn bind(
        &self,
        source: &Path,
        target: &Path,
        mode: crate::modules::namespace::BindMode,
    ) -> Result<()> {
        self.message_tx.send(SessionMessage::Bind {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            mode,
        })?;
        Ok(())
    }

    /// Unmount a filesystem at the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to unmount
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the unmount request was successfully queued
    /// * `Err` if the request could not be sent
    pub fn unmount(&self, path: &Path) -> Result<()> {
        self.message_tx.send(SessionMessage::Unmount {
            path: path.to_path_buf(),
        })?;
        Ok(())
    }

    /// Shutdown the session cleanly.
    ///
    /// This method stops the message processing thread and ensures all
    /// resources are properly cleaned up.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if shutdown was successful
    /// * `Err` if there was an error during shutdown
    pub fn shutdown(mut self) -> Result<()> {
        info!("Shutting down session");
        self.message_tx.send(SessionMessage::Shutdown)?;
        let thread = std::mem::replace(&mut self.message_thread, thread::spawn(|| {}));
        thread
            .join()
            .map_err(|_| anyhow::anyhow!("Failed to join message thread"))?;
        Ok(())
    }
}

/// Implements cleanup on drop.
///
/// When a Session is dropped, it ensures the message processing thread
/// is properly shut down.
impl Drop for Session {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Err(e) = self.message_tx.send(SessionMessage::Shutdown) {
            error!("Error sending shutdown message: {}", e);
        }
    }
}
