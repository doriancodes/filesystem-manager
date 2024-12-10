use anyhow::Result;
use fuser::{FileAttr, FileType};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use super::constants::*;
use super::proto::BoundEntry;

/// Represents different modes for binding operations
#[derive(Debug, Clone, PartialEq)]
pub enum BindMode {
    /// Replace existing content at the mountpoint
    Replace,
    /// Add content with higher priority
    Before,
    /// Add content with lower priority
    After,
    /// Create mountpoint if needed
    Create,
}

/// Entry in the namespace representing a bind operation
#[derive(Debug, Clone)]
pub struct NamespaceEntry {
    /// Source path for the bind operation
    pub source: PathBuf,
    /// Target path where the source is bound
    pub target: PathBuf,
    /// Mode of the bind operation
    pub bind_mode: BindMode,
    /// Optional remote node identifier
    pub remote_node: Option<String>,
}

/// Manages the filesystem namespace and bindings
#[derive(Debug, Clone)]
pub struct NamespaceManager {
    /// The namespace mapping from paths to their bind entries
    pub namespace: Arc<RwLock<HashMap<PathBuf, Vec<NamespaceEntry>>>>,
    /// Root directory of the filesystem
    pub root: PathBuf,
    /// Mapping of inodes to bound entries
    pub bindings: Arc<Mutex<HashMap<u64, (OsString, BoundEntry)>>>,
    /// Next available inode number
    pub next_inode: Arc<Mutex<u64>>,
}

impl NamespaceManager {
    /// Creates a new namespace manager with the given root directory
    /// 
    /// # Arguments
    /// 
    /// * `root` - Root directory path for the namespace
    /// 
    /// # Returns
    /// 
    /// A new `NamespaceManager` instance
    /// 
    /// # Errors
    /// 
    /// Returns an error if the root directory cannot be created
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;

        let mut bindings = HashMap::new();
        bindings.insert(
            ROOT_INODE,
            (
                OsString::from("."),
                BoundEntry {
                    attr: create_root_attr(),
                    content: None,
                },
            ),
        );

        Ok(Self {
            namespace: Arc::new(RwLock::new(HashMap::new())),
            root,
            bindings: Arc::new(Mutex::new(bindings)),
            next_inode: Arc::new(Mutex::new(INITIAL_INODE)),
        })
    }

    /// Resolves a path through the namespace bindings.
    /// 
    /// # Arguments
    /// * `original_path` - The path to resolve
    /// 
    /// # Returns
    /// * `Result<PathBuf>` - The resolved path
    pub fn resolve_path(&self, original_path: &Path) -> Result<PathBuf> {
        let abs_path = fs::canonicalize(original_path)?;
        let namespace = self.namespace.read().unwrap();

        if let Some(entries) = namespace.get(&abs_path) {
            for entry in entries.iter().rev() {
                match entry.bind_mode {
                    BindMode::Replace => return Ok(entry.source.clone()),
                    BindMode::Before | BindMode::After | BindMode::Create => {
                        let mut new_path = entry.source.clone();
                        new_path.push(abs_path.strip_prefix(&entry.target)?);
                        return Ok(new_path);
                    }
                }
            }
        }

        Ok(abs_path)
    }

    /// Lists all entries in the namespace.
    /// 
    /// # Returns
    /// * `Vec<NamespaceEntry>` - All namespace entries
    pub fn list_namespace(&self) -> Vec<NamespaceEntry> {
        let namespace = self.namespace.read().unwrap();
        namespace
            .values()
            .flat_map(|entries| entries.clone())
            .collect()
    }
}

