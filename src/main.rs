use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use filesystem_manager::modules::namespace::BindMode;
use filesystem_manager::FilesystemManager;
use filesystem_manager::NineP;

#[tokio::main]
async fn main() -> Result<()> {
    // Create HelloFS with /tmp as root
    let hello_fs = NineP::new(PathBuf::from("/tmp"))?;
    let fs_mngr = FilesystemManager::new(hello_fs);

    // Create necessary directories
    fs::create_dir_all("/tmp/source")?;
    fs::create_dir_all("/tmp/target")?;
    fs::create_dir_all("/tmp/mount_point")?;

    // Perform bind operation
    fs_mngr.bind(
        Path::new("/tmp/test"),
        Path::new("/tmp/test2"),
        BindMode::Replace,
    )?;

    // Perform mount operation
    fs_mngr.mount(
        Path::new("/tmp/test"),
        Path::new("/tmp/test2"),
        "remote_node_123",
    )?;

    Ok(())
}
