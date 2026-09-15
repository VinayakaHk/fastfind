use clap::Parser;
use eframe::egui::{self, Color32, RichText};
use fastfind::features::{FeatureStatus, EVERYTHING_FEATURES};
use fastfind::live;
use fastfind::{scan, EventSink, Index, RootStatus, SearchResult, TelemetryEvent};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

const RESULT_LIMIT: usize = 5_000;

#[derive(Parser, Debug)]
#[command(
    name = "fastfind-gui",
    version,
    about = "Native FastFind search window"
)]
struct GuiArgs {
    #[arg(long)]
    database: Option<PathBuf>,

    /// Suppress the first-launch feature matrix for automated UI tests.
    #[arg(long, hide = true)]
    skip_feature_matrix: bool,
}

#[derive(Debug)]
enum WorkerCommand {
    Search {
        request_id: u64,
        query: String,
        match_path: bool,
    },
    Scan(PathBuf),
    FilesystemEvent(notify::Event),
    WatchError(String),
    RefreshStatus,
    Shutdown,
}

#[derive(Debug)]
enum WorkerEvent {
    Ready(Vec<RootStatus>),
    Results {
        request_id: u64,
        results: Vec<SearchResult>,
        elapsed_ms: u64,
    },
    ScanFinished,
    IndexChanged(u64),
    WatcherDegraded(String),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultFilter {
    Everything,
    Audio,
    Archives,
    Documents,
    Executables,
    Folders,
    Images,
    Video,
}

impl ResultFilter {
    const ALL: [Self; 8] = [
        Self::Everything,
        Self::Audio,
        Self::Archives,
        Self::Documents,
        Self::Executables,
        Self::Folders,
        Self::Images,
        Self::Video,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Everything => "Everything",
            Self::Audio => "Audio",
            Self::Archives => "Compressed",
            Self::Documents => "Document",
            Self::Executables => "Executable",
            Self::Folders => "Folder",
            Self::Images => "Image",
            Self::Video => "Video",
        }
    }

