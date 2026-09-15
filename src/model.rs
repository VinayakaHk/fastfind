use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

impl EntryKind {
    pub fn as_i64(self) -> i64 {
        match self {
            Self::File => 1,
            Self::Directory => 2,
            Self::Symlink => 3,
            Self::Other => 4,
        }
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::File,
            2 => Self::Directory,
            3 => Self::Symlink,
            _ => Self::Other,
        }
    }
}

#[derive(Debug)]
pub struct IndexedEntry {
    pub relative_path: Vec<u8>,
    pub display_path: String,
    pub name: Vec<u8>,
    pub display_name: String,
    pub normalized_name: String,
    pub kind: EntryKind,
    pub device_id: u64,
    pub inode: u64,
    pub size: u64,
    pub mtime_ns: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_ns: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RootStatus {
    pub id: i64,
    pub path: String,
    pub device_id: u64,
    pub entry_count: u64,
    pub state: String,
    pub last_scan_ms: Option<i64>,
    pub last_scan_duration_ms: Option<u64>,
    pub error_count: u64,
}
