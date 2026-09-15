use crate::model::{EntryKind, IndexedEntry, RootStatus};
use crate::Index;
use anyhow::Result;
use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

pub fn apply_event(
    index: &mut Index,
    roots: &[RootStatus],
    event: &Event,
    database: &Path,
) -> Result<u64> {
    let mut changed = 0;
    let recursive = matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::To | RenameMode::Both))
    );
    let rename_mode = match event.kind {
        EventKind::Modify(ModifyKind::Name(mode)) => Some(mode),
        _ => None,
    };

    for (position, path) in event.paths.iter().enumerate() {
        if is_database_path(path, database) {
            continue;
        }
        let Some(root) = matching_root(roots, path) else {
            continue;
        };
        let root_path = Path::new(&root.path);
        let relative = match path.strip_prefix(root_path) {
            Ok(value) if !value.as_os_str().is_empty() => value,
            _ => continue,
        };
        let relative_bytes = relative.as_os_str().as_bytes();
        let is_old_rename_path = matches!(rename_mode, Some(RenameMode::From))
            || matches!(rename_mode, Some(RenameMode::Both)) && position == 0
            || matches!(rename_mode, Some(RenameMode::Any))
                && event.paths.len() > 1
                && position == 0;

        if matches!(event.kind, EventKind::Remove(_)) || is_old_rename_path || !path.exists() {
            changed += index.delete_subtree(root.id, relative_bytes)?;
            continue;
        }

        if recursive && path.is_dir() {
            changed += index.delete_subtree(root.id, relative_bytes)?;
            let entries = collect_subtree(root_path, path)?;
            changed += entries.len() as u64;
            index.upsert_batch(root.id, generation(), &entries)?;
        } else if let Some(entry) = entry_for_path(root_path, path)? {
            index.upsert_batch(root.id, generation(), &[entry])?;
            changed += 1;
        }
    }
    Ok(changed)
}

fn matching_root<'a>(roots: &'a [RootStatus], path: &Path) -> Option<&'a RootStatus> {
    roots
        .iter()
        .filter(|root| path.starts_with(&root.path))
        .max_by_key(|root| Path::new(&root.path).components().count())
}

fn is_database_path(path: &Path, database: &Path) -> bool {
    database
        .parent()
        .is_some_and(|directory| path.starts_with(directory))
}

fn collect_subtree(root: &Path, subtree: &Path) -> Result<Vec<IndexedEntry>> {
    let root_device = fs::metadata(root)?.dev();
    let mut entries = Vec::new();
    for item in WalkDir::new(subtree).follow_links(false) {
        let item = match item {
            Ok(item) => item,
            Err(_) => continue,
        };
        let metadata = match fs::symlink_metadata(item.path()) {
            Ok(metadata) if metadata.dev() == root_device => metadata,
            _ => continue,
        };
        if let Some(entry) = build_entry(root, item.path(), &metadata) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn entry_for_path(root: &Path, path: &Path) -> Result<Option<IndexedEntry>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    Ok(build_entry(root, path, &metadata))
}

fn build_entry(root: &Path, path: &Path, metadata: &fs::Metadata) -> Option<IndexedEntry> {
    let relative = path.strip_prefix(root).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    let name = path.file_name()?;
    let display_name = name.to_string_lossy().into_owned();
    let file_type = metadata.file_type();
    let kind = if file_type.is_file() {
        EntryKind::File
    } else if file_type.is_dir() {
        EntryKind::Directory
    } else if file_type.is_symlink() {
        EntryKind::Symlink
    } else {
        EntryKind::Other
    };
    Some(IndexedEntry {
        relative_path: relative.as_os_str().as_bytes().to_vec(),
        display_path: relative.to_string_lossy().into_owned(),
        name: name.as_bytes().to_vec(),
        normalized_name: display_name.to_lowercase(),
        display_name,
        kind,
        device_id: metadata.dev(),
        inode: metadata.ino(),
        size: metadata.size(),
        mtime_ns: metadata
            .mtime()
            .saturating_mul(1_000_000_000)
            .saturating_add(metadata.mtime_nsec()),
    })
}

fn generation() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64
}