    fn matches(self, result: &SearchResult) -> bool {
        use fastfind::model::EntryKind;
        if self == Self::Everything {
            return true;
        }
        if self == Self::Folders {
            return matches!(result.kind, EntryKind::Directory);
        }
        if !matches!(result.kind, EntryKind::File | EntryKind::Symlink) {
            return false;
        }
        let extension = Path::new(&result.name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let extensions: &[&str] = match self {
            Self::Audio => &["aac", "flac", "m4a", "mp3", "ogg", "opus", "wav", "wma"],
            Self::Archives => &["7z", "bz2", "gz", "rar", "tar", "xz", "zip", "zst"],
            Self::Documents => &[
                "csv", "doc", "docx", "epub", "md", "odt", "pdf", "ppt", "pptx", "rtf", "txt",
                "xls", "xlsx",
            ],
            Self::Executables => &["appimage", "bin", "deb", "desktop", "run", "sh"],
            Self::Images => &[
                "avif", "bmp", "gif", "heic", "jpeg", "jpg", "png", "svg", "tif", "tiff", "webp",
            ],
            Self::Video => &[
                "avi", "flv", "m4v", "mkv", "mov", "mp4", "mpeg", "webm", "wmv",
            ],
            Self::Everything | Self::Folders => &[],
        };
        extensions.contains(&extension.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Name,
    Path,
    Size,
    Modified,
    Type,
}

struct FastFindApp {
    commands: Sender<WorkerCommand>,
    events: Receiver<WorkerEvent>,
    telemetry: Receiver<TelemetryEvent>,
    query: String,
    request_id: u64,
    results: Vec<SearchResult>,
    selected_id: Option<i64>,
    roots: Vec<RootStatus>,
    filter: ResultFilter,
    case_sensitive: bool,
    match_path: bool,
    sort_column: SortColumn,
    sort_descending: bool,
    health: String,
    detail: String,
    query_latency_ms: u64,
    scan_rate: u64,
    scan_seen: u64,
    scan_errors: u64,
    dark_mode: bool,
    show_features: bool,
    feature_query: String,
    show_add_root: bool,
    root_input: String,
    show_index_manager: bool,
    show_about: bool,
}

impl FastFindApp {
    fn new(database: PathBuf, ctx: &egui::Context, show_features: bool) -> Self {
        let (commands, events, telemetry) = spawn_worker(database);
        let _ = commands.send(WorkerCommand::Search {
            request_id: 1,
            query: String::new(),
            match_path: false,
        });
        ctx.set_visuals(egui::Visuals::light());
        Self {
            commands,
            events,
            telemetry,
            query: String::new(),
            request_id: 1,
            results: Vec::new(),
            selected_id: None,
            roots: Vec::new(),
            filter: ResultFilter::Everything,
            case_sensitive: false,
            match_path: false,
            sort_column: SortColumn::Name,
            sort_descending: false,
            health: "initializing".to_string(),
            detail: "Opening index…".to_string(),
            query_latency_ms: 0,
            scan_rate: 0,
            scan_seen: 0,
            scan_errors: 0,
            dark_mode: false,
            show_features,
            feature_query: String::new(),
            show_add_root: false,
            root_input: env::var("HOME").unwrap_or_default(),
            show_index_manager: false,
            show_about: false,
        }
    }

    fn send_search(&mut self) {
        self.request_id = self.request_id.wrapping_add(1);
        let _ = self.commands.send(WorkerCommand::Search {
            request_id: self.request_id,
            query: self.query.clone(),
            match_path: self.match_path,
        });
        self.detail = "Searching…".to_string();
    }

    fn poll_worker(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                WorkerEvent::Ready(statuses) => {
                    self.roots = statuses;
                    self.health = if self.roots.iter().any(|root| root.state == "degraded") {
                        "degraded".to_string()
                    } else {
                        "live".to_string()
                    };
                }
                WorkerEvent::Results {
                    request_id,
                    results,
                    elapsed_ms,
                } if request_id == self.request_id => {
                    self.results = results;
                    self.query_latency_ms = elapsed_ms;
                    self.sort_results();
                    self.detail = format!("{} indexed matches", self.results.len());
                }
                WorkerEvent::Results { .. } => {}
                WorkerEvent::ScanFinished => {
                    self.detail = "Index synchronized; live monitoring active".to_string();
                    let _ = self.commands.send(WorkerCommand::RefreshStatus);
                    self.send_search();
                }
                WorkerEvent::IndexChanged(changed) => {
                    self.health = "live".to_string();
                    self.detail = format!("Live index update: {changed} changed entries");
                    self.send_search();
                }
                WorkerEvent::WatcherDegraded(reason) => {
                    self.health = "degraded".to_string();
                    self.detail = reason;
                }
                WorkerEvent::Error(error) => {
                    self.health = "error".to_string();
                    self.detail = error;
                }
            }
        }

        while let Ok(event) = self.telemetry.try_recv() {
            match event {
                TelemetryEvent::HealthChanged { state, reason, .. } => {
                    self.health = state.to_string();
                    if let Some(reason) = reason {
                        self.detail = reason.replace('_', " ");
                    }
                }
                TelemetryEvent::ScanStarted { root_id, .. } => {
                    self.detail = format!("Indexing root {root_id}");
                }
                TelemetryEvent::ScanProgress {
                    entries_seen,
                    errors,
                    entries_per_second,
                    ..
                } => {
                    self.scan_seen = entries_seen;
                    self.scan_errors = errors;
                    self.scan_rate = entries_per_second;
                    self.detail = format!("Indexing {entries_seen} objects");
                }
                TelemetryEvent::ScanCompleted {
                    entries_seen,
                    errors,
                    synchronized,
                    ..
                } => {
                    self.scan_seen = entries_seen;
                    self.scan_errors = errors;
                    self.detail = if synchronized {
                        format!("Indexed {entries_seen} objects")
                    } else {
                        format!("Indexed with {errors} errors; reconciliation required")
                    };
                }
                TelemetryEvent::QueryCompleted { elapsed_ms, .. } => {
                    self.query_latency_ms = elapsed_ms;
                }
            }
        }
    }

    fn sort_results(&mut self) {
        let column = self.sort_column;
        let descending = self.sort_descending;
        self.results.sort_by(|left, right| {
            let ordering = match column {
                SortColumn::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                SortColumn::Path => left.path.to_lowercase().cmp(&right.path.to_lowercase()),
                SortColumn::Size => left.size.cmp(&right.size),
                SortColumn::Modified => left.modified_ns.cmp(&right.modified_ns),
                SortColumn::Type => kind_label(left).cmp(kind_label(right)),
            };
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }

    fn set_sort(&mut self, column: SortColumn) {
        if self.sort_column == column {
            self.sort_descending = !self.sort_descending;
        } else {
            self.sort_column = column;
            self.sort_descending = matches!(column, SortColumn::Size | SortColumn::Modified);
        }
        self.sort_results();
    }

    fn selected(&self) -> Option<&SearchResult> {
        let id = self.selected_id?;
        self.results.iter().find(|result| result.id == id)
    }

    fn open_selected(&self) {
        if let Some(result) = self.selected() {
            open_path(Path::new(&result.path));
        }
    }

    fn reveal_selected(&self) {
        if let Some(result) = self.selected() {
            if let Some(parent) = Path::new(&result.path).parent() {
                open_path(parent);
            }
        }
    }

    fn copy_selected(&self, ctx: &egui::Context) {
        if let Some(result) = self.selected() {
            ctx.output_mut(|output| output.copied_text = result.path.clone());
        }
    }

    fn export_csv(&mut self) {
        let destination = env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("fastfind-results.csv");
        match write_csv(&destination, &self.visible_results()) {
            Ok(()) => self.detail = format!("Exported {}", destination.display()),
            Err(error) => self.detail = format!("Export failed: {error}"),
        }
    }

    fn visible_results(&self) -> Vec<SearchResult> {
        self.results
            .iter()
            .filter(|result| self.filter.matches(result))
            .filter(|result| {
                !self.case_sensitive
                    || self.query.is_empty()
                    || result.name.contains(self.query.as_str())
            })
            .cloned()
            .collect()
    }

    fn menu_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Window    Ctrl+N").clicked() {
                    if let Ok(executable) = env::current_exe() {
                        let _ = Command::new(executable).spawn();
                    }
                    ui.close_menu();
                }
                if ui.button("Index Folder…").clicked() {
                    self.show_add_root = true;
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Export Results…").clicked() {
                    self.export_csv();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(
                        self.selected().is_some(),
                        egui::Button::new("Open    Enter"),
                    )
                    .clicked()
                {
                    self.open_selected();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(self.selected().is_some(), egui::Button::new("Open Path"))
                    .clicked()
                {
                    self.reveal_selected();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        self.selected().is_some(),
                        egui::Button::new("Copy Full Path    Ctrl+Shift+C"),
                    )
                    .clicked()
                {
                    self.copy_selected(ctx);
                    ui.close_menu();
                }
                ui.separator();
                ui.add_enabled(
                    false,
                    egui::Button::new("Cut / Copy / Delete / Rename (planned)"),
                );
            });
            ui.menu_button("View", |ui| {
                if ui.checkbox(&mut self.dark_mode, "Dark Theme").changed() {
                    if self.dark_mode {
                        ctx.set_visuals(egui::Visuals::dark());
                    } else {
                        ctx.set_visuals(egui::Visuals::light());
                    }
                }
                if ui.button("Refresh    F5").clicked() {
                    self.send_search();
                    ui.close_menu();
                }
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Preview Pane (planned)"));
                ui.add_enabled(false, egui::Button::new("Thumbnails (planned)"));
            });
            ui.menu_button("Search", |ui| {
                ui.checkbox(&mut self.case_sensitive, "Match Case");
                if ui.checkbox(&mut self.match_path, "Match Path").changed() {
                    self.send_search();
                }
                ui.add_enabled(false, egui::Button::new("Match Whole Word (planned)"));
                ui.add_enabled(false, egui::Button::new("Regular Expressions (planned)"));
                ui.separator();
                for filter in ResultFilter::ALL {
                    if ui
                        .radio_value(&mut self.filter, filter, filter.label())
                        .changed()
                    {
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Advanced Search… (planned)"));
            });
            ui.menu_button("Bookmarks", |ui| {
                ui.add_enabled(false, egui::Button::new("Add to Bookmarks… (planned)"));
                ui.add_enabled(false, egui::Button::new("Organize Bookmarks… (planned)"));
            });
            ui.menu_button("Tools", |ui| {
                if ui.button("Index Manager…").clicked() {
                    self.show_index_manager = true;
                    let _ = self.commands.send(WorkerCommand::RefreshStatus);
                    ui.close_menu();
                }
                if ui.button("Reindex All").clicked() {
                    for root in &self.roots {
                        let _ = self
                            .commands
                            .send(WorkerCommand::Scan(PathBuf::from(&root.path)));
                    }
                    ui.close_menu();
                }
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Options… (planned)"));
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Everything Feature Matrix").clicked() {
                    self.show_features = true;
                    ui.close_menu();
                }
                if ui.button("About FastFind").clicked() {
                    self.show_about = true;
                    ui.close_menu();
                }
            });
        });
    }

