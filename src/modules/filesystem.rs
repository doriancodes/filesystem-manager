

// #[derive(Debug, Clone)]
// pub struct BoundEntry {
//     pub attr: FileAttr,
//     pub content: Option<Vec<u8>>,
// }

// #[derive(Debug, Clone)]
// pub struct HelloFS {
//     pub namespace_manager: NamespaceManager,
// }

// impl HelloFS {
//     pub fn new(path: PathBuf) -> Result<Self> {
//         let ns_mngr = NamespaceManager::new(path)?;

//         Ok(HelloFS {
//             namespace_manager: ns_mngr,
//         })
//     }
// }

// impl Filesystem for HelloFS {
//     fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
//         println!("Lookup for parent: {}, name: {:?}", parent, name); // DEBUG

//         let bindings = self.namespace_manager.bindings.lock().unwrap();

//         for (_inode, (entry_name, entry)) in bindings.iter() {
//             if parent != 1 {
//                 continue; // Only allow lookups in the root directory for now
//             }
//             if entry_name.as_os_str() == name {
//                 println!(
//                     "Found entry for name: {:?}, inode: {}",
//                     name, entry.attr.ino
//                 ); // DEBUG
//                 reply.entry(&TTL, &entry.attr, 0);
//                 return;
//             }
//         }
//         println!("Entry not found for name: {:?}", name); // DEBUG
//         reply.error(ENOENT);
//     }

//     fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
//         let bindings = self.namespace_manager.bindings.lock().unwrap();
//         if let Some((_, entry)) = bindings.get(&ino) {
//             reply.attr(&TTL, &entry.attr);
//         } else {
//             reply.error(ENOENT);
//         }
//     }

//     fn read(
//         &mut self,
//         _req: &Request,
//         ino: u64,
//         _fh: u64,
//         offset: i64,
//         _size: u32,
//         _flags: i32,
//         _lock: Option<u64>,
//         reply: ReplyData,
//     ) {
//         let bindings = self.namespace_manager.bindings.lock().unwrap();
//         if let Some((_, entry)) = bindings.get(&ino) {
//             if let Some(ref content) = entry.content {
//                 reply.data(&content[offset as usize..]);
//             } else {
//                 reply.error(ENOENT);
//             }
//         } else {
//             reply.error(ENOENT);
//         }
//     }

//     fn readdir(
//         &mut self,
//         _req: &Request,
//         ino: u64,
//         _fh: u64,
//         offset: i64,
//         mut reply: ReplyDirectory,
//     ) {
//         let bindings = self.namespace_manager.bindings.lock().unwrap();
//         if ino != 1 {
//             reply.error(ENOENT);
//             return;
//         }

//         let mut entries = vec![
//             (1, FileType::Directory, "."),
//             (1, FileType::Directory, ".."),
//         ];

//         for (inode, (entry_name, entry)) in bindings.iter() {
//             if entry.attr.ino != ino {
//                 continue;
//             }
//             entries.push((*inode, entry.attr.kind, entry_name.to_str().unwrap()));
//         }

//         for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
//             if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
//                 break;
//             }
//         }
//         reply.ok();
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use fuser::FileAttr;
//     use tempfile::tempdir;

//     // Helper function to create a test filesystem
//     fn setup_test_fs() -> Result<HelloFS> {
//         let temp_dir = tempdir()?;
//         HelloFS::new(temp_dir.path().to_path_buf())
//     }

//     // Helper function to create a test file entry
//     fn create_test_file_entry(
//         ino: u64,
//         name: &str,
//         content: Option<Vec<u8>>,
//     ) -> (OsString, BoundEntry) {
//         let attr = FileAttr {
//             ino,
//             size: content.as_ref().map_or(0, |c| c.len() as u64),
//             blocks: 1,
//             atime: UNIX_EPOCH,
//             mtime: UNIX_EPOCH,
//             ctime: UNIX_EPOCH,
//             crtime: UNIX_EPOCH,
//             kind: FileType::RegularFile,
//             perm: 0o644,
//             nlink: 1,
//             uid: 1000,
//             gid: 1000,
//             rdev: 0,
//             flags: 0,
//             blksize: 512,
//         };

//         (OsString::from(name), BoundEntry { attr, content })
//     }

//     #[test]
//     fn test_create_filesystem() -> Result<()> {
//         let fs = setup_test_fs()?;
//         let bindings = fs.namespace_manager.bindings.lock().unwrap();

