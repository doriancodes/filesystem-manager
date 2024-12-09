use super::constants::*;
use super::namespace::NamespaceManager;
use anyhow::{anyhow, Result};
use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

// 9P protocol constants
const QTDIR: u8 = 0x80;
const QTAPPEND: u8 = 0x40;
const QTEXCL: u8 = 0x20;
const QTAUTH: u8 = 0x08;

// File access modes
#[derive(Debug, Clone, Copy)]
pub struct OpenFlags(pub u32);

impl OpenFlags {
    pub const O_RDONLY: u32 = 0x00;
    pub const O_WRONLY: u32 = 0x01;
    pub const O_RDWR: u32 = 0x02;
    pub const O_EXEC: u32 = 0x03;
    pub const O_TRUNC: u32 = 0x10;
}

// Helper struct for file stats
#[derive(Debug, Clone)]
pub struct Stat {
    pub size: u16,    // Total size of stat message
    pub typ: u16,     // For kernel use
    pub dev: u32,     // For kernel use
    pub qid: Qid,     // Unique id from server
    pub mode: u32,    // Permissions and flags
    pub atime: u32,   // Last access time
    pub mtime: u32,   // Last modification time
    pub length: u64,  // Length of file in bytes
    pub name: String, // File name
    pub uid: String,  // Owner name
    pub gid: String,  // Group name
    pub muid: String, // Name of last modifier
}

#[derive(Debug, Clone)]
pub struct BoundEntry {
    pub attr: FileAttr,
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy)]
pub enum OpenMode {
    Read = 0,
    Write = 1,
    ReadWrite = 2,
    Execute = 3,
}

#[derive(Debug, Clone)]
pub struct Qid {
    pub version: u32,
    pub path: u64,
    pub file_type: u8,
}

/// A 9P filesystem implementation.
///
/// The `NineP` struct provides a full implementation of the 9P protocol,
/// allowing you to create a virtual filesystem that can be mounted and
/// accessed by clients.
///
/// # Example
///
/// ```rust
/// use filesystem_manager::NineP;
/// use anyhow::Result;
/// use std::path::PathBuf;
///
/// fn main() -> Result<()> {
///     // Create a new NineP filesystem with "/tmp/test" as the root directory
///     let hello_fs = NineP::new(PathBuf::from("/tmp/test"))?;
///
///     // Perform various filesystem operations using the NineP instance
///     hello_fs.version("9P2000", 8192)?;
///     let qid = hello_fs.attach(0, None, "user", "default")?;
///     let qids = hello_fs.walk(0, 1, &["dir1", "file.txt"])?;
///
///     Ok(())
/// }
/// ```
///
#[derive(Debug, Clone)]
pub struct NineP {
    /// The namespace manager for the NineP filesystem.
    pub namespace_manager: NamespaceManager,
    /// A mapping of file IDs (fids) to their corresponding file paths.
    fids: Arc<Mutex<HashMap<u32, PathBuf>>>,
    /// The maximum message size for the 9P protocol.
    msize: u32,
    /// The version of the 9P protocol.
    version: String,
}

impl NineP {
    /// Creates a new NineP filesystem with the specified root directory.
    ///
    /// # Arguments
    /// * `path` - The root directory for the NineP filesystem.
    ///
    /// # Returns
    /// A new `NineP` instance.
    pub fn new(path: PathBuf) -> Result<Self> {
        Ok(Self {
            namespace_manager: NamespaceManager::new(path)?,
            fids: Arc::new(Mutex::new(HashMap::new())),
            msize: 8192,
            version: "9P2000".to_string(),
        })
    }

    fn qid_from_attr(attr: &FileAttr) -> Qid {
        Qid {
            version: 0,
            path: attr.ino,
            file_type: if attr.kind == FileType::Directory {
                QTDIR
            } else {
                0
            },
        }
    }