    fn feature_matrix(&mut self, ctx: &egui::Context) {
        if !self.show_features {
            return;
        }
        let mut open = self.show_features;
        egui::Window::new("Everything functionality matrix")
            .open(&mut open)
            .default_size([820.0, 650.0])
            .vscroll(true)
            .show(ctx, |ui| {
                ui.heading("Everything feature inventory for FastFind");
                ui.label("This inventory is shown on first launch. It records parity honestly: disabled menu items are not presented as working features.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Filter features:");
                    ui.text_edit_singleline(&mut self.feature_query);
                });
                ui.horizontal_wrapped(|ui| {
                    status_legend(ui, FeatureStatus::Implemented, Color32::from_rgb(30, 150, 70));
                    status_legend(ui, FeatureStatus::Partial, Color32::from_rgb(210, 140, 20));
                    status_legend(ui, FeatureStatus::Planned, Color32::from_rgb(70, 120, 200));
                    status_legend(ui, FeatureStatus::WindowsSpecific, Color32::GRAY);
                });
                ui.separator();

                let needle = self.feature_query.to_lowercase();
                let mut category = "";
                for feature in EVERYTHING_FEATURES.iter().filter(|feature| {
                    needle.is_empty()
                        || feature.name.to_lowercase().contains(&needle)
                        || feature.category.to_lowercase().contains(&needle)
                        || feature.description.to_lowercase().contains(&needle)
                }) {
                    if category != feature.category {
                        category = feature.category;
                        ui.add_space(8.0);
                        ui.heading(category);
                    }
                    ui.horizontal_top(|ui| {
                        let color = status_color(feature.status);
                        ui.label(RichText::new(feature.status.label()).color(color).strong());
                        ui.vertical(|ui| {
                            ui.label(RichText::new(feature.name).strong());
                            ui.label(feature.description);
                        });
                    });
                    ui.separator();
                }
            });
        self.show_features = open;
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.show_add_root {
            let mut open = self.show_add_root;
            let mut close = false;
            egui::Window::new("Index Folder")
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Folder path on the ext4 filesystem:");
                    ui.text_edit_singleline(&mut self.root_input);
                    ui.label("The scan runs on a worker thread. Progress remains visible in the status bar.");
                    ui.horizontal(|ui| {
                        if ui.button("Index").clicked() {
                            let path = PathBuf::from(self.root_input.trim());
                            if path.is_dir() {
                                let _ = self.commands.send(WorkerCommand::Scan(path));
                                self.health = "scanning".to_string();
                                close = true;
                            } else {
                                self.detail = "That directory does not exist".to_string();
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            self.show_add_root = open && !close;
        }

        if self.show_index_manager {
            let mut open = self.show_index_manager;
            egui::Window::new("Index Manager")
                .open(&mut open)
                .default_size([720.0, 360.0])
                .show(ctx, |ui| {
                    if self.roots.is_empty() {
                        ui.label("No indexed roots. Use File → Index Folder…");
                    }
                    for root in &self.roots {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&root.path).strong());
                                ui.label(&root.state);
                            });
                            ui.label(format!(
                                "{} objects · {} errors · last scan {}",
                                root.entry_count,
                                root.error_count,
                                root.last_scan_ms
                                    .map(format_timestamp_ms)
                                    .unwrap_or_else(|| "never".to_string())
                            ));
                            if ui.button("Reindex now").clicked() {
                                let _ = self
                                    .commands
                                    .send(WorkerCommand::Scan(PathBuf::from(&root.path)));
                            }
                        });
                    }
                });
            self.show_index_manager = open;
        }

        if self.show_about {
            let mut open = self.show_about;
            egui::Window::new("About FastFind")
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.heading("FastFind 0.1.0");
                    ui.label("A native, private filename indexer for Linux.");
                    ui.label("Inspired by the interaction model of Voidtools Everything; not affiliated with Voidtools.");
                });
            self.show_about = open;
        }
    }
}

