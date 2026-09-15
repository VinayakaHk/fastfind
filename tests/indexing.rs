use fastfind::{scan, EventSink, Index, OutputMode};
use std::fs;
use tempfile::tempdir;

#[test]
fn scan_search_and_reconcile_deleted_entries() {
    let workspace = tempdir().unwrap();
    let root = workspace.path().join("files");
    fs::create_dir_all(root.join("reports/annual")).unwrap();
    fs::write(root.join("reports/annual/report-2026.txt"), b"not indexed").unwrap();
    fs::write(root.join("notes.md"), b"not indexed").unwrap();

    let database = workspace.path().join("index/index.db");
    let mut index = Index::open(&database).unwrap();
    let events = EventSink::new(OutputMode::Silent);
    let first = scan(&mut index, &root, &[database.clone()], &events).unwrap();

    assert!(first.synchronized);
    assert_eq!(first.entries_seen, 4);
    let results = index.search("port-20", 20).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].path.ends_with("report-2026.txt"));

    fs::remove_file(root.join("notes.md")).unwrap();
    let second = scan(&mut index, &root, &[database], &events).unwrap();
    assert_eq!(second.removed_entries, 1);
    assert!(index.search("notes", 20).unwrap().is_empty());
}

#[test]
fn short_queries_use_substring_fallback() {
    let workspace = tempdir().unwrap();
    let root = workspace.path().join("files");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("README"), b"").unwrap();

    let database = workspace.path().join("index.db");
    let mut index = Index::open(&database).unwrap();
    scan(
        &mut index,
        &root,
        &[database],
        &EventSink::new(OutputMode::Silent),
    )
    .unwrap();

    let results = index.search("ad", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "README");
}
