use anyhow::Result;
use filesystem_manager::modules::namespace::BindMode;
use filesystem_manager::FilesystemManager;
use filesystem_manager::NineP;
use std::fs;
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    // Create necessary directories first
    let mount_point = Path::new("/tmp/mnt/ninep");
    fs::create_dir_all(mount_point)?;
    fs::create_dir_all("/tmp/test")?;
    fs::create_dir_all("/tmp/test2")?;

    // Add some test files
    fs::write("/tmp/test2/file1.txt", "test1")?;
    fs::write("/tmp/test2/file2.txt", "test2")?;
    println!("Created test files");
    println!("Contents of /tmp/test2:");
    for entry in fs::read_dir("/tmp/test2")? {
        let entry = entry?;
        println!("{:?}", entry.path());
    }

    // Create NineP filesystem with /tmp/test as root
    let hello_fs = NineP::new(PathBuf::from("/tmp/test"))?;
    let fs_mngr = FilesystemManager::new(hello_fs);

    println!("About to bind directories");
    // Perform bind operation - swap source and target
    fs_mngr.bind(
        Path::new("/tmp/test2"), // This should be source
        Path::new("/tmp/test"),  // This should be target
        BindMode::Before,
    )?;

    println!("Directory bound, about to mount");
    // Mount the NineP filesystem to /tmp/mnt/ninep
    fs_mngr.mount(Path::new("/tmp/test"), mount_point, "remote_node_123")?;

    println!("Mount complete");
    Ok(())
}
