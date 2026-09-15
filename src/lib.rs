pub mod db;
pub mod model;
pub mod scanner;
pub mod telemetry;

pub use db::Index;
pub use model::{RootStatus, SearchResult};
pub use scanner::{scan, ScanOutcome};
pub use telemetry::{EventSink, OutputMode, TelemetryEvent};
