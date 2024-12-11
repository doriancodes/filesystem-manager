use anyhow::Result;
use clap::{Parser, Subcommand};
use env_logger;
use froggr::modules::namespace::BindMode;
use froggr::modules::session::Session;
use froggr::{FilesystemManager, NineP};
use log::{info, error, LevelFilter};
use std::path::PathBuf;
use std::env;
use tokio::signal;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Bind a source directory to a target directory
    Bind {
        /// Bind before (default if no mode specified)
        #[arg(short = 'b', long = "before", group = "bind_mode")]
        before: bool,
        /// Bind after
        #[arg(short = 'a', long = "after", group = "bind_mode")]
        after: bool,
        /// Replace existing binding
        #[arg(short = 'r', long = "replace", group = "bind_mode")]
        replace: bool,
        /// Create new binding
        #[arg(short = 'c', long = "create", group = "bind_mode")]
        create: bool,
        /// Source directory path
        source: PathBuf,
        /// Target directory path
        target: PathBuf,
    },
    /// Mount a directory to a mount point
    Mount {
        /// Directory to mount
        source: PathBuf,
        /// Mount point
        mount_point: PathBuf,
        /// Node ID (optional, defaults to localhost)
        #[arg(default_value = "localhost")]
        node_id: String,
    },
    /// Start a new session daemon
    Session {
        /// Root directory for the session (defaults to current directory)
        #[arg(short = 'r', long = "root")]
        root: Option<PathBuf>,
        /// PID file location (defaults to /tmp/froggr.pid, or /var/run/froggr.pid if running as root)
        #[arg(long)]
        pid_file: Option<PathBuf>,
        /// Run with elevated privileges (stores PID in /var/run)
        #[arg(long)]
        privileged: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logger based on verbose flag
    let log_level = if cli.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    env_logger::Builder::new()
        .filter_level(log_level)
        .init();
    
    info!("Starting froggr...");

    match &cli.command {
        Commands::Session { root, pid_file, privileged } => {
            info!("Initializing session...");
            
            // Use provided root or current directory
            let root_path = match root {
                Some(path) => path.clone(),
                None => env::current_dir()?,
            };
            
            // Determine PID file location
            let pid_path = match (pid_file, privileged) {
                (Some(path), false) => path.clone(),
                (Some(path), true) => {
                    if !nix::unistd::Uid::effective().is_root() {
                        return Err(anyhow::anyhow!("Privileged mode requires root permissions"));
                    }
                    path.clone()
                },
                (None, true) => {
                    if !nix::unistd::Uid::effective().is_root() {
                        return Err(anyhow::anyhow!("Privileged mode requires root permissions"));
                    }
                    PathBuf::from("/var/run/froggr.pid")
                },
                (None, false) => PathBuf::from("/tmp/froggr.pid"),
            };
            
            info!("Using root directory: {}", root_path.display());
            
            // Start a new session
            let session = Session::new(&root_path)?;
            info!("Session started with root directory: {}", root_path.display());

            // Write PID file
            let pid = std::process::id().to_string().into_bytes();
            std::fs::write(&pid_path, pid)?;
            info!("PID file written to: {}", pid_path.display());

            info!("Session running. Press Ctrl+C to stop.");

            // Wait for shutdown signal
            signal::ctrl_c().await?;
            info!("Received shutdown signal");

            // Clean shutdown
            session.shutdown()?;
            
            // Cleanup PID file
            if pid_path.exists() {
                std::fs::remove_file(pid_path)?;
            }
            
            info!("Session terminated");
        }
        Commands::Bind {
            before,
            after,
            replace,
            create,
            source,
            target,
        } => {
            let bind_mode = match (before, after, replace, create) {
                (_, _, true, _) => BindMode::Replace,
                (_, _, _, true) => BindMode::Create,
                (_, true, _, _) => BindMode::After,
                _ => BindMode::Before,
            };

            let hello_fs = NineP::new(target.clone())?;
            let fs_mngr = FilesystemManager::new(hello_fs);

            fs_mngr.bind(source.as_path(), target.as_path(), bind_mode)?;
            info!(
                "Successfully bound {} to {}",
                source.display(),
                target.display()
            );
        }
        Commands::Mount {
            source,
            mount_point,
            node_id,
        } => {
            let hello_fs = NineP::new(source.clone())?;
            let fs_mngr = FilesystemManager::new(hello_fs);

            fs_mngr.mount(&source.as_path(), &mount_point.as_path(), &node_id)?;
            info!(
                "Successfully mounted {} to {}",
                source.display(),
                mount_point.display()
            );
        }
    }

    Ok(())
}