    /// Negotiates the version and maximum message size for the 9P protocol.
    ///
    /// # Arguments
    /// * `requested_version` - The requested version of the 9P protocol.
    /// * `msize` - The requested maximum message size.
    ///
    /// # Returns
    /// A tuple containing the negotiated maximum message size and version.
    pub fn version(&mut self, requested_version: &str, msize: u32) -> Result<(u32, String)> {
        self.msize = std::cmp::min(msize, 8192); // Cap at 8K
        let version = if requested_version == "9P2000" {
            "9P2000".to_string()
        } else {
            "unknown".to_string()
        };
        self.version = version.clone();
        Ok((self.msize, version))
    }

    /// Authenticates a user with the 9P filesystem.
    ///
    /// # Arguments
    /// * `uname` - The username.
    /// * `aname` - The authentication name.
    /// * `afid` - The authentication file ID.
    ///
    /// # Returns
    /// The Qid (unique identifier) of the authenticated user.
    pub fn auth(&mut self, uname: &str, aname: &str, afid: u32) -> Result<Qid> {
        // For this implementation, we'll return an error as we're not implementing auth
        Err(anyhow!("Authentication not required"))
    }

    /// Attaches a file ID (fid) to the root directory of the NineP filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID to attach.
    /// * `afid` - The authentication file ID (optional).
    /// * `uname` - The username.
    /// * `aname` - The authentication name.
    ///
    /// # Returns
    /// The Qid (unique identifier) of the root directory.
    pub fn attach(&mut self, fid: u32, afid: Option<u32>, uname: &str, aname: &str) -> Result<Qid> {
        let mut fids = self.fids.lock().unwrap();
        fids.insert(fid, PathBuf::from("/"));

        Ok(Qid {
            version: 0,
            path: 1, // Root directory
            file_type: QTDIR,
        })
    }

