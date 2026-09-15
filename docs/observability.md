# UI observability

Fastfind treats observability as product state, not as a log viewer. The CLI, daemon protocol, and future GTK UI consume the same versioned `TelemetryEvent` model.

## User-visible state

The UI will expose:

- Health: `initializing`, `scanning`, `live`, `degraded`, or `error`, with a typed reason.
- Freshness: last successful reconciliation time, separate from live-event backlog age.
- Scan progress: entries seen, errors, elapsed time, and entries per second. Total remains unknown until a prior scan provides an estimate.
- Watch health: backend (`inotify` or `fanotify`), queue depth, oldest event age, overflow count, and whether a rescan is required.
- Query performance: current query duration plus rolling p50/p95/p99 latency without recording the query text.
- Resource health: index size, database write latency, memory, and dropped telemetry-event count.

## Presentation

The main window gets a compact health indicator and freshness label. An expandable activity panel shows indexing rate, current phase, recoverable errors, watcher lag, and recent operations. A diagnostics page provides aggregate metrics and an exportable redacted report. Error states always include a user action such as retry, rescan, or fix permissions.

## Privacy and overhead

Default telemetry contains counts, durations, IDs, and typed error categories—not filenames, full paths, or search strings. Diagnostic path logging must be explicit and temporary. Metrics never use paths as labels. Telemetry delivery will use a bounded, non-blocking channel; dropped updates increment a visible counter, while state snapshots allow the UI to recover.

## Acceptance criteria

- UI health changes appear within one second.
- Scan progress updates no more than four times per second.
- Index freshness and watcher lag are displayed separately.
- Watch overflow changes health to `degraded` and offers a rescan.
- Default logs and JSON telemetry contain no query text or indexed filenames.
- Slow or disconnected UI clients cannot block indexing.
- Reconnecting clients receive a complete state snapshot before incremental events.

## Observing and testing the GTK UI

The GTK layer will be a projection of a testable `UiState`, not a place where daemon logic lives. A development-only event injector will replay deterministic scenarios such as a healthy scan, permission failures, watcher overflow, slow queries, and reconnection. Every interactive widget will have an accessible role, name, and stable semantic identifier.

Automated UI verification will run under a headless display and capture screenshots at fixed window sizes for known states. Tests will inspect the GTK accessibility tree and exercise keyboard navigation as well as pointer actions. Failed tests retain the screenshot, event transcript, accessibility snapshot, and redacted daemon diagnostics. During manual development, GTK Inspector and the same event transcript make layout and state transitions directly observable without indexing a large real filesystem.
