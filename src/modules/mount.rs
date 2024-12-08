use super::constants::BLOCK_SIZE;
use super::filesystem::{BoundEntry, HelloFS};
use super::namespace::{BindMode, NamespaceEntry};
use anyhow::{anyhow, Result};
use fuser::{FileAttr, FileType};
use libc::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::ffi::CString;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::UNIX_EPOCH;

#[cfg(target_os = "macos")]
extern "C" {
    pub fn unmount(path: *const i8, flags: i32) -> i32;
}

#[cfg(target_os = "linux")]
extern "C" {
    pub fn umount(path: *const i8) -> i32;
}

pub struct FilesystemManager {
    pub fs: HelloFS,
}

impl FilesystemManager {
    pub fn new(fs: HelloFS) -> Self {
        Self { fs }
    }

    pub fn bind_directory(&self, dir_path: &str) -> Result<()> {
        let entries = fs::read_dir(dir_path)?;
        let mut bindings = self.fs.namespace_manager.bindings.lock().unwrap();
        let mut next_inode = self.fs.namespace_manager.next_inode.lock().unwrap();

        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = entry.metadata().unwrap();
            let file_name = entry.file_name();

            println!("Binding file: {:?}, inode: {}", path, *next_inode); // DEBUG

            let inode = *next_inode;
            *next_inode += 1;

            let file_attr = FileAttr {
                ino: inode,
                size: metadata.len(),
                blocks: (metadata.len() + BLOCK_SIZE - 1) / BLOCK_SIZE,
                atime: metadata.accessed().unwrap_or(UNIX_EPOCH),
                mtime: metadata.modified().unwrap_or(UNIX_EPOCH),
                ctime: UNIX_EPOCH,
                crtime: UNIX_EPOCH,
                kind: if metadata.is_dir() {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                },
                perm: 0o755,
                nlink: 1,
                uid: 501,
                gid: 20,
                rdev: 0,
                flags: 0,
                blksize: 512,
            };

            bindings.insert(
                inode,
                (
                    file_name.clone(),
                    BoundEntry {
                        attr: file_attr,
                        content: if metadata.is_file() {
                            Some(fs::read(path).unwrap_or_default())
                        } else {
                            None
                        },
                    },
                ),
            );
        }