//         // Should have exactly one entry (the root directory)
//         assert_eq!(bindings.len(), 1);

//         // That entry should be inode 1 (root directory)
//         assert!(bindings.contains_key(&1));

//         // Verify it's a directory
//         if let Some((_, entry)) = bindings.get(&1) {
//             assert_eq!(entry.attr.kind, FileType::Directory);
//         } else {
//             panic!("Root directory not found");
//         }
//         Ok(())
//     }

//     #[test]
//     fn test_file_attributes() -> Result<()> {
//         let fs = setup_test_fs()?;
//         let content = b"Hello, World!".to_vec();
//         let (name, entry) = create_test_file_entry(2, "test.txt", Some(content.clone()));

//         let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
//         bindings.insert(2, (name, entry.clone()));

//         assert_eq!(entry.attr.size, 13); // "Hello, World!".len()
//         assert_eq!(entry.attr.kind, FileType::RegularFile);
//         assert_eq!(entry.content.unwrap(), content);
//         Ok(())
//     }

//     #[test]
//     fn test_root_directory_attributes() -> Result<()> {
//         let fs = setup_test_fs()?;
//         let bindings = fs.namespace_manager.bindings.lock().unwrap();

//         // Root directory should have inode 1
//         let root_exists = bindings.iter().any(|(ino, _)| *ino == 1);
//         assert!(root_exists);
//         Ok(())
//     }

//     #[test]
//     fn test_file_lookup() -> Result<()> {
//         let fs = setup_test_fs()?;
//         let (name, entry) = create_test_file_entry(2, "test.txt", Some(b"content".to_vec()));

//         let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
//         bindings.insert(2, (name, entry));

//         // File should be findable by inode
//         assert!(bindings.contains_key(&2));

//         // Content check
//         if let Some((_, entry)) = bindings.get(&2) {
//             assert_eq!(entry.content.as_ref().unwrap(), b"content");
//         } else {
//             panic!("File not found");
//         }
//         Ok(())
//     }

//     #[test]
//     fn test_directory_listing() -> Result<()> {
//         let fs = setup_test_fs()?;
//         let (name1, entry1) = create_test_file_entry(2, "test1.txt", Some(b"content1".to_vec()));
//         let (name2, entry2) = create_test_file_entry(3, "test2.txt", Some(b"content2".to_vec()));

//         let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
//         bindings.insert(2, (name1, entry1));
//         bindings.insert(3, (name2, entry2));

//         // Should have root dir (1) plus our two files
//         assert_eq!(bindings.len(), 3);

//         // Check file entries exist
//         assert!(bindings.contains_key(&2));
//         assert!(bindings.contains_key(&3));

//         // Verify file names
//         let files: Vec<_> = bindings
//             .iter()
//             .filter(|(ino, _)| **ino != 1) // Exclude root directory
//             .map(|(_, (name, _))| name.to_str().unwrap())
//             .collect();

//         assert!(files.contains(&"test1.txt"));
//         assert!(files.contains(&"test2.txt"));
//         Ok(())
//     }

//     #[test]
//     fn test_file_content() -> Result<()> {
//         let fs = setup_test_fs()?;
//         let content = b"Hello, World!".to_vec();
//         let (name, entry) = create_test_file_entry(2, "test.txt", Some(content.clone()));

//         let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
//         bindings.insert(2, (name, entry));

//         if let Some((_, entry)) = bindings.get(&2) {
//             assert_eq!(entry.content.as_ref().unwrap(), &content);
//             assert_eq!(entry.attr.size, content.len() as u64);
//         } else {
//             panic!("File not found");
//         }
//         Ok(())
//     }

//     #[test]
//     fn test_empty_file() -> Result<()> {
//         let fs = setup_test_fs()?;
//         let (name, entry) = create_test_file_entry(2, "empty.txt", None);

//         let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
//         bindings.insert(2, (name, entry));

//         if let Some((_, entry)) = bindings.get(&2) {
//             assert!(entry.content.is_none());
//             assert_eq!(entry.attr.size, 0);
//         } else {
//             panic!("File not found");
//         }
//         Ok(())
//     }
// }

// // #[cfg(test)]
// // mod tests {
// //     use super::*;
// //     use std::ffi::OsString;
// //     use tempfile::TempDir;

// //     struct TestReply {
// //         attr: Option<FileAttr>,
// //         error: Option<i32>,
// //         data: Option<Vec<u8>>,
// //     }

