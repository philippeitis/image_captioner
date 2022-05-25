use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::default::Default;
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct EntryData {
    hash: u64,
    path: OsString,
    modify_time: SystemTime,
}

impl Default for EntryData {
    fn default() -> Self {
        Self {
            hash: 0,
            path: OsString::new(),
            modify_time: SystemTime::now(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum FileSystem {
    Directory {
        info: EntryData,
        entries: HashSet<FileSystem>,
    },
    File {
        info: EntryData,
    },
}

impl Hash for FileSystem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_hash().hash(state);
    }
}

impl PartialEq<Self> for FileSystem {
    fn eq(&self, other: &Self) -> bool {
        self.get_hash() == other.get_hash()
    }
}

impl Eq for FileSystem {}

impl Default for FileSystem {
    fn default() -> Self {
        Self::Directory {
            info: Default::default(),
            entries: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct FileSystemDiff {
    pub added: Vec<PathBuf>,
    pub removed: Vec<PathBuf>,
}

impl FileSystem {
    pub(crate) fn empty() -> Self {
        FileSystem::Directory {
            info: EntryData {
                hash: 0,
                path: OsString::default(),
                modify_time: SystemTime::now(),
            },
            entries: Default::default(),
        }
    }

    fn info(&self) -> &EntryData {
        match self {
            FileSystem::Directory { info, .. } => info,
            FileSystem::File { info } => info,
        }
    }

    fn get_hash(&self) -> u64 {
        self.info().hash
    }

    pub(crate) fn deep_scan<P: AsRef<Path>>(path: P) -> std::io::Result<FileSystem> {
        let path = path.as_ref();
        let metadata = path.metadata()?;
        let path_chunk = path
            .file_name()
            .unwrap_or_else(|| path.parent().unwrap().as_ref())
            .to_os_string();
        let modify_time = metadata.modified()?;

        let mut hasher = DefaultHasher::new();
        path_chunk.hash(&mut hasher);
        modify_time.hash(&mut hasher);

        let dir_iter = match path.read_dir() {
            Ok(dir_iter) => dir_iter,
            Err(_) => {
                return Ok(FileSystem::File {
                    info: EntryData {
                        hash: hasher.finish(),
                        path: path_chunk,
                        modify_time,
                    },
                })
            }
        };

        // we already read whole file into libraw - simultaneously hash file
        let mut entries = HashSet::new();
        for entry in dir_iter {
            let entry = FileSystem::deep_scan(entry?.path())?;
            entries.insert(entry);
        }
        let mut hashes = entries.iter().map(|x| x.get_hash()).collect::<Vec<_>>();
        hashes.sort();
        hashes.hash(&mut hasher);
        Ok(FileSystem::Directory {
            info: EntryData {
                hash: hasher.finish(),
                path: path_chunk,
                modify_time,
            },
            entries,
        })
    }

    fn files<P: AsRef<Path>>(&self, parent: P) -> Vec<PathBuf> {
        let path = {
            let mut path = parent.as_ref().to_path_buf();
            path.push(&self.info().path);
            path
        };
        match self {
            FileSystem::Directory { entries, .. } => entries
                .iter()
                .map(|entry| entry.files(&path))
                .flatten()
                .collect(),
            FileSystem::File { .. } => vec![path],
        }
    }

    /// Returns a diff between this and the `after` FileSystem, where `self` is the before filesystem.
    /// Files present in `self` and not `after` are "removed", and files present in `after` and not
    /// `self` are "added" - files in both are not present in either.
    pub(crate) fn diff<P: AsRef<Path>>(&self, after: &Self, parent: P) -> FileSystemDiff {
        // TODO: This should be more intelligent
        if self == after {
            return FileSystemDiff {
                added: vec![],
                removed: vec![],
            };
        }

        match (self, after) {
            (
                FileSystem::Directory {
                    info: info_b,
                    entries: entries_b,
                },
                FileSystem::Directory {
                    info: info_a,
                    entries: entries_a,
                },
            ) => {
                if info_b.path != info_a.path {
                    FileSystemDiff {
                        removed: self.files(&parent),
                        added: after.files(&parent),
                    }
                } else {
                    let mut entries_b: Vec<_> = entries_b.iter().collect();
                    let mut entries_a: Vec<_> = entries_a.iter().collect();
                    entries_b.sort_by_key(|fs| &fs.info().path);
                    entries_a.sort_by_key(|fs| &fs.info().path);

                    let mut added = vec![];
                    let mut removed = vec![];
                    while let (Some(before_entry), Some(after_entry)) =
                        (entries_b.last(), entries_a.last())
                    {
                        match before_entry.info().path.cmp(&after_entry.info().path) {
                            Ordering::Less => {
                                added.append(&mut after_entry.files(&parent));
                                entries_a.pop();
                            }
                            Ordering::Equal => {
                                let path = {
                                    let mut path = parent.as_ref().to_path_buf();
                                    path.push(&self.info().path);
                                    path
                                };
                                let mut diff = before_entry.diff(after_entry, path);
                                added.append(&mut diff.added);
                                removed.append(&mut diff.removed);
                                entries_b.pop();
                                entries_a.pop();
                            }
                            Ordering::Greater => {
                                removed.append(&mut before_entry.files(&parent));
                                entries_b.pop();
                            }
                        }
                    }
                    FileSystemDiff { added, removed }
                }
            }
            (_, _) => FileSystemDiff {
                removed: self.files(&parent),
                added: after.files(&parent),
            },
        }
    }
}

// Want mutable FileTree?
// Eg.  when adding a particular image from new filetree, add to old filetree
//