    /// Walks the file tree, resolving the specified file names.
    ///
    /// # Arguments
    /// * `fid` - The file ID to start the walk from.
    /// * `newfid` - The new file ID to associate with the final path.
    /// * `wnames` - The file names to walk through.
    ///
    /// # Returns
    /// A vector of Qids (unique identifiers) for the resolved file names.
    pub fn walk(&mut self, fid: u32, newfid: u32, wnames: &[String]) -> Result<Vec<Qid>> {
        let mut qids = Vec::new();
        let fids = self.fids.lock().unwrap();

        // Get starting path
        let start_path = fids
            .get(&fid)
            .ok_or_else(|| anyhow!("Invalid fid"))?
            .clone();

        let mut current_path = start_path;
        let bindings = self.namespace_manager.bindings.lock().unwrap();

        for name in wnames {
            current_path.push(name);

            // Find the entry in bindings
            let mut found = false;
            for (_, (entry_name, entry)) in bindings.iter() {
                if entry_name.to_string_lossy() == name.as_str() {
                    qids.push(Self::qid_from_attr(&entry.attr));
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(anyhow!("Path not found"));
            }
        }

        // Update newfid with final path if walk was successful
        if !qids.is_empty() {
            let mut fids = self.fids.lock().unwrap();
            fids.insert(newfid, current_path);
        }

        Ok(qids)
    }

    /// Opens a file in the 9P filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID of the file to open.
    /// * `flags` - The file access flags.
    ///
    /// # Returns
    /// A tuple containing the Qid (unique identifier) of the opened file and the maximum message size.
    pub fn open(&mut self, fid: u32, flags: OpenFlags) -> Result<(Qid, u32)> {
        let fids = self.fids.lock().unwrap();
        let path = fids.get(&fid).ok_or_else(|| anyhow!("Invalid fid"))?;

        let bindings = self.namespace_manager.bindings.lock().unwrap();

        // Find the entry
        for (_, (entry_name, entry)) in bindings.iter() {
            if entry_name.to_string_lossy() == path.to_string_lossy() {
                let qid = Self::qid_from_attr(&entry.attr);
                return Ok((qid, self.msize));
            }
        }

        Err(anyhow!("File not found"))
    }

    /// Creates a new file in the 9P filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID of the parent directory.
    /// * `name` - The name of the new file.
    /// * `perm` - The permissions for the new file.
    /// * `mode` - The file access mode.
    ///
    /// # Returns
    /// A tuple containing the Qid (unique identifier) of the new file and the maximum message size.
    pub fn create(
        &mut self,
        fid: u32,
        name: &str,
        perm: u32,
        mode: OpenFlags,
    ) -> Result<(Qid, u32)> {
        let fids = self.fids.lock().unwrap();
        let parent_path = fids.get(&fid).ok_or_else(|| anyhow!("Invalid fid"))?;

        let mut new_path = parent_path.clone();
        new_path.push(name);

        let mut bindings = self.namespace_manager.bindings.lock().unwrap();
        let mut next_inode = self.namespace_manager.next_inode.lock().unwrap();

        let inode = *next_inode;
        *next_inode += 1;

        let attr = FileAttr {
            ino: inode,
            size: 0,
            blocks: 0,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: SystemTime::now(),
            kind: FileType::RegularFile,
            perm: perm as u16,
            nlink: 1,
            uid: 501,
            gid: 20,
            rdev: 0,
            flags: 0,
            blksize: 512,
        };

        let entry = BoundEntry {
            attr,
            content: Some(Vec::new()),
        };

        bindings.insert(inode, (OsString::from(name), entry));

        Ok((
            Qid {
                version: 0,
                path: inode,
                file_type: 0,
            },
            self.msize,
        ))
    }

    /// Reads data from a file in the 9P filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID of the file to read from.
    /// * `offset` - The offset within the file to start reading from.
    /// * `count` - The number of bytes to read.
    ///
    /// # Returns
    /// The data read from the file.
    pub fn read(&self, fid: u32, offset: u64, count: u32) -> Result<Vec<u8>> {
        let fids = self.fids.lock().unwrap();
        let path = fids.get(&fid).ok_or_else(|| anyhow!("Invalid fid"))?;

        let bindings = self.namespace_manager.bindings.lock().unwrap();

        for (_, (_, entry)) in bindings.iter() {
            if let Some(ref content) = entry.content {
                let start = offset as usize;
                let end = std::cmp::min(start + count as usize, content.len());
                return Ok(content[start..end].to_vec());
            }
        }

        Err(anyhow!("File not found"))
    }

    /// Writes data to a file in the 9P filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID of the file to write to.
    /// * `offset` - The offset within the file to start writing at.
    /// * `data` - The data to write to the file.
    ///
    /// # Returns
    /// The number of bytes written to the file.
    pub fn write(&mut self, fid: u32, offset: u64, data: &[u8]) -> Result<u32> {
        let fids = self.fids.lock().unwrap();
        let path = fids.get(&fid).ok_or_else(|| anyhow!("Invalid fid"))?;

        let mut bindings = self.namespace_manager.bindings.lock().unwrap();

        for (_, (_, entry)) in bindings.iter_mut() {
            if let Some(ref mut content) = entry.content {
                let start = offset as usize;
                let end = start + data.len();

                if end > content.len() {
                    content.resize(end, 0);
                }

                content[start..end].copy_from_slice(data);
                return Ok(data.len() as u32);
            }
        }

        Err(anyhow!("File not found"))
    }

    /// Closes a file in the 9P filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID of the file to close.
    ///
    /// # Returns
    /// An empty result indicating the success of the operation.
    pub fn clunk(&mut self, fid: u32) -> Result<()> {
        let mut fids = self.fids.lock().unwrap();
        if fids.remove(&fid).is_some() {
            Ok(())
        } else {
            Err(anyhow!("Invalid fid"))
        }
    }

    /// Removes a file from the 9P filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID of the file to remove.
    ///
    /// # Returns
    /// An empty result indicating the success of the operation.
    pub fn remove(&mut self, fid: u32) -> Result<()> {
        let mut fids = self.fids.lock().unwrap();
        let path = fids.remove(&fid).ok_or_else(|| anyhow!("Invalid fid"))?;

        let mut bindings = self.namespace_manager.bindings.lock().unwrap();

        // Find and remove the entry
        let mut found_inode = None;
        for (inode, (entry_name, _)) in bindings.iter() {
            if entry_name.to_string_lossy() == path.to_string_lossy() {
                found_inode = Some(*inode);
                break;
            }
        }

        if let Some(inode) = found_inode {
            bindings.remove(&inode);
            Ok(())
        } else {
            Err(anyhow!("File not found"))
        }
    }

    /// Retrieves the attributes of a file or directory in the 9P filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID of the file or directory to retrieve attributes for.
    ///
    /// # Returns
    /// The file or directory attributes as a `Stat` struct.
    pub fn stat(&self, fid: u32) -> Result<Stat> {
        let fids = self.fids.lock().unwrap();
        let path = fids.get(&fid).ok_or_else(|| anyhow!("Invalid fid"))?;

        let bindings = self.namespace_manager.bindings.lock().unwrap();

        for (_, (entry_name, entry)) in bindings.iter() {
            if entry_name.to_string_lossy() == path.to_string_lossy() {
                return Ok(Stat {
                    size: 0, // Will be filled by protocol
                    typ: 0,
                    dev: 0,
                    qid: Self::qid_from_attr(&entry.attr),
                    mode: entry.attr.perm as u32,
                    atime: entry
                        .attr
                        .atime
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as u32,
                    mtime: entry
                        .attr
                        .mtime
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as u32,
                    length: entry.attr.size,
                    name: entry_name.to_string_lossy().to_string(),
                    uid: "user".to_string(),
                    gid: "user".to_string(),
                    muid: "user".to_string(),
                });
            }
        }

        Err(anyhow!("File not found"))
    }

    /// Modifies the attributes of a file or directory in the 9P filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file ID of the file or directory to modify.
    /// * `stat` - The new attributes to apply.
    ///
    /// # Returns
    /// An empty result indicating the success of the operation.
    pub fn wstat(&mut self, fid: u32, stat: &Stat) -> Result<()> {
        let fids = self.fids.lock().unwrap();
        let path = fids.get(&fid).ok_or_else(|| anyhow!("Invalid fid"))?;

        let mut bindings = self.namespace_manager.bindings.lock().unwrap();

        for (_, (_, entry)) in bindings.iter_mut() {
            let mut attr = entry.attr;
            attr.perm = stat.mode as u16;
            // Update other attributes as needed
            entry.attr = attr;
            return Ok(());
        }

        Err(anyhow!("File not found"))
    }

    /// Flushes a pending operation in the 9P filesystem.
    ///
    /// # Arguments
    /// * `oldtag` - The tag of the operation to flush.
    ///
    /// # Returns
    /// An empty result indicating the success of the operation.
    pub fn flush(&mut self, oldtag: u16) -> Result<()> {
        // In this implementation, we don't queue operations, so flush is a no-op
        Ok(())
    }
}

impl Filesystem for NineP {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        println!("Lookup for parent: {}, name: {:?}", parent, name);

