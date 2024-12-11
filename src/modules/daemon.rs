use std::os::unix::io::AsRawFd;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use anyhow::Result;
use log::{error, info};
use nix::sys::stat;
use nix::unistd::{self, fork, ForkResult};

/// A Unix daemon process manager.
/// 
/// Handles the creation and management of background processes (daemons)
/// including process detachment, file descriptor cleanup, and PID file management.
pub struct Daemon {
    pid_file: String,
    work_dir: String,
}

impl Daemon {
    /// Creates a new daemon instance.
    /// 
    /// # Arguments
    /// 
    /// * `pid_file` - Path to the PID file where the daemon's process ID will be written
    /// * `work_dir` - Working directory for the daemon process
    /// 
    /// # Returns
    /// 
    /// A new `Daemon` instance configured with the specified parameters
    pub fn new(pid_file: String, work_dir: String) -> Self {
        Self { pid_file, work_dir }
    }

    /// Starts the daemon process.
    /// 
    /// This method:
    /// 1. Performs the double-fork to create a daemon process
    /// 2. Sets up the daemon environment (working directory, file descriptors)
    /// 3. Creates the PID file
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the daemon was successfully started
    /// * `Err` if any step of the daemon creation process failed
    pub fn start(&self) -> Result<()> {
        // First fork: create background process
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child: _ }) => {
                std::process::exit(0);
            }
            Ok(ForkResult::Child) => {
                // Create new session
                unistd::setsid()?;

                // Second fork: prevent reacquiring terminal
                match unsafe { fork() } {
                    Ok(ForkResult::Parent { child: _ }) => {
                        std::process::exit(0);
                    }
                    Ok(ForkResult::Child) => {
                        // Set file creation mask
                        stat::umask(stat::Mode::empty());

                        // Change working directory
                        std::env::set_current_dir(&self.work_dir)?;

                        // Close standard file descriptors
                        self.close_file_descriptors()?;

                        // Write PID file
                        self.write_pid_file()?;

                        info!("Daemon started successfully");
                        Ok(())
                    }
                    Err(err) => {
                        error!("Second fork failed: {}", err);
                        Err(err.into())
                    }
                }
            }
            Err(err) => {
                error!("First fork failed: {}", err);
                Err(err.into())
            }
        }
    }

    fn write_pid_file(&self) -> Result<()> {
        let pid = std::process::id();
        let mut file = File::create(&self.pid_file)?;
        writeln!(file, "{}", pid)?;
        Ok(())
    }

    fn close_file_descriptors(&self) -> Result<()> {
        // Redirect standard file descriptors to /dev/null
        let null_file = File::open("/dev/null")?;
        let null_fd = null_file.as_raw_fd();
        
        for fd in 0..3 {
            unistd::dup2(null_fd, fd)?;
        }
        Ok(())
    }
} 