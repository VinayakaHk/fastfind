use crate::db::Index;
use crate::model::{EntryKind, IndexedEntry};
use crate::telemetry::{now_ms, EventSink, TelemetryEvent};
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const BATCH_SIZE: usize = 2_000;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Serialize)]
pub struct ScanOutcome {
    pub root_id: i64,
    pub entries_seen: u64,
    pub removed_entries: u64,
    pub errors: u64,
    pub elapsed_ms: u64,
    pub synchronized: bool,
}

pub fn scan(
    index: &mut Index,
    requested_root: &Path,
    exclusions: &[PathBuf],
    events: &EventSink,
) -> Result<ScanOutcome> {
    let root = requested_root
        .canonicalize()
        .with_context(|| format!("cannot access {}", requested_root.display()))?;
    let root_metadata = fs::metadata(&root)
        .with_context(|| format!("cannot read metadata for {}", root.display()))?;
    if !root_metadata.is_dir() {
        anyhow::bail!("{} is not a directory", root.display());
    }

    let root_id = index.register_root(&root, root_metadata.dev())?;
    index.begin_scan(root_id)?;
    events.emit(&TelemetryEvent::HealthChanged {
        schema_version: 1,
        timestamp_ms: now_ms(),
        state: "scanning",
        reason: None,
    });
    events.emit(&TelemetryEvent::ScanStarted {
        schema_version: 1,
        timestamp_ms: now_ms(),
        root_id,
    });

    let generation = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64;
    let started = Instant::now();
    let mut last_progress = Instant::now();
    let mut entries_seen = 0_u64;
    let mut errors = 0_u64;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let excluded = normalize_exclusions(exclusions);

    let walker = WalkDir::new(&root)
        .follow_links(false)
        .same_file_system(true)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_excluded(entry.path(), &excluded));

    for item in walker {
        let directory_entry = match item {
            Ok(entry) => entry,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        if directory_entry.depth() == 0 {
            continue;
        }

        let path = directory_entry.path();
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        let relative = match path.strip_prefix(&root) {
            Ok(relative) => relative,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
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
        let display_name = directory_entry.file_name().to_string_lossy().into_owned();
        let mtime_ns = metadata
            .mtime()
            .saturating_mul(1_000_000_000)
            .saturating_add(metadata.mtime_nsec());

        batch.push(IndexedEntry {
            relative_path: relative.as_os_str().as_bytes().to_vec(),
            display_path: relative.to_string_lossy().into_owned(),
            name: directory_entry.file_name().as_bytes().to_vec(),
            normalized_name: display_name.to_lowercase(),
            display_name,
            kind,
            device_id: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            mtime_ns,
        });
        entries_seen += 1;

        if batch.len() >= BATCH_SIZE {
            index.upsert_batch(root_id, generation, &batch)?;
            batch.clear();
        }
        if last_progress.elapsed() >= PROGRESS_INTERVAL {
            emit_progress(events, root_id, entries_seen, errors, started.elapsed());
            last_progress = Instant::now();
        }
    }

    if !batch.is_empty() {
        index.upsert_batch(root_id, generation, &batch)?;
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let removed_entries =
        index.finish_scan(root_id, generation, now_ms() as i64, elapsed_ms, errors)?;
    let synchronized = errors == 0;
    events.emit(&TelemetryEvent::ScanCompleted {
        schema_version: 1,
        timestamp_ms: now_ms(),
        root_id,
        entries_seen,
        removed_entries,
        errors,
        elapsed_ms,
        synchronized,
    });
    events.emit(&TelemetryEvent::HealthChanged {
        schema_version: 1,
        timestamp_ms: now_ms(),
        state: if synchronized { "live" } else { "degraded" },
        reason: if synchronized {
            None
        } else {
            Some("scan_errors")
        },
    });

    Ok(ScanOutcome {
        root_id,
        entries_seen,
        removed_entries,
        errors,
        elapsed_ms,
        synchronized,
    })
}

fn emit_progress(
    events: &EventSink,
    root_id: i64,
    entries_seen: u64,
    errors: u64,
    elapsed: Duration,
) {
    let elapsed_ms = elapsed.as_millis() as u64;
    let entries_per_second = if elapsed_ms == 0 {
        0
    } else {
        entries_seen.saturating_mul(1_000) / elapsed_ms
    };
    events.emit(&TelemetryEvent::ScanProgress {
        schema_version: 1,
        timestamp_ms: now_ms(),
        root_id,
        entries_seen,
        errors,
        elapsed_ms,
        entries_per_second,
    });
}

fn normalize_exclusions(exclusions: &[PathBuf]) -> Vec<PathBuf> {
    exclusions
        .iter()
        .flat_map(|path| {
            let mut paths = vec![path.clone()];
            let raw = path.as_os_str().as_bytes();
            let mut wal = raw.to_vec();
            wal.extend_from_slice(b"-wal");
            paths.push(PathBuf::from(std::ffi::OsString::from_vec(wal)));
            let mut shm = raw.to_vec();
            shm.extend_from_slice(b"-shm");
            paths.push(PathBuf::from(std::ffi::OsString::from_vec(shm)));
            paths
        })
        .collect()
}

fn is_excluded(path: &Path, exclusions: &[PathBuf]) -> bool {
    exclusions.iter().any(|excluded| path == excluded)
}

use std::os::unix::ffi::OsStringExt;