        Ok(())
    }

    pub fn bind(&self, source: &Path, target: &Path, mode: BindMode) -> Result<()> {
        let abs_source = fs::canonicalize(source)?;
        let abs_target = fs::canonicalize(target)?;

        if !abs_source.exists() {
            return Err(anyhow!("Source path does not exist"));
        }

        if !abs_target.exists() {
            return Err(anyhow!("Target path does not exist"));
        }

        let entry = NamespaceEntry {
            source: abs_source,
            target: abs_target.clone(),
            bind_mode: mode,
            remote_node: None,
        };

        let mut namespace = self.fs.namespace_manager.namespace.write().unwrap();
        namespace
            .entry(abs_target.clone())
            .or_insert_with(Vec::new)
            .push(entry);

        self.bind_directory(abs_target.to_str().unwrap())?;

        Ok(())
    }

    pub fn mount(&self, remote_path: &Path, local_path: &Path, remote_node: &str) -> Result<()> {
        let abs_local = fs::canonicalize(local_path)?;

        if !abs_local.exists() {
            return Err(anyhow!("Local mount point does not exist"));
        }

        let entry = NamespaceEntry {
            source: remote_path.to_path_buf(),
            target: abs_local.clone(),
            bind_mode: BindMode::Replace,
            remote_node: Some(remote_node.to_string()),
        };

        let mut namespace = self.fs.namespace_manager.namespace.write().unwrap();
        namespace
            .entry(abs_local.clone())
            .or_insert_with(Vec::new)
            .push(entry);

        let mount_thread = {
            let remote_path_clone = remote_path.to_path_buf();
            let hello_fs_clone = self.fs.clone();
            thread::spawn(move || {
                fuser::mount2(hello_fs_clone, &remote_path_clone, &[]).unwrap();
            })
        };

        // Signal handling
        let mut signals = Signals::new(&[SIGINT, SIGTERM])?;
        for sig in signals.forever() {
            match sig {
                SIGINT | SIGTERM => {
                    println!("Received signal, unmounting...");
                    Self::handle_unmount(remote_path.to_str().unwrap());
                    break;
                }
                _ => {}
            }
        }

        mount_thread.join().unwrap();

        Ok(())
    }

    pub fn unmount(&self, path: &Path, specific_source: Option<&Path>) -> Result<()> {
        let abs_path = fs::canonicalize(path)?;

        let mut namespace = self.fs.namespace_manager.namespace.write().unwrap();

        if let Some(entries) = namespace.get_mut(&abs_path) {
            if let Some(specific_source) = specific_source {
                let abs_specific_source = fs::canonicalize(specific_source)?;
                entries.retain(|entry| entry.source.clone() != abs_specific_source);
            } else {
                entries.clear();
            }

            if entries.is_empty() {
                namespace.remove(&abs_path);
            }
        }

        Ok(())
    }

    // Platform-specific unmount handler
    fn handle_unmount(path: &str) {
        let c_path = CString::new(path).expect("CString::new failed");

        #[cfg(target_os = "macos")]
        unsafe {
            if unmount(c_path.as_ptr(), 0) != 0 {
                eprintln!("Failed to unmount {}", path);
            }
        }

        #[cfg(target_os = "linux")]
        unsafe {
            if umount(c_path.as_ptr()) != 0 {
                eprintln!("Failed to unmount {}", path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_manager() -> (TempDir, FilesystemManager) {
        let temp_dir = tempfile::tempdir().unwrap();
        let fs = HelloFS::new(temp_dir.path().to_path_buf()).unwrap();
        let manager = FilesystemManager::new(fs);
        (temp_dir, manager)
    }

    #[test]
    fn test_bind_directory() -> Result<()> {
        let (temp_dir, manager) = setup_test_manager();

        let source_dir = temp_dir.path().join("source");
        let target_dir = temp_dir.path().join("target");

        fs::create_dir_all(&source_dir)?;
        fs::create_dir_all(&target_dir)?;

        fs::write(source_dir.join("test.txt"), "test content")?;

        manager.bind(&source_dir, &target_dir, BindMode::Replace)?;

        let namespace = manager.fs.namespace_manager.namespace.read().unwrap();
        assert_eq!(namespace.len(), 1);

        Ok(())
    }

    #[test]
    fn test_multiple_binds() -> Result<()> {
        let (temp_dir, manager) = setup_test_manager();

        let dirs: Vec<_> = (0..3)
            .map(|i| {
                let dir = temp_dir.path().join(format!("dir{}", i));
                fs::create_dir_all(&dir).unwrap();
                dir
            })
            .collect();

        manager.bind(&dirs[0], &dirs[1], BindMode::Replace)?;
        manager.bind(&dirs[1], &dirs[2], BindMode::Replace)?;

        let namespace = manager.fs.namespace_manager.namespace.read().unwrap();
        assert_eq!(namespace.len(), 2);

        Ok(())
    }

    #[test]
    fn test_unmount() -> Result<()> {
        let (temp_dir, manager) = setup_test_manager();

        let source_dir = temp_dir.path().join("source");
        let target_dir = temp_dir.path().join("target");

        fs::create_dir_all(&source_dir)?;
        fs::create_dir_all(&target_dir)?;

        // First bind and verify
        manager.bind(&source_dir, &target_dir, BindMode::Replace)?;
        {
            let namespace = manager.fs.namespace_manager.namespace.read().unwrap();
            assert_eq!(namespace.len(), 1);
        }

        // Then unmount and verify
        manager.unmount(&target_dir, None)?;
        {
            let namespace = manager.fs.namespace_manager.namespace.read().unwrap();
            assert!(namespace.is_empty());
        }

        Ok(())
    }
}