// //     impl ReplyEntry for TestReply {
// //         fn error(&mut self, err: i32) {
// //             self.error = Some(err);
// //         }
// //         fn entry(&mut self, _: &std::time::Duration, attr: &FileAttr, _: u64) {
// //             self.attr = Some(*attr);
// //         }
// //     }

// //     impl ReplyAttr for TestReply {
// //         fn attr(&mut self, _: &std::time::Duration, attr: &FileAttr) {
// //             self.attr = Some(*attr);
// //         }
// //         fn error(&mut self, err: i32) {
// //             self.error = Some(err);
// //         }
// //     }

// //     impl ReplyData for TestReply {
// //         fn data(&mut self, data: &[u8]) {
// //             self.data = Some(data.to_vec());
// //         }
// //         fn error(&mut self, err: i32) {
// //             self.error = Some(err);
// //         }
// //     }

// //     fn setup_test_fs() -> (TempDir, HelloFS) {
// //         let temp_dir = tempfile::tempdir().unwrap();
// //         let fs = HelloFS::new(temp_dir.path().to_path_buf()).unwrap();
// //         (temp_dir, fs)
// //     }

// //     #[test]
// //     fn test_filesystem_creation() {
// //         let (_temp_dir, fs) = setup_test_fs();
// //         assert!(fs
// //             .namespace_manager
// //             .bindings
// //             .lock()
// //             .unwrap()
// //             .contains_key(&1));
// //     }

// //     #[test]
// //     fn test_lookup() {
// //         let (_temp_dir, mut fs) = setup_test_fs();

// //         {
// //             let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
// //             bindings.insert(
// //                 2,
// //                 (
// //                     OsString::from("test.txt"),
// //                     BoundEntry {
// //                         attr: FileAttr {
// //                             ino: 2,
// //                             size: 0,
// //                             blocks: 0,
// //                             atime: UNIX_EPOCH,
// //                             mtime: UNIX_EPOCH,
// //                             ctime: UNIX_EPOCH,
// //                             crtime: UNIX_EPOCH,
// //                             kind: FileType::RegularFile,
// //                             perm: 0o644,
// //                             nlink: 1,
// //                             uid: 501,
// //                             gid: 20,
// //                             rdev: 0,
// //                             flags: 0,
// //                             blksize: 512,
// //                         },
// //                         content: Some(Vec::new()),
// //                     },
// //                 ),
// //             );
// //         }

// //         let mut reply = TestReply {
// //             attr: None,
// //             error: None,
// //             data: None,
// //         };
// //         fs.lookup(&Request::new(0), 1, OsStr::new("test.txt"), &mut reply);

// //         assert!(reply.error.is_none());
// //         assert!(reply.attr.is_some());
// //         assert_eq!(reply.attr.unwrap().ino, 2);
// //     }

// //     #[test]
// //     fn test_read_file() {
// //         let (_temp_dir, mut fs) = setup_test_fs();
// //         let test_content = b"Hello, world!".to_vec();

// //         {
// //             let mut bindings = fs.namespace_manager.bindings.lock().unwrap();
// //             bindings.insert(
// //                 2,
// //                 (
// //                     OsString::from("test.txt"),
// //                     BoundEntry {
// //                         attr: FileAttr {
// //                             ino: 2,
// //                             size: test_content.len() as u64,
// //                             blocks: 1,
// //                             atime: UNIX_EPOCH,
// //                             mtime: UNIX_EPOCH,
// //                             ctime: UNIX_EPOCH,
// //                             crtime: UNIX_EPOCH,
// //                             kind: FileType::RegularFile,
// //                             perm: 0o644,
// //                             nlink: 1,
// //                             uid: 501,
// //                             gid: 20,
// //                             rdev: 0,
// //                             flags: 0,
// //                             blksize: 512,
// //                         },
// //                         content: Some(test_content.clone()),
// //                     },
// //                 ),
// //             );
// //         }

// //         let mut reply = TestReply {
// //             attr: None,
// //             error: None,
// //             data: None,
// //         };
// //         fs.read(
// //             &Request::new(0),
// //             2,
// //             0,
// //             0,
// //             test_content.len() as u32,
// //             0,
// //             None,
// //             &mut reply,
// //         );

// //         assert!(reply.error.is_none());
// //         assert_eq!(reply.data.unwrap(), test_content);
// //     }
// // }
