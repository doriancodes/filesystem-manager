use anyhow::Result;
use clap::{Parser, Subcommand};
use frogger::modules::namespace::BindMode;
use frogger::FilesystemManager;
use frogger::NineP;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

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

            let hello_fs = NineP::new(target.clone())?;
            let fs_mngr = FilesystemManager::new(hello_fs);

            fs_mngr.bind(source.as_path(), target.as_path(), bind_mode)?;
            println!(
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
            println!(
                "Successfully mounted {} to {}",
                source.display(),
                mount_point.display()
            );
        }
    }

    Ok(())
}