        let bindings = self.namespace_manager.bindings.lock().unwrap();
        println!("Current bindings: {:?}", bindings.keys());

        for (inode, (entry_name, entry)) in bindings.iter() {
            println!(
                "Comparing entry name: {:?} with lookup name: {:?}",
                entry_name, name
            );

            println!(
                "inode: {}, name: {:?}, kind: {:?}",
                inode, entry_name, entry.attr.kind
            );

            // Only check files in the root directory for now
            if parent != 1 {
                println!("Skipping non-root parent: {}", parent);
                continue;
            }

            // Compare the actual filename without any path components
            let entry_filename = Path::new(entry_name)
                .file_name()
                .unwrap_or_else(|| entry_name.as_os_str());

            if entry_filename == name {
                println!("Found match for {:?}", name);
                reply.entry(&TTL, &entry.attr, 0);
                return;
            }
        }

        println!("No match found for {:?}", name);
        reply.error(ENOENT);
    }

    // fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
    //     println!("Lookup for parent: {}, name: {:?}", parent, name);

    //     let bindings = self.namespace_manager.bindings.lock().unwrap();
    //     println!(
    //         "Current bindings: {:?}",
    //         bindings.keys().collect::<Vec<_>>()
    //     );

    //     // For each binding
    //     for (inode, (entry_name, entry)) in bindings.iter() {
    //         println!("Checking entry: {:?} against name: {:?}", entry_name, name);
    //         if parent != 1 {
    //             continue; // Only allow lookups in the root directory for now
    //         }
    //         // Compare just the final component of the path
    //         if entry_name == name {
    //             println!("Found match for {:?}", name);
    //             reply.entry(&TTL, &entry.attr, 0);
    //             return;
    //         }
    //     }

