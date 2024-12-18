use anyhow::Result;
use clap::{Parser, Subcommand};
use froggr::modules::namespace::BindMode;
use froggr::modules::session::SessionManager;
use log::{debug, error, info};
use std::path::PathBuf;
use env_logger;

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
    /// Manage filesystem sessions
    Session {
        /// List all active sessions
        #[arg(short = 'l', long = "list")]
        list: bool,
        /// Kill a specific session
        #[arg(short = 'k', long = "kill")]
        kill: bool,
        /// Kill all active sessions
        #[arg(short = 'p', long = "purge")]
        purge: bool,
        /// Session ID (required for kill and show operations)
        session_id: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the logger at the start of main
    env_logger::init();
    
    info!("Froggr starting up");
    
    let cli = Cli::parse();
    
    if cli.verbose {
        debug!("Verbose mode enabled");
    }
    
    let session_manager = SessionManager::new()?;

    match &cli.command {
        Commands::Bind { before, after, replace, create, source, target } => {
            info!("Starting bind operation in process {}", std::process::id());
            let mode = match (before, after, replace, create) {
                (_, _, true, _) => BindMode::Replace,
                (_, _, _, true) => BindMode::Create,
                (_, true, _, _) => BindMode::After,
                _ => BindMode::Before,
            };

            let session_manager = SessionManager::new()?;
            let session_id = session_manager.create_session(target.clone())?;
            println!("Created new session: {}", session_id);

            if let Some(session) = session_manager.get_session(&session_id)? {
                info!("Found session with PID {}", session.pid);
                session_manager.send_bind_command(&session_id, source.clone(), target.clone(), mode)?;
                info!("Sent bind command to session");
            } else {
                error!("No session found for bind operation");
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        Commands::Mount { source, mount_point, node_id } => {
            info!("Starting mount operation in process {}", std::process::id());
            let session_manager = SessionManager::new()?;
            info!("Created session manager");
            
            let session_id = session_manager.create_session(mount_point.clone())?;
            info!("Created session: {}", session_id);
            println!("Created new session: {}", session_id);

            if let Some(session) = session_manager.get_session(&session_id)? {
                info!("Found session with PID {}", session.pid);
                info!("Sending mount command...");
                session_manager.send_mount_command(
                    &session_id,
                    source.clone(),
                    mount_point.clone(),
                    node_id.clone()
                )?;
                info!("Mount command sent to session");
            } else {
                error!("No session found for mount operation");
            }

            info!("Waiting for mount operation to complete");
            std::thread::sleep(std::time::Duration::from_secs(1));
            info!("Mount operation completed");
        }
        Commands::Session { list, kill, purge, session_id } => {
            if *list {
                let sessions = session_manager.list_sessions()?;
                println!("Active sessions:");
                for session in sessions {
                    println!("ID: {}", session.id);
                    println!("  PID: {}", session.pid);
                    println!("  Root: {}", session.root.display());
                    println!();
                }
            } else if *kill {
                if let Some(id) = session_id {
                    session_manager.kill_session(id)?;
                    println!("Session {} terminated", id);
                } else {
                    println!("Session ID required for kill operation");
                }
            } else if *purge {
                let killed = session_manager.purge_sessions()?;
                println!("Purged {} sessions", killed);
            } else if let Some(id) = session_id {
                if let Some(session) = session_manager.get_session(id)? {
                    println!("Session Details:");
                    println!("ID: {}", session.id);
                    println!("PID: {}", session.pid);
                    println!("Root: {}", session.root.display());
                    println!("\nMounts:");
                    for (source, target) in &session.mounts {
                        println!("  {} -> {}", source.display(), target.display());
                    }
                    println!("\nBinds:");
                    for (source, target) in &session.binds {
                        println!("  {} -> {}", source.display(), target.display());
                    }
                } else {
                    println!("Session not found: {}", id);
                }
            }
        }
    }

    Ok(())
}
