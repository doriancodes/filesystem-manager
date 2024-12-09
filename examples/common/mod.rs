use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn setup_directories(mount_point: &Path) -> Result<()> {
    // Create necessary directories
    fs::create_dir_all(mount_point)?;
    fs::create_dir_all("/tmp/source")?;
    fs::create_dir_all("/tmp/target")?;

    // Add some test files
    fs::write("/tmp/source/file1.txt", "test1")?;
    fs::write("/tmp/source/file2.txt", "test2")?;
    fs::write("/tmp/target/file3.txt", "test3")?;

    Ok(())
}