    //     println!("No match found for {:?}", name);
    //     reply.error(ENOENT);
    // }
    // fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
    //     println!("Lookup for parent: {}, name: {:?}", parent, name); // DEBUG
    //     let bindings = self.namespace_manager.bindings.lock().unwrap();

    //     println!("Current bindings: {:?}", bindings.keys());

    //     for (_inode, (entry_name, entry)) in bindings.iter() {
    //         println!("Checking entry: {:?}", entry_name);

    //         if parent != 1 {
    //             continue; // Only allow lookups in the root directory for now
    //         }
    //         if entry_name.as_os_str() == name {
    //             println!("Found match for {:?}", name);

    //             reply.entry(&TTL, &entry.attr, 0);
    //             return;
    //         }
    //     }

    //     println!("No match found for {:?}", name);
    //     reply.error(ENOENT);
    // }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let bindings = self.namespace_manager.bindings.lock().unwrap();
        if let Some((_, entry)) = bindings.get(&ino) {
            reply.attr(&TTL, &entry.attr);
        } else {
            reply.error(ENOENT);
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        _size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        let bindings = self.namespace_manager.bindings.lock().unwrap();
        if let Some((_, entry)) = bindings.get(&ino) {
            if let Some(ref content) = entry.content {
                reply.data(&content[offset as usize..]);
            } else {
                reply.error(ENOENT);
            }
        } else {
            reply.error(ENOENT);
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let bindings = self.namespace_manager.bindings.lock().unwrap();
        if ino != 1 {
            reply.error(ENOENT);
            return;
        }

        let mut entries = vec![
            (1, FileType::Directory, "."),
            (1, FileType::Directory, ".."),
        ];

        for (inode, (entry_name, entry)) in bindings.iter() {
            if entry.attr.ino != ino {
                continue;
            }
            entries.push((*inode, entry.attr.kind, entry_name.to_str().unwrap()));
        }

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                break;
            }
        }
        reply.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuser::FileAttr;
    use tempfile::tempdir;

    // Helper function to create a test filesystem
    fn setup_test_fs() -> Result<NineP> {
        let temp_dir = tempdir()?;
        NineP::new(temp_dir.path().to_path_buf())
    }

    // Helper function to create a test file entry
    fn create_test_file_entry(
        ino: u64,
        name: &str,
        content: Option<Vec<u8>>,
    ) -> (OsString, BoundEntry) {
        let attr = FileAttr {
            ino,
            size: content.as_ref().map_or(0, |c| c.len() as u64),
            blocks: 1,
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o644,
            nlink: 1,
            uid: 1000,
            gid: 1000,
            rdev: 0,
            flags: 0,
            blksize: 512,
        };

        (OsString::from(name), BoundEntry { attr, content })
    }