// Helper function to create root file attributes
fn create_root_attr() -> FileAttr {
    FileAttr {
        ino: ROOT_INODE,
        size: 0,
        blocks: 0,
        atime: std::time::UNIX_EPOCH,
        mtime: std::time::UNIX_EPOCH,
        ctime: std::time::UNIX_EPOCH,
        crtime: std::time::UNIX_EPOCH,
        kind: FileType::Directory,
        perm: DEFAULT_PERMISSION,
        nlink: 2,
        uid: DEFAULT_UID,
        gid: DEFAULT_GID,
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_namespace_manager_creation() -> Result<()> {
        let temp_dir = setup_test_dir();
        let manager = NamespaceManager::new(temp_dir.path().to_path_buf())?;

        assert!(manager.namespace.read().unwrap().is_empty());
        assert_eq!(manager.root, temp_dir.path());

        Ok(())
    }

    // #[test]
    // fn test_resolve_path_with_replace_binding() -> Result<()> {
    //     let temp_dir = setup_test_dir();
    //     let manager = NamespaceManager::new(temp_dir.path().to_path_buf())?;

    //     let source = temp_dir.path().join("source.txt");
    //     let target = temp_dir.path().join("target.txt");

    //     fs::write(&source, "test content")?;

    //     {
    //         let mut namespace = manager.namespace.write().unwrap();
    //         namespace.insert(
    //             target.clone(),
    //             vec![NamespaceEntry {
    //                 source: source.clone(),
    //                 target: target.clone(),
    //                 bind_mode: BindMode::Replace,
    //                 remote_node: None,
    //             }],
    //         );
    //     }

    //     let resolved = manager.resolve_path(&target)?;
    //     assert_eq!(resolved, source);

    //     Ok(())
    // }

    #[test]
    fn test_list_namespace() -> Result<()> {
        let temp_dir = setup_test_dir();
        let manager = NamespaceManager::new(temp_dir.path().to_path_buf())?;

        let source = temp_dir.path().join("source");
        let target = temp_dir.path().join("target");

        {
            let mut namespace = manager.namespace.write().unwrap();
            namespace.insert(
                target.clone(),
                vec![NamespaceEntry {
                    source: source.clone(),
                    target: target.clone(),
                    bind_mode: BindMode::Replace,
                    remote_node: None,
                }],
            );
        }

        let entries = manager.list_namespace();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, source);
        assert_eq!(entries[0].target, target);

        Ok(())
    }

    #[test]
    fn test_bind_modes() -> Result<()> {
        let temp_dir = setup_test_dir();
        let manager = NamespaceManager::new(temp_dir.path().to_path_buf())?;

        let source = temp_dir.path().join("source");
        let target = temp_dir.path().join("target");
        fs::create_dir_all(&source)?;
        fs::create_dir_all(&target)?;

        for mode in [
            BindMode::Replace,
            BindMode::Before,
            BindMode::After,
            BindMode::Create,
        ] {
            let mut namespace = manager.namespace.write().unwrap();
            namespace.clear();
            namespace.insert(
                target.clone(),
                vec![NamespaceEntry {
                    source: source.clone(),
                    target: target.clone(),
                    bind_mode: mode.clone(),
                    remote_node: None,
                }],
            );

            let resolved = manager.resolve_path(&target)?;
            match mode {
                BindMode::Replace => assert_eq!(resolved, source),
                _ => assert!(resolved.starts_with(&source)),
            }
        }

        Ok(())
    }

    // #[test]
    // fn test_multiple_bindings() -> Result<()> {
    //     let temp_dir = setup_test_dir();
    //     let manager = NamespaceManager::new(temp_dir.path().to_path_buf())?;

    //     let source1 = temp_dir.path().join("source1");
    //     let source2 = temp_dir.path().join("source2");
    //     let target = temp_dir.path().join("target");

    //     fs::create_dir_all(&source1)?;
    //     fs::create_dir_all(&source2)?;
    //     fs::create_dir_all(&target)?;

    //     {
    //         let mut namespace = manager.namespace.write().unwrap();
    //         namespace.insert(
    //             target.clone(),
    //             vec![
    //                 NamespaceEntry {
    //                     source: source1.clone(),
    //                     target: target.clone(),
    //                     bind_mode: BindMode::Before,
    //                     remote_node: None,
    //                 },
    //                 NamespaceEntry {
    //                     source: source2.clone(),
    //                     target: target.clone(),
    //                     bind_mode: BindMode::After,
    //                     remote_node: None,
    //                 },
    //             ],
    //         );
    //     }

    //     let entries = manager.list_namespace();
    //     assert_eq!(entries.len(), 2);

    //     Ok(())
    // }
  
}