impl eframe::App for FastFindApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();
        ctx.request_repaint_after(Duration::from_millis(50));

        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ctx, ui));
        egui::TopBottomPanel::top("search").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Search:").strong());
                let response = ui.add_sized(
                    [ui.available_width() - 155.0, 28.0],
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Type part of a filename…"),
                );
                if response.changed() {
                    self.send_search();
                }
                egui::ComboBox::from_id_source("filter")
                    .selected_text(self.filter.label())
                    .width(125.0)
                    .show_ui(ui, |ui| {
                        for filter in ResultFilter::ALL {
                            ui.selectable_value(&mut self.filter, filter, filter.label());
                        }
                    });
                if ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::F)) {
                    response.request_focus();
                }
                if response.has_focus() && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                    self.query.clear();
                    self.send_search();
                }
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let visible_count = self
                .results
                .iter()
                .filter(|result| self.filter.matches(result))
                .count();
            ui.horizontal(|ui| {
                ui.label(format!("{visible_count} objects"));
                ui.separator();
                ui.label(&self.detail);
                if self.health == "scanning" {
                    ui.separator();
                    ui.label(format!("{} objects · {}/s", self.scan_seen, self.scan_rate));
                }
                if self.scan_errors > 0 {
                    ui.separator();
                    ui.colored_label(Color32::YELLOW, format!("{} errors", self.scan_errors));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} ms", self.query_latency_ms));
                    if self.match_path {
                        ui.separator();
                        ui.label(RichText::new("PATH").strong());
                    }
                    ui.separator();
                    ui.label(RichText::new(self.health.to_uppercase()).strong());
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Grid::new("result_header")
                .num_columns(5)
                .striped(false)
                .show(ui, |ui| {
                    if ui
                        .button(sort_title(
                            "Name",
                            self.sort_column == SortColumn::Name,
                            self.sort_descending,
                        ))
                        .clicked()
                    {
                        self.set_sort(SortColumn::Name);
                    }
                    if ui
                        .button(sort_title(
                            "Path",
                            self.sort_column == SortColumn::Path,
                            self.sort_descending,
                        ))
                        .clicked()
                    {
                        self.set_sort(SortColumn::Path);
                    }
                    if ui
                        .button(sort_title(
                            "Size",
                            self.sort_column == SortColumn::Size,
                            self.sort_descending,
                        ))
                        .clicked()
                    {
                        self.set_sort(SortColumn::Size);
                    }
                    if ui
                        .button(sort_title(
                            "Date Modified",
                            self.sort_column == SortColumn::Modified,
                            self.sort_descending,
                        ))
                        .clicked()
                    {
                        self.set_sort(SortColumn::Modified);
                    }
                    if ui
                        .button(sort_title(
                            "Type",
                            self.sort_column == SortColumn::Type,
                            self.sort_descending,
                        ))
                        .clicked()
                    {
                        self.set_sort(SortColumn::Type);
                    }
                    ui.end_row();
                });
            ui.separator();

            let visible = self.visible_results();
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("results")
                        .num_columns(5)
                        .striped(true)
                        .min_col_width(80.0)
                        .show(ui, |ui| {
                            for result in visible {
                                let selected = self.selected_id == Some(result.id);
                                let name_response = ui
                                    .selectable_label(selected, &result.name)
                                    .on_hover_text(&result.path);
                                if name_response.clicked() {
                                    self.selected_id = Some(result.id);
                                }
                                if name_response.double_clicked() {
                                    open_path(Path::new(&result.path));
                                }
                                name_response.context_menu(|ui| {
                                    if ui.button("Open").clicked() {
                                        open_path(Path::new(&result.path));
                                        ui.close_menu();
                                    }
                                    if ui.button("Open Path").clicked() {
                                        if let Some(parent) = Path::new(&result.path).parent() {
                                            open_path(parent);
                                        }
                                        ui.close_menu();
                                    }
                                    if ui.button("Copy Full Path").clicked() {
                                        ctx.output_mut(|output| {
                                            output.copied_text = result.path.clone()
                                        });
                                        ui.close_menu();
                                    }
                                });
                                ui.label(
                                    Path::new(&result.path)
                                        .parent()
                                        .map(|path| path.to_string_lossy())
                                        .unwrap_or_default(),
                                );
                                ui.label(format_size(result.size));
                                ui.label(format_timestamp_ns(result.modified_ns));
                                ui.label(kind_label(&result));
                                ui.end_row();
                            }
                        });
                });
        });

        if ctx.input(|input| input.key_pressed(egui::Key::Enter)) && self.selected().is_some() {
            self.open_selected();
        }
        if ctx.input(|input| {
            input.modifiers.ctrl && input.modifiers.shift && input.key_pressed(egui::Key::C)
        }) {
            self.copy_selected(ctx);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::F5)) {
            self.send_search();
        }

        self.feature_matrix(ctx);
        self.dialogs(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
    }
}

