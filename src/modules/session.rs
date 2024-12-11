use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread::{self, JoinHandle};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use anyhow::Result;
use log::{info, error};
use std::path::{Path, PathBuf};
use std::fs;
use libc::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use anyhow::anyhow;

#[derive(Debug)]
enum DaemonMessage {
    Mount {
        source: PathBuf,
        target: PathBuf,
        node_id: String,
    },
    Unmount {
        path: PathBuf,
    },
    Shutdown,
}

/// A filesystem session manager that handles communication with a background daemon.
///
/// The Session struct manages a background daemon process that handles filesystem
/// operations. It provides a clean interface for:
/// - Mounting and unmounting filesystems
/// - Managing daemon lifecycle
/// - Handling inter-process communication
pub struct Session {
    root: PathBuf,
    daemon_tx: Sender<DaemonMessage>,
    daemon_thread: JoinHandle<()>,
    is_running: Arc<AtomicBool>,
}

impl Session {
    /// Creates a new Session with the specified root directory.
    ///
    /// # Arguments
    ///
    /// * `root` - The root directory path for this session
    ///
    /// # Returns
    ///
    /// A Result containing the new Session instance or an error if initialization fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use froggr::modules::session::Session;
    ///
    /// let session = Session::new(Path::new("/tmp/session")).unwrap();
    /// ```
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let (tx, rx) = channel();
        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = is_running.clone();

        // Start the daemon thread
        let daemon_thread = thread::spawn(move || {
            Self::run_daemon(rx, is_running_clone);
        });

        info!("Session daemon started in {}", root.display());

        Ok(Self {
            root,
            daemon_tx: tx,
            daemon_thread,
            is_running,
        })
    }

    fn run_daemon(rx: Receiver<DaemonMessage>, is_running: Arc<AtomicBool>) {
        let mut signals = Signals::new(&[SIGINT, SIGTERM]).unwrap();
        let signal_handler = thread::spawn(move || {
            for sig in signals.forever() {
                match sig {
                    SIGINT | SIGTERM => {
                        info!("Daemon received shutdown signal");
                        return;
                    }
                    _ => {}
                }
            }
        });

        while is_running.load(Ordering::SeqCst) {
            match rx.recv() {
                Ok(message) => match message {
                    DaemonMessage::Mount { source, target, node_id } => {
                        info!("Daemon received mount request: {:?} -> {:?} (node: {})", 
                            source, target, node_id);
                    }
                    DaemonMessage::Unmount { path } => {
                        info!("Daemon received unmount request: {:?}", path);
                    }
                    DaemonMessage::Shutdown => {
                        info!("Daemon received shutdown message");
                        break;
                    }
                },
                Err(e) => {
                    error!("Daemon channel error: {}", e);
                    break;
                }
            }
        }

        is_running.store(false, Ordering::SeqCst);
        signal_handler.join().unwrap();
        info!("Session daemon stopped");
    }

    /// Requests the daemon to mount a filesystem.
    ///
    /// # Arguments
    ///
    /// * `source` - The source directory to mount
    /// * `mount_point` - Where to mount the filesystem
    /// * `node_id` - Identifier for the node (usually hostname)
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure of sending the mount request
    pub fn mount(&self, source: &Path, mount_point: &Path, node_id: &str) -> Result<()> {
        info!("Sending mount request to daemon");
        self.daemon_tx.send(DaemonMessage::Mount {
            source: source.to_path_buf(),
            target: mount_point.to_path_buf(),
            node_id: node_id.to_string(),
        })?;
        Ok(())
    }

    /// Requests the daemon to unmount a filesystem.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to unmount
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure of sending the unmount request
    pub fn unmount(&self, path: &Path) -> Result<()> {
        let abs_path = fs::canonicalize(path)?;
        
        info!("Sending unmount request to daemon");
        self.daemon_tx.send(DaemonMessage::Unmount {
            path: abs_path,
        })?;
        Ok(())
    }

    /// Shuts down the session and its daemon process.
    ///
    /// This method:
    /// 1. Sends a shutdown message to the daemon
    /// 2. Waits for the daemon thread to complete
    /// 3. Cleans up resources
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure of the shutdown process
    pub fn shutdown(mut self) -> Result<()> {
        info!("Shutting down session");
        self.daemon_tx.send(DaemonMessage::Shutdown)?;
        if let Ok(thread) = std::mem::replace(&mut self.daemon_thread, thread::spawn(|| {})).join() {
            Ok(())
        } else {
            Err(anyhow!("Failed to join daemon thread"))
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Err(e) = self.daemon_tx.send(DaemonMessage::Shutdown) {
            error!("Error sending shutdown message: {}", e);
        }
    }
}
