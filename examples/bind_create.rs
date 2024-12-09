mod common;

use anyhow::Result;
use common::setup_directories;
use filesystem_manager::modules::namespace::BindMode;
use filesystem_manager::FilesystemManager;
use filesystem_manager::NineP;
use std::fs;
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    // Create necessary directories
    let mount_point = Path::new("/tmp/mnt/ninep");

    common::setup_directories(&mount_point)?;

    // Create NineP filesystem with /tmp/target as root
    let hello_fs = NineP::new(PathBuf::from("/tmp/target"))?;
    let fs_mngr = FilesystemManager::new(hello_fs);

    // BindMode::Create
    println!("Binding with BindMode::Create");
    fs_mngr.bind(
        Path::new("/tmp/source"),
        Path::new("/tmp/target"),
        BindMode::Create,
    )?;
    fs_mngr.mount(Path::new("/tmp/target"), mount_point, "remote_node_123")?;
    println!("Mount complete");
    // The contents of /tmp/target will be the contents of /tmp/source, but all files/directories will be read-only

    Ok(())
}