fn spawn_worker(
    database: PathBuf,
) -> (
    Sender<WorkerCommand>,
    Receiver<WorkerEvent>,
    Receiver<TelemetryEvent>,
) {
    let (command_tx, command_rx) = mpsc::channel();
    let internal_tx = command_tx.clone();
    let (event_tx, event_rx) = mpsc::channel();
    let (telemetry_tx, telemetry_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut index = match Index::open(&database) {
            Ok(index) => index,
            Err(error) => {
                let _ = event_tx.send(WorkerEvent::Error(format!("{error:#}")));
                return;
            }
        };
        let callback_tx = internal_tx.clone();
        let mut watcher: Option<RecommendedWatcher> =
            match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                match result {
                    Ok(event) => {
                        let _ = callback_tx.send(WorkerCommand::FilesystemEvent(event));
                    }
                    Err(error) => {
                        let _ = callback_tx.send(WorkerCommand::WatchError(error.to_string()));
                    }
                }
            }) {
                Ok(watcher) => Some(watcher),
                Err(error) => {
                    let _ = event_tx.send(WorkerEvent::WatcherDegraded(format!(
                        "Live monitoring unavailable: {error}"
                    )));
                    None
                }
            };
        let statuses = index.statuses().unwrap_or_default();
        let mut watched_roots = HashSet::new();
        for root in &statuses {
            let path = PathBuf::from(&root.path);
            if watcher
                .as_mut()
                .is_some_and(|watcher| watcher.watch(&path, RecursiveMode::Recursive).is_ok())
            {
                watched_roots.insert(path);
            }
        }
        let _ = event_tx.send(WorkerEvent::Ready(statuses.clone()));
        if statuses.is_empty() {
            if let Some(home) = env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_dir())
            {
                if watcher
                    .as_mut()
                    .is_some_and(|watcher| watcher.watch(&home, RecursiveMode::Recursive).is_ok())
                {
                    watched_roots.insert(home.clone());
                }
                let _ = internal_tx.send(WorkerCommand::Scan(home));
            }
        }

        let telemetry = EventSink::channel(telemetry_tx);
        let mut recovery_queued = false;
        while let Ok(command) = command_rx.recv() {
            match command {
                WorkerCommand::Search {
                    request_id,
                    query,
                    match_path,
                } => {
                    let started = Instant::now();
                    match index.search_with_options(
                        &query,
                        RESULT_LIMIT,
                        fastfind::query::SearchOptions { match_path },
                    ) {
                        Ok(results) => {
                            let _ = event_tx.send(WorkerEvent::Results {
                                request_id,
                                results,
                                elapsed_ms: started.elapsed().as_millis() as u64,
                            });
                        }
                        Err(error) => {
                            let _ = event_tx
                                .send(WorkerEvent::Error(format!("Search failed: {error:#}")));
                        }
                    }
                }
                WorkerCommand::Scan(path) => {
                    match scan(&mut index, &path, &[database.clone()], &telemetry) {
                        Ok(_) => {
                            let canonical = path.canonicalize().unwrap_or(path);
                            if !watched_roots.contains(&canonical) {
                                if watcher.as_mut().is_some_and(|watcher| {
                                    watcher.watch(&canonical, RecursiveMode::Recursive).is_ok()
                                }) {
                                    watched_roots.insert(canonical);
                                }
                            }
                            recovery_queued = false;
                            let _ = event_tx.send(WorkerEvent::ScanFinished);
                        }
                        Err(error) => {
                            let _ = event_tx
                                .send(WorkerEvent::Error(format!("Indexing failed: {error:#}")));
                        }
                    }
                }
                WorkerCommand::FilesystemEvent(event) => {
                    let statuses = index.statuses().unwrap_or_default();
                    match live::apply_event(&mut index, &statuses, &event, &database) {
                        Ok(changed) if changed > 0 => {
                            let _ = event_tx.send(WorkerEvent::IndexChanged(changed));
                            if let Ok(statuses) = index.statuses() {
                                let _ = event_tx.send(WorkerEvent::Ready(statuses));
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            let _ = event_tx.send(WorkerEvent::WatcherDegraded(format!(
                                "Live update failed: {error:#}"
                            )));
                        }
                    }
                }
                WorkerCommand::WatchError(error) => {
                    let _ = event_tx.send(WorkerEvent::WatcherDegraded(format!(
                        "Watcher lost events; reconciliation queued: {error}"
                    )));
                    if !recovery_queued {
                        recovery_queued = true;
                        for root in index.statuses().unwrap_or_default() {
                            let _ = internal_tx.send(WorkerCommand::Scan(PathBuf::from(root.path)));
                        }
                    }
                }
                WorkerCommand::RefreshStatus => match index.statuses() {
                    Ok(statuses) => {
                        let _ = event_tx.send(WorkerEvent::Ready(statuses));
                    }
                    Err(error) => {
                        let _ =
                            event_tx.send(WorkerEvent::Error(format!("Status failed: {error:#}")));
                    }
                },
                WorkerCommand::Shutdown => break,
            }
        }
    });
    (command_tx, event_rx, telemetry_rx)
}

