use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use froggr::modules::namespace::BindMode;
use froggr::modules::session::SessionManager;
use froggr::FilesystemManager;
use log::{error, info};
use std::path::PathBuf;
use std::str::FromStr;

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
    /// Unmount a filesystem
    Unmount {
        /// Path to unmount
        #[clap(value_parser)]
        mount_point: PathBuf,

        /// Force unmount even if busy
        #[clap(short, long)]
        force: bool,

        /// Enable verbose output
        #[clap(short, long)]
        verbose: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
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
            info!("Starting mount operation");
            let session_id = session_manager.create_session(mount_point.clone())?;
            println!("Created new session: {}", session_id);

            if let Some(session_info) = session_manager.get_session(&session_id)? {
                info!("Found session with PID {}", session_info.pid);
                session_manager.send_mount_command(
                    &session_id,
                    source.clone(),
                    mount_point.clone(),
                    node_id.clone()
                )?;
                info!("Mount command sent to session");
                
                std::thread::sleep(std::time::Duration::from_secs(1));
                
                if let Some(updated_info) = session_manager.get_session(&session_id)? {
                    info!("Current mounts: {:?}", updated_info.mounts);
                }
            } else {
                error!("No session found for mount operation");
            }
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
        Commands::Unmount { mount_point, force, verbose } => {
            info!("Starting unmount operation for {}", mount_point.display());
            
            if let Some(session) = session_manager.find_session_for_mount(&mount_point)? {
                info!("Found session {} managing mount point", session.id);
                
                session_manager.send_unmount_command(
                    &session.id,
                    mount_point.clone(),
                    *force,
                )?;
                
                println!("Unmount request sent successfully");
                if *verbose {
                    println!("Session ID: {}", session.id);
                    println!("Mount point: {}", mount_point.display());
                }
            } else {
                error!("No session found managing mount point: {}", mount_point.display());
                return Err(anyhow!("Mount point not found in any active session"));
            }
        }
    }

    Ok(())
}
