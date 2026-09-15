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

#[test]
fn searches_full_paths_explicitly_or_with_match_path() {
    let workspace = tempdir().unwrap();
    let root = workspace.path().join("files");
    fs::create_dir_all(root.join("My Projects")).unwrap();
    fs::write(root.join("My Projects/annual-report.txt"), b"").unwrap();
    fs::write(root.join("outside.txt"), b"").unwrap();
    let database = workspace.path().join("index.db");
    let mut index = Index::open(&database).unwrap();
    scan(
        &mut index,
        &root,
        &[database],
        &EventSink::new(OutputMode::Silent),
    )
    .unwrap();

    assert!(!index
        .search("projects", 20)
        .unwrap()
        .iter()
        .any(|result| result.name == "annual-report.txt"));
    assert!(index
        .search("path:\"My Projects\"", 20)
        .unwrap()
        .iter()
        .any(|r| r.name == "annual-report.txt"));
    assert_eq!(index.search("path:projects annual", 20).unwrap().len(), 1);
    assert!(index
        .search_with_options(
            "projects",
            20,
            fastfind::query::SearchOptions { match_path: true }
        )
        .unwrap()
        .iter()
        .any(|r| r.name == "annual-report.txt"));
    assert!(index.search("path:", 20).is_err());
}

#[test]
fn supports_case_insensitive_name_and_path_wildcards() {
    let workspace = tempdir().unwrap();
    let root = workspace.path().join("files");
    fs::create_dir_all(root.join("target/logs")).unwrap();
    fs::write(root.join("Report-FINAL.PDF"), b"").unwrap();
    fs::write(root.join("data1.csv"), b"").unwrap();
    fs::write(root.join("data12.csv"), b"").unwrap();
    fs::write(root.join("target/logs/app.log"), b"").unwrap();
    let database = workspace.path().join("index.db");
    let mut index = Index::open(&database).unwrap();
    scan(
        &mut index,
        &root,
        &[database],
        &EventSink::new(OutputMode::Silent),
    )
    .unwrap();

    assert_eq!(index.search("*.pdf", 20).unwrap().len(), 1);
    assert_eq!(index.search("data?.csv", 20).unwrap().len(), 1);
    assert!(index
        .search("path:*target/logs/*.log", 20)
        .unwrap()
        .iter()
        .any(|r| r.name == "app.log"));
    assert_eq!(
        index.search("path:*target/logs/* *.log", 20).unwrap().len(),
        1
    );
    assert!(index
        .search_with_options(
            "*target*",
            20,
            fastfind::query::SearchOptions { match_path: true }
        )
        .unwrap()
        .iter()
        .any(|r| r.name == "app.log"));
}
