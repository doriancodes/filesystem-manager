//! Filesystem mounting and management functionality.
//! 
//! This module provides the core functionality for mounting and managing
//! filesystem bindings through the `FilesystemManager`.

use super::constants::BLOCK_SIZE;
use super::namespace::{BindMode, NamespaceEntry};
use super::proto::{BoundEntry, NineP};
use anyhow::{anyhow, Result, Context};
use fuser::{FileAttr, FileType, MountOption};
use libc::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::CString;
use std::ffi::{OsString, OsStr};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::UNIX_EPOCH;
use log::{info, debug, warn};
use std::cell::RefCell;
use std::sync::Arc;
use crate::session::Session;
use log::error;

#[cfg(target_os = "macos")]
extern "C" {
    /// Unmounts a filesystem on macOS.
    /// 
    /// # Arguments
    /// * `path` - Path to unmount
    /// * `flags` - Unmount flags
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

/// Manages filesystem mounting and binding operations.
#[derive(Clone)]
pub struct FilesystemManager {
    /// The underlying 9P filesystem implementation.
    pub fs: NineP,
}

thread_local! {
    static CURRENT_SESSION: RefCell<Option<Arc<Session>>> = RefCell::new(None);
}

impl FilesystemManager {
    /// Creates a new filesystem manager.
    /// 
    /// # Arguments
    /// 
    /// * `fs` - The 9P filesystem implementation to manage
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

    /// Binds a directory to a target location.
    fn bind_directory(&self, dir_path: &str, source_path: &Path, mode: BindMode) -> Result<()> {
        debug!("Binding directory: {} from source: {:?}", dir_path, source_path);

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

    /// Binds a source path to a target path with the specified mode.
    /// 
    /// This method creates a binding between two filesystem paths, allowing the contents
    /// of the source path to be accessed through the target path. The behavior of the
    /// binding is determined by the specified `BindMode`.
    /// 
    /// # Arguments
    /// 
    /// * `source` - The source path to bind from
    /// * `target` - The target path to bind to
    /// * `mode` - The binding mode to use:
    ///   - `Replace`: Replaces any existing content at the target
    ///   - `Before`: Adds content with higher priority than existing bindings
    ///   - `After`: Adds content with lower priority than existing bindings
    ///   - `Create`: Creates a new binding, failing if the target exists
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the binding was successful
    /// * `Err(...)` if the binding failed (e.g., invalid paths, permission issues)
    pub fn bind(&self, source: &Path, target: &Path, mode: BindMode) -> Result<()> {
        info!("Binding {:?} to {:?} with mode {:?}", source, target, mode);
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
        
        // After successful bind
        info!("Bind operation successful, notifying session");
        if let Some(session) = self.get_session() {
            info!("Found current session, sending notification");
            session.notify_bind_success(source.to_path_buf(), target.to_path_buf())?;
            info!("Notification sent successfully");
        } else {
            warn!("No current session found for bind notification");
        }
        
        Ok(())
    }

    /// Mounts a filesystem at the specified path.
    /// 
    /// # Arguments
    /// * `source` - The source path to mount from
    /// * `target` - The target path to mount to
    /// * `node_id` - Node identifier for the mount
    /// 
    /// # Returns
    /// * `Ok(())` if the mount was successful
    /// * `Err` with a descriptive error message if:
    ///   - Source path doesn't exist
    ///   - Target path doesn't exist
    ///   - Target is not a directory
    ///   - Mount operation fails
    ///   - Insufficient permissions
    pub fn mount(&self, source: &Path, target: &Path, node_id: &str) -> Result<()> {
        info!("Mounting {} to {} for node {}", source.display(), target.display(), node_id);
        
        // Verify source exists and is a directory
        if !source.exists() {
            return Err(anyhow!("Source path does not exist: {}", source.display())
                .context("Mount source verification failed"));
        }
        if !source.is_dir() {
            return Err(anyhow!("Source path is not a directory: {}", source.display())
                .context("Mount source must be a directory"));
        }

        // Verify target exists and is a directory
        if !target.exists() {
            return Err(anyhow!("Target path does not exist: {}", target.display())
                .context("Mount target verification failed"));
        }
        if !target.is_dir() {
            return Err(anyhow!("Target path is not a directory: {}", target.display())
                .context("Mount target must be a directory"));
        }

        // Warn if target is not empty
        if target.read_dir()?.next().is_some() {
            warn!("Target directory is not empty: {}", target.display());
        }

        // Resolve paths
        let abs_source = fs::canonicalize(source)
            .with_context(|| format!("Failed to resolve source path: {}", source.display()))?;
        let abs_target = fs::canonicalize(target)
            .with_context(|| format!("Failed to resolve target path: {}", target.display()))?;

        // Set up mount options
        let mount_options = vec![
            MountOption::RW,
            MountOption::FSName("froggr".to_string()),
            MountOption::AllowOther,
        ];

        match fuser::mount2(self.fs.clone(), &abs_target, &mount_options) {
            Ok(_) => {
                info!("Successfully mounted {} to {}", abs_source.display(), abs_target.display());
                
                // Update namespace
                let entry = NamespaceEntry {
                    source: abs_source.clone(),
                    target: abs_target.clone(),
                    bind_mode: BindMode::Before,
                    remote_node: Some(node_id.to_string()),
                };

                if let Ok(mut namespace) = self.fs.namespace_manager.namespace.write() {
                    namespace
                        .entry(abs_target.clone())
                        .or_insert_with(Vec::new)
                        .push(entry);
                } else {
                    error!("Failed to acquire namespace write lock");
                }

                // Notify session of successful mount
                if let Some(session) = Self::get_current_session() {
                    info!("Notifying session of successful mount");
                    session.notify_mount_success(source.to_path_buf(), target.to_path_buf())?;
                } else {
                    warn!("No session found to notify of mount success");
                }

                Ok(())
            },
            Err(e) => {
                Err(anyhow!("Mount operation failed: {}", e)
                    .context(format!("Failed to mount {} to {}", 
                        abs_source.display(), abs_target.display())))
            }
        }
    }

    /// Unmounts a filesystem at the specified path
    /// 
    /// # Arguments
    /// * `path` - The path to unmount
    /// * `force` - Whether to force unmount even if busy
    /// 
    /// # Returns
    /// * `Ok(())` if unmount was successful
    /// * `Err` if unmount failed
    pub fn unmount(&self, path: &Path, force: bool) -> Result<()> {
        info!("Unmounting filesystem at {}", path.display());

        // Verify path exists
        if !path.exists() {
            return Err(anyhow!("Path does not exist: {}", path.display()));
        }

        // Resolve to absolute path
        let abs_path = fs::canonicalize(path)
            .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

        // Convert path to C string for system call
        let c_path = CString::new(abs_path.to_str().unwrap())
            .map_err(|e| anyhow!("Invalid path: {}", e))?;

        // Perform platform-specific unmount
        let result = unsafe {
            #[cfg(target_os = "macos")]
            {
                unmount(c_path.as_ptr(), if force { 0x00080000 } else { 0 })
            }
            #[cfg(target_os = "linux")]
            {
                if force {
                    libc::umount2(c_path.as_ptr(), libc::MNT_FORCE)
                } else {
                    umount(c_path.as_ptr())
                }
            }
        };

        if result != 0 {
            let err = std::io::Error::last_os_error();
            return Err(anyhow!("Failed to unmount {}: {}", path.display(), err));
        }

        // Update namespace
        let mut namespace = self.fs.namespace_manager.namespace.write()
            .map_err(|_| anyhow!("Failed to acquire namespace lock"))?;
        
        namespace.remove(&abs_path);

        // Notify session
        if let Some(session) = Self::get_current_session() {
            session.notify_unmount_success(path.to_path_buf())?;
        }

        info!("Successfully unmounted {}", path.display());
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

    /// Gets the current session from thread-local storage.
    /// 
    /// # Returns
    /// * `Some(Arc<Session>)` if there is a session associated with the current thread
    /// * `None` if no session is currently associated
    pub fn get_current_session() -> Option<Arc<Session>> {
        CURRENT_SESSION.with(|current| current.borrow().clone())
    }

    /// Gets the current session associated with this filesystem manager instance.
    /// 
    /// This method retrieves the session from thread-local storage if one exists.
    /// 
    /// # Returns
    /// * `Some(Arc<Session>)` if there is a session associated with the current thread
    /// * `None` if no session is currently associated
    pub fn get_session(&self) -> Option<Arc<Session>> {
        Self::get_current_session()
    }

    /// Sets the current session for this filesystem manager.
    /// 
    /// This method stores the provided session in thread-local storage for later retrieval.
    /// 
    /// # Arguments
    /// * `session` - The session to associate with the current thread
    pub fn set_current_session(session: Arc<Session>) {
        CURRENT_SESSION.with(|current| {
            *current.borrow_mut() = Some(session);
        });
    }

    /// Internal method to mount a directory.
    /// 
    /// # Arguments
    /// * `dir_path` - The target directory path to mount to
    /// * `source_path` - The source directory path to mount from
    /// 
    /// # Returns
    /// * `Result<()>` indicating success or failure
    fn mount_directory(&self, dir_path: &str, source_path: &Path) -> Result<()> {
        debug!("Mounting directory: {} from source: {:?}", dir_path, source_path);

        let mut bindings = self.fs.namespace_manager.bindings.lock().unwrap();
        let mut next_inode = self.fs.namespace_manager.next_inode.lock().unwrap();

        // Convert paths to absolute paths
        let abs_source = fs::canonicalize(source_path)?;
        let abs_target = fs::canonicalize(Path::new(dir_path))?;

        info!(
            "Resolved paths - source: {:?}, target: {:?}",
            abs_source, abs_target
        );

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

        info!("Final bindings: {:?}", bindings.keys().collect::<Vec<_>>());
        for (inode, (name, entry)) in bindings.iter() {
            debug!(
                "inode: {}, name: {:?}, kind: {:?}",
                inode, name, entry.attr.kind
            );
        }
        Ok(())
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
