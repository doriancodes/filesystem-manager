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
use std::collections::HashMap;
use std::fs;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use nix::unistd::{fork, ForkResult};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::signal::ctrl_c;

/// Information about a running filesystem session.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Unique identifier for the session
    pub id: String,
    /// Process ID of the session
    pub pid: i32,
    /// Root directory path for the session
    pub root: PathBuf,
    /// List of active mounts (source, target)
    pub mounts: Vec<(PathBuf, PathBuf)>,
    /// List of active binds (source, target)
    pub binds: Vec<(PathBuf, PathBuf)>,
}

/// Manages filesystem sessions, including creation, listing, and termination.
pub struct SessionManager {
    /// Directory where session information is stored
    sessions_dir: PathBuf,
}

impl SessionManager {
    /// Creates a new SessionManager.
    ///
    /// Initializes the sessions directory at `/tmp/froggr/sessions`.
    ///
    /// # Returns
    /// * `Ok(SessionManager)` on success
    /// * `Err` if the sessions directory cannot be created
    pub fn new() -> Result<Self> {
        let sessions_dir = PathBuf::from("/tmp/froggr/sessions");
        fs::create_dir_all(&sessions_dir)?;
        Ok(Self { sessions_dir })
    }

    /// Creates a new filesystem session.
    ///
    /// Forks a new process to run the session and stores session information.
    ///
    /// # Arguments
    /// * `root` - Root directory path for the new session
    ///
    /// # Returns
    /// * `Ok(String)` - Session ID of the created session
    /// * `Err` if session creation fails
    pub fn create_session(&self, root: PathBuf) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();
        
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child }) => {
                let session_info = SessionInfo {
                    id: session_id.clone(),
                    pid: child.as_raw(),
                    root: root.clone(),
                    mounts: Vec::new(),
                    binds: Vec::new(),
                };
                
                let session_file = self.sessions_dir.join(&session_id);
                fs::write(&session_file, serde_json::to_string(&session_info)?)?;
                
                info!("Created new session: {}", session_id);
                Ok(session_id)
            }
            Ok(ForkResult::Child) => {
                // Create a new runtime in the child process
                std::thread::spawn(move || {
                    let runtime = tokio::runtime::Runtime::new().unwrap();
                    let session = Session::new(root).unwrap();
                    
                    runtime.block_on(async {
                        if let Err(e) = session.run().await {
                            error!("Session error: {}", e);
                        }
                    });
                });
                
                // Keep the process alive
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Err(e) => Err(anyhow::anyhow!("Fork failed: {}", e)),
        }
    }

    /// Lists all active sessions.
    ///
    /// # Returns
    /// * `Ok(Vec<SessionInfo>)` - Information about all active sessions
    /// * `Err` if reading session information fails
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(info) = serde_json::from_str(&content) {
                    sessions.push(info);
                }
            }
        }
        Ok(sessions)
    }

    /// Terminates a specific session.
    ///
    /// # Arguments
    /// * `session_id` - ID of the session to terminate
    ///
    /// # Returns
    /// * `Ok(())` if the session was successfully terminated
    /// * `Err` if the session doesn't exist or termination fails
    pub fn kill_session(&self, session_id: &str) -> Result<()> {
        let session_file = self.sessions_dir.join(session_id);
        if let Ok(content) = fs::read_to_string(&session_file) {
            let info: SessionInfo = serde_json::from_str(&content)?;
            signal::kill(Pid::from_raw(info.pid), Signal::SIGTERM)?;
            fs::remove_file(session_file)?;
            info!("Killed session: {}", session_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }

    /// Terminates all active sessions.
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of sessions terminated
    /// * `Err` if termination of any session fails
    pub fn purge_sessions(&self) -> Result<usize> {
        let sessions = self.list_sessions()?;
        let mut killed = 0;

        for session in sessions {
            match self.kill_session(&session.id) {
                Ok(_) => killed += 1,
                Err(e) => error!("Failed to kill session {}: {}", session.id, e),
            }
        }

        // Clean up any orphaned session files
        if let Ok(entries) = fs::read_dir(&self.sessions_dir) {
            for entry in entries.flatten() {
                if let Err(e) = fs::remove_file(entry.path()) {
                    error!("Failed to remove session file: {}", e);
                }
            }
        }

        info!("Purged {} sessions", killed);
        Ok(killed)
    }

    /// Get details for a specific session.
    ///
    /// # Arguments
    /// * `session_id` - ID of the session to retrieve
    ///
    /// # Returns
    /// * `Ok(Some(SessionInfo))` if the session exists
    /// * `Ok(None)` if the session doesn't exist
    /// * `Err` if reading session information fails
    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionInfo>> {
        let session_file = self.sessions_dir.join(session_id);
        if session_file.exists() {
            let content = fs::read_to_string(&session_file)?;
            let info: SessionInfo = serde_json::from_str(&content)?;
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }
}

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

    /// Runs the session in a loop until a shutdown signal is received.
    ///
    /// # Returns
    /// * `Ok(())` if the session shuts down cleanly
    /// * `Err` if an error occurs during session execution
    pub async fn run(self) -> Result<()> {
        info!("Session running. Waiting for shutdown signal...");
        
        // Wait for shutdown signal
        ctrl_c().await?;
        info!("Received shutdown signal");
        
        self.shutdown()?;
        info!("Session terminated");
        
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
