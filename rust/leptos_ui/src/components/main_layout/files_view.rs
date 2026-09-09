use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use api_types::{
    backup_config::DecryptedBackupConfigWithRemoteStorage,
    file_filter::{FileTypeGroup, file_type_group, is_system_file},
    remote_file_version::{
        BackedUpFile, DEFAULT_PAGE_SIZE, FileVersionSummary, ListBackedUpFilesResponse,
        ListBinVersionsResponse,
    },
    remote_storage::RemoteStorageType,
};
use leptos::prelude::*;
use serde::Serialize;
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;

use super::restore_modal::{RestoreModal, RestoreSelection};
use crate::components::common::{
    abbrev_home_path, format_local_datetime, tauri_invoke, tauri_invoke_no_args,
    tauri_invoke_with_args,
};
use crate::state::*;

/// Arguments for the paginated `list_backed_up_files` Tauri command.
#[derive(Serialize)]
struct ListFilesArgs {
    backup_config_id: Uuid,
    cursor: Option<Vec<u8>>,
    limit: Option<i64>,
}

/// Arguments for the `move_all_versions_to_bin` Tauri command.
#[derive(Serialize)]
struct MoveAllVersionsToBinArgs {
    backup_config_id: Uuid,
    /// Corresponds to `BackedUpFile.blind_index` — identifies the file on the server.
    name_blind_index: Vec<u8>,
}

// ═══════════════════════════════════════════════════════════════════
// SHARED HELPERS
// ═══════════════════════════════════════════════════════════════════

fn format_size(bytes: i64) -> String {
    shared_ui::utils::format_bytes(bytes as u64)
}

#[derive(Clone)]
enum FileTreeNode {
    Directory {
        name: String,
        children: BTreeMap<String, FileTreeNode>,
    },
    File {
        name: String,
        file: BackedUpFile,
    },
}

fn build_file_tree(
    files: &[BackedUpFile],
    source_directory: &str,
) -> BTreeMap<String, FileTreeNode> {
    let mut root: BTreeMap<String, FileTreeNode> = BTreeMap::new();
    for f in files {
        let relative = f.path.strip_prefix(source_directory).unwrap_or(&f.path);
        let relative = relative.trim_start_matches('/');
        let parts: Vec<&str> = relative.split('/').collect();
        insert_into_tree(&mut root, &parts, f);
    }
    root
}

fn insert_into_tree(
    tree: &mut BTreeMap<String, FileTreeNode>,
    parts: &[&str],
    file: &BackedUpFile,
) {
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        tree.insert(
            parts[0].to_string(),
            FileTreeNode::File {
                name: parts[0].to_string(),
                file: file.clone(),
            },
        );
        return;
    }
    let entry = tree
        .entry(parts[0].to_string())
        .or_insert_with(|| FileTreeNode::Directory {
            name: parts[0].to_string(),
            children: BTreeMap::new(),
        });
    if let FileTreeNode::Directory { children, .. } = entry {
        insert_into_tree(children, &parts[1..], file);
    }
}

fn collect_files(node: &FileTreeNode) -> Vec<BackedUpFile> {
    match node {
        FileTreeNode::File { file, .. } => vec![file.clone()],
        FileTreeNode::Directory { children, .. } => {
            children.values().flat_map(collect_files).collect()
        }
    }
}

fn collect_files_from_tree(children: &BTreeMap<String, FileTreeNode>) -> Vec<BackedUpFile> {
    children.values().flat_map(collect_files).collect()
}

fn storage_type_icon(storage_type: &RemoteStorageType) -> &'static str {
    match storage_type {
        RemoteStorageType::Aws => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/><path d="M3 12c0 1.66 4.03 3 9 3s9-1.34 9-3"/></svg>"#
        }
        RemoteStorageType::GoogleDrive => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2L2 19.5h7.5L12 14l2.5 5.5H22L12 2z"/><path d="M2 19.5h20"/><path d="M7.5 12L12 2l4.5 10"/></svg>"#
        }
        RemoteStorageType::OneDrive => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 18.5h9.5a4.5 4.5 0 10-1.08-8.87A6 6 0 004 11.5a3.5 3.5 0 003 7z"/><path d="M8.5 13.5h7"/><path d="M12 10v7"/></svg>"#
        }
        RemoteStorageType::LocalFilesystem => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12H2"/><path d="M5.45 5.11L2 12v6a2 2 0 002 2h16a2 2 0 002-2v-6l-3.45-6.89A2 2 0 0016.76 4H7.24a2 2 0 00-1.79 1.11z"/><line x1="6" y1="16" x2="6.01" y2="16"/><line x1="10" y1="16" x2="10.01" y2="16"/></svg>"#
        }
        RemoteStorageType::Sftp => {
            r#"<svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="14" rx="2"/><path d="M7 20h10"/><path d="M12 18v2"/><path d="M7 8h.01"/><path d="M10 8h.01"/><path d="M13 8h4"/><path d="M7 12h10"/></svg>"#
        }
    }
}

fn file_type_icon(filename: &str) -> &'static str {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => {
            r#"<svg class="w-4 h-4 text-red-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>"#
        }
        "jpg" | "jpeg" | "png" | "gif" | "svg" | "webp" | "bmp" | "ico" | "heic" => {
            r#"<svg class="w-4 h-4 text-blue-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>"#
        }
        "mp4" | "mov" | "avi" | "mkv" | "webm" => {
            r#"<svg class="w-4 h-4 text-purple-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"/><line x1="7" y1="2" x2="7" y2="22"/><line x1="17" y1="2" x2="17" y2="22"/><line x1="2" y1="12" x2="22" y2="12"/><line x1="2" y1="7" x2="7" y2="7"/><line x1="2" y1="17" x2="7" y2="17"/><line x1="17" y1="7" x2="22" y2="7"/><line x1="17" y1="17" x2="22" y2="17"/></svg>"#
        }
        "mp3" | "wav" | "flac" | "aac" | "ogg" => {
            r#"<svg class="w-4 h-4 text-pink-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>"#
        }
        "zip" | "tar" | "gz" | "rar" | "7z" | "bz2" | "xz" => {
            r#"<svg class="w-4 h-4 text-yellow-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></svg>"#
        }
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
        | "html" | "css" | "scss" | "sh" | "rb" | "swift" | "kt" => {
            r#"<svg class="w-4 h-4 text-green-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>"#
        }
        "json" | "yaml" | "yml" | "toml" | "xml" | "csv" | "ini" | "env" => {
            r#"<svg class="w-4 h-4 text-cyan-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><circle cx="10" cy="13" r="2"/><path d="M20 17l-2-2"/></svg>"#
        }
        "doc" | "docx" | "txt" | "rtf" | "md" | "odt" => {
            r#"<svg class="w-4 h-4 text-blue-300 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><line x1="10" y1="9" x2="8" y2="9"/></svg>"#
        }
        "xls" | "xlsx" | "ods" => {
            r#"<svg class="w-4 h-4 text-emerald-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><rect x="8" y="12" width="8" height="6"/><line x1="12" y1="12" x2="12" y2="18"/><line x1="8" y1="15" x2="16" y2="15"/></svg>"#
        }
        "ppt" | "pptx" | "odp" => {
            r#"<svg class="w-4 h-4 text-orange-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><rect x="8" y="12" width="8" height="6" rx="1"/></svg>"#
        }
        _ => {
            r#"<svg class="w-4 h-4 text-text-secondary flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>"#
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortField {
    Name,
    Size,
    Date,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    List,
    Folder,
    Bin,
}

fn sort_files(files: &mut [BackedUpFile], field: SortField, dir: SortDirection) {
    files.sort_by(|a, b| {
        let cmp = match field {
            SortField::Name => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
            SortField::Size => a.size.cmp(&b.size),
            SortField::Date => {
                let a_date = a.versions.last().map(|v| &v.created_at);
                let b_date = b.versions.last().map(|v| &v.created_at);
                a_date.cmp(&b_date)
            }
        };
        match dir {
            SortDirection::Asc => cmp,
            SortDirection::Desc => cmp.reverse(),
        }
    });
}

fn filter_files(
    files: &[BackedUpFile],
    query: &str,
    type_filter: FileTypeGroup,
    show_system: bool,
) -> Vec<BackedUpFile> {
    let q = query.to_lowercase();
    let searching = !query.is_empty();
    files
        .iter()
        .filter(|f| {
            // Hide system/build artifacts unless the user has opted in.
            // An active search query also overrides the hide, so power users
            // can still locate .DS_Store etc. by typing the name.
            if !show_system && !searching && is_system_file(&f.path) {
                return false;
            }
            // Apply file-type group filter.
            if type_filter != FileTypeGroup::All && file_type_group(&f.path) != type_filter {
                return false;
            }
            // Apply search query.
            if searching {
                f.path.to_lowercase().contains(&q)
            } else {
                true
            }
        })
        .cloned()
        .collect()
}

// Arc-wrapped types with pointer equality for efficient Memo comparison.
// Avoids deep-comparing 20k files on every reactive tick.

#[derive(Clone)]
struct ArcFileList(Arc<Vec<BackedUpFile>>);

impl PartialEq for ArcFileList {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone)]
struct ArcFileTree(Option<Arc<BTreeMap<String, FileTreeNode>>>);

