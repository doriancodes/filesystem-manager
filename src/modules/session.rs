use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread::{self, JoinHandle};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use anyhow::Result;
use log::{info, error};
use std::path::{Path, PathBuf};
use crate::FilesystemManager;

#[derive(Debug)]
enum SessionMessage {
    Mount {
        source: PathBuf,
        target: PathBuf,
        node_id: String,
    },
    Bind {
        source: PathBuf,
        target: PathBuf,
        mode: crate::modules::namespace::BindMode,
    },
    Unmount {
        path: PathBuf,
    },
    Shutdown,
}

/// A filesystem state manager that handles mounting and binding operations.
pub struct Session {
    pub fs_manager: FilesystemManager,
    message_tx: Sender<SessionMessage>,
    message_thread: JoinHandle<()>,
    is_running: Arc<AtomicBool>,
}

impl Session {
    /// Creates a new Session with the specified root directory.
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

    fn run_message_handler(
        rx: Receiver<SessionMessage>, 
        is_running: Arc<AtomicBool>,
        fs_manager: FilesystemManager,
    ) {
        while is_running.load(Ordering::SeqCst) {
            match rx.recv() {
                Ok(message) => match message {
                    SessionMessage::Mount { source, target, node_id } => {
                        info!("Processing mount request: {:?} -> {:?} (node: {})", 
                            source, target, node_id);
                        if let Err(e) = fs_manager.mount(&source, &target, &node_id) {
                            error!("Mount failed: {}", e);
                        }
                    },
                    SessionMessage::Bind { source, target, mode } => {
                        info!("Processing bind request: {:?} -> {:?}", source, target);
                        if let Err(e) = fs_manager.bind(&source, &target, mode) {
                            error!("Bind failed: {}", e);
                        }
                    },
                    SessionMessage::Unmount { path } => {
                        info!("Processing unmount request: {:?}", path);
                        if let Err(e) = fs_manager.unmount(&path, None) {
                            error!("Unmount failed: {}", e);
                        }
                    },
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

    /// Mount a filesystem at the specified path
    pub fn mount(&self, source: &Path, target: &Path, node_id: &str) -> Result<()> {
        self.message_tx.send(SessionMessage::Mount {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            node_id: node_id.to_string(),
        })?;
        Ok(())
    }

    /// Bind a source path to a target path
    pub fn bind(&self, source: &Path, target: &Path, mode: crate::modules::namespace::BindMode) -> Result<()> {
        self.message_tx.send(SessionMessage::Bind {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            mode,
        })?;
        Ok(())
    }

    /// Unmount a filesystem at the specified path
    pub fn unmount(&self, path: &Path) -> Result<()> {
        self.message_tx.send(SessionMessage::Unmount {
            path: path.to_path_buf(),
        })?;
        Ok(())
    }

    /// Shutdown the session
    pub fn shutdown(mut self) -> Result<()> {
        info!("Shutting down session");
        self.message_tx.send(SessionMessage::Shutdown)?;
        let thread = std::mem::replace(&mut self.message_thread, thread::spawn(|| {}));
        thread.join().map_err(|_| anyhow::anyhow!("Failed to join message thread"))?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Err(e) = self.message_tx.send(SessionMessage::Shutdown) {
            error!("Error sending shutdown message: {}", e);
        }
    }
}
