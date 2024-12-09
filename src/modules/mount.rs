use super::constants::BLOCK_SIZE;
use super::namespace::{BindMode, NamespaceEntry};
use super::proto::{BoundEntry, NineP};
use anyhow::{anyhow, Result};
use fuser::{FileAttr, FileType};
use libc::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::CString;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
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

#[derive(Clone)]
struct DirectoryEntry {
    name: String,
    path: PathBuf,
    metadata: fs::Metadata,
}

pub struct FilesystemManager {
    pub fs: NineP,
}

impl FilesystemManager {
    pub fn new(fs: NineP) -> Self {
        Self { fs }
    }

    // Helper function to create FileAttr from metadata
    fn create_file_attr(&self, inode: u64, metadata: &fs::Metadata) -> FileAttr {
        FileAttr {
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
        }
    }

    fn read_directory_entries_recursive(
        &self,
        base_path: &Path,
        current_path: &Path,
        parent_inode: u64,
        next_inode: &mut u64,
        bindings: &mut HashMap<u64, (OsString, BoundEntry)>,
    ) -> Result<()> {
        println!("Reading directory recursively: {:?}", current_path);
        let mut queue = VecDeque::new();
        queue.push_back((current_path.to_path_buf(), parent_inode));

        while let Some((path, parent)) = queue.pop_front() {
            for entry in fs::read_dir(&path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                let entry_path = entry.path();
                let relative_path = entry_path.strip_prefix(base_path)?;

                // Skip if this is the root directory itself
                if relative_path.as_os_str().is_empty() {
                    continue;
                }

                let inode = {
                    let current = *next_inode;
                    *next_inode += 1;
                    current
                };

                let file_name = entry.file_name();
                println!("Adding binding for: {:?} with inode: {}", file_name, inode);

                let file_attr = self.create_file_attr(inode, &metadata);
                let content = if metadata.is_file() {
                    Some(fs::read(&entry_path)?)
                } else {
                    None
                };

                bindings.insert(
                    inode,
                    (
                        file_name,
                        BoundEntry {
                            attr: file_attr,
                            content,
                        },
                    ),
                );

                if metadata.is_dir() {
                    queue.push_back((entry_path, inode));
                }
            }
        }

        Ok(())
    }

    pub fn bind_directory(&self, dir_path: &str, source_path: &Path, mode: BindMode) -> Result<()> {
        println!(
            "Binding directory: {} from source: {:?}",
            dir_path, source_path
        );

        let mut bindings = self.fs.namespace_manager.bindings.lock().unwrap();
        let mut next_inode = self.fs.namespace_manager.next_inode.lock().unwrap();

        // Convert paths to absolute paths
        let abs_source = fs::canonicalize(source_path)?;
        let abs_target = fs::canonicalize(Path::new(dir_path))?;

        println!(
            "Resolved paths - source: {:?}, target: {:?}",
            abs_source, abs_target
        );

        match mode {
            BindMode::Replace => {
                // Clear existing bindings but keep root
                bindings.retain(|&ino, _| ino == 1);

                // Read source directory recursively
                self.read_directory_entries_recursive(
                    &abs_source,
                    &abs_source,
                    1,
                    &mut next_inode,
                    &mut bindings,
                )?;
            }
            BindMode::Before => {
                let mut new_bindings = HashMap::new();

                // Read source directory recursively
                self.read_directory_entries_recursive(
                    &abs_source,
                    &abs_source,
                    1,
                    &mut next_inode,
                    &mut new_bindings,
                )?;

                // Read target directory and add non-conflicting entries
                let mut target_bindings = HashMap::new();
                self.read_directory_entries_recursive(
                    &abs_target,
                    &abs_target,
                    1,
                    &mut next_inode,
                    &mut target_bindings,
                )?;

                for (inode, (path, entry)) in target_bindings {
                    if !new_bindings.values().any(|(p, _)| p == &path) {
                        new_bindings.insert(inode, (path, entry));
                    }
                }

                bindings.extend(new_bindings);
            }
            BindMode::After => {
                // Read target directory first
                let mut target_bindings = HashMap::new();
                self.read_directory_entries_recursive(
                    &abs_target,
                    &abs_target,
                    1,
                    &mut next_inode,
                    &mut target_bindings,
                )?;

                bindings.extend(target_bindings);

                // Add non-conflicting source entries
                let mut source_bindings = HashMap::new();
                self.read_directory_entries_recursive(
                    &abs_source,
                    &abs_source,
                    1,
                    &mut next_inode,
                    &mut source_bindings,
                )?;

                for (inode, (path, entry)) in source_bindings {
                    if !bindings.values().any(|(p, _)| p == &path) {
                        bindings.insert(inode, (path, entry));
                    }
                }
            }
            BindMode::Create => {
                // Clear existing bindings but keep root
                bindings.retain(|&ino, _| ino == 1);

                // Read source directory recursively
                let mut new_bindings = HashMap::new();
                self.read_directory_entries_recursive(
                    &abs_source,
                    &abs_source,
                    1,
                    &mut next_inode,
                    &mut new_bindings,
                )?;

                // Make all entries read-only
                for (_, (_, entry)) in new_bindings.iter_mut() {
                    entry.attr.perm &= 0o555;
                }

                bindings.extend(new_bindings);
            }
        }

        println!("Final bindings: {:?}", bindings.keys().collect::<Vec<_>>());
        for (inode, (name, entry)) in bindings.iter() {
            println!(
                "inode: {}, name: {:?}, kind: {:?}",
                inode, name, entry.attr.kind
            );
        }
        Ok(())
    }

