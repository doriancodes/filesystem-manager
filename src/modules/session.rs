#![allow(unused_variables)]

//! Session management for filesystem operations.
//!
//! This module provides the `Session` type which manages filesystem sessions
//! and handles communication between the client and filesystem operations.
//! It provides a thread-safe way to perform mount, bind, and unmount operations.

use crate::FilesystemManager;
use anyhow::Result;
use log::{error, info, warn};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::fs;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use nix::unistd::{fork, ForkResult};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::signal::ctrl_c;
use parking_lot::RwLock;
use crate::BindMode;

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
        info!("Creating new session for root: {}", root.display());
        
        // First, check if there's an existing session for this root
        info!("Checking for existing sessions...");
        let existing_sessions = self.list_sessions()?;
        for session in existing_sessions {
            if session.root == root {
                info!("Found existing session {} for root {}", session.id, root.display());
                // Verify the session is still active
                if let Ok(_) = signal::kill(Pid::from_raw(session.pid), Signal::SIGCONT) {
                    info!("Reusing existing session {}", session.id);
                    return Ok(session.id);
                } else {
                    info!("Existing session is dead, removing it");
                    let session_file = self.sessions_dir.join(&session.id);
                    if let Err(e) = fs::remove_file(session_file) {
                        error!("Failed to remove dead session file: {}", e);
                    }
                }
            }
        }

        info!("No existing session found, creating new one");
        let session_id = Uuid::new_v4().to_string();
        info!("Generated new session ID: {}", session_id);
        
        info!("Attempting to fork process...");
        let fork_result = unsafe { fork() };
        info!("Fork completed");
        
        match fork_result {
            Ok(ForkResult::Parent { child }) => {
                info!("In parent process. Child PID: {}", child);
                let session_info = SessionInfo {
                    id: session_id.clone(),
                    pid: child.as_raw(),
                    root: root.clone(),
                    mounts: Vec::new(),
                    binds: Vec::new(),
                };
                
                let session_file = self.sessions_dir.join(&session_id);
                info!("Saving session info to: {}", session_file.display());
                match fs::write(&session_file, serde_json::to_string(&session_info)?) {
                    Ok(_) => info!("Session info saved successfully"),
                    Err(e) => error!("Failed to save session info: {}", e),
                }
                
                info!("Parent process completed successfully");
                Ok(session_id)
            }
            Ok(ForkResult::Child) => {
                info!("In child process");
                
                // Create the pipe immediately
                let pipe_path = self.sessions_dir.join(format!("{}.pipe", session_id));
                info!("Creating pipe at: {}", pipe_path.display());
                if !pipe_path.exists() {
                    match nix::unistd::mkfifo(&pipe_path, nix::sys::stat::Mode::S_IRWXU) {
                        Ok(_) => info!("Pipe created successfully"),
                        Err(e) => error!("Failed to create pipe: {}", e),
                    }
                }

                info!("Child process entering main loop");
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Err(e) => {
                error!("Fork failed with error: {}", e);
                Err(anyhow::anyhow!("Fork failed: {}", e))
            }
        }
    }

    /// Lists all active sessions.
    ///
    /// # Returns
    /// * `Ok(Vec<SessionInfo>)` - Information about all active sessions
    /// * `Err` if reading session information fails
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        info!("Starting to list sessions from: {}", self.sessions_dir.display());
        let mut sessions = Vec::new();
        
        match fs::read_dir(&self.sessions_dir) {
            Ok(entries) => {
                info!("Successfully read sessions directory");
                for entry_result in entries {
                    match entry_result {
                        Ok(entry) => {
                            info!("Processing entry: {:?}", entry.path());
                            if entry.path().extension().map_or(false, |ext| ext == "json") {
                                match fs::read_to_string(entry.path()) {
                                    Ok(content) => {
                                        info!("Read session file content");
                                        match serde_json::from_str(&content) {
                                            Ok(info) => {
                                                info!("Successfully parsed session info");
                                                sessions.push(info);
                                            }
                                            Err(e) => error!("Failed to parse session info: {}", e),
                                        }
                                    }
                                    Err(e) => error!("Failed to read session file: {}", e),
                                }
                            }
                        }
                        Err(e) => error!("Failed to process directory entry: {}", e),
                    }
                }
            }
            Err(e) => {
                error!("Failed to read sessions directory: {}", e);
                return Err(anyhow::anyhow!("Failed to read sessions directory: {}", e));
            }
        }
        
        info!("Found {} sessions", sessions.len());
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

    /// Sends a bind command to a running session.
    ///
    /// # Arguments
    /// * `session_id` - ID of the target session
    /// * `source` - Source path to bind from
    /// * `target` - Target path to bind to
    /// * `mode` - Binding mode to use
    ///
    /// # Returns
    /// * `Ok(())` if the command was sent successfully
    /// * `Err` if the session doesn't exist or the command couldn't be sent
    pub fn send_bind_command(&self, session_id: &str, source: PathBuf, target: PathBuf, mode: BindMode) -> Result<()> {
        info!("Sending bind command to session {}", session_id);
        if let Some(session) = self.get_session(session_id)? {
            // Ensure the pipe exists
            let pipe_path = self.sessions_dir.join(format!("{}.pipe", session_id));
            if !pipe_path.exists() {
                nix::unistd::mkfifo(&pipe_path, nix::sys::stat::Mode::S_IRWXU)?;
            }

            // Write the bind command to the pipe
            let command = SessionCommand::Bind {
                source,
                target,
                mode,
            };
            let command_str = serde_json::to_string(&command)?;
            
            // Open pipe for writing
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&pipe_path)?;
            
            use std::io::Write;
            file.write_all(command_str.as_bytes())?;
            
            info!("Bind command sent through pipe");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }

    /// Gets a reference to an active session.
    ///
    /// # Arguments
    /// * `session_id` - ID of the session to retrieve
    ///
    /// # Returns
    /// * `Ok(Some(Arc<Session>))` if the session exists and is active
    /// * `Ok(None)` if the session doesn't exist
    /// * `Err` if there was an error accessing the session
    pub fn get_active_session(&self, session_id: &str) -> Result<Option<Arc<Session>>> {
        info!("Getting active session for ID: {}", session_id);
        if let Some(session_info) = self.get_session(session_id)? {
            // Create or get the session instance
            let session = Session::new(session_info.root, session_id.to_string())?;
            info!("Retrieved active session");
            Ok(Some(session))
        } else {
            info!("No active session found for ID: {}", session_id);
            Ok(None)
        }
    }

    /// Sends a mount command to a running session.
    ///
    /// # Arguments
    /// * `session_id` - ID of the target session
    /// * `source` - Source path to mount from
    /// * `target` - Target path to mount to
    /// * `node_id` - Node identifier for the mount
    ///
    /// # Returns
    /// * `Ok(())` if the command was sent successfully
    /// * `Err` if the session doesn't exist or the command couldn't be sent
    pub fn send_mount_command(&self, session_id: &str, source: PathBuf, target: PathBuf, node_id: String) -> Result<()> {
        info!("Sending mount command to session {}", session_id);
        if let Some(active_session) = self.get_active_session(session_id)? {
            // Fork before mounting
            match unsafe { fork() }? {
                ForkResult::Parent { child } => {
                    info!("Started mount process with PID: {}", child);
                    
                    // Continue with sending the command through the pipe
                    let pipe_path = self.sessions_dir.join(format!("{}.pipe", session_id));
                    if !pipe_path.exists() {
                        nix::unistd::mkfifo(&pipe_path, nix::sys::stat::Mode::S_IRWXU)?;
                    }

                    let command = SessionCommand::Mount {
                        source,
                        target,
                        node_id,
                    };
                    let command_str = serde_json::to_string(&command)?;
                    
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .open(&pipe_path)?;
                    
                    use std::io::Write;
                    file.write_all(command_str.as_bytes())?;
                    
                    info!("Mount command sent through pipe");
                    Ok(())
                }
                ForkResult::Child => {
                    // Child process handles the FUSE mount
                    let fs_manager = active_session.fs_manager.clone();
                    if let Err(e) = fs_manager.mount(&source, &target, &node_id) {
                        error!("Mount failed in child process: {}", e);
                        std::process::exit(1);
                    }
                    std::process::exit(0);
                }
            }
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }
}

