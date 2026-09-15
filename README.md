# fastfind

Fastfind is an early native Linux filename indexer inspired by Everything. The current milestone scans one or more directory trees on the same filesystem, persists filenames in SQLite, performs fast case-insensitive substring searches, reconciles deletions, and emits versioned telemetry suitable for a future daemon and GTK UI.

## Current capabilities

- Filename-only indexing; file contents are never opened.
- Traversal stays on the selected filesystem and does not follow symlinks.
- Raw Linux filename bytes are preserved alongside a searchable display form.
- SQLite WAL persistence with FTS5 trigram substring search.
- Batched reconciliation that only removes stale entries after an error-free scan.
- Human-readable CLI and machine-readable JSON/JSON Lines telemetry.
- Health, progress, indexing rate, error count, freshness, and query-latency signals.

Live `inotify`, a long-running daemon, `fanotify`, and GTK are planned next.

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

## UI observability

See [`docs/observability.md`](docs/observability.md). The design deliberately separates index freshness from live watcher lag and excludes filenames, paths, and query text from default telemetry.