fn status_color(status: FeatureStatus) -> Color32 {
    match status {
        FeatureStatus::Implemented => Color32::from_rgb(30, 150, 70),
        FeatureStatus::Partial => Color32::from_rgb(210, 140, 20),
        FeatureStatus::Planned => Color32::from_rgb(70, 120, 200),
        FeatureStatus::WindowsSpecific => Color32::GRAY,
    }
}

fn status_legend(ui: &mut egui::Ui, status: FeatureStatus, color: Color32) {
    ui.label(RichText::new(format!("● {}", status.label())).color(color));
}

fn sort_title(label: &str, active: bool, descending: bool) -> String {
    if active {
        format!("{label} {}", if descending { "▼" } else { "▲" })
    } else {
        label.to_string()
    }
}

fn kind_label(result: &SearchResult) -> &'static str {
    use fastfind::model::EntryKind;
    match result.kind {
        EntryKind::File => "File",
        EntryKind::Directory => "Folder",
        EntryKind::Symlink => "Symbolic link",
        EntryKind::Other => "Other",
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_timestamp_ns(timestamp_ns: i64) -> String {
    let seconds = timestamp_ns.div_euclid(1_000_000_000);
    let nanos = timestamp_ns.rem_euclid(1_000_000_000) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanos)
        .map(|value| {
            value
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".to_string())
}

fn format_timestamp_ms(timestamp_ms: i64) -> String {
    format_timestamp_ns(timestamp_ms.saturating_mul(1_000_000))
}

fn open_path(path: &Path) {
    let _ = Command::new("xdg-open").arg(path).spawn();
}

fn write_csv(destination: &Path, results: &[SearchResult]) -> std::io::Result<()> {
    let mut file = File::create(destination)?;
    writeln!(file, "Name,Path,Size,Date Modified,Type")?;
    for result in results {
        writeln!(
            file,
            "{},{},{},{},{}",
            csv_field(&result.name),
            csv_field(&result.path),
            result.size,
            csv_field(&format_timestamp_ns(result.modified_ns)),
            csv_field(kind_label(result)),
        )?;
    }
    Ok(())
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn default_database_path() -> PathBuf {
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("fastfind/index.db");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/fastfind/index.db");
    }
    PathBuf::from("fastfind.db")
}

fn main() -> eframe::Result<()> {
    let args = GuiArgs::parse();
    let show_features = !args.skip_feature_matrix;
    let database = args.database.unwrap_or_else(default_database_path);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FastFind")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "FastFind",
        options,
        Box::new(move |creation_context| {
            Box::new(FastFindApp::new(
                database,
                &creation_context.egui_ctx,
                show_features,
            ))
        }),
    )
}
