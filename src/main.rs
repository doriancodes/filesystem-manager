use anyhow::Result;
use clap::{Parser, Subcommand};
use env_logger;
use froggr::modules::namespace::BindMode;
use froggr::modules::session::{Session, SessionManager};
use froggr::{FilesystemManager, NineP};
use log::{error, info, LevelFilter};
use std::env;
use std::path::PathBuf;
use tokio::signal;
use uuid::Uuid;

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
    /// List all active sessions
    Sessions,
    /// Kill a specific session
    Kill {
        /// Session ID to kill
        session_id: String,
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

    env_logger::Builder::new().filter_level(log_level).init();

    info!("Starting froggr...");

    let session_manager = SessionManager::new()?;

    match &cli.command {
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

            let session_id = session_manager.create_session(target.clone())?;
            // Initialize binding in the new session
            println!("Created new session: {}", session_id);
        }
        Commands::Mount {
            source,
            mount_point,
            node_id,
        } => {
            let session_id = session_manager.create_session(source.clone())?;
            // Initialize mounting in the new session
            println!("Created new session: {}", session_id);
        }
        Commands::Sessions => {
            let sessions = session_manager.list_sessions()?;
            println!("Active sessions:");
            for session in sessions {
                println!("ID: {}", session.id);
                println!("  PID: {}", session.pid);
                println!("  Root: {}", session.root.display());
                println!("  Mounts: {:?}", session.mounts);
                println!("  Binds: {:?}", session.binds);
                println!();
            }
        }
        Commands::Kill { session_id } => {
            session_manager.kill_session(session_id)?;
            println!("Session {} terminated", session_id);
        }
    }

    Ok(())
}
