use crate::model::{IndexedEntry, RootStatus, SearchResult};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Index {
    connection: Connection,
    path: PathBuf,
}

impl Index {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let connection = Connection::open(path)
            .with_context(|| format!("failed to open index {}", path.display()))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA temp_store=MEMORY;

             CREATE TABLE IF NOT EXISTS roots (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE,
                 device_id INTEGER NOT NULL,
                 state TEXT NOT NULL DEFAULT 'initializing',
                 last_scan_ms INTEGER,
                 last_scan_duration_ms INTEGER,
                 error_count INTEGER NOT NULL DEFAULT 0
             );

             CREATE TABLE IF NOT EXISTS entries (
                 id INTEGER PRIMARY KEY,
                 root_id INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE,
                 relative_path BLOB NOT NULL,
                 display_path TEXT NOT NULL,
                 name BLOB NOT NULL,
                 display_name TEXT NOT NULL,
                 normalized_name TEXT NOT NULL,
                 kind INTEGER NOT NULL,
                 device_id INTEGER NOT NULL,
                 inode INTEGER NOT NULL,
                 size INTEGER NOT NULL,
                 mtime_ns INTEGER NOT NULL,
                 scan_generation INTEGER NOT NULL,
                 UNIQUE(root_id, relative_path)
             );

             CREATE INDEX IF NOT EXISTS entries_root_generation
                 ON entries(root_id, scan_generation);
             CREATE INDEX IF NOT EXISTS entries_root_parent_path
                 ON entries(root_id, display_path);

             CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
                 normalized_name,
                 display_path,
                 content='entries',
                 content_rowid='id',
                 tokenize='trigram'
             );

             CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
                 INSERT INTO entries_fts(rowid, normalized_name, display_path)
                 VALUES (new.id, new.normalized_name, new.display_path);
             END;
             CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
                 INSERT INTO entries_fts(entries_fts, rowid, normalized_name, display_path)
                 VALUES ('delete', old.id, old.normalized_name, old.display_path);
             END;
             CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE ON entries BEGIN
                 INSERT INTO entries_fts(entries_fts, rowid, normalized_name, display_path)
                 VALUES ('delete', old.id, old.normalized_name, old.display_path);
                 INSERT INTO entries_fts(rowid, normalized_name, display_path)
                 VALUES (new.id, new.normalized_name, new.display_path);
             END;",
        )?;

        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn register_root(&self, path: &Path, device_id: u64) -> Result<i64> {
        let display_path = path.to_string_lossy();
        self.connection.execute(
            "INSERT INTO roots(path, device_id, state)
             VALUES (?1, ?2, 'initializing')
             ON CONFLICT(path) DO UPDATE SET device_id=excluded.device_id",
            params![display_path.as_ref(), device_id as i64],
        )?;
        Ok(self.connection.query_row(
            "SELECT id FROM roots WHERE path=?1",
            [display_path.as_ref()],
            |row| row.get(0),
        )?)
    }

    pub fn begin_scan(&self, root_id: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE roots SET state='scanning', error_count=0 WHERE id=?1",
            [root_id],
        )?;
        Ok(())
    }

    pub fn upsert_batch(
        &mut self,
        root_id: i64,
        generation: i64,
        entries: &[IndexedEntry],
    ) -> Result<()> {
        let transaction = self.connection.transaction()?;
        {
            let mut statement = transaction.prepare_cached(
                "INSERT INTO entries(
                    root_id, relative_path, display_path, name, display_name,
                    normalized_name, kind, device_id, inode, size, mtime_ns,
                    scan_generation
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(root_id, relative_path) DO UPDATE SET
                    display_path=excluded.display_path,
                    name=excluded.name,
                    display_name=excluded.display_name,
                    normalized_name=excluded.normalized_name,
                    kind=excluded.kind,
                    device_id=excluded.device_id,
                    inode=excluded.inode,
                    size=excluded.size,
                    mtime_ns=excluded.mtime_ns,
                    scan_generation=excluded.scan_generation",
            )?;
            for entry in entries {
                statement.execute(params![
                    root_id,
                    &entry.relative_path,
                    &entry.display_path,
                    &entry.name,
                    &entry.display_name,
                    &entry.normalized_name,
                    entry.kind.as_i64(),
                    entry.device_id as i64,
                    entry.inode as i64,
                    entry.size as i64,
                    entry.mtime_ns,
                    generation,
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_subtree(&self, root_id: i64, relative_path: &[u8]) -> Result<u64> {
        let removed = self.connection.execute(
            "DELETE FROM entries
             WHERE root_id=?1 AND (
                 relative_path=?2 OR (
                     length(relative_path)>length(?2)
                     AND substr(relative_path, 1, length(?2))=?2
                     AND substr(relative_path, length(?2)+1, 1)=x'2F'
                 )
             )",
            params![root_id, relative_path],
        )?;
        Ok(removed as u64)
    }

    pub fn finish_scan(
        &self,
        root_id: i64,
        generation: i64,
        completed_at_ms: i64,
        elapsed_ms: u64,
        errors: u64,
    ) -> Result<u64> {
        let synchronized = errors == 0;
        let removed = if synchronized {
            self.connection.execute(
                "DELETE FROM entries WHERE root_id=?1 AND scan_generation<>?2",
                params![root_id, generation],
            )? as u64
        } else {
            0
        };
        self.connection.execute(
            "UPDATE roots
             SET state=?2, last_scan_ms=?3, last_scan_duration_ms=?4, error_count=?5
             WHERE id=?1",
            params![
                root_id,
                if synchronized { "live" } else { "degraded" },
                completed_at_ms,
                elapsed_ms as i64,
                errors as i64,
            ],
        )?;
        Ok(removed)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let normalized = query.to_lowercase();
        let use_fts = normalized.chars().count() >= 3;
        let sql = if use_fts {
            "SELECT e.id, r.path, e.display_path, e.display_name, e.kind, e.size, e.mtime_ns
             FROM entries_fts
             JOIN entries e ON e.id=entries_fts.rowid
             JOIN roots r ON r.id=e.root_id
             WHERE entries_fts MATCH ?1
             ORDER BY length(e.display_name), e.display_name
             LIMIT ?2"
        } else {
            "SELECT e.id, r.path, e.display_path, e.display_name, e.kind, e.size, e.mtime_ns
             FROM entries e
             JOIN roots r ON r.id=e.root_id
             WHERE instr(e.normalized_name, ?1)>0
             ORDER BY length(e.display_name), e.display_name
             LIMIT ?2"
        };
        let search_term = if use_fts {
            format!("normalized_name:\"{}\"", normalized.replace('"', "\"\""))
        } else {
            normalized
        };

        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(params![search_term, limit as i64], |row| {
            let root: String = row.get(1)?;
            let relative: String = row.get(2)?;
            let path = Path::new(&root)
                .join(relative)
                .to_string_lossy()
                .into_owned();
            Ok(SearchResult {
                id: row.get(0)?,
                path,
                name: row.get(3)?,
                kind: crate::model::EntryKind::from_i64(row.get(4)?),
                size: row.get::<_, i64>(5)? as u64,
                modified_ns: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn statuses(&self) -> Result<Vec<RootStatus>> {
        let mut statement = self.connection.prepare(
            "SELECT r.id, r.path, r.device_id,
                    (SELECT count(*) FROM entries e WHERE e.root_id=r.id),
                    r.state, r.last_scan_ms, r.last_scan_duration_ms, r.error_count
             FROM roots r ORDER BY r.path",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(RootStatus {
                id: row.get(0)?,
                path: row.get(1)?,
                device_id: row.get::<_, i64>(2)? as u64,
                entry_count: row.get::<_, i64>(3)? as u64,
                state: row.get(4)?,
                last_scan_ms: row.get(5)?,
                last_scan_duration_ms: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                error_count: row.get::<_, i64>(7)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
