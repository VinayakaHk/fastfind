use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
pub enum OutputMode {
    Human,
    Json,
    Silent,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TelemetryEvent {
    HealthChanged {
        schema_version: u8,
        timestamp_ms: u64,
        state: &'static str,
        reason: Option<&'static str>,
    },
    ScanStarted {
        schema_version: u8,
        timestamp_ms: u64,
        root_id: i64,
    },
    ScanProgress {
        schema_version: u8,
        timestamp_ms: u64,
        root_id: i64,
        entries_seen: u64,
        errors: u64,
        elapsed_ms: u64,
        entries_per_second: u64,
    },
    ScanCompleted {
        schema_version: u8,
        timestamp_ms: u64,
        root_id: i64,
        entries_seen: u64,
        removed_entries: u64,
        errors: u64,
        elapsed_ms: u64,
        synchronized: bool,
    },
    QueryCompleted {
        schema_version: u8,
        timestamp_ms: u64,
        query_length: usize,
        result_count: usize,
        elapsed_ms: u64,
    },
}

pub struct EventSink {
    mode: OutputMode,
}

impl EventSink {
    pub fn new(mode: OutputMode) -> Self {
        Self { mode }
    }

    pub fn emit(&self, event: &TelemetryEvent) {
        match self.mode {
            OutputMode::Silent => {}
            OutputMode::Json => {
                if let Ok(encoded) = serde_json::to_string(event) {
                    eprintln!("{encoded}");
                }
            }
            OutputMode::Human => match event {
                TelemetryEvent::HealthChanged { state, reason, .. } => {
                    if let Some(reason) = reason {
                        eprintln!("health: {state} ({reason})");
                    } else {
                        eprintln!("health: {state}");
                    }
                }
                TelemetryEvent::ScanStarted { root_id, .. } => {
                    eprintln!("indexing root {root_id}...");
                }
                TelemetryEvent::ScanProgress {
                    entries_seen,
                    errors,
                    entries_per_second,
                    ..
                } => eprintln!(
                    "indexed {entries_seen} entries ({entries_per_second}/s, {errors} errors)"
                ),
                TelemetryEvent::ScanCompleted {
                    entries_seen,
                    removed_entries,
                    errors,
                    elapsed_ms,
                    synchronized,
                    ..
                } => eprintln!(
                    "scan complete: {entries_seen} entries, {removed_entries} removed, {errors} errors, {elapsed_ms} ms, synchronized={synchronized}"
                ),
                TelemetryEvent::QueryCompleted {
                    result_count,
                    elapsed_ms,
                    ..
                } => eprintln!("query: {result_count} results in {elapsed_ms} ms"),
            },
        }
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