    pub fn bind(&self, source: &Path, target: &Path, mode: BindMode) -> Result<()> {
        println!("Binding {:?} to {:?} with mode {:?}", source, target, mode);

        let abs_source = fs::canonicalize(source)?;
        let abs_target = fs::canonicalize(target)?;

        if !abs_source.exists() {
            return Err(anyhow!("Source path does not exist: {:?}", abs_source));
        }

        if !abs_target.exists() {
            return Err(anyhow!("Target path does not exist: {:?}", abs_target));
        }

        let entry = NamespaceEntry {
            source: abs_source.clone(),
            target: abs_target.clone(),
            bind_mode: mode.clone(),
            remote_node: None,
        };

        let mut namespace = self.fs.namespace_manager.namespace.write().unwrap();
        namespace
            .entry(abs_target.clone())
            .or_insert_with(Vec::new)
            .push(entry);

        self.bind_directory(abs_target.to_str().unwrap(), &abs_source, mode)?;

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
        let fs = NineP::new(temp_dir.path().to_path_buf()).unwrap();
        let manager = FilesystemManager::new(fs);
        (temp_dir, manager)
    }

    fn create_temp_dir_with_files(parent: &Path) -> Result<TempDir> {
        let dir = tempfile::tempdir_in(parent)?;
        fs::write(dir.path().join("test.txt"), "test content")?;
        Ok(dir)
    }

    // figure out how to test bind_directory
    // #[test]
    // fn test_bind_directory() -> Result<()> {
    //     let (root_dir, manager) = setup_test_manager();
    //     let source_dir = create_temp_dir_with_files(root_dir.path())?;
    //     let target_dir = tempfile::tempdir_in(root_dir.path())?;

    //     // Only test the namespace manipulation, not the actual mounting
    //     let abs_source = fs::canonicalize(source_dir.path())?;
    //     let abs_target = fs::canonicalize(target_dir.path())?;

    //     let entry = NamespaceEntry {
    //         source: abs_source.clone(),
    //         target: abs_target.clone(),
    //         bind_mode: BindMode::Replace,
    //         remote_node: None,
    //     };

    //     {
    //         let mut namespace = manager.fs.namespace_manager.namespace.write().unwrap();
    //         namespace
    //             .entry(abs_target.clone())
    //             .or_insert_with(Vec::new)
    //             .push(entry);
    //     }

    //     let namespace = manager.fs.namespace_manager.namespace.read().unwrap();
    //     assert_eq!(namespace.len(), 1);
    //     Ok(())
    // }

    // #[test]
    // fn test_multiple_binds() -> Result<()> {
    //     let (root_dir, manager) = setup_test_manager();

    //     let temp_dirs: Vec<TempDir> = (0..3)
    //         .map(|_| tempfile::tempdir_in(root_dir.path()).unwrap())
    //         .collect();

    //     // Test namespace manipulation directly instead of using bind()
    //     let abs_source1 = fs::canonicalize(temp_dirs[0].path())?;
    //     let abs_target1 = fs::canonicalize(temp_dirs[1].path())?;
    //     let abs_target2 = fs::canonicalize(temp_dirs[2].path())?;

    //     {
    //         let mut namespace = manager.fs.namespace_manager.namespace.write().unwrap();
            
    //         // First binding
    //         namespace
    //             .entry(abs_target1.clone())
    //             .or_insert_with(Vec::new)
    //             .push(NamespaceEntry {
    //                 source: abs_source1.clone(),
    //                 target: abs_target1.clone(),
    //                 bind_mode: BindMode::Replace,
    //                 remote_node: None,
    //             });

    //         // Second binding
    //         namespace
    //             .entry(abs_target2.clone())
    //             .or_insert_with(Vec::new)
    //             .push(NamespaceEntry {
    //                 source: abs_target1.clone(),
    //                 target: abs_target2.clone(),
    //                 bind_mode: BindMode::Replace,
    //                 remote_node: None,
    //             });
    //     }

    //     let namespace = manager.fs.namespace_manager.namespace.read().unwrap();
    //     assert_eq!(namespace.len(), 2);
    //     Ok(())
    // }

    // figure out how to test unmount
    // #[test]
    // fn test_unmount() -> Result<()> {
    //     let (root_dir, manager) = setup_test_manager();
    //     let source_dir = create_temp_dir_with_files(root_dir.path())?;
    //     let target_dir = tempfile::tempdir_in(root_dir.path())?;

    //     let abs_source = fs::canonicalize(source_dir.path())?;
    //     let abs_target = fs::canonicalize(target_dir.path())?;

    //     // First set up the binding directly in the namespace
    //     {
    //         let mut namespace = manager.fs.namespace_manager.namespace.write().unwrap();
    //         namespace
    //             .entry(abs_target.clone())
    //             .or_insert_with(Vec::new)
    //             .push(NamespaceEntry {
    //                 source: abs_source.clone(),
    //                 target: abs_target.clone(),
    //                 bind_mode: BindMode::Replace,
    //                 remote_node: None,
    //             });
    //     }

    //     // Verify initial binding
    //     {
    //         let namespace = manager.fs.namespace_manager.namespace.read().unwrap();
    //         assert_eq!(namespace.len(), 1);
    //     }

    //     // Test unmount
    //     manager.unmount(target_dir.path(), None)?;

    //     // Verify unmount
    //     {
    //         let namespace = manager.fs.namespace_manager.namespace.read().unwrap();
    //         assert!(namespace.is_empty());
    //     }
    //     Ok(())
    // }
}
