use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureStatus {
    Implemented,
    Partial,
    Planned,
    WindowsSpecific,
}

impl FeatureStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Implemented => "Implemented",
            Self::Partial => "Partial",
            Self::Planned => "Planned",
            Self::WindowsSpecific => "Windows-specific",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Feature {
    pub category: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub status: FeatureStatus,
}

pub const EVERYTHING_FEATURES: &[Feature] = &[
    Feature { category: "Core", name: "Instant filename search", description: "Search a persistent filename and folder-name index as text is entered.", status: FeatureStatus::Implemented },
    Feature { category: "Core", name: "Fast indexing and startup", description: "Build, persist, and reopen a compact index without reading file contents.", status: FeatureStatus::Partial },
    Feature { category: "Core", name: "Real-time index updates", description: "Reflect creates, deletes, moves, and renames through filesystem notifications.", status: FeatureStatus::Implemented },
    Feature { category: "Core", name: "Offline and private operation", description: "Run locally without analytics, remote calls, or user-data collection.", status: FeatureStatus::Implemented },
    Feature { category: "Search", name: "Substring and case-insensitive search", description: "Match partial filenames immediately with an FTS trigram index.", status: FeatureStatus::Implemented },
    Feature { category: "Search", name: "Full-path matching", description: "Match explicit path: terms or apply ordinary terms to full paths with the Search menu toggle.", status: FeatureStatus::Implemented },
    Feature { category: "Search", name: "Boolean operators and grouping", description: "AND, OR, NOT, grouping, quoted text, and operator precedence.", status: FeatureStatus::Planned },
    Feature { category: "Search", name: "Wildcards and character entities", description: "Whole-name * and ? wildcard patterns are implemented; literal character entities remain planned.", status: FeatureStatus::Partial },
    Feature { category: "Search", name: "Search modifiers", description: "Case, diacritics, whole-word, path, prefix, suffix, punctuation, whitespace, and regex modifiers.", status: FeatureStatus::Planned },
    Feature { category: "Search", name: "Regular expressions", description: "Regex mode and regex terms within otherwise normal searches.", status: FeatureStatus::Planned },
    Feature { category: "Search", name: "Property search functions", description: "Name, path, extension, type, size, dates, attributes, owner, hashes, media properties, parent/child, and other metadata functions.", status: FeatureStatus::Planned },
    Feature { category: "Search", name: "Content search/indexing", description: "Optional text-content matching, separate from the default filename-only index.", status: FeatureStatus::Planned },
    Feature { category: "Search", name: "Filters and macros", description: "Built-in Audio, Document, Executable, Image, Video, and Archive filters plus custom saved macros.", status: FeatureStatus::Partial },
    Feature { category: "Search", name: "Advanced Search dialog", description: "Build complex searches without memorizing query syntax.", status: FeatureStatus::Planned },
    Feature { category: "Search", name: "Duplicate, distinct, and unique search", description: "Group duplicate names, sizes, dates, properties, or content hashes.", status: FeatureStatus::Planned },
    Feature { category: "Results", name: "Details result list", description: "Name, path, size, type, and date-modified columns with row selection.", status: FeatureStatus::Implemented },
    Feature { category: "Results", name: "Column sorting and visibility", description: "Sort ascending/descending and show, hide, resize, or reorder columns.", status: FeatureStatus::Partial },
    Feature { category: "Results", name: "Open and reveal results", description: "Open a result or reveal its containing folder with the desktop file manager.", status: FeatureStatus::Implemented },
    Feature { category: "Results", name: "Clipboard and export", description: "Copy paths and export current results to CSV or text/file-list formats.", status: FeatureStatus::Partial },
    Feature { category: "Results", name: "File operations and properties", description: "Cut, copy, delete, rename, multi-rename, properties, drag-and-drop, and undo history.", status: FeatureStatus::Planned },
    Feature { category: "Results", name: "Preview pane and thumbnails", description: "Preview supported files and switch between details and thumbnail views.", status: FeatureStatus::Planned },
    Feature { category: "Results", name: "Jump-to and highlighted matches", description: "Keyboard jump-to, highlighted terms, mouseover rows, and selection status.", status: FeatureStatus::Planned },
    Feature { category: "Workspace", name: "Windows and search tabs", description: "Multiple windows, tabs, recently closed tabs, and session restore.", status: FeatureStatus::Planned },
    Feature { category: "Workspace", name: "Folder, filter, and bookmark sidebars", description: "Browse indexed roots and organize reusable searches from side panels.", status: FeatureStatus::Planned },
    Feature { category: "Workspace", name: "Bookmarks and home search", description: "Save search text, options, filter, columns, sort, view, and index selection.", status: FeatureStatus::Planned },
    Feature { category: "Workspace", name: "Search and run history", description: "Recall searches, track launches, rank frequently opened items, and manage retention.", status: FeatureStatus::Planned },
    Feature { category: "Indexing", name: "Linux/ext4 folder indexes", description: "Add selected roots, preserve raw Linux names, and reconcile persistent SQLite indexes.", status: FeatureStatus::Implemented },
    Feature { category: "Indexing", name: "inotify/fanotify monitoring", description: "Use recursive inotify changes now, with fanotify and overflow reconciliation as the privileged future backend.", status: FeatureStatus::Partial },
    Feature { category: "Indexing", name: "Index journal and recent changes", description: "Record index mutations and expose a searchable recent-change view.", status: FeatureStatus::Planned },
    Feature { category: "Indexing", name: "Include/exclude rules", description: "Exclude roots, files, wildcards, regexes, hidden entries, and selected metadata.", status: FeatureStatus::Planned },
    Feature { category: "Indexing", name: "Property indexes and fast sorts", description: "Optional size/date/attributes/property indexes with memory-cost controls.", status: FeatureStatus::Partial },
    Feature { category: "Indexing", name: "Offline file lists", description: "Create and search snapshots for removable media, optical media, and offline trees.", status: FeatureStatus::Planned },
    Feature { category: "Indexing", name: "Network folders and remote indexes", description: "Index NAS shares or query synchronized indexes hosted by another machine.", status: FeatureStatus::Planned },
    Feature { category: "Indexing", name: "NTFS/ReFS USN journal", description: "Windows volume metadata and durable USN Journal integration.", status: FeatureStatus::WindowsSpecific },
    Feature { category: "Integration", name: "Command-line interface", description: "Initialize, index, search, inspect status, and emit JSON from scripts.", status: FeatureStatus::Implemented },
    Feature { category: "Integration", name: "Background service", description: "A persistent per-user daemon serving CLI and GUI clients.", status: FeatureStatus::Planned },
    Feature { category: "Integration", name: "HTTP, ETP, and index server", description: "Remote search protocols, browser access, and shared indexes.", status: FeatureStatus::Planned },
    Feature { category: "Integration", name: "SDK, IPC, and URL protocol", description: "Stable APIs for clients, shell integration, and search URLs.", status: FeatureStatus::Planned },
    Feature { category: "Integration", name: "Plugins", description: "Extend indexed properties, previews, search behavior, or result actions.", status: FeatureStatus::Planned },
    Feature { category: "Integration", name: "Multiple named instances", description: "Independent configurations, databases, services, and windows.", status: FeatureStatus::Planned },
    Feature { category: "Customization", name: "Light and dark themes", description: "Switch the complete search window between native light and dark visuals.", status: FeatureStatus::Implemented },
    Feature { category: "Customization", name: "Fonts, colors, scale, and layout", description: "Customize result fonts, state colors, row styling, panes, and UI scale.", status: FeatureStatus::Partial },
    Feature { category: "Customization", name: "Keyboard shortcuts and global hotkeys", description: "Configurable commands by global, search-box, and result-list context.", status: FeatureStatus::Partial },
    Feature { category: "Customization", name: "Context-menu commands and external file manager", description: "Configure which result actions appear and the commands they invoke.", status: FeatureStatus::Partial },
    Feature { category: "Customization", name: "Settings import/export and advanced settings", description: "Back up, restore, reset, search, and override configuration values.", status: FeatureStatus::Planned },
    Feature { category: "Customization", name: "Languages and translation", description: "Localized UI resources and a translation workflow.", status: FeatureStatus::Planned },
    Feature { category: "Desktop", name: "Tray, startup, and background behavior", description: "Tray controls, launch-at-login, desktop integration, and global show/toggle hotkeys.", status: FeatureStatus::Planned },
    Feature { category: "Support", name: "Diagnostics and troubleshooting", description: "Observable health, freshness, scan progress, watcher lag, privacy-safe reports, and recovery actions.", status: FeatureStatus::Partial },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_is_broad_and_has_unique_names() {
        assert!(EVERYTHING_FEATURES.len() >= 45);
        let mut names = std::collections::HashSet::new();
        for feature in EVERYTHING_FEATURES {
            assert!(
                names.insert(feature.name),
                "duplicate feature: {}",
                feature.name
            );
        }
    }

    #[test]
    fn inventory_exposes_every_status() {
        for status in [
            FeatureStatus::Implemented,
            FeatureStatus::Partial,
            FeatureStatus::Planned,
            FeatureStatus::WindowsSpecific,
        ] {
            assert!(EVERYTHING_FEATURES
                .iter()
                .any(|feature| feature.status == status));
        }
    }
}