/// Messages that can be sent to the session handler thread.
#[derive(Debug)]
enum SessionMessage {
    Mount {
        source: PathBuf,
        target: PathBuf,
        node_id: String,
    },
    MountSuccess {
        source: PathBuf,
        target: PathBuf,
    },
    Bind {
        source: PathBuf,
        target: PathBuf,
        mode: BindMode,
    },
    BindSuccess {
        source: PathBuf,
        target: PathBuf,
    },
    Unmount {
        path: PathBuf,
    },
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
#[derive(Debug)]
pub struct Session {
    /// The filesystem manager instance
    pub fs_manager: FilesystemManager,
    /// Channel sender for session messages
    message_tx: Sender<SessionMessage>,
    /// Handle to the message processing thread
    message_thread: JoinHandle<()>,
    /// Flag indicating if the session is running
    is_running: Arc<AtomicBool>,
    /// Session state
    state: Arc<RwLock<SessionState>>,
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
    pub fn new(root: PathBuf, session_id: String) -> Result<Arc<Self>> {
        let fs = crate::NineP::new(root.clone())?;
        let fs_manager = FilesystemManager::new(fs);
        let (tx, rx) = channel();
        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = is_running.clone();
        let fs_manager_clone = fs_manager.clone();
        
        let state = Arc::new(RwLock::new(SessionState::load(&root, session_id.clone())?));
        let state_clone = state.clone();

        let message_thread = thread::spawn(move || {
            Self::run_message_handler(rx, is_running_clone, fs_manager_clone, state_clone);
        });

        let session = Arc::new(Self {
            fs_manager,
            message_tx: tx,
            message_thread,
            is_running,
            state,
        });

        // Set up command listener
        let session_clone = session.clone();
        let pipe_path = format!("/tmp/froggr/sessions/{}.pipe", session_id);
        std::thread::spawn(move || {
            Self::run_command_listener(session_clone, &pipe_path);
        });

        FilesystemManager::set_current_session(session.clone());
        info!("Session started in {} with ID {}", root.display(), session_id);

        Ok(session)
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
        state: Arc<RwLock<SessionState>>,
    ) {
        info!("Message handler started");
        while is_running.load(Ordering::SeqCst) {
            match rx.recv() {
                Ok(message) => {
                    info!("Message handler received: {:?}", message);
                    match message {
                        SessionMessage::Mount { source, target, node_id } => {
                            info!("Processing mount request: {:?} -> {:?} (node: {})", 
                                source, target, node_id);
                            match fs_manager.mount(&source, &target, &node_id) {
                                Ok(_) => {
                                    info!("Mount successful, updating state");
                                    let mut state = state.write();
                                    state.add_mount(source.clone(), target.clone());
                                    
                                    // Update session info file immediately
                                    let session_info = SessionInfo {
                                        id: state.id.clone(),
                                        pid: std::process::id() as i32,
                                        root: state.root.clone(),
                                        mounts: state.mounts.clone(),
                                        binds: state.binds.clone(),
                                    };
                                    
                                    drop(state); // Release the write lock
                                    
                                    if let Ok(session_json) = serde_json::to_string(&session_info) {
                                        let session_file = format!("/tmp/froggr/sessions/{}", session_info.id);
                                        info!("Updating session file: {}", session_file);
                                        if let Err(e) = fs::write(&session_file, session_json) {
                                            error!("Failed to update session file: {}", e);
                                        } else {
                                            info!("Session file updated successfully");
                                        }
                                    }
                                }
                                Err(e) => error!("Mount failed: {}", e),
                            }
                        },
                        SessionMessage::MountSuccess { source, target } => {
                            info!("Processing mount success: {:?} -> {:?}", source, target);
                            let mut state = state.write();
                            state.add_mount(source.clone(), target.clone());
                            info!("Updated state with mount: {:?} -> {:?}", source, target);
                            
                            // Update session info file
                            let session_info = SessionInfo {
                                id: state.id.clone(),
                                pid: std::process::id() as i32,
                                root: state.root.clone(),
                                mounts: state.mounts.clone(),
                                binds: state.binds.clone(),
                            };
                            
                            drop(state); // Release the write lock
                            
                            if let Ok(session_json) = serde_json::to_string(&session_info) {
                                let session_file = format!("/tmp/froggr/sessions/{}", session_info.id);
                                info!("Updating session file: {}", session_file);
                                if let Err(e) = fs::write(&session_file, session_json) {
                                    error!("Failed to update session file: {}", e);
                                } else {
                                    info!("Session file updated successfully");
                                }
                            }
                        },
                        SessionMessage::Bind { source, target, mode } => {
                            info!("Processing bind request: {:?} -> {:?}", source, target);
                            if let Err(e) = fs_manager.bind(&source, &target, mode) {
                                error!("Bind failed: {}", e);
                            } else {
                                info!("Bind successful, updating state");
                                let mut state = state.write();
                                state.add_bind(source.clone(), target.clone());
                                info!("Current binds after update: {:?}", state.binds);
                                
                                // Update session info file
                                let session_info = SessionInfo {
                                    id: state.id.clone(),
                                    pid: std::process::id() as i32,
                                    root: state.root.clone(),
                                    mounts: state.mounts.clone(),
                                    binds: state.binds.clone(),
                                };
                                
                                if let Ok(session_json) = serde_json::to_string(&session_info) {
                                    let session_file = format!("/tmp/froggr/sessions/{}", state.id);
                                    if let Err(e) = fs::write(&session_file, session_json) {
                                        error!("Failed to update session file: {}", e);
                                    } else {
                                        info!("Session file updated successfully");
                                    }
                                }
                            }
                        },
                        SessionMessage::BindSuccess { source, target } => {
                            info!("Processing BindSuccess message");
                            {
                                let mut state = state.write();
                                info!("Adding bind to state: {:?} -> {:?}", source, target);
                                state.add_bind(source.clone(), target.clone());
                                info!("Current binds after update: {:?}", state.binds);
                            }
                            
                            // Update session info file
                            let state = state.read();
                            let session_info = SessionInfo {
                                id: state.id.clone(),
                                pid: std::process::id() as i32,
                                root: state.root.clone(),
                                mounts: state.mounts.clone(),
                                binds: state.binds.clone(),
                            };
                            
                            info!("Updating session file");
                            if let Ok(session_json) = serde_json::to_string(&session_info) {
                                let session_file = format!("/tmp/froggr/sessions/{}", state.id);
                                if let Err(e) = fs::write(&session_file, session_json) {
                                    error!("Failed to update session info: {}", e);
                                } else {
                                    info!("Session info updated successfully");
                                }
                            }
                        },
                        SessionMessage::Unmount { path } => {
                            info!("Processing unmount request: {:?}", path);
                            if let Err(e) = fs_manager.unmount(&path, None) {
                                error!("Unmount failed: {}", e);
                            } else {
                                info!("Unmount successful, updating state");
                                let mut state = state.write();
                                state.remove_mount(&path);
                                info!("Current mounts after update: {:?}", state.mounts);
                                
                                // Update session info file
                                let session_info = SessionInfo {
                                    id: state.id.clone(),
                                    pid: std::process::id() as i32,
                                    root: state.root.clone(),
                                    mounts: state.mounts.clone(),
                                    binds: state.binds.clone(),
                                };
                                
                                if let Ok(session_json) = serde_json::to_string(&session_info) {
                                    let session_file = format!("/tmp/froggr/sessions/{}", state.id);
                                    if let Err(e) = fs::write(&session_file, session_json) {
                                        error!("Failed to update session file: {}", e);
                                    } else {
                                        info!("Session file updated successfully");
                                    }
                                }
                            }
                        },
                        SessionMessage::Shutdown => {
                            info!("Received shutdown message");
                            break;
                        }
                    }
                },
                Err(e) => {
                    error!("Message channel error: {}", e);
                    break;
                }
            }
        }
        info!("Message handler terminated");
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
    pub fn shutdown(&self) -> Result<()> {
        info!("Shutting down session...");
        self.is_running.store(false, Ordering::SeqCst);
        
        // Send shutdown message
        self.message_tx.send(SessionMessage::Shutdown)?;
        
        // Clean up session file
        let session_file = format!("/tmp/froggr/sessions/{}", self.state.read().id);
        if let Err(e) = fs::remove_file(&session_file) {
            warn!("Failed to remove session file: {}", e);
        }
        
        Ok(())
    }

    /// Runs the session in a loop until a shutdown signal is received.
    ///
    /// # Returns
    /// * `Ok(())` if the session shuts down cleanly
    /// * `Err` if an error occurs during session execution
    pub async fn run(&self) -> Result<()> {
        info!("Session running. Waiting for shutdown signal...");
        
        // Wait for shutdown signal
        ctrl_c().await?;
        info!("Received shutdown signal");
        
        self.shutdown()?;
        info!("Session terminated");
        
        Ok(())
    }

    /// Get current bind
    pub fn get_current_bind(&self) -> Option<(PathBuf, PathBuf)> {
        // Get the current bind from the session state
        let state = self.state.read();
        state.binds.last().cloned()
    }

    /// Notify of successful bind
    pub fn notify_bind_success(&self, source: PathBuf, target: PathBuf) -> Result<()> {
        info!("Notifying bind success: {:?} -> {:?}", source, target);
        {
            let mut state = self.state.write();
            state.add_bind(source.clone(), target.clone());
            info!("State updated, current binds: {:?}", state.binds);
        }
        
        // Also send through message channel for consistency
        self.message_tx.send(SessionMessage::BindSuccess { 
            source, 
            target 
        })?;
        
        info!("Bind success notification sent");
        Ok(())
    }

    /// Runs a listener for commands sent through the named pipe
    fn run_command_listener(session: Arc<Session>, pipe_path: &str) {
        info!("Starting command listener for pipe {}", pipe_path);
        loop {
            match fs::read_to_string(pipe_path) {
                Ok(command_str) => {
                    info!("Received command string: {}", command_str);
                    match serde_json::from_str::<SessionCommand>(&command_str) {
                        Ok(command) => {
                            info!("Parsed command: {:?}", command);
                            match command {
                                SessionCommand::Mount { source, target, node_id } => {
                                    info!("Processing mount command: {:?} -> {:?}", source, target);
                                    match session.fs_manager.mount(&source, &target, &node_id) {
                                        Ok(_) => {
                                            info!("Mount operation successful, notifying session");
                                            if let Err(e) = session.notify_mount_success(source.clone(), target.clone()) {
                                                error!("Failed to notify mount success: {}", e);
                                            }
                                        }
                                        Err(e) => error!("Mount operation failed: {}", e),
                                    }
                                }
                                SessionCommand::Bind { source, target, mode } => {
                                    info!("Processing bind command: {:?} -> {:?}", source, target);
                                    match session.fs_manager.bind(&source, &target, mode) {
                                        Ok(_) => {
                                            info!("Bind operation successful, updating session state");
                                            // Directly update session state here
                                            if let Err(e) = session.notify_bind_success(source.clone(), target.clone()) {
                                                error!("Failed to update session state: {}", e);
                                            }
                                            
                                            // Debug: Print current state
                                            let state = session.state.read();
                                            info!("Current binds after update: {:?}", state.binds);
                                            
                                            // Force update of session file
                                            let session_info = SessionInfo {
                                                id: state.id.clone(),
                                                pid: std::process::id() as i32,
                                                root: state.root.clone(),
                                                mounts: state.mounts.clone(),
                                                binds: state.binds.clone(),
                                            };
                                            
                                            if let Ok(session_json) = serde_json::to_string(&session_info) {
                                                let session_file = format!("/tmp/froggr/sessions/{}", state.id);
                                                if let Err(e) = fs::write(&session_file, session_json) {
                                                    error!("Failed to update session file: {}", e);
                                                } else {
                                                    info!("Session file updated successfully");
                                                }
                                            }
                                        }
                                        Err(e) => error!("Bind operation failed: {}", e),
                                    }
                                }
                            }
                        }
                        Err(e) => error!("Failed to parse command: {}", e),
                    }
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::WouldBlock {
                        error!("Error reading from pipe: {}", e);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    /// Notifies the session of a successful mount operation and updates the session state.
    ///
    /// # Arguments
    /// * `source` - The source path that was mounted
    /// * `target` - The target path where the source was mounted
    ///
    /// # Returns
    /// * `Ok(())` if the notification was successful
    /// * `Err` if the state update or notification failed
    pub fn notify_mount_success(&self, source: PathBuf, target: PathBuf) -> Result<()> {
        info!("Notifying mount success: {:?} -> {:?}", source, target);
        {
            let mut state = self.state.write();
            state.add_mount(source.clone(), target.clone());
            info!("State updated, current mounts: {:?}", state.mounts);
        }
        
        // Also send through message channel for consistency
        self.message_tx.send(SessionMessage::MountSuccess { 
            source, 
            target 
        })?;
        
        info!("Mount success notification sent");
        Ok(())
    }

    /// Sends a mount request message to the session.
    ///
    /// # Arguments
    /// * `source` - Source path to mount from
    /// * `target` - Target path to mount to
    /// * `node_id` - Node identifier for the mount
    ///
    /// # Returns
    /// * `Ok(())` if the message was sent successfully
    /// * `Err` if the message could not be sent
    pub fn mount(&self, source: PathBuf, target: PathBuf, node_id: String) -> Result<()> {
        info!("Sending mount message to session");
        self.message_tx.send(SessionMessage::Mount {
            source: source.clone(),
            target: target.clone(),
            node_id,
        })?;
        info!("Mount message sent successfully");
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
struct SessionState {
    id: String,
    root: PathBuf,
    mounts: Vec<(PathBuf, PathBuf)>,
    binds: Vec<(PathBuf, PathBuf)>,
}

impl SessionState {
    fn load<P: AsRef<Path>>(root: P, id: String) -> Result<Self> {
        Ok(SessionState {
            id,
            root: root.as_ref().to_path_buf(),
            mounts: Vec::new(),
            binds: Vec::new(),
        })
    }

    fn add_mount(&mut self, source: PathBuf, target: PathBuf) {
        info!("Adding mount to state: {:?} -> {:?}", source, target);
        // Remove any existing mount for this target
        self.mounts.retain(|(_, t)| t != &target);
        // Add the new mount
        self.mounts.push((source, target));
        info!("Current mounts after update: {:?}", self.mounts);
    }

    fn remove_mount(&mut self, path: &Path) {
        info!("Removing mount for path: {:?}", path);
        self.mounts.retain(|(_, target)| target != path);
        info!("Current mounts after removal: {:?}", self.mounts);
    }

    fn add_bind(&mut self, source: PathBuf, target: PathBuf) {
        info!("Adding bind to state: {:?} -> {:?}", source, target);
        self.binds.push((source, target));
        info!("Current binds after update: {:?}", self.binds);
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum SessionCommand {
    Bind {
        source: PathBuf,
        target: PathBuf,
        mode: BindMode,
    },
    Mount {
        source: PathBuf,
        target: PathBuf,
        node_id: String,
    },
    // Add other commands as needed
}
