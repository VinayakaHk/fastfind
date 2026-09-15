# fastfind

Fastfind is an early native Linux filename indexer inspired by Everything. It scans one or more directory trees on the same filesystem, persists filenames in SQLite, performs fast case-insensitive substring searches, reconciles deletions, and provides both a CLI and a native X11 desktop search window.

## Current capabilities

- Filename-only indexing; file contents are never opened.
- Traversal stays on the selected filesystem and does not follow symlinks.
- Raw Linux filename bytes are preserved alongside a searchable display form.
- SQLite WAL persistence with FTS5 trigram substring search.
- Batched reconciliation that only removes stale entries after an error-free scan.
- Human-readable CLI and machine-readable JSON/JSON Lines telemetry.
- Native Everything-style menu, search field, filters, sortable results, and status bar.
- Background UI worker for searches and scans, plus live health and progress signals.
- Recursive inotify monitoring that updates created, removed, renamed, and modified paths without rescanning the whole root.
- The Start Menu launch reuses the persistent index and automatically indexes `$HOME` only when no roots exist.
- Open, reveal, copy-path, CSV export, index manager, and light/dark theme actions.
- A first-launch feature matrix documenting implemented, partial, planned, and Windows-specific Everything functionality.

Live `inotify` updates are implemented. A long-running daemon and filesystem-wide `fanotify` backend are planned next.

## Build

This machine uses an Ubuntu-signed, project-local Rust toolchain. Activate it with the `dev` wrapper:

```bash
chmod +x dev
./dev cargo build
./dev cargo test
```

The resulting executable is a normal native Linux binary and does not need the toolchain at runtime:

```bash
./target/debug/fastfind --help
```

## Usage

Use an explicit test database while developing:

```bash
./target/debug/fastfind --database ./fastfind.db init
./target/debug/fastfind --database ./fastfind.db index ~/Projects
./target/debug/fastfind --database ./fastfind.db search fastfind
./target/debug/fastfind --database ./fastfind.db status
```

For structured output, add `--json`. Command results are written to stdout; versioned telemetry events are emitted as JSON Lines on stderr.

```bash
./target/debug/fastfind --json --database ./fastfind.db index ~/Projects
```

Without `--database`, the index is stored at `$XDG_DATA_HOME/fastfind/index.db` or `~/.local/share/fastfind/index.db`.

## Native desktop UI

Launch the Everything-style native window against the default index:

```bash
./target/debug/fastfind-gui
```

Or select a development database explicitly:

```bash
./target/debug/fastfind-gui --database ./fastfind.db
```

The first launch opens the complete feature matrix. Close it to use the main search window; reopen it from **Help → Everything Feature Matrix**. Use **File → Index Folder…** to add a root. Searches and scans run on a background worker so the window remains responsive.

## UI observability

See [`docs/observability.md`](docs/observability.md). The design deliberately separates index freshness from live watcher lag and excludes filenames, paths, and query text from default telemetry.

## Automated UI search validation

The X11 interaction test creates a deterministic index, launches the real native window, clicks the title bar and search field with PyAutoGUI, types a query, captures the FastFind window, and uses Tesseract OCR to assert the query, expected filename, and one-match status.

```bash
./dev cargo build --bins
./tests/run_ui_search_test.sh
```

The Python dependency is isolated and pinned by `uv run --with PyAutoGUI==0.9.54`; it is not installed into the system Python environment.