    #[test]
    fn test_create_filesystem() -> Result<()> {
        let fs = setup_test_fs()?;
        let bindings = fs.namespace_manager.bindings.lock().unwrap();

        // Should have exactly one entry (the root directory)
        assert_eq!(bindings.len(), 1);

        // That entry should be inode 1 (root directory)
        assert!(bindings.contains_key(&1));

        // Verify it's a directory
        if let Some((_, entry)) = bindings.get(&1) {
            assert_eq!(entry.attr.kind, FileType::Directory);
        } else {
            panic!("Root directory not found");
        }
        Ok(())
    }

    #[test]
    fn test_file_attributes() -> Result<()> {
        let fs = setup_test_fs()?;
        let content = b"Hello, World!".to_vec();
        let (name, entry) = create_test_file_entry(2, "test.txt", Some(content.clone()));

        let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
        bindings.insert(2, (name, entry.clone()));

        assert_eq!(entry.attr.size, 13); // "Hello, World!".len()
        assert_eq!(entry.attr.kind, FileType::RegularFile);
        assert_eq!(entry.content.unwrap(), content);
        Ok(())
    }

    #[test]
    fn test_root_directory_attributes() -> Result<()> {
        let fs = setup_test_fs()?;
        let bindings = fs.namespace_manager.bindings.lock().unwrap();

        // Root directory should have inode 1
        let root_exists = bindings.iter().any(|(ino, _)| *ino == 1);
        assert!(root_exists);
        Ok(())
    }

    #[test]
    fn test_file_lookup() -> Result<()> {
        let fs = setup_test_fs()?;
        let (name, entry) = create_test_file_entry(2, "test.txt", Some(b"content".to_vec()));

        let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
        bindings.insert(2, (name, entry));

        // File should be findable by inode
        assert!(bindings.contains_key(&2));

        // Content check
        if let Some((_, entry)) = bindings.get(&2) {
            assert_eq!(entry.content.as_ref().unwrap(), b"content");
        } else {
            panic!("File not found");
        }
        Ok(())
    }

    #[test]
    fn test_directory_listing() -> Result<()> {
        let fs = setup_test_fs()?;
        let (name1, entry1) = create_test_file_entry(2, "test1.txt", Some(b"content1".to_vec()));
        let (name2, entry2) = create_test_file_entry(3, "test2.txt", Some(b"content2".to_vec()));

        let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
        bindings.insert(2, (name1, entry1));
        bindings.insert(3, (name2, entry2));

        // Should have root dir (1) plus our two files
        assert_eq!(bindings.len(), 3);

        // Check file entries exist
        assert!(bindings.contains_key(&2));
        assert!(bindings.contains_key(&3));

        // Verify file names
        let files: Vec<_> = bindings
            .iter()
            .filter(|(ino, _)| **ino != 1) // Exclude root directory
            .map(|(_, (name, _))| name.to_str().unwrap())
            .collect();

        assert!(files.contains(&"test1.txt"));
        assert!(files.contains(&"test2.txt"));
        Ok(())
    }

    #[test]
    fn test_file_content() -> Result<()> {
        let fs = setup_test_fs()?;
        let content = b"Hello, World!".to_vec();
        let (name, entry) = create_test_file_entry(2, "test.txt", Some(content.clone()));

        let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
        bindings.insert(2, (name, entry));

        if let Some((_, entry)) = bindings.get(&2) {
            assert_eq!(entry.content.as_ref().unwrap(), &content);
            assert_eq!(entry.attr.size, content.len() as u64);
        } else {
            panic!("File not found");
        }
        Ok(())
    }

    #[test]
    fn test_empty_file() -> Result<()> {
        let fs = setup_test_fs()?;
        let (name, entry) = create_test_file_entry(2, "empty.txt", None);

        let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
        bindings.insert(2, (name, entry));

        if let Some((_, entry)) = bindings.get(&2) {
            assert!(entry.content.is_none());
            assert_eq!(entry.attr.size, 0);
        } else {
            panic!("File not found");
        }
        Ok(())
    }
}