impl PartialEq for ArcFileTree {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// FILES VIEW — top-level page wrapper
// ═══════════════════════════════════════════════════════════════════

#[component]
pub fn FilesView() -> impl IntoView {
    view! {
        <div>
            <h2 class="text-2xl md:text-3xl font-display font-bold gradient-text mb-2">"Files"</h2>
            <p class="text-text-secondary mb-6 md:mb-8 text-sm md:text-base">"Browse and restore protected files"</p>
            <FileBrowser />
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// RESTORE PROGRESS BANNER
// ═══════════════════════════════════════════════════════════════════

#[component]
fn RestoreProgressBanner() -> impl IntoView {
    let (restore_progress, set_restore_progress) = use_context::<RestoreProgressSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (_, set_settings_tab) = use_context::<SettingsTabSignal>().unwrap();

    move || {
        let rp = restore_progress.get();
        if !rp.is_running && rp.phase != "Completed" && !rp.phase.starts_with("Failed") {
            return ().into_any();
        }

        let is_failed = rp.phase.starts_with("Failed");
        let is_auth_error = rp.phase.contains("Auth token expired");
        let is_completed = rp.phase == "Completed";

        let banner_class = if is_failed {
            "flex flex-col gap-2 bg-error-tint border border-error-border rounded-xl p-3 mb-4"
        } else if is_completed {
            "flex flex-col gap-2 bg-success-tint border border-success rounded-xl p-3 mb-4"
        } else {
            "flex flex-col gap-2 glass bg-elevation-1 border border-elevation-1-border rounded-xl p-3 mb-4"
        };

        let icon_class = if is_failed {
            "w-4 h-4 text-error flex-shrink-0"
        } else if is_completed {
            "w-4 h-4 text-success flex-shrink-0"
        } else {
            "w-4 h-4 text-accent flex-shrink-0"
        };

        let overall_pct = if rp.total_files > 0 {
            (rp.completed_files as f64 / rp.total_files as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        view! {
            <div class=banner_class>
                <div class="flex items-center justify-between">
                    <div class="flex items-center gap-2 min-w-0">
                        <svg class=icon_class fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        <span class="text-sm font-medium truncate">
                            {if is_completed {
                                format!("Restore completed — {} files", rp.total_files)
                            } else if is_failed {
                                rp.phase.clone()
                            } else {
                                format!("Restore: {} — {} / {} files", rp.phase, rp.completed_files, rp.total_files)
                            }}
                        </span>
                    </div>
                    {if is_completed || is_failed {
                        view! {
                            <button
                                class="ml-2 text-text-secondary hover:text-text-primary flex-shrink-0"
                                title="Dismiss"
                                on:click=move |_| set_restore_progress.set(RestoreProgress::default())
                            >
                                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                </div>

                {if is_auth_error {
                    view! {
                        <div class="flex items-center gap-3">
                            <p class="text-xs text-text-secondary">
                                "Storage authorization expired."
                            </p>
                            <button
                                class="text-xs font-medium text-primary-light hover:text-primary bg-primary-tint px-3 py-1 rounded-lg transition-colors flex-shrink-0"
                                on:click=move |_| {
                                    set_settings_tab.set("infrastructure");
                                    set_phase.set(AppPhase::Main(MainView::Settings));
                                }
                            >
                                "Reconnect Storage"
                            </button>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }}

                {if rp.is_running {
                    view! {
                        <div class="h-1.5 bg-input border border-border rounded-full overflow-hidden">
                            <div
                                class="h-full bg-gradient-to-r from-accent to-accent-light rounded-full transition-all duration-500"
                                style:width=format!("{}%", overall_pct)
                            ></div>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }}

                {if rp.is_running && !rp.current_file.is_empty() {
                    view! {
                        <div class="text-xs text-text-secondary truncate">{rp.current_file.clone()}</div>
                    }.into_any()
                } else {
                    ().into_any()
                }}
            </div>
        }.into_any()
    }
}

// ═══════════════════════════════════════════════════════════════════
// FILE BROWSER — state router
// ═══════════════════════════════════════════════════════════════════

#[component]
fn FileBrowser() -> impl IntoView {
    let (session, _) = use_context::<SessionSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();
    let configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>> = RwSignal::new(Vec::new());
    let selected_config: RwSignal<Option<DecryptedBackupConfigWithRemoteStorage>> =
        RwSignal::new(None);
    let files: RwSignal<Vec<BackedUpFile>> = RwSignal::new(Vec::new());
    let bin_files: RwSignal<Vec<BackedUpFile>> = RwSignal::new(Vec::new());
    let loading_files: RwSignal<bool> = RwSignal::new(false);
    let loading_more: RwSignal<bool> = RwSignal::new(false);
    let next_cursor: RwSignal<Option<Vec<u8>>> = RwSignal::new(None);
    let has_more: RwSignal<bool> = RwSignal::new(false);
    let total_files: RwSignal<Option<i64>> = RwSignal::new(None);
    let view_mode: RwSignal<ViewMode> = RwSignal::new(ViewMode::List);
    let search_query: RwSignal<String> = RwSignal::new(String::new());
    let type_filter: RwSignal<FileTypeGroup> = RwSignal::new(FileTypeGroup::All);
    let show_system: RwSignal<bool> = RwSignal::new(false);
    let sort_field: RwSignal<SortField> = RwSignal::new(SortField::Name);
    let sort_dir: RwSignal<SortDirection> = RwSignal::new(SortDirection::Asc);
    let restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>> = RwSignal::new(None);
    // Tracks which file IDs the user has checked for bulk actions.
    let selected_file_ids: RwSignal<BTreeSet<Uuid>> = RwSignal::new(BTreeSet::new());
    // True while a bulk move-to-bin operation is in flight; disables all bulk buttons.
    let bulk_op_pending: RwSignal<bool> = RwSignal::new(false);

    // Fetch all backup configs on mount
    Effect::new(move || {
        if !is_unlocked.get() {
            return;
        }
        spawn_local(async move {
            match tauri_invoke_no_args::<Vec<DecryptedBackupConfigWithRemoteStorage>>(
                "list_all_backup_configs",
            )
            .await
            {
                Ok(resp) => {
                    configs.set(resp);
                }
                Err(e) => {
                    leptos::logging::warn!("FileBrowser: failed to load configs: {}", e);
                }
            }
        });
    });

    let on_select_config = Callback::new(move |config: DecryptedBackupConfigWithRemoteStorage| {
        let config_id = config.config_id;
        selected_config.set(Some(config));
        loading_files.set(true);
        files.set(Vec::new());
        bin_files.set(Vec::new());
        next_cursor.set(None);
        has_more.set(false);
        total_files.set(None);
        search_query.set(String::new());
        selected_file_ids.set(BTreeSet::new());

        spawn_local(async move {
            let args = ListFilesArgs {
                backup_config_id: config_id,
                cursor: None,
                limit: Some(DEFAULT_PAGE_SIZE),
            };
            match tauri_invoke_with_args::<_, ListBackedUpFilesResponse>(
                "list_backed_up_files",
                &args,
            )
            .await
            {
                Ok(resp) => {
                    has_more.set(resp.has_more);
                    next_cursor.set(resp.next_cursor);
                    total_files.set(resp.total_files);
                    files.set(resp.files);
                }
                Err(e) => {
                    leptos::logging::warn!("Failed to load files: {}", e);
                }
            }
            // Also load bin files
            match tauri_invoke::<_, ListBinVersionsResponse>(
                "list_bin_versions",
                "backup_config_id",
                config_id,
            )
            .await
            {
                Ok(resp) => {
                    bin_files.set(resp.files);
                }
                Err(e) => {
                    leptos::logging::warn!("Failed to load bin files: {}", e);
                }
            }
            loading_files.set(false);
        });
    });

    // When the user clicks a config in the system tray, the navigate-to-config
    // event sets TrayNavSignal. This effect fires when configs loads and
    // auto-selects the target config, then clears the signal.
    let tray_nav = use_context::<TrayNavSignal>().unwrap();
    Effect::new(move || {
        let current_configs = configs.get();
        if current_configs.is_empty() {
            return;
        }
        let Some(target_id) = tray_nav.get_untracked() else {
            return;
        };
        if let Some(config) = current_configs
            .into_iter()
            .find(|c| c.config_id == target_id)
        {
            tray_nav.set(None);
            on_select_config.run(config);
        }
    });

    let on_back = Callback::new(move |()| {
        selected_config.set(None);
        files.set(Vec::new());
        bin_files.set(Vec::new());
        next_cursor.set(None);
        has_more.set(false);
        total_files.set(None);
        search_query.set(String::new());
        view_mode.set(ViewMode::List);
        selected_file_ids.set(BTreeSet::new());
    });

    // Clear selection whenever the user navigates to Bin — bin has its own state.
    Effect::new(move || {
        if view_mode.get() == ViewMode::Bin {
            selected_file_ids.set(BTreeSet::new());
        }
    });

    // Background auto-fetch: when switching to Folder view, automatically
    // load all remaining pages so the full directory tree can be built.
    // Uses a larger page size (200) for efficiency.
    let auto_fetching: RwSignal<bool> = RwSignal::new(false);

    Effect::new(move || {
        let mode = view_mode.get();
        if mode != ViewMode::Folder {
            return;
        }
        // Only trigger if there are more pages to load
        if !has_more.get_untracked() || auto_fetching.get_untracked() {
            return;
        }
        let config_id = match selected_config.get_untracked() {
            Some(cfg) => cfg.config_id,
            None => return,
        };
        auto_fetching.set(true);
        spawn_local(async move {
            let mut current_cursor = next_cursor.get_untracked();
            loop {
                let args = ListFilesArgs {
                    backup_config_id: config_id,
                    cursor: current_cursor,
                    limit: Some(200),
                };
                match tauri_invoke_with_args::<_, ListBackedUpFilesResponse>(
                    "list_backed_up_files",
                    &args,
                )
                .await
                {
                    Ok(resp) => {
                        has_more.set(resp.has_more);
                        next_cursor.set(resp.next_cursor.clone());
                        files.update(|f| f.extend(resp.files));
                        if !resp.has_more {
                            break;
                        }
                        current_cursor = resp.next_cursor;
                    }
                    Err(e) => {
                        leptos::logging::warn!("Auto-fetch failed: {}", e);
                        break;
                    }
                }
            }
            auto_fetching.set(false);
        });
    });

    let on_load_more = Callback::new(move |()| {
        let cursor = next_cursor.get_untracked();
        if cursor.is_none() || loading_more.get_untracked() {
            return;
        }
        let config_id = match selected_config.get_untracked() {
            Some(cfg) => cfg.config_id,
            None => return,
        };

        loading_more.set(true);
        spawn_local(async move {
            let args = ListFilesArgs {
                backup_config_id: config_id,
                cursor,
                limit: Some(DEFAULT_PAGE_SIZE),
            };
            match tauri_invoke_with_args::<_, ListBackedUpFilesResponse>(
                "list_backed_up_files",
                &args,
            )
            .await
            {
                Ok(resp) => {
                    has_more.set(resp.has_more);
                    next_cursor.set(resp.next_cursor);
                    // Append to existing files
                    files.update(|f| f.extend(resp.files));
                }
                Err(e) => {
                    leptos::logging::warn!("Failed to load more files: {}", e);
                }
            }
            loading_more.set(false);
        });
    });

    view! {
        // Restore modal overlay
        {move || {
            restore_modal.get().map(|(cid, sels)| {
                let is_current = selected_config.get()
                    .map(|cfg| cfg.physical_device_id.as_deref() == Some(session.get().physical_device_id.as_str()))
                    .unwrap_or(false);
                view! {
                    <RestoreModal
                        config_id=cid
                        selections=sels
                        is_current_device=is_current
                        on_close=Callback::new(move |started: bool| {
                            restore_modal.set(None);
                            // Clear selection only when the restore job was actually confirmed.
                            if started {
                                selected_file_ids.set(BTreeSet::new());
                            }
                        })
                    />
                }
            })
        }}

        <RestoreProgressBanner />

        // State router: ConfigGrid | LoadingSkeleton | FileListView
        <div class="glass bg-surface border border-border rounded-2xl overflow-hidden">
            {move || {
                if selected_config.get().is_none() {
                    view! {
                        <ConfigGrid
                            configs=configs
                            session=session
                            on_select_config=on_select_config
                        />
                    }.into_any()
                } else if loading_files.get() {
                    view! { <LoadingSkeleton /> }.into_any()
                } else {
                    let config = selected_config.get().unwrap();
                    view! {
                        <FileListView
                            config=config
                            files=files
                            bin_files=bin_files
                            view_mode=view_mode
                            search_query=search_query
                            type_filter=type_filter
                            show_system=show_system
                            sort_field=sort_field
                            sort_dir=sort_dir
                            restore_modal=restore_modal
                            loading_more=loading_more
                            auto_fetching=auto_fetching
                            has_more=has_more
                            total_files=total_files
                            on_back=on_back
                            on_load_more=on_load_more
                            selected_file_ids=selected_file_ids
                            bulk_op_pending=bulk_op_pending
                        />
                    }.into_any()
                }
            }}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// CONFIG GRID — responsive backup config selector
// ═══════════════════════════════════════════════════════════════════

#[component]
fn ConfigGrid(
    configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>>,
    session: ReadSignal<SessionInfo>,
    on_select_config: Callback<DecryptedBackupConfigWithRemoteStorage>,
) -> impl IntoView {
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();

    move || {
        let physical_device_id = session.get().physical_device_id.clone();
        let mut cfgs: Vec<_> = configs.get().into_iter().filter(|c| c.is_active).collect();
        if cfgs.is_empty() {
            return view! {
                <div class="flex flex-col items-center justify-center h-64 gap-3 text-center">
                    <p class="text-text-secondary text-sm">"No folders are being protected yet"</p>
                    <button
                        class="px-4 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                        on:click=move |_| set_phase.set(AppPhase::Main(MainView::Settings))
                    >
                        "Add Folder to Protect"
                    </button>
                </div>
            }
            .into_any();
        }
        cfgs.sort_by(|a, b| {
            let a_current = a.physical_device_id.as_deref() == Some(physical_device_id.as_str());
            let b_current = b.physical_device_id.as_deref() == Some(physical_device_id.as_str());
            b_current.cmp(&a_current)
        });
        view! {
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3 p-4">
                {cfgs.into_iter().map(|cfg| {
                    let is_current = cfg.physical_device_id.as_deref() == Some(physical_device_id.as_str());
                    let cfg_clone = cfg.clone();
                    let icon = storage_type_icon(&cfg.storage_type);
                    let card_class = if is_current {
                        "flex items-center gap-3 p-4 bg-primary-tint border border-primary-glow rounded-xl hover:bg-primary-tint hover-lift transition-all text-left"
                    } else {
                        "flex items-center gap-3 p-4 bg-bg/30 border border-border rounded-xl hover:bg-primary-tint hover:border-primary-glow hover-lift transition-all text-left"
                    };
                    view! {
                        <button
                            class=card_class
                            data-testid="config-card"
                            on:click=move |_| on_select_config.run(cfg_clone.clone())
                        >
                            <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center flex-shrink-0"
                                inner_html=icon
                            ></div>
                            <div class="min-w-0 flex-1">
                                <div class="flex items-center gap-2">
                                    <span class="text-sm font-medium truncate">{cfg.display_name.clone()}</span>
                                    {if is_current {
                                        view! {
                                            <span class="inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-medium bg-accent-tint text-accent flex-shrink-0">
                                                "This device"
                                            </span>
                                        }.into_any()
                                    } else {
                                        ().into_any()
                                    }}
                                </div>
                                <div class="text-xs text-text-secondary truncate" title=cfg.source_directory.clone()>
                                    {abbrev_home_path(&cfg.source_directory)}
                                </div>
                            </div>
                        </button>
                    }
                }).collect_view()}
            </div>
        }.into_any()
    }
}

// ═══════════════════════════════════════════════════════════════════
// LOADING SKELETON
// ═══════════════════════════════════════════════════════════════════

#[component]
fn LoadingSkeleton() -> impl IntoView {
    view! {
        <div class="p-4 space-y-2">
            {(0..6).map(|_| view! {
                <div class="flex items-center gap-2 py-2 md:py-2 animate-pulse" style="min-height: 40px">
                    <div class="w-4 h-4 bg-bg rounded flex-shrink-0"></div>
                    <div class="h-4 bg-bg rounded flex-1"></div>
                    <div class="h-4 w-16 bg-bg rounded"></div>
                </div>
            }).collect_view()}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// FILE LIST VIEW — orchestrator for selected-config state
// ═══════════════════════════════════════════════════════════════════

#[component]
fn FileListView(
    config: DecryptedBackupConfigWithRemoteStorage,
    files: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    view_mode: RwSignal<ViewMode>,
    search_query: RwSignal<String>,
    type_filter: RwSignal<FileTypeGroup>,
    show_system: RwSignal<bool>,
    sort_field: RwSignal<SortField>,
    sort_dir: RwSignal<SortDirection>,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    loading_more: RwSignal<bool>,
    auto_fetching: RwSignal<bool>,
    has_more: RwSignal<bool>,
    total_files: RwSignal<Option<i64>>,
    on_back: Callback<()>,
    on_load_more: Callback<()>,
    selected_file_ids: RwSignal<BTreeSet<Uuid>>,
    bulk_op_pending: RwSignal<bool>,
) -> impl IntoView {
    let config_id = config.config_id;
    let source_dir = config.source_directory.clone();
    let source_dir_for_content = source_dir.clone();
    let source_dir_for_tree = source_dir.clone();

    // Memo: filtered + sorted file list. Only recomputes when files, search, or sort change.
    // Uses ArcFileList wrapper with pointer-equality to avoid deep comparison of 20k files.
    let filtered_sorted: Memo<ArcFileList> = Memo::new(move |_| {
        let all_files = files.get();
        let query = search_query.get();
        let mut filtered = filter_files(&all_files, &query, type_filter.get(), show_system.get());
        sort_files(&mut filtered, sort_field.get(), sort_dir.get());
        ArcFileList(Arc::new(filtered))
    });

    // Memo: folder tree. Only recomputes when files or search change.
    // Skips sort (BTreeMap handles alphabetical ordering).
    // Returns None while auto-fetching to defer expensive tree build.
    let file_tree: Memo<ArcFileTree> = Memo::new(move |_| {
        if auto_fetching.get() {
            return ArcFileTree(None);
        }
        let all_files = files.get();
        let query = search_query.get();
        let filtered = filter_files(&all_files, &query, type_filter.get(), show_system.get());
        ArcFileTree(Some(Arc::new(build_file_tree(
            &filtered,
            &source_dir_for_tree,
        ))))
    });

    view! {
        <FileListHeader
            config=config
            view_mode=view_mode
            search_query=search_query
            type_filter=type_filter
            show_system=show_system
            sort_field=sort_field
            sort_dir=sort_dir
            bin_files=bin_files
            on_back=on_back
        />

        // Bulk action bar — visible when ≥1 file selected and not in Bin view.
        {move || {
            if selected_file_ids.get().is_empty() || view_mode.get() == ViewMode::Bin {
                return ().into_any();
            }
            view! {
                <BulkActionBar
                    selected_file_ids=selected_file_ids
                    files=files
                    config_id=config_id
                    restore_modal=restore_modal
                    bulk_op_pending=bulk_op_pending
                    bin_files=bin_files
                    has_more=has_more
                    total_files=total_files
                />
            }.into_any()
        }}

        <div class="min-h-[200px]">
            {move || {
                // Bin view
                if view_mode.get() == ViewMode::Bin {
                    let all_bin = bin_files.get();
                    if all_bin.is_empty() {
                        return view! {
                            <div class="flex items-center justify-center h-64 text-text-secondary text-sm">
                                "Bin is empty"
                            </div>
                        }.into_any();
                    }
                    let query = search_query.get();
                    let filtered_bin = filter_files(&all_bin, &query, type_filter.get(), show_system.get());
                    return view! {
                        <BinFileList files=filtered_bin source_directory=source_dir_for_content.clone() config_id=config_id bin_files=bin_files main_files=files />
                    }.into_any();
                }

                // Folder view
                if view_mode.get() == ViewMode::Folder {
                    return match file_tree.get().0 {
                        None => {
                            // Still auto-fetching — show progress instead of building tree
                            let loaded = files.with(|f| f.len());
                            view! {
                                <div class="flex flex-col items-center justify-center h-64 text-text-secondary text-sm gap-3">
                                    <div class="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
                                    <span>{format!("Loading files for folder view... {} loaded", loaded)}</span>
                                </div>
                            }.into_any()
                        }
                        Some(ref tree) if tree.is_empty() => {
                            view! {
                                <div class="flex items-center justify-center h-64 text-text-secondary text-sm">
                                    "No backed up files found"
                                </div>
                            }.into_any()
                        }
                        Some(tree) => {
                            let tree_owned: BTreeMap<String, FileTreeNode> = (*tree).clone();
                            view! {
                                <FolderTreeView
                                    tree=tree_owned
                                    config_id=config_id
                                    restore_modal=restore_modal
                                    files_signal=files
                                    bin_files=bin_files
                                    selected_file_ids=selected_file_ids
                                />
                            }.into_any()
                        }
                    };
                }

                // List view — uses memoized filtered+sorted files
                let filtered = filtered_sorted.get().0;
                if filtered.is_empty() {
                    let is_searching = search_query.with(|q| !q.is_empty());
                    return if is_searching {
                        let query = search_query.get();
                        view! {
                            <div class="flex flex-col items-center justify-center h-48 text-text-secondary text-sm gap-2">
                                <span>{format!("No files matching \"{}\"", query)}</span>
                                <button
                                    class="text-xs text-accent hover:text-accent-light transition-colors"
                                    on:click=move |_| search_query.set(String::new())
                                >
                                    "Clear search"
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="flex items-center justify-center h-64 text-text-secondary text-sm">
                                "No backed up files found"
                            </div>
                        }.into_any()
                    };
                }

                view! {
                    <div>
                        // Desktop table
                        <div class="hidden md:block">
                            <FileListTableDesktop
                                files=filtered.clone()
                                source_directory=source_dir_for_content.clone()
                                sort_field=sort_field
                                sort_dir=sort_dir
                                config_id=config_id
                                restore_modal=restore_modal
                                files_signal=files
                                bin_files=bin_files
                                selected_file_ids=selected_file_ids
                            />
                        </div>
                        // Mobile list
                        <div class="md:hidden">
                            <FileListMobile
                                files=filtered
                                source_directory=source_dir_for_content.clone()
                                config_id=config_id
                                restore_modal=restore_modal
                                files_signal=files
                                bin_files=bin_files
                                selected_file_ids=selected_file_ids
                            />
                        </div>
                    </div>
                }.into_any()
            }}
        </div>

        <PaginationFooter
            files=files
            loading_more=loading_more
            auto_fetching=auto_fetching
            has_more=has_more
            total_files=total_files
            on_load_more=on_load_more
        />
    }
}

// ═══════════════════════════════════════════════════════════════════
// FILE LIST HEADER — breadcrumb, view mode tabs, search bar
// ═══════════════════════════════════════════════════════════════════

#[component]
fn FileListHeader(
    config: DecryptedBackupConfigWithRemoteStorage,
    view_mode: RwSignal<ViewMode>,
    search_query: RwSignal<String>,
    type_filter: RwSignal<FileTypeGroup>,
    show_system: RwSignal<bool>,
    sort_field: RwSignal<SortField>,
    sort_dir: RwSignal<SortDirection>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    on_back: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="px-4 md:px-5 py-3 border-b border-border space-y-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="flex items-center gap-2 min-w-0">
                    <button
                        class="text-sm text-accent hover:text-accent-light transition-colors flex-shrink-0"
                        on:click=move |_| on_back.run(())
                    >
                        "Configs"
                    </button>
                    <span class="text-text-secondary text-sm">"/"</span>
                    <span class="text-sm font-medium truncate">{config.display_name.clone()}</span>
                </div>
                <div class="flex gap-1 bg-bg rounded-lg p-0.5 flex-shrink-0">
                    <button
                        class=move || if view_mode.get() == ViewMode::List {
                            "px-3 py-1 rounded-md text-sm font-medium bg-primary text-white transition-all"
                        } else {
                            "px-3 py-1 rounded-md text-sm font-medium text-text-secondary hover:text-text-primary transition-all"
                        }
                        on:click=move |_| view_mode.set(ViewMode::List)
                    >
                        "List"
                    </button>
                    <button
                        class=move || if view_mode.get() == ViewMode::Folder {
                            "px-3 py-1 rounded-md text-sm font-medium bg-primary text-white transition-all"
                        } else {
                            "px-3 py-1 rounded-md text-sm font-medium text-text-secondary hover:text-text-primary transition-all"
                        }
                        on:click=move |_| view_mode.set(ViewMode::Folder)
                    >
                        "Folder"
                    </button>
                    <button
                        class=move || {
                            let bin_count = bin_files.get().len();
                            let base = if view_mode.get() == ViewMode::Bin {
                                "px-3 py-1 rounded-md text-sm font-medium bg-red-500/80 text-white transition-all"
                            } else if bin_count > 0 {
                                "px-3 py-1 rounded-md text-sm font-medium text-red-400 hover:text-red-300 transition-all"
                            } else {
                                "px-3 py-1 rounded-md text-sm font-medium text-text-secondary hover:text-text-primary transition-all"
                            };
                            base
                        }
                        on:click=move |_| view_mode.set(ViewMode::Bin)
                    >
                        {move || {
                            let count = bin_files.get().len();
                            if count > 0 {
                                format!("Bin ({})", count)
                            } else {
                                "Bin".to_string()
                            }
                        }}
                    </button>
                </div>
            </div>
            // Search bar
            <div class="relative">
                <svg class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-secondary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                <input
                    type="text"
                    class="w-full bg-input border border-border rounded-lg pl-9 pr-8 py-2 text-sm focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary-tint transition-all"
                    placeholder="Search files..."
                    prop:value=move || search_query.get()
                    on:input=move |ev| search_query.set(event_target_value(&ev))
                />
                {move || (!search_query.get().is_empty()).then(|| view! {
                    <button
                        class="absolute right-2 top-1/2 -translate-y-1/2 w-5 h-5 flex items-center justify-center rounded-full text-text-secondary hover:text-text-primary hover:bg-bg transition-colors"
                        on:click=move |_| search_query.set(String::new())
                    >
                        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                            <line x1="18" y1="6" x2="6" y2="18" />
                            <line x1="6" y1="6" x2="18" y2="18" />
                        </svg>
                    </button>
                })}
            </div>
            // File-type filter pills + system-files toggle
            <div class="flex flex-wrap items-center gap-1.5">
                {[
                    ("All",       FileTypeGroup::All),
                    ("Documents", FileTypeGroup::Documents),
                    ("Images",    FileTypeGroup::Images),
                    ("Videos",    FileTypeGroup::Videos),
                    ("Audio",     FileTypeGroup::Audio),
                    ("Archives",  FileTypeGroup::Archives),
                ].into_iter().map(|(label, group)| {
                    view! {
                        <button
                            class=move || if type_filter.get() == group {
                                "px-2.5 py-1 rounded-full text-xs font-medium bg-primary text-white transition-colors"
                            } else {
                                "px-2.5 py-1 rounded-full text-xs font-medium bg-bg text-text-secondary hover:text-text-primary transition-colors"
                            }
                            on:click=move |_| type_filter.set(group)
                        >
                            {label}
                        </button>
                    }
                }).collect_view()}
                // Separator
                <span class="text-border text-xs select-none">"·"</span>
                // System-files toggle pill
                <button
                    class=move || if show_system.get() {
                        "px-2.5 py-1 rounded-full text-xs font-medium bg-warning-tint text-warning border border-warning-border transition-colors"
                    } else {
                        "px-2.5 py-1 rounded-full text-xs font-medium bg-bg text-text-secondary hover:text-text-primary transition-colors"
                    }
                    on:click=move |_| show_system.update(|v| *v = !*v)
                    title="Show or hide system files (.DS_Store, node_modules, build artifacts…)"
                >
                    {move || if show_system.get() { "System files: on" } else { "System files" }}
                </button>
            </div>
            // Mobile sort pills (hidden on desktop — desktop uses table column headers)
            <div class="md:hidden flex items-center gap-1.5">
                <span class="text-xs text-text-secondary flex-shrink-0">"Sort:"</span>
                {[("Name", SortField::Name), ("Size", SortField::Size), ("Date", SortField::Date)].into_iter().map(|(label, field)| {
                    view! {
                        <button
                            class=move || if sort_field.get() == field {
                                "px-2.5 py-1 rounded-full text-xs font-medium bg-primary text-white transition-colors"
                            } else {
                                "px-2.5 py-1 rounded-full text-xs font-medium bg-bg text-text-secondary hover:text-text-primary transition-colors"
                            }
                            on:click=move |_| {
                                if sort_field.get() == field {
                                    sort_dir.set(match sort_dir.get() {
                                        SortDirection::Asc => SortDirection::Desc,
                                        SortDirection::Desc => SortDirection::Asc,
                                    });
                                } else {
                                    sort_field.set(field);
                                    sort_dir.set(SortDirection::Asc);
                                }
                            }
                        >
                            {move || {
                                if sort_field.get() == field {
                                    let arrow = if sort_dir.get() == SortDirection::Asc { "\u{2191}" } else { "\u{2193}" };
                                    format!("{} {}", label, arrow)
                                } else {
                                    label.to_string()
                                }
                            }}
                        </button>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// BULK ACTION BAR — appears above list/folder view when files are selected
// ═══════════════════════════════════════════════════════════════════

#[component]
fn BulkActionBar(
    selected_file_ids: RwSignal<BTreeSet<Uuid>>,
    files: RwSignal<Vec<BackedUpFile>>,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    bulk_op_pending: RwSignal<bool>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    has_more: RwSignal<bool>,
    total_files: RwSignal<Option<i64>>,
) -> impl IntoView {
    // Label: "N selected" or "N selected (M of T loaded)" when pagination is incomplete.
    let count_label = move || {
        let selected = selected_file_ids.get().len();
        if has_more.get() {
            let loaded = files.with(|f| f.len());
            if let Some(total) = total_files.get() {
                return format!("{} selected ({} of {} loaded)", selected, loaded, total);
            }
        }
        format!("{} selected", selected)
    };

    // Opens the restore modal pre-populated with the latest version of every selected file.
    let on_restore = move |_| {
        let ids = selected_file_ids.get();
        let all_files = files.get();
        let selections: Vec<RestoreSelection> = ids
            .iter()
            .filter_map(|id| all_files.iter().find(|f| &f.file_id == id))
            .filter_map(|f| {
                f.versions.last().map(|v| RestoreSelection {
                    version_id: v.version_id,
                    file_path: f.path.clone(),
                    size: v.size,
                    version: v.version,
                })
            })
            .collect();
        if !selections.is_empty() {
            restore_modal.set(Some((config_id, selections)));
        }
    };

    // Calls `move_all_versions_to_bin` once per selected file sequentially.
    // Files are removed from the main list and the bin is reloaded on success.
    let on_move_to_bin = move |_| {
        let ids: Vec<Uuid> = selected_file_ids.get().into_iter().collect();
        let all_files = files.get();
        // Collect (file_id, blind_index) for each selected file that exists in the loaded list.
        let targets: Vec<(Uuid, Vec<u8>)> = ids
            .iter()
            .filter_map(|id| {
                all_files
                    .iter()
                    .find(|f| &f.file_id == id)
                    .map(|f| (f.file_id, f.blind_index.clone()))
            })
            .collect();

        if targets.is_empty() {
            return;
        }

        bulk_op_pending.set(true);
        spawn_local(async move {
            let mut deleted_ids: Vec<Uuid> = Vec::new();
            for (file_id, blind_index) in targets {
                match tauri_invoke_with_args::<_, ()>(
                    "move_all_versions_to_bin",
                    &MoveAllVersionsToBinArgs {
                        backup_config_id: config_id,
                        name_blind_index: blind_index,
                    },
                )
                .await
                {
                    Ok(()) => deleted_ids.push(file_id),
                    Err(e) => {
                        leptos::logging::warn!("Bulk move to bin failed for {}: {}", file_id, e)
                    }
                }
            }

            if !deleted_ids.is_empty() {
                files.update(|f| f.retain(|file| !deleted_ids.contains(&file.file_id)));
                // Reload bin so the count badge and bin view stay accurate.
                if let Ok(resp) = tauri_invoke::<_, ListBinVersionsResponse>(
                    "list_bin_versions",
                    "backup_config_id",
                    config_id,
                )
                .await
                {
                    bin_files.set(resp.files);
                }
            }
            selected_file_ids.set(BTreeSet::new());
            bulk_op_pending.set(false);
        });
    };

    view! {
        <div
            class="px-4 py-2 flex items-center gap-3 border-b border-border bg-primary-tint/20 text-sm flex-wrap"
            data-testid="bulk-action-bar"
        >
            <span class="text-text-secondary text-xs flex-shrink-0">{count_label}</span>
            <div class="flex items-center gap-2">
                // Restore selected files (opens modal with latest version of each)
                <button
                    class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-accent hover:bg-accent-tint transition-colors text-xs font-medium disabled:opacity-50 disabled:cursor-not-allowed"
                    data-testid="bulk-restore-button"
                    disabled=move || bulk_op_pending.get()
                    on:click=on_restore
                >
                    <svg class="w-3.5 h-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <polyline points="1 4 1 10 7 10"/>
                        <path d="M3.51 15a9 9 0 102.13-9.36L1 10"/>
                    </svg>
                    "Restore"
                </button>
                // Move all versions of each selected file to the bin
                <button
                    class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-red-400 hover:bg-red-500/10 transition-colors text-xs font-medium disabled:opacity-50 disabled:cursor-not-allowed"
                    data-testid="bulk-move-to-bin-button"
                    disabled=move || bulk_op_pending.get()
                    on:click=on_move_to_bin
                >
                    <svg class="w-3.5 h-3.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <polyline points="3 6 5 6 21 6"/>
                        <path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/>
                    </svg>
                    {move || if bulk_op_pending.get() { "Working..." } else { "Move to Bin" }}
                </button>
                // Deselect all without performing any action
                <button
                    class="px-3 py-1.5 rounded-lg text-text-secondary hover:text-text-primary hover:bg-bg transition-colors text-xs font-medium disabled:opacity-50"
                    data-testid="bulk-clear-button"
                    disabled=move || bulk_op_pending.get()
                    on:click=move |_| selected_file_ids.set(BTreeSet::new())
                >
                    "Clear"
                </button>
            </div>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// FILE LIST TABLE — DESKTOP (virtual scroll, sortable columns, hover actions)
// ═══════════════════════════════════════════════════════════════════

/// Estimated row height in pixels for virtual scroll calculations.
const DESKTOP_ROW_HEIGHT: f64 = 36.0;
const MOBILE_ROW_HEIGHT: f64 = 56.0;
/// Number of extra rows rendered above/below the visible area.
const SCROLL_BUFFER: usize = 10;

#[component]
fn FileListTableDesktop(
    files: Arc<Vec<BackedUpFile>>,
    source_directory: String,
    sort_field: RwSignal<SortField>,
    sort_dir: RwSignal<SortDirection>,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    files_signal: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    selected_file_ids: RwSignal<BTreeSet<Uuid>>,
) -> impl IntoView {
    let expanded: RwSignal<Option<Uuid>> = RwSignal::new(None);
    let scroll_top: RwSignal<f64> = RwSignal::new(0.0);
    let container_height: RwSignal<f64> = RwSignal::new(600.0);
    let total_count = files.len();

    let toggle_sort = move |field: SortField| {
        if sort_field.get() == field {
            sort_dir.set(match sort_dir.get() {
                SortDirection::Asc => SortDirection::Desc,
                SortDirection::Desc => SortDirection::Asc,
            });
        } else {
            sort_field.set(field);
            sort_dir.set(SortDirection::Asc);
        }
    };

    let sort_indicator = move |field: SortField| -> &'static str {
        if sort_field.get() == field {
            if sort_dir.get() == SortDirection::Asc {
                "\u{25B2}"
            } else {
                "\u{25BC}"
            }
        } else {
            ""
        }
    };

    // Measure container on mount
    let container_ref = NodeRef::<leptos::html::Div>::new();
    Effect::new(move || {
        if let Some(el) = container_ref.get() {
            let h: f64 = el.client_height() as f64;
            if h > 0.0 {
                container_height.set(h);
            }
        }
    });

    view! {
        <div>
            // Column headers (sticky, outside scroll)
            <div class="flex items-center pl-2 pr-4 py-2 text-xs text-text-secondary uppercase tracking-wider border-b border-border bg-bg/30">
                // Select-all checkbox: checks/unchecks all currently-loaded files
                <div class="w-8 flex items-center justify-center flex-shrink-0">
                    {
                        let files_for_checked = files.clone();
                        let files_for_change = files.clone();
                        view! {
                            <input
                                type="checkbox"
                                data-testid="select-all-checkbox"
                                class="w-3.5 h-3.5 rounded accent-primary cursor-pointer"
                                prop:checked=move || {
                                    let ids = selected_file_ids.get();
                                    !ids.is_empty() && files_for_checked.iter().all(|f| ids.contains(&f.file_id))
                                }
                                on:change=move |ev| {
                                    use wasm_bindgen::JsCast;
                                    let checked = ev.target().unwrap()
                                        .unchecked_into::<web_sys::HtmlInputElement>()
                                        .checked();
                                    if checked {
                                        selected_file_ids.update(|s| {
                                            s.extend(files_for_change.iter().map(|f| f.file_id));
                                        });
                                    } else {
                                        selected_file_ids.update(|s| {
                                            for f in files_for_change.iter() {
                                                s.remove(&f.file_id);
                                            }
                                        });
                                    }
                                }
                            />
                        }
                    }
                </div>
                <div class="flex-1 grid grid-cols-[1fr_80px_140px_60px] gap-2">
                    <button class="text-left hover:text-text-primary transition-colors flex items-center gap-1" on:click=move |_| toggle_sort(SortField::Name)>
                        "Path"
                        <span class="text-[10px] text-primary">{move || sort_indicator(SortField::Name)}</span>
                    </button>
                    <button class="text-right hover:text-text-primary transition-colors flex items-center justify-end gap-1" on:click=move |_| toggle_sort(SortField::Size)>
                        "Size"
                        <span class="text-[10px] text-primary">{move || sort_indicator(SortField::Size)}</span>
                    </button>
                    <button class="text-right hover:text-text-primary transition-colors flex items-center justify-end gap-1" on:click=move |_| toggle_sort(SortField::Date)>
                        "Date"
                        <span class="text-[10px] text-primary">{move || sort_indicator(SortField::Date)}</span>
                    </button>
                    <div class="text-right">"Ver"</div>
                </div>
            </div>

            // Virtual scroll container
            <div
                node_ref=container_ref
                class="overflow-y-auto"
                style="max-height: min(70vh, 800px)"
                on:scroll=move |ev| {
                    use wasm_bindgen::JsCast;
                    if let Some(target) = ev.target() {
                        let el: web_sys::Element = target.unchecked_into();
                        scroll_top.set(el.scroll_top() as f64);
                    }
                }
            >
                {move || {
                    let st = scroll_top.get();
                    let ch = container_height.get();
                    let start = ((st / DESKTOP_ROW_HEIGHT).floor() as usize).saturating_sub(SCROLL_BUFFER);
                    let visible_count = (ch / DESKTOP_ROW_HEIGHT).ceil() as usize + 2 * SCROLL_BUFFER;
                    let end = (start + visible_count).min(total_count);

                    let top_spacer = start as f64 * DESKTOP_ROW_HEIGHT;
                    let bottom_spacer = total_count.saturating_sub(end) as f64 * DESKTOP_ROW_HEIGHT;

                    let visible_files: Vec<BackedUpFile> = files[start..end].to_vec();
                    let source_dir = source_directory.clone();

                    view! {
                        <div style=format!("height: {}px", top_spacer)></div>
                        {visible_files.into_iter().map(|file| {
                            let file_id = file.file_id;
                            let relative_path = file.path.strip_prefix(&source_dir)
                                .unwrap_or(&file.path)
                                .trim_start_matches('/')
                                .to_string();
                            let size_str = format_size(file.size);
                            let version_count = file.versions.len();
                            let versions = file.versions.clone();
                            let file_name = relative_path.rsplit('/').next().unwrap_or(&relative_path).to_string();
                            let icon = file_type_icon(&file_name);
                            let last_date = file.versions.last()
                                .map(|v| format_local_datetime(&v.created_at))
                                .unwrap_or_default();

                            view! {
                                <div>
                                    // Row: checkbox cell + expand button side-by-side
                                    <div class=move || {
                                        if selected_file_ids.get().contains(&file_id) {
                                            "group flex items-stretch pl-2 hover:bg-bg transition-colors bg-primary/5"
                                        } else {
                                            "group flex items-stretch pl-2 hover:bg-bg transition-colors"
                                        }
                                    }>
                                        // Checkbox — clicking it does not expand/collapse the row
                                        <div class="w-8 flex items-center justify-center flex-shrink-0">
                                            <input
                                                type="checkbox"
                                                data-testid="file-checkbox"
                                                class="w-3.5 h-3.5 rounded accent-primary cursor-pointer"
                                                prop:checked=move || selected_file_ids.get().contains(&file_id)
                                                on:change=move |_| {
                                                    selected_file_ids.update(|s| {
                                                        if s.contains(&file_id) {
                                                            s.remove(&file_id);
                                                        } else {
                                                            s.insert(file_id);
                                                        }
                                                    });
                                                }
                                            />
                                        </div>
                                        // Expand/collapse button with the file metadata columns
                                        <button
                                            class="flex-1 grid grid-cols-[1fr_80px_140px_60px] gap-2 pr-4 py-1.5 transition-colors text-left"
                                            data-testid="file-row"
                                            on:click=move |_| {
                                                if expanded.get() == Some(file_id) {
                                                    expanded.set(None);
                                                } else {
                                                    expanded.set(Some(file_id));
                                                }
                                            }
                                        >
                                            <div class="flex items-center gap-2 min-w-0">
                                                <div class="flex-shrink-0" inner_html=icon></div>
                                                <span class="text-sm truncate">{relative_path}</span>
                                            </div>
                                            <div class="text-sm text-text-secondary text-right">{size_str}</div>
                                            <div class="text-xs text-text-secondary text-right">{last_date}</div>
                                            <div class="text-right">
                                                <span class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded-full bg-primary-tint text-primary text-xs font-medium">
                                                    {version_count}
                                                </span>
                                            </div>
                                        </button>
                                    </div>
                                    {move || {
                                        if expanded.get() == Some(file_id) {
                                            let vers = versions.clone();
                                            let fp = file.path.clone();
                                            view! {
                                                <div class="px-4 pb-2 ml-6">
                                                    {vers.into_iter().map(|v| {
                                                        view! { <VersionRowDesktop v=v file_path=fp.clone() config_id=config_id restore_modal=restore_modal files=files_signal bin_files=bin_files /> }
                                                    }).collect_view()}
                                                </div>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }
                                    }}
                                </div>
                            }
                        }).collect_view()}
                        <div style=format!("height: {}px", bottom_spacer)></div>
                    }
                }}
            </div>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// FILE LIST — MOBILE (virtual scroll, compact cards, tap-to-expand)
// ═══════════════════════════════════════════════════════════════════

#[component]
fn FileListMobile(
    files: Arc<Vec<BackedUpFile>>,
    source_directory: String,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    files_signal: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    selected_file_ids: RwSignal<BTreeSet<Uuid>>,
) -> impl IntoView {
    let expanded: RwSignal<Option<Uuid>> = RwSignal::new(None);
    let tapped_path: RwSignal<Option<Uuid>> = RwSignal::new(None);
    let scroll_top: RwSignal<f64> = RwSignal::new(0.0);
    let container_height: RwSignal<f64> = RwSignal::new(600.0);
    let total_count = files.len();

    let container_ref = NodeRef::<leptos::html::Div>::new();
    Effect::new(move || {
        if let Some(el) = container_ref.get() {
            let h: f64 = el.client_height() as f64;
            if h > 0.0 {
                container_height.set(h);
            }
        }
    });

    view! {
        <div
            node_ref=container_ref
            class="overflow-y-auto"
            style="max-height: min(70vh, 800px)"
            on:scroll=move |ev| {
                use wasm_bindgen::JsCast;
                if let Some(target) = ev.target() {
                    let el: web_sys::Element = target.unchecked_into();
                    scroll_top.set(el.scroll_top() as f64);
                }
            }
        >
            {move || {
                let st = scroll_top.get();
                let ch = container_height.get();
                let start = ((st / MOBILE_ROW_HEIGHT).floor() as usize).saturating_sub(SCROLL_BUFFER);
                let visible_count = (ch / MOBILE_ROW_HEIGHT).ceil() as usize + 2 * SCROLL_BUFFER;
                let end = (start + visible_count).min(total_count);

                let top_spacer = start as f64 * MOBILE_ROW_HEIGHT;
                let bottom_spacer = total_count.saturating_sub(end) as f64 * MOBILE_ROW_HEIGHT;

                let visible_files: Vec<BackedUpFile> = files[start..end].to_vec();
                let source_dir = source_directory.clone();

                view! {
                    <div style=format!("height: {}px", top_spacer)></div>
                    {visible_files.into_iter().map(|file| {
                        let file_id = file.file_id;
                        let relative_path = file.path.strip_prefix(&source_dir)
                            .unwrap_or(&file.path)
                            .trim_start_matches('/')
                            .to_string();
                        let size_str = format_size(file.size);
                        let version_count = file.versions.len();
                        let versions = file.versions.clone();
                        let file_name = relative_path.rsplit('/').next().unwrap_or(&relative_path).to_string();
                        let icon = file_type_icon(&file_name);

                        view! {
                            <div>
                                // Row: checkbox + expand button
                                <div class=move || {
                                    if selected_file_ids.get().contains(&file_id) {
                                        "flex items-stretch border-b border-border-alpha bg-primary/5 hover:bg-bg active:bg-bg/80 transition-colors"
                                    } else {
                                        "flex items-stretch border-b border-border-alpha hover:bg-bg active:bg-bg/80 transition-colors"
                                    }
                                }
                                style="min-height: 48px">
                                    // Checkbox with generous touch target
                                    <div class="flex items-center justify-center pl-3 pr-1 flex-shrink-0">
                                        <input
                                            type="checkbox"
                                            data-testid="file-checkbox"
                                            class="w-4 h-4 rounded accent-primary cursor-pointer"
                                            prop:checked=move || selected_file_ids.get().contains(&file_id)
                                            on:change=move |_| {
                                                selected_file_ids.update(|s| {
                                                    if s.contains(&file_id) {
                                                        s.remove(&file_id);
                                                    } else {
                                                        s.insert(file_id);
                                                    }
                                                });
                                            }
                                        />
                                    </div>
                                    // Expand/collapse button
                                    <button
                                        class="flex-1 px-3 py-3 text-left"
                                        on:click=move |_| {
                                            if expanded.get() == Some(file_id) {
                                                expanded.set(None);
                                            } else {
                                                expanded.set(Some(file_id));
                                            }
                                        }
                                    >
                                    <div class="flex items-center gap-2 mb-1">
                                        <div class="flex-shrink-0" inner_html=icon></div>
                                        {
                                            let rp = relative_path.clone();
                                            view! {
                                                <span
                                                    class=move || if tapped_path.get() == Some(file_id) {
                                                        "text-sm break-all whitespace-normal"
                                                    } else {
                                                        "text-sm truncate"
                                                    }
                                                    on:click=move |ev: leptos::ev::MouseEvent| {
                                                        ev.stop_propagation();
                                                        if tapped_path.get() == Some(file_id) {
                                                            tapped_path.set(None);
                                                        } else {
                                                            tapped_path.set(Some(file_id));
                                                        }
                                                    }
                                                >
                                                    {rp}
                                                </span>
                                            }
                                        }
                                    </div>
                                    <div class="flex items-center gap-3 ml-6 text-xs text-text-secondary">
                                        <span>{size_str}</span>
                                        <span class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded-full bg-primary-tint text-primary text-xs font-medium">
                                            {format!("{} ver", version_count)}
                                        </span>
                                    </div>
                                    </button>
                                </div> // close flex row (checkbox + button)
                                {move || {
                                    if expanded.get() == Some(file_id) {
                                        let vers = versions.clone();
                                        let fp = file.path.clone();
                                        view! {
                                            <div class="px-3 pb-2 ml-4">
                                                {vers.into_iter().map(|v| {
                                                    view! { <VersionRowMobile v=v file_path=fp.clone() config_id=config_id restore_modal=restore_modal files=files_signal bin_files=bin_files /> }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    } else {
                                        ().into_any()
                                    }
                                }}
                            </div>
                        }
                    }).collect_view()}
                    <div style=format!("height: {}px", bottom_spacer)></div>
                }
            }}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// PAGINATION FOOTER
// ═══════════════════════════════════════════════════════════════════

#[component]
fn PaginationFooter(
    files: RwSignal<Vec<BackedUpFile>>,
    loading_more: RwSignal<bool>,
    auto_fetching: RwSignal<bool>,
    has_more: RwSignal<bool>,
    total_files: RwSignal<Option<i64>>,
    on_load_more: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="mt-4 flex flex-col items-center gap-3">
            // File count status
            <div class="text-xs text-text-secondary">
                {move || {
                    let loaded = files.get().len();
                    let fetching = auto_fetching.get();
                    match total_files.get() {
                        Some(total) if has_more.get() && fetching =>
                            format!("Loading files... {} of {} loaded", loaded, total),
                        Some(total) if has_more.get() =>
                            format!("Showing {} of {} files", loaded, total),
                        Some(total) => format!("{} files", total),
                        None if fetching => format!("Loading files... {} loaded", loaded),
                        None => format!("{} files loaded", loaded),
                    }
                }}
            </div>

            // Load more button (hidden during auto-fetch)
            {move || {
                if has_more.get() && !auto_fetching.get() {
                    view! {
                        <button
                            class="px-6 py-2.5 border border-border text-text-secondary rounded-xl text-sm font-medium hover:bg-surface hover:text-text-primary transition-all disabled:opacity-50"
                            disabled=move || loading_more.get()
                            on:click=move |_| on_load_more.run(())
                        >
                            {move || if loading_more.get() {
                                "Loading..."
                            } else {
                                "Load More"
                            }}
                        </button>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// VERSION ROWS — desktop and mobile variants
// ═══════════════════════════════════════════════════════════════════

/// Responsive wrapper used by FolderTreeView (which is shared across both layouts).
#[component]
fn VersionRow(
    v: FileVersionSummary,
    file_path: String,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    files: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
) -> impl IntoView {
    let v2 = v.clone();
    let fp2 = file_path.clone();

    view! {
        <div class="animate-expand-in">
            <div class="hidden md:block">
                <VersionRowDesktop v=v file_path=file_path config_id=config_id restore_modal=restore_modal files=files bin_files=bin_files />
            </div>
            <div class="md:hidden">
                <VersionRowMobile v=v2 file_path=fp2 config_id=config_id restore_modal=restore_modal files=files bin_files=bin_files />
            </div>
        </div>
    }
}

#[component]
fn VersionRowDesktop(
    v: FileVersionSummary,
    file_path: String,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    files: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
) -> impl IntoView {
    let v_size = format_size(v.size);
    let v_date = format_local_datetime(&v.created_at);
    let v_status = format!("{:?}", v.status);
    let version_id = v.version_id;
    let version = v.version;
    let size = v.size;
    let deleting = RwSignal::new(false);

    view! {
        <div class="animate-expand-in flex items-center gap-4 py-1 px-3 text-xs text-text-secondary border-l-2 border-primary-light bg-primary-tint/30 rounded-r-lg mb-0.5 group">
            <span class="inline-flex items-center justify-center min-w-[28px] h-5 px-1.5 rounded-full bg-primary-tint text-primary text-[10px] font-semibold">
                {format!("v{}", version)}
            </span>
            <span class="w-16 text-right">{v_size}</span>
            <span class="truncate flex-1">{v_status}</span>
            <span class="flex-shrink-0 w-36 text-right">{v_date}</span>
            <button
                class="flex-shrink-0 flex items-center gap-1 px-2 py-1 rounded-lg text-accent opacity-0 group-hover:opacity-100 hover:bg-accent-tint transition-all text-xs font-medium"
                style="min-height: 26px"
                data-testid="version-restore-button"
                on:click=move |ev: leptos::ev::MouseEvent| {
                    ev.stop_propagation();
                    restore_modal.set(Some((config_id, vec![RestoreSelection {
                        version_id,
                        file_path: file_path.clone(),
                        size,
                        version,
                    }])));
                }
            >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="1 4 1 10 7 10"/>
                    <path d="M3.51 15a9 9 0 102.13-9.36L1 10"/>
                </svg>
                "Restore"
            </button>
            <button
                class="flex-shrink-0 flex items-center gap-1 px-2 py-1 rounded-lg text-red-400 opacity-0 group-hover:opacity-100 hover:bg-red-500/10 transition-all text-xs font-medium"
                style="min-height: 26px"
                disabled=move || deleting.get()
                on:click=move |ev: leptos::ev::MouseEvent| {
                    ev.stop_propagation();
                    deleting.set(true);
                    spawn_local(async move {
                        match tauri_invoke::<_, ()>("move_version_to_bin", "version_id", version_id).await {
                            Ok(()) => {
                                // Remove version from files signal
                                files.update(|f| {
                                    for file in f.iter_mut() {
                                        file.versions.retain(|v| v.version_id != version_id);
                                    }
                                    f.retain(|file| !file.versions.is_empty());
                                });
                                // Reload bin count
                                if let Ok(resp) = tauri_invoke::<_, ListBinVersionsResponse>("list_bin_versions", "backup_config_id", config_id).await {
                                    bin_files.set(resp.files);
                                }
                            }
                            Err(e) => {
                                leptos::logging::warn!("Failed to move to bin: {}", e);
                            }
                        }
                        deleting.set(false);
                    });
                }
            >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="3 6 5 6 21 6"/>
                    <path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/>
                </svg>
                {move || if deleting.get() { "..." } else { "Delete" }}
            </button>
        </div>
    }
}

#[component]
fn VersionRowMobile(
    v: FileVersionSummary,
    file_path: String,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    files: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
) -> impl IntoView {
    let v_size = format_size(v.size);
    let v_date = format_local_datetime(&v.created_at);
    let version_id = v.version_id;
    let version = v.version;
    let size = v.size;
    let deleting = RwSignal::new(false);

    view! {
        <div class="animate-expand-in py-2 px-3 text-xs text-text-secondary border-l-2 border-primary-light bg-primary-tint/30 rounded-r-lg mb-0.5">
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-2">
                    <span class="inline-flex items-center justify-center min-w-[28px] h-5 px-1.5 rounded-full bg-primary-tint text-primary text-[10px] font-semibold">
                        {format!("v{}", version)}
                    </span>
                    <span>{v_size}</span>
                </div>
                <div class="flex items-center gap-1">
                    <button
                        class="flex-shrink-0 w-10 h-10 flex items-center justify-center rounded-lg text-accent hover:bg-accent-tint transition-colors"
                        on:click=move |ev: leptos::ev::MouseEvent| {
                            ev.stop_propagation();
                            restore_modal.set(Some((config_id, vec![RestoreSelection {
                                version_id,
                                file_path: file_path.clone(),
                                size,
                                version,
                            }])));
                        }
                    >
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <polyline points="1 4 1 10 7 10"/>
                            <path d="M3.51 15a9 9 0 102.13-9.36L1 10"/>
                        </svg>
                    </button>
                    <button
                        class="flex-shrink-0 w-10 h-10 flex items-center justify-center rounded-lg text-red-400 hover:bg-red-500/10 transition-colors"
                        disabled=move || deleting.get()
                        on:click=move |ev: leptos::ev::MouseEvent| {
                            ev.stop_propagation();
                            deleting.set(true);
                            spawn_local(async move {
                                match tauri_invoke::<_, ()>("move_version_to_bin", "version_id", version_id).await {
                                    Ok(()) => {
                                        files.update(|f| {
                                            for file in f.iter_mut() {
                                                file.versions.retain(|v| v.version_id != version_id);
                                            }
                                            f.retain(|file| !file.versions.is_empty());
                                        });
                                        if let Ok(resp) = tauri_invoke::<_, ListBinVersionsResponse>("list_bin_versions", "backup_config_id", config_id).await {
                                            bin_files.set(resp.files);
                                        }
                                    }
                                    Err(e) => {
                                        leptos::logging::warn!("Failed to move to bin: {}", e);
                                    }
                                }
                                deleting.set(false);
                            });
                        }
                    >
                        {move || if deleting.get() {
                            view! { <span class="text-xs">"..."</span> }.into_any()
                        } else {
                            view! {
                                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                    <polyline points="3 6 5 6 21 6"/>
                                    <path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/>
                                </svg>
                            }.into_any()
                        }}
                    </button>
                </div>
            </div>
            <div class="mt-0.5 text-[10px] text-text-secondary/70">{v_date}</div>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// FOLDER TREE VIEW — shared by both desktop and mobile
// ═══════════════════════════════════════════════════════════════════

#[component]
fn FolderTreeView(
    tree: BTreeMap<String, FileTreeNode>,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    files_signal: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    selected_file_ids: RwSignal<BTreeSet<Uuid>>,
) -> impl IntoView {
    view! {
        <div class="p-2">
            <TreeNodeList nodes=tree depth=0 config_id=config_id restore_modal=restore_modal files_signal=files_signal bin_files=bin_files selected_file_ids=selected_file_ids />
        </div>
    }
}

#[component]
fn TreeNodeList(
    nodes: BTreeMap<String, FileTreeNode>,
    depth: usize,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    files_signal: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    selected_file_ids: RwSignal<BTreeSet<Uuid>>,
) -> impl IntoView {
    nodes
        .into_iter()
        .map(|(_, node)| {
            view! { <TreeNodeView node=node depth=depth config_id=config_id restore_modal=restore_modal files_signal=files_signal bin_files=bin_files selected_file_ids=selected_file_ids /> }
        })
        .collect_view()
}

#[component]
fn TreeNodeView(
    node: FileTreeNode,
    depth: usize,
    config_id: Uuid,
    restore_modal: RwSignal<Option<(Uuid, Vec<RestoreSelection>)>>,
    files_signal: RwSignal<Vec<BackedUpFile>>,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    selected_file_ids: RwSignal<BTreeSet<Uuid>>,
) -> impl IntoView {
    let expanded: RwSignal<bool> = RwSignal::new(false);
    let padding_left = format!("{}px", depth * 16 + 8);

    match node {
        FileTreeNode::Directory { name, children } => {
            let folder_files = collect_files_from_tree(&children);
            let folder_files_for_restore = folder_files.clone();
            // Pre-collect descendant file IDs for checkbox state tracking.
            let descendant_ids: Vec<Uuid> = folder_files.iter().map(|f| f.file_id).collect();
            let descendant_ids_for_change = descendant_ids.clone();
            let descendant_files_for_change = folder_files.clone();

            // NodeRef used to imperatively set the indeterminate DOM property,
            // which cannot be expressed as a plain HTML attribute.
            let dir_checkbox_ref = NodeRef::<leptos::html::Input>::new();

            Effect::new(move || {
                let ids = selected_file_ids.get();
                let selected_count = descendant_ids.iter().filter(|id| ids.contains(id)).count();
                let total = descendant_ids.len();
                let is_indeterminate = selected_count > 0 && selected_count < total;
                if let Some(el) = dir_checkbox_ref.get() {
                    el.set_indeterminate(is_indeterminate);
                }
            });

            view! {
                <div>
                    <div
                        class="w-full flex items-center gap-2 py-2.5 hover:bg-bg transition-colors border-b border-border-alpha"
                        style=format!("padding-left: {}; min-height: 44px", padding_left)
                    >
                        // Directory checkbox: selects/deselects all descendant files.
                        // Indeterminate state is set reactively via the Effect above.
                        <div class="flex items-center justify-center flex-shrink-0">
                            {
                                let desc_ids = descendant_ids_for_change.clone();
                                let desc_files = descendant_files_for_change.clone();
                                view! {
                                    <input
                                        type="checkbox"
                                        class="w-3.5 h-3.5 rounded accent-primary cursor-pointer"
                                        node_ref=dir_checkbox_ref
                                        prop:checked=move || {
                                            let ids = selected_file_ids.get();
                                            !desc_ids.is_empty() && desc_ids.iter().all(|id| ids.contains(id))
                                        }
                                        on:change=move |ev| {
                                            use wasm_bindgen::JsCast;
                                            let checked = ev.target().unwrap()
                                                .unchecked_into::<web_sys::HtmlInputElement>()
                                                .checked();
                                            selected_file_ids.update(|s| {
                                                if checked {
                                                    s.extend(desc_files.iter().map(|f| f.file_id));
                                                } else {
                                                    for f in &desc_files {
                                                        s.remove(&f.file_id);
                                                    }
                                                }
                                            });
                                        }
                                    />
                                }
                            }
                        </div>
                        <button
                            class="flex items-center gap-2 flex-1 min-w-0"
                            on:click=move |_| expanded.update(|v| *v = !*v)
                        >
                            <svg
                                class=move || if expanded.get() {
                                    "w-3 h-3 text-text-secondary transition-transform rotate-90 flex-shrink-0"
                                } else {
                                    "w-3 h-3 text-text-secondary transition-transform flex-shrink-0"
                                }
                                fill="currentColor" viewBox="0 0 20 20"
                            >
                                <path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd" />
                            </svg>
                            <svg class="w-4 h-4 text-primary flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                    d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                            </svg>
                            <span class="text-sm font-medium truncate">{name}</span>
                        </button>
                        <button
                            class="flex-shrink-0 flex items-center gap-1 px-2 py-1 rounded-lg text-accent hover:bg-accent-tint transition-colors text-xs font-medium mr-2"
                            style="min-height: 32px"
                            title="Restore folder"
                            on:click={
                                let files = folder_files_for_restore.clone();
                                move |ev: leptos::ev::MouseEvent| {
                                    ev.stop_propagation();
                                    let selections: Vec<RestoreSelection> = files.iter().flat_map(|f| {
                                        f.versions.last().map(|v| RestoreSelection {
                                            version_id: v.version_id,
                                            file_path: f.path.clone(),
                                            size: v.size,
                                            version: v.version,
                                        })
                                    }).collect();
                                    if !selections.is_empty() {
                                        restore_modal.set(Some((config_id, selections)));
                                    }
                                }
                            }
                        >
                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <polyline points="1 4 1 10 7 10"/>
                                <path d="M3.51 15a9 9 0 102.13-9.36L1 10"/>
                            </svg>
                            <span class="hidden md:inline">"Restore"</span>
                        </button>
                    </div>
                    {move || {
                        if expanded.get() {
                            let children_clone = children.clone();
                            view! { <TreeNodeList nodes=children_clone depth={depth+1} config_id=config_id restore_modal=restore_modal files_signal=files_signal bin_files=bin_files selected_file_ids=selected_file_ids /> }.into_any()
                        } else {
                            ().into_any()
                        }
                    }}
                </div>
            }.into_any()
        }
        FileTreeNode::File { name, file } => {
            let file_id = file.file_id;
            let version_count = file.versions.len();
            let size_str = format_size(file.size);
            let versions = file.versions.clone();
            let show_versions: RwSignal<bool> = RwSignal::new(false);
            let file_icon = file_type_icon(&name);

            view! {
                <div>
                    // Row: checkbox + expand button
                    <div
                        class=move || {
                            if selected_file_ids.get().contains(&file_id) {
                                "w-full flex items-center gap-2 hover:bg-bg transition-colors border-b border-border-alpha bg-primary/5"
                            } else {
                                "w-full flex items-center gap-2 hover:bg-bg transition-colors border-b border-border-alpha"
                            }
                        }
                        style=format!("padding-left: {}; min-height: 44px", padding_left)
                    >
                        // File checkbox (does not trigger version expand)
                        <div class="flex items-center justify-center flex-shrink-0">
                            <input
                                type="checkbox"
                                data-testid="file-checkbox"
                                class="w-3.5 h-3.5 rounded accent-primary cursor-pointer"
                                prop:checked=move || selected_file_ids.get().contains(&file_id)
                                on:change=move |_| {
                                    selected_file_ids.update(|s| {
                                        if s.contains(&file_id) {
                                            s.remove(&file_id);
                                        } else {
                                            s.insert(file_id);
                                        }
                                    });
                                }
                            />
                        </div>
                        // Expand/collapse button
                        <button
                            class="flex-1 flex items-center gap-2 min-w-0 py-2.5 text-left"
                            on:click=move |_| show_versions.update(|v| *v = !*v)
                        >
                            <div class="flex-shrink-0" inner_html=file_icon></div>
                            <div class="flex-1 min-w-0">
                                <span class="text-sm truncate block">{name}</span>
                                <span class="md:hidden text-xs text-text-secondary">{size_str.clone()}</span>
                            </div>
                            <span class="hidden md:inline text-xs text-text-secondary flex-shrink-0 mr-2">{size_str}</span>
                            <span class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded-full bg-primary-tint text-primary text-xs font-medium mr-2 flex-shrink-0">
                                {version_count}
                            </span>
                        </button>
                    </div>
                    {move || {
                        if show_versions.get() {
                            let vers = versions.clone();
                            let fp = file.path.clone();
                            let indent = format!("{}px", (depth + 1) * 16 + 8 + 28);
                            view! {
                                <div style=format!("padding-left: {}", indent)>
                                    {vers.into_iter().map(|v| {
                                        view! {
                                            <VersionRow v=v file_path=fp.clone() config_id=config_id restore_modal=restore_modal files=files_signal bin_files=bin_files />
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        } else {
                            ().into_any()
                        }
                    }}
                </div>
            }.into_any()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// BIN FILE LIST — shows soft-deleted files with restore buttons
// ═══════════════════════════════════════════════════════════════════

#[component]
fn BinFileList(
    files: Vec<BackedUpFile>,
    source_directory: String,
    config_id: Uuid,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    main_files: RwSignal<Vec<BackedUpFile>>,
) -> impl IntoView {
    let expanded: RwSignal<Option<Uuid>> = RwSignal::new(None);

    view! {
        <div>
            <div class="px-4 py-2 text-xs text-text-secondary border-b border-border bg-red-500/5">
                "Items in the bin will be permanently deleted after the retention period expires."
            </div>
            {files.into_iter().map(|file| {
                let file_id = file.file_id;
                let relative_path = file.path.strip_prefix(&source_directory)
                    .unwrap_or(&file.path)
                    .trim_start_matches('/')
                    .to_string();
                let size_str = format_size(file.size);
                let version_count = file.versions.len();
                let versions = file.versions.clone();
                let file_name = relative_path.rsplit('/').next().unwrap_or(&relative_path).to_string();
                let icon = file_type_icon(&file_name);

                view! {
                    <div>
                        <button
                            class="w-full px-4 py-2 hover:bg-bg transition-colors text-left border-b border-border-alpha"
                            on:click=move |_| {
                                if expanded.get() == Some(file_id) {
                                    expanded.set(None);
                                } else {
                                    expanded.set(Some(file_id));
                                }
                            }
                        >
                            <div class="flex items-center gap-2">
                                <div class="flex-shrink-0 opacity-50" inner_html=icon></div>
                                <span class="text-sm truncate flex-1 text-text-secondary">{relative_path}</span>
                                <span class="text-xs text-text-secondary">{size_str}</span>
                                <span class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded-full bg-red-500/10 text-red-400 text-xs font-medium">
                                    {version_count}
                                </span>
                            </div>
                        </button>
                        {move || {
                            if expanded.get() == Some(file_id) {
                                let vers = versions.clone();
                                view! {
                                    <div class="px-4 pb-2 ml-6">
                                        {vers.into_iter().map(|v| {
                                            view! { <BinVersionRow v=v config_id=config_id bin_files=bin_files main_files=main_files /> }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            } else {
                                ().into_any()
                            }
                        }}
                    </div>
                }
            }).collect_view()}
        </div>
    }
}

#[component]
fn BinVersionRow(
    v: FileVersionSummary,
    config_id: Uuid,
    bin_files: RwSignal<Vec<BackedUpFile>>,
    main_files: RwSignal<Vec<BackedUpFile>>,
) -> impl IntoView {
    let v_size = format_size(v.size);
    let v_date = format_local_datetime(&v.created_at);
    let version_id = v.version_id;
    let version = v.version;
    let restoring = RwSignal::new(false);

    view! {
        <div class="animate-expand-in flex items-center gap-4 py-1 px-3 text-xs text-text-secondary border-l-2 border-red-400/50 bg-red-500/5 rounded-r-lg mb-0.5 group">
            <span class="inline-flex items-center justify-center min-w-[28px] h-5 px-1.5 rounded-full bg-red-500/10 text-red-400 text-[10px] font-semibold">
                {format!("v{}", version)}
            </span>
            <span class="w-16 text-right">{v_size}</span>
            <span class="flex-1 truncate">{v_date}</span>
            <button
                class="flex-shrink-0 flex items-center gap-1 px-2 py-1 rounded-lg text-accent hover:bg-accent-tint transition-all text-xs font-medium"
                style="min-height: 26px"
                disabled=move || restoring.get()
                on:click=move |ev: leptos::ev::MouseEvent| {
                    ev.stop_propagation();
                    restoring.set(true);
                    spawn_local(async move {
                        match tauri_invoke::<_, ()>("restore_version_from_bin", "version_id", version_id).await {
                            Ok(()) => {
                                // Remove from bin
                                bin_files.update(|f| {
                                    for file in f.iter_mut() {
                                        file.versions.retain(|v| v.version_id != version_id);
                                    }
                                    f.retain(|file| !file.versions.is_empty());
                                });
                                // Reload main files
                                if let Ok(resp) = tauri_invoke::<_, ListBackedUpFilesResponse>("list_backed_up_files", "backup_config_id", config_id).await {
                                    main_files.set(resp.files);
                                }
                            }
                            Err(e) => {
                                leptos::logging::warn!("Failed to restore from bin: {}", e);
                            }
                        }
                        restoring.set(false);
                    });
                }
            >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="1 4 1 10 7 10"/>
                    <path d="M3.51 15a9 9 0 102.13-9.36L1 10"/>
                </svg>
                {move || if restoring.get() { "Restoring..." } else { "Restore" }}
            </button>
        </div>
    }
}
