use crate::components::common::{
    file_basename, format_local_datetime, format_local_datetime_sec, tauri_invoke,
    tauri_invoke_no_args, tauri_invoke_with_args,
};
use crate::state::*;
use api_types::backup_config::DecryptedBackupConfigWithRemoteStorage;
use api_types::backup_job::{
    BackupJobSummary, DecryptedBackupJobDetail, DecryptedBackupJobDetailResponse,
    GetBackupJobDetailRequest, GetResumableBackupJobRequest, GetResumableBackupJobResponse,
    ListBackupJobsRequest, ListBackupJobsResponse, display_file_status, display_job_status,
};
use api_types::file_filter::is_system_file;
use api_types::remote_storage::RemoteStorageType;
use api_types::restore_job::{
    DecryptedRestoreJobDetail, DecryptedRestoreJobDetailResponse, GetRestoreJobDetailRequest,
    ListRestoreJobsRequest, ListRestoreJobsResponse, RestoreJobSummary,
};
use leptos::prelude::*;
use serde::Serialize;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, Serialize)]
struct StartBackupRequest {
    pub config_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
struct OpenRestoredFileArgs {
    pub destination_type: String,
    pub destination_storage_id: Option<Uuid>,
    pub destination_path: String,
}

/// Shortens a path to show at most the last 2 components (for mobile).
fn short_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.rsplit('/').take(2).collect();
    match parts.len() {
        0 => path.to_string(),
        1 => parts[0].to_string(),
        _ => format!(".../{}/{}", parts[1], parts[0]),
    }
}

/// Builds a best-effort open target for restore reports created before the
/// per-file destination path was persisted.
fn fallback_restore_destination_path(
    destination_type: &str,
    destination_prefix: Option<&str>,
    source_directory: &str,
    file_path: &str,
) -> String {
    if destination_type != "configured_storage" {
        return file_path.to_string();
    }

    let Some(prefix) = destination_prefix else {
        return file_path.to_string();
    };
    let relative = file_path
        .strip_prefix(source_directory)
        .unwrap_or(file_path)
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/");
    if relative.is_empty() {
        prefix.to_string()
    } else {
        format!("{}/{}", prefix.trim_end_matches('/'), relative)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ReportStatusFilter {
    All,
    Completed,
    CompletedWithErrors,
    Failed,
    Running,
}

#[derive(Clone)]
struct BackupReportIdentity {
    display_name: String,
    source_directory: String,
    storage_label: String,
    device_label: String,
}

/// Builds the human report identity from decrypted config metadata kept on the client.
fn backup_report_identity(
    configs: &[DecryptedBackupConfigWithRemoteStorage],
    backup_config_id: Uuid,
) -> BackupReportIdentity {
    configs
        .iter()
        .find(|config| config.config_id == backup_config_id)
        .map(|config| BackupReportIdentity {
            display_name: config.display_name.clone(),
            source_directory: config.source_directory.clone(),
            storage_label: format!("{}", config.storage_type),
            device_label: config
                .physical_device_id
                .clone()
                .unwrap_or_else(|| "Unknown device".to_string()),
        })
        .unwrap_or_else(|| BackupReportIdentity {
            display_name: "Unknown backup".to_string(),
            source_directory: "Unknown source".to_string(),
            storage_label: "Unknown storage".to_string(),
            device_label: "Unknown device".to_string(),
        })
}

/// Shortens a UUID to a stable support identifier that fits dense report tables.
fn short_job_id(id: Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

/// Keeps long machine identifiers readable in report headers while preserving full value in tooltips.
fn short_device_label(label: &str) -> String {
    if label.len() > 12 {
        label.chars().take(8).collect()
    } else {
        label.to_string()
    }
}

/// Returns true when a raw backend status belongs in the selected report filter.
fn report_status_matches(status: &str, filter: ReportStatusFilter) -> bool {
    match filter {
        ReportStatusFilter::All => true,
        ReportStatusFilter::Completed => status == "completed",
        ReportStatusFilter::CompletedWithErrors => status == "completed_with_errors",
        ReportStatusFilter::Failed => status == "failed" || status == "interrupted",
        ReportStatusFilter::Running => matches!(status, "running" | "in_progress" | "queued"),
    }
}

/// Formats a report duration from the server timestamps without implying precision beyond seconds.
fn report_duration_label(
    started_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> String {
    let Some(completed_at) = completed_at else {
        return "In progress".to_string();
    };
    let seconds = completed_at
        .signed_duration_since(started_at)
        .num_seconds()
        .max(0);
    if seconds < 60 {
        format!("{}s", seconds)
    } else {
        let minutes = seconds / 60;
        let rem_seconds = seconds % 60;
        format!("{}m {}s", minutes, rem_seconds)
    }
}

/// Formats backup savings in a status-aware way so failed zero-byte jobs do not look successful.
fn backup_savings_label(status: &str, original_bytes: i64, uploaded_bytes: i64) -> String {
    if original_bytes <= 0 {
        return "No source data".to_string();
    }
    if uploaded_bytes <= 0 && status != "completed" {
        return "No data stored".to_string();
    }
    let saved = (original_bytes - uploaded_bytes).max(0);
    let pct = (saved as f64 / original_bytes as f64 * 100.0).max(0.0);
    format!("{:.1}% saved", pct)
}

/// Checks report search text against identity, status, date, and short job id fields.
fn backup_report_matches_query(
    query: &str,
    identity: &BackupReportIdentity,
    status: &str,
    started: &str,
    job_id: Uuid,
) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {} {} {}",
        identity.display_name,
        identity.source_directory,
        identity.storage_label,
        status,
        started,
        short_job_id(job_id),
    )
    .to_lowercase();
    haystack.contains(&query)
}

#[component]
pub fn BackupView(#[prop(default = "backups")] initial_tab: &'static str) -> impl IntoView {
    let (active_tab, set_active_tab) = signal(initial_tab);
    let (session, _) = use_context::<SessionSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>> = RwSignal::new(Vec::new());
    let loading: RwSignal<bool> = RwSignal::new(true);

    // Fetch all backup configs for the user
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
                    leptos::logging::warn!("BackupView: failed to load configs: {}", e);
                }
            }
            loading.set(false);
        });
    });

    let tabs = vec![("backups", "Backups"), ("reports", "Reports")];

    view! {
        <div>
            <h2 class="text-2xl md:text-3xl font-display font-bold gradient-text mb-2">"Backup"</h2>
            <p class="text-text-secondary mb-6 md:mb-8 text-sm md:text-base">"Run backups and review backup history"</p>

            // Tab bar
            <div class="flex gap-1 mb-6 bg-surface border border-border rounded-xl p-1 overflow-x-auto flex-nowrap">
                {tabs.into_iter().map(|(id, label)| {
                    view! {
                        <button
                            class=move || if active_tab.get() == id {
                                "flex-1 py-2 px-3 md:px-4 rounded-lg bg-primary-tint text-primary-light font-medium text-xs md:text-sm transition-all whitespace-nowrap"
                            } else {
                                "flex-1 py-2 px-3 md:px-4 rounded-lg text-text-secondary hover:text-text-primary text-xs md:text-sm transition-all whitespace-nowrap"
                            }
                            on:click=move |_| set_active_tab.set(id)
                        >
                            {label}
                        </button>
                    }
                }).collect_view()}
            </div>

            // Tab content
            {move || match active_tab.get() {
                "reports" => view! { <ReportsTab configs=configs /> }.into_any(),
                _ => view! { <BackupsTab configs=configs loading=loading session=session /> }.into_any(),
            }}
        </div>
    }
}

// ── Backups Tab ────────────────────────────────────────────────────

#[component]
fn BackupsTab(
    configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>>,
    loading: RwSignal<bool>,
    session: ReadSignal<SessionInfo>,
) -> impl IntoView {
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (backup_progress, _) = use_context::<BackupProgressSignal>().unwrap();

    view! {
        <div>
            // Show backup progress panel while running or just completed
            {move || {
                let bp = backup_progress.get();
                if bp.is_running || bp.phase == "Completed" {
                    view! { <BackupProgressPanel /> }.into_any()
                } else {
                    ().into_any()
                }
            }}

            // Config cards — show ALL configs (active + disabled)
            {move || {
                if loading.get() {
                    return view! {
                        <div class="space-y-4">
                            {(0..2).map(|_| view! {
                                <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-5 animate-pulse">
                                    <div class="flex items-start gap-3">
                                        <div class="w-10 h-10 rounded-xl bg-bg"></div>
                                        <div class="flex-1">
                                            <div class="h-4 bg-bg rounded w-1/3 mb-2"></div>
                                            <div class="h-3 bg-bg rounded w-2/3 mb-1"></div>
                                            <div class="h-3 bg-bg rounded w-1/2"></div>
                                        </div>
                                    </div>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any();
                }

                let mut cfgs = configs.get();
                if cfgs.is_empty() {
                    return view! {
                        <div class="glass bg-surface border border-border rounded-2xl p-6 md:p-8 flex flex-col items-center gap-4">
                            <p class="text-text-secondary">"No folders are being protected yet."</p>
                            <button
                                class="px-4 py-2 bg-primary text-white rounded-lg text-sm font-medium hover:bg-primary-light transition-colors"
                                on:click=move |_| set_phase.set(AppPhase::Main(MainView::Settings))
                            >
                                "Add Folder to Protect"
                            </button>
                        </div>
                    }.into_any();
                }

                let physical_device_id = session.get().physical_device_id.clone();

                // Sort: current device first, then active before disabled
                cfgs.sort_by(|a, b| {
                    let a_current = a.physical_device_id.as_deref() == Some(&physical_device_id);
                    let b_current = b.physical_device_id.as_deref() == Some(&physical_device_id);
                    b_current.cmp(&a_current)
                        .then(b.is_active.cmp(&a.is_active))
                });

                view! {
                    <div class="grid grid-cols-1 gap-4">
                        {cfgs.into_iter().map(|cfg| {
                            let is_current_device = cfg.physical_device_id.as_deref() == Some(&physical_device_id);
                            view! {
                                <BackupConfigCard config=cfg is_current_device=is_current_device />
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Reports Tab ────────────────────────────────────────────────────

#[component]
fn ReportsTab(configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>>) -> impl IntoView {
    let (report_tab, set_report_tab) = signal("backup");
    let (selected_backup_job, set_selected_backup_job) = signal(Option::<Uuid>::None);
    let (selected_restore_job, set_selected_restore_job) = signal(Option::<Uuid>::None);

    let sub_tabs = vec![("backup", "Backup reports"), ("restore", "Restore reports")];

    view! {
        <div>
            // Sub-tab bar
            <div class="flex gap-1 mb-4 bg-bg/50 rounded-lg p-0.5 w-fit">
                {sub_tabs.into_iter().map(|(id, label)| {
                    view! {
                        <button
                            class=move || if report_tab.get() == id {
                                "px-4 py-1.5 rounded-md text-sm font-medium bg-primary-tint text-primary-light transition-all"
                            } else {
                                "px-4 py-1.5 rounded-md text-sm font-medium text-text-secondary hover:text-text-primary transition-all"
                            }
                            on:click=move |_| {
                                set_report_tab.set(id);
                                set_selected_backup_job.set(None);
                                set_selected_restore_job.set(None);
                            }
                        >
                            {label}
                        </button>
                    }
                }).collect_view()}
            </div>

            // Sub-tab content
            {move || match report_tab.get() {
                "restore" => {
                    match selected_restore_job.get() {
                        Some(job_id) => {
                            view! { <RestoreJobDetailView job_id=job_id configs=configs on_back=move || set_selected_restore_job.set(None) /> }.into_any()
                        }
                        None => {
                            view! { <RestoreJobsList configs=configs on_select=Callback::new(move |id| set_selected_restore_job.set(Some(id))) /> }.into_any()
                        }
                    }
                }
                _ => {
                    match selected_backup_job.get() {
                        Some(job_id) => {
                            view! { <BackupJobDetailView job_id=job_id configs=configs on_back=move || set_selected_backup_job.set(None) /> }.into_any()
                        }
                        None => {
                            view! { <BackupJobsList configs=configs on_select=Callback::new(move |id| set_selected_backup_job.set(Some(id))) /> }.into_any()
                        }
                    }
                }
            }}
        </div>
    }
}

// ── Backup Jobs List ───────────────────────────────────────────────

#[component]
fn BackupJobsList(
    configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>>,
    on_select: Callback<Uuid>,
) -> impl IntoView {
    let (jobs, set_jobs) = signal(Vec::<BackupJobSummary>::new());
    let (total_count, set_total_count) = signal(0i64);
    let (page, set_page) = signal(1i64);
    let (loading, set_loading) = signal(true);
    let search_query: RwSignal<String> = RwSignal::new(String::new());
    let status_filter: RwSignal<ReportStatusFilter> = RwSignal::new(ReportStatusFilter::All);
    let (backup_progress, _) = use_context::<BackupProgressSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();
    let page_size = 10i64;

    let fetch_jobs = move || {
        set_loading.set(true);
        let current_page = page.get_untracked();
        spawn_local(async move {
            let req = ListBackupJobsRequest {
                page: current_page,
                page_size,
            };
            match tauri_invoke::<_, ListBackupJobsResponse>("list_backup_jobs", "request", req)
                .await
            {
                Ok(resp) => {
                    set_jobs.set(resp.jobs);
                    set_total_count.set(resp.total_count);
                }
                Err(e) => leptos::logging::warn!("Failed to list backup jobs: {}", e),
            }
            set_loading.set(false);
        });
    };

    // Fetch on mount and when page changes
    Effect::new(move || {
        if !is_unlocked.get() {
            return;
        }
        let _ = page.get();
        fetch_jobs();
    });

    let total_pages = move || ((total_count.get() as f64) / (page_size as f64)).ceil() as i64;

    view! {
        <div class="min-h-[120px]">
            {move || {
                if loading.get() {
                    return view! {
                        <div class="space-y-2">
                            {(0..3).map(|_| view! {
                                <div class="glass bg-surface border border-border rounded-xl p-4 animate-pulse">
                                    <div class="flex items-center justify-between">
                                        <div class="flex items-center gap-3">
                                            <div class="h-5 w-16 bg-bg rounded-full"></div>
                                            <div class="h-4 w-28 bg-bg rounded"></div>
                                        </div>
                                        <div class="h-4 w-32 bg-bg rounded"></div>
                                    </div>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any();
                }

                let job_list = jobs.get();
                if job_list.is_empty() {
                    return view! {
                        <div class="glass bg-surface border border-border rounded-2xl p-6 text-center">
                            <p class="text-text-secondary">"No backup jobs found."</p>
                        </div>
                    }.into_any();
                }

                let bp = backup_progress.get();
                let config_list = configs.get();
                let query = search_query.get();
                let active_status_filter = status_filter.get();
                let visible_jobs: Vec<_> = job_list
                    .into_iter()
                    .filter(|job| report_status_matches(&job.status, active_status_filter))
                    .filter(|job| {
                        let identity = backup_report_identity(&config_list, job.backup_config_id);
                        let started = format_local_datetime(&job.started_at);
                        backup_report_matches_query(&query, &identity, &job.status, &started, job.id)
                    })
                    .collect();
                let has_visible_jobs = !visible_jobs.is_empty();

                view! {
                    <div class="glass bg-surface border border-border rounded-2xl overflow-hidden">
                        <div class="px-4 md:px-5 py-3 border-b border-border space-y-3">
                            <div class="relative">
                                <svg class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-secondary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                    <circle cx="11" cy="11" r="8" />
                                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                                </svg>
                                <input
                                    type="text"
                                    class="w-full bg-input border border-border rounded-lg pl-9 pr-8 py-2 text-sm focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary-tint transition-all"
                                    placeholder="Search backup reports..."
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
                            <div class="flex flex-wrap items-center gap-1.5">
                                {[
                                    (ReportStatusFilter::All, "All"),
                                    (ReportStatusFilter::Completed, "Completed"),
                                    (ReportStatusFilter::CompletedWithErrors, "With errors"),
                                    (ReportStatusFilter::Failed, "Failed"),
                                    (ReportStatusFilter::Running, "Running"),
                                ].into_iter().map(|(filter, label)| view! {
                                    <button
                                        class=move || if status_filter.get() == filter {
                                            "px-2.5 py-1 rounded-full text-xs font-medium bg-primary text-white transition-colors"
                                        } else {
                                            "px-2.5 py-1 rounded-full text-xs font-medium bg-bg text-text-secondary hover:text-text-primary transition-colors"
                                        }
                                        on:click=move |_| status_filter.set(filter)
                                    >
                                        {label}
                                    </button>
                                }).collect_view()}
                            </div>
                        </div>

                        <div class="hidden md:block overflow-hidden">
                            <table class="w-full table-fixed text-sm">
                                <thead>
                                    <tr class="border-b border-border text-text-secondary">
                                        <th class="text-left p-2 w-[132px]">"Status"</th>
                                        <th class="text-left p-2">"Backup"</th>
                                        <th class="text-left p-2 w-[132px]">"Started"</th>
                                        <th class="text-right p-2 w-[56px]">"Files"</th>
                                        <th class="text-right p-2 w-[84px]">"Stored"</th>
                                        <th class="text-right p-2 w-[112px]">"Saved"</th>
                                        <th class="hidden lg:table-cell text-right p-2 w-[78px]">"Duration"</th>
                                        <th class="hidden xl:table-cell text-right p-2 w-[76px]">"Job"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {visible_jobs.iter().map(|job| {
                                        let job_id = job.id;
                                        let identity = backup_report_identity(&config_list, job.backup_config_id);
                                        let is_running = matches!(job.status.as_str(), "running" | "in_progress" | "queued");
                                        let status_class = match job.status.as_str() {
                                            "completed" => "bg-success-tint text-success",
                                            "failed" | "interrupted" => "bg-error-tint text-error",
                                            "completed_with_errors" => "bg-warning-tint text-warning",
                                            _ => "bg-warning-tint text-warning",
                                        };
                                        let started = format_local_datetime(&job.started_at);
                                        let display_total_files = if is_running { bp.total_files } else { job.total_files as usize };
                                        let display_uploaded_bytes = if is_running { bp.uploaded_bytes } else { job.uploaded_bytes as u64 };
                                        let savings = if is_running {
                                            backup_savings_label(job.status.as_str(), bp.completed_original_bytes as i64, bp.uploaded_bytes as i64)
                                        } else {
                                            backup_savings_label(job.status.as_str(), job.original_bytes, job.uploaded_bytes)
                                        };
                                        let duration = report_duration_label(job.started_at, job.completed_at);
                                        view! {
                                            <tr
                                                data-testid="backup-report-row"
                                                class="border-b border-border-alpha hover:bg-bg/30 cursor-pointer transition-colors"
                                                on:click=move |_| on_select.run(job_id)
                                            >
                                                <td class="p-2 whitespace-nowrap overflow-hidden">
                                                    <span class={format!("px-2 py-0.5 rounded-full text-xs font-medium {}", status_class)}>
                                                        {display_job_status(job.status.as_str())}
                                                    </span>
                                                </td>
                                                <td class="p-2 min-w-0">
                                                    <div class="font-medium truncate">{identity.display_name}</div>
                                                    <div class="text-xs text-text-secondary truncate" title={identity.source_directory.clone()}>
                                                        {format!("{} -> {}", short_path(&identity.source_directory), identity.storage_label)}
                                                    </div>
                                                </td>
                                                <td class="p-2 whitespace-nowrap">{started}</td>
                                                <td class="p-2 text-right whitespace-nowrap">{display_total_files}</td>
                                                <td class="p-2 text-right whitespace-nowrap">{format_bytes(display_uploaded_bytes)}</td>
                                                <td class="p-2 text-right whitespace-nowrap text-green-400">{savings}</td>
                                                <td class="hidden lg:table-cell p-2 text-right whitespace-nowrap">{duration}</td>
                                                <td class="hidden xl:table-cell p-2 text-right whitespace-nowrap text-text-secondary">{short_job_id(job.id)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>

                        <div class="md:hidden divide-y divide-border-alpha">
                            {visible_jobs.into_iter().map(|job| {
                                let job_id = job.id;
                                let identity = backup_report_identity(&config_list, job.backup_config_id);
                                let is_running = matches!(job.status.as_str(), "running" | "in_progress" | "queued");
                                let status_class = match job.status.as_str() {
                                    "completed" => "bg-success-tint text-success",
                                    "failed" | "interrupted" => "bg-error-tint text-error",
                                    "completed_with_errors" => "bg-warning-tint text-warning",
                                    _ => "bg-warning-tint text-warning",
                                };
                                let started = format_local_datetime(&job.started_at);
                                let display_total_files = if is_running { bp.total_files } else { job.total_files as usize };
                                let display_uploaded_bytes = if is_running { bp.uploaded_bytes } else { job.uploaded_bytes as u64 };
                                let savings = if is_running {
                                    backup_savings_label(job.status.as_str(), bp.completed_original_bytes as i64, bp.uploaded_bytes as i64)
                                } else {
                                    backup_savings_label(job.status.as_str(), job.original_bytes, job.uploaded_bytes)
                                };
                                view! {
                                    <button
                                        data-testid="backup-report-row"
                                        class="w-full text-left px-4 py-3 hover:bg-bg/30 active:bg-bg/60 transition-colors"
                                        on:click=move |_| on_select.run(job_id)
                                    >
                                        <div class="flex items-center gap-2 mb-1.5">
                                            <span class={format!("px-2 py-0.5 rounded-full text-xs font-medium {}", status_class)}>
                                                {display_job_status(job.status.as_str())}
                                            </span>
                                            <span class="text-xs text-text-secondary">{started}</span>
                                        </div>
                                        <div class="font-medium text-sm truncate">{identity.display_name}</div>
                                        <div class="text-xs text-text-secondary truncate">{format!("{} • {}", short_path(&identity.source_directory), identity.storage_label)}</div>
                                        <div class="flex items-center flex-wrap gap-x-3 gap-y-1 text-xs text-text-secondary mt-1.5">
                                            <span>{format!("{} files", display_total_files)}</span>
                                            <span>{format!("{} stored", format_bytes(display_uploaded_bytes))}</span>
                                            <span class="text-green-400">{savings}</span>
                                            <span>{format!("Job {}", short_job_id(job.id))}</span>
                                        </div>
                                    </button>
                                }
                            }).collect_view()}
                        </div>

                        {if !has_visible_jobs {
                            view! {
                                <div class="px-4 py-8 text-center text-sm text-text-secondary">
                                    "No backup reports match the current filters."
                                </div>
                            }.into_any()
                        } else {
                            ().into_any()
                        }}
                    </div>
                }.into_any()
            }}

            // Pagination
            {move || {
                let tp = total_pages();
                if tp <= 1 {
                    return ().into_any();
                }
                let current = page.get();
                let prev_disabled = current <= 1;
                let next_disabled = current >= tp;
                let start = (current - 1) * page_size + 1;
                let end = (current * page_size).min(total_count.get());
                let total = total_count.get();
                let label = format!("Showing {}\u{2013}{} of {}", start, end, total);
                view! {
                    <div class="flex items-center justify-center gap-4 mt-6">
                        <button
                            class="px-4 py-2 rounded-xl text-sm font-medium border border-border hover:border-primary-glow transition-all disabled:opacity-30 disabled:cursor-not-allowed"
                            disabled=prev_disabled
                            on:click=move |_| set_page.update(|p| *p -= 1)
                        >
                            "Prev"
                        </button>
                        <span class="text-sm text-text-secondary">
                            {label}
                        </span>
                        <button
                            class="px-4 py-2 rounded-xl text-sm font-medium border border-border hover:border-primary-glow transition-all disabled:opacity-30 disabled:cursor-not-allowed"
                            disabled=next_disabled
                            on:click=move |_| set_page.update(|p| *p += 1)
                        >
                            "Next"
                        </button>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Backup Job Detail View ─────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum FileStatusFilter {
    All,
    Completed,
    InProgress,
    Failed,
    Skipped,
}

/// Returns the parent directory portion of a path, abbreviated to `~/…/parent` form.
fn file_parent_dir(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.rsplitn(2, '/').collect();
    if parts.len() >= 2 {
        let parent = parts[1];
        // Abbreviate home prefix
        for prefix in &["/Users/", "/home/"] {
            if let Some(rest) = parent.strip_prefix(prefix) {
                if let Some(slash) = rest.find('/') {
                    return format!("~/…/{}", rest[slash + 1..].rsplit('/').next().unwrap_or(""));
                }
            }
        }
        // Fallback: just show the last directory component
        parent.rsplit('/').next().unwrap_or(parent).to_string()
    } else {
        String::new()
    }
}

#[component]
fn BackupJobDetailView<F: Fn() + 'static>(
    job_id: Uuid,
    configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>>,
    on_back: F,
) -> impl IntoView {
    let (detail, set_detail) = signal(Option::<DecryptedBackupJobDetail>::None);
    let (loading, set_loading) = signal(true);
    let show_system = RwSignal::new(false);
    let file_status_filter = RwSignal::new(FileStatusFilter::All);
    let (backup_progress, _) = use_context::<BackupProgressSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();

    // Fetch job detail on mount and refresh every 5 s while the job is running.
    // This keeps the file table up to date without requiring per-file event tracking.
    // KPI counters are driven reactively by BackupProgressSignal (no refresh needed for those).
    let refresh_tick = RwSignal::new(0u32);

    Effect::new(move || {
        let tick = refresh_tick.get();
        spawn_local(async move {
            let req = GetBackupJobDetailRequest { id: job_id };
            match tauri_invoke::<_, DecryptedBackupJobDetailResponse>(
                "get_backup_job_detail",
                "request",
                req,
            )
            .await
            {
                Ok(resp) => {
                    let still_running = matches!(
                        resp.job.status.as_str(),
                        "in_progress" | "running" | "queued"
                    );
                    set_detail.set(Some(resp.job));
                    if tick == 0 {
                        set_loading.set(false);
                    }
                    // Only schedule the next poll when a backup is actually running.
                    // Without this guard, a job stuck in "running" state (e.g. after the
                    // app was force-killed) causes infinite polling on every open.
                    if still_running && backup_progress.get_untracked().is_running {
                        let window = web_sys::window().unwrap();
                        let cb = wasm_bindgen::closure::Closure::once(move || {
                            refresh_tick.update(|n| *n += 1);
                        });
                        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                            cb.as_ref().unchecked_ref(),
                            5_000,
                        );
                        cb.forget();
                    }
                }
                Err(e) => {
                    leptos::logging::warn!("Failed to get job detail: {}", e);
                    if tick == 0 {
                        set_loading.set(false);
                    }
                }
            }
        });
    });

    // When the backup signal transitions from running → stopped, trigger an immediate
    // re-fetch so the Report reflects the final DB state (completed or failed) without
    // waiting for the next 5-second poll tick.
    //
    // Two constraints prevent an infinite loop:
    // 1. `detail` is read with `get_untracked()` so updating it doesn't re-fire this
    //    effect (otherwise: set_detail → effect → refresh_tick++ → fetch → set_detail → …).
    // 2. `backup_was_running` tracks the previous `is_running` value so the effect only
    //    fires on the running→stopped edge, not when the view opens on a stale "running"
    //    job from a prior killed session.
    let backup_was_running = StoredValue::new(backup_progress.get_untracked().is_running);
    Effect::new(move || {
        let bp = backup_progress.get();
        let previously_running = backup_was_running.get_value();
        backup_was_running.set_value(bp.is_running);
        if previously_running && !bp.is_running {
            if detail
                .get_untracked()
                .map(|j| matches!(j.status.as_str(), "in_progress" | "running" | "queued"))
                .unwrap_or(false)
            {
                refresh_tick.update(|n| *n += 1);
            }
        }
    });

    // True while this job is the actively running one — drives reactive KPI cells.
    let is_live = Signal::derive(move || {
        detail
            .get()
            .map(|j| matches!(j.status.as_str(), "in_progress" | "running" | "queued"))
            .unwrap_or(false)
            && backup_progress.get().is_running
    });

    view! {
        <div>
            <button
                class="flex items-center gap-1 text-sm text-text-secondary hover:text-text-primary mb-4 transition-colors"
                on:click=move |_| on_back()
            >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                </svg>
                "Back to reports"
            </button>

            {move || {
                if loading.get() {
                    return view! {
                        <div class="text-text-secondary text-sm">"Loading job details..."</div>
                    }.into_any();
                }

                match detail.get() {
                    None => view! {
                        <div class="text-text-secondary text-sm">"Job not found."</div>
                    }.into_any(),
                    Some(job) => {
                        let status_class = match job.status.as_str() {
                            "completed" => "bg-success-tint text-success",
                            "failed" => "bg-error-tint text-error",
                            _ => "bg-warning-tint text-warning",
                        };
                        let is_running = matches!(job.status.as_str(), "in_progress" | "running" | "queued");
                        let started = format_local_datetime_sec(&job.started_at);
                        let completed_label = if is_running {
                            "In progress".to_string()
                        } else {
                            job.completed_at.as_ref().map(|t| format_local_datetime_sec(t)).unwrap_or_else(|| "—".to_string())
                        };
                        let total_files_succeeded = job.total_files_succeeded;
                        let total_files_failed = job.total_files_failed;
                        let total_files = job.total_files;
                        let uploaded_label = if is_running && job.uploaded_bytes == 0 {
                            "Calculating...".to_string()
                        } else {
                            format_bytes(job.uploaded_bytes as u64)
                        };
                        let original_bytes_label = format_bytes(job.original_bytes as u64);
                        let dedup_chunks = job.deduplicated_chunks;
                        let original_bytes = job.original_bytes;
                        let uploaded_bytes = job.uploaded_bytes;
                        let deduplicated_bytes = job.deduplicated_bytes;
                        let savings_pct = if original_bytes > 0 {
                            ((original_bytes - uploaded_bytes) as f64 / original_bytes as f64 * 100.0).max(0.0)
                        } else {
                            0.0
                        };
                        let space_saved_label = if is_running && original_bytes == 0 {
                            "Calculating...".to_string()
                        } else {
                            format!("{} ({:.1}%)", format_bytes((original_bytes - uploaded_bytes).max(0) as u64), savings_pct)
                        };
                        let skipped_label = format_bytes(deduplicated_bytes as u64);
                        let has_dedup = deduplicated_bytes > 0;
                        let identity = backup_report_identity(&configs.get(), job.backup_config_id);
                        let job_short_id = short_job_id(job.id);
                        let device_short_label = short_device_label(&identity.device_label);

                        let is_interrupted = job.status == "interrupted";

                        view! {
                            <div>
                                <div class="mb-5">
                                    <h3 class="text-xl font-semibold text-text-primary truncate">{identity.display_name.clone()}</h3>
                                    <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-text-secondary mt-1">
                                        <span>"Backup report"</span>
                                        <span>"•"</span>
                                        <span>{display_job_status(job.status.as_str())}</span>
                                        <span>"•"</span>
                                        <span>{format!("Job {}", job_short_id)}</span>
                                    </div>
                                    <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-text-secondary mt-2">
                                        <span class="truncate max-w-full" title={identity.source_directory.clone()}>{short_path(&identity.source_directory)}</span>
                                        <span class="text-text-muted">"→"</span>
                                        <span>{identity.storage_label.clone()}</span>
                                        <span class="text-text-muted">"•"</span>
                                        <span title={identity.device_label.clone()}>{format!("Device {}", device_short_label)}</span>
                                    </div>
                                </div>

                                // Summary stats
                                <div class="glass bg-surface border border-border rounded-2xl p-5 mb-4">
                                    <div class="flex items-center gap-3 mb-4">
                                        <span class={format!("px-2.5 py-1 rounded-full text-xs font-medium {}", status_class)}>
                                            {display_job_status(job.status.as_str())}
                                        </span>
                                        {job.error_message.clone().map(|e| view! {
                                            <span class="text-sm text-red-400">{e}</span>
                                        })}
                                        {is_interrupted.then(|| view! {
                                            <button
                                                class="ml-auto text-xs font-medium px-3 py-1.5 rounded-lg bg-warning-tint text-warning border border-warning-border hover:bg-warning/10 transition-colors"
                                                on:click=move |_| set_phase.set(AppPhase::Main(MainView::Backup))
                                            >
                                                "Resume Backup"
                                            </button>
                                        })}
                                    </div>

                                    // Default KPIs — simple and user-facing
                                    <div class="grid grid-cols-2 md:grid-cols-3 gap-4 text-sm">
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Started"</div>
                                            <div>{started}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Completed"</div>
                                            <div>{completed_label}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Total files"</div>
                                            <div>{move || if is_live.get() {
                                                backup_progress.get().total_files.to_string()
                                            } else {
                                                total_files.to_string()
                                            }}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Files completed"</div>
                                            <div>{move || if is_live.get() {
                                                backup_progress.get().completed_files.to_string()
                                            } else {
                                                total_files_succeeded.to_string()
                                            }}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Files failed"</div>
                                            <div class={if total_files_failed > 0 { "text-error" } else { "" }}>{move || if is_live.get() {
                                                backup_progress.get().failed_files.to_string()
                                            } else {
                                                total_files_failed.to_string()
                                            }}</div>
                                        </div>
                                    </div>

                                    // Advanced details — collapsed by default
                                    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm mt-4">
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Stored"</div>
                                            <div>{move || if is_live.get() {
                                                format_bytes(backup_progress.get().uploaded_bytes)
                                            } else {
                                                uploaded_label.clone()
                                            }}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Original size"</div>
                                            <div>{move || if is_live.get() {
                                                format_bytes(backup_progress.get().total_bytes)
                                            } else {
                                                original_bytes_label.clone()
                                            }}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Storage saved"</div>
                                            <div class="text-green-400">{move || if is_live.get() {
                                                let bp = backup_progress.get();
                                                if bp.completed_original_bytes > 0 {
                                                    let saved = bp.completed_original_bytes.saturating_sub(bp.uploaded_bytes);
                                                    let pct = (saved as f64 / bp.completed_original_bytes as f64 * 100.0).max(0.0);
                                                    format!("{} ({:.1}%)", format_bytes(saved), pct)
                                                } else {
                                                    "Calculating...".to_string()
                                                }
                                            } else {
                                                space_saved_label.clone()
                                            }}</div>
                                        </div>
                                    </div>

                                    <details class="mt-4">
                                        <summary class="text-xs text-text-secondary cursor-pointer hover:text-text-primary transition-colors select-none">
                                            "Advanced details"
                                        </summary>
                                        <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm mt-3">
                                            {move || if !is_live.get() { view! {
                                                <div>
                                                    <div class="text-text-secondary text-xs mb-1">"Chunks reused"</div>
                                                    <div>{dedup_chunks.to_string()}</div>
                                                </div>
                                            }.into_any() } else { ().into_any() }}
                                            <div>
                                                <div class="text-text-secondary text-xs mb-1">"Backup config"</div>
                                                <div class="text-text-secondary">{short_job_id(job.backup_config_id)}</div>
                                            </div>
                                        </div>
                                        {if has_dedup { view! {
                                            <div class="mt-3 text-sm">
                                                <div class="text-text-secondary text-xs mb-1">"Skipped (already stored)"</div>
                                                <div class="text-green-400">{move || if is_live.get() {
                                                    format_bytes(backup_progress.get().deduplicated_bytes)
                                                } else {
                                                    skipped_label.clone()
                                                }}</div>
                                            </div>
                                        }.into_any() } else { ().into_any() }}
                                    </details>
                                </div>

                                // File table
                                {
                                    let hidden_count = job.files.iter().filter(|f| is_system_file(&f.file_path)).count();
                                    let active_filter = file_status_filter.get();
                                    let files: Vec<_> = job.files.into_iter()
                                        .filter(|f| show_system.get() || !is_system_file(&f.file_path))
                                        .filter(|f| match active_filter {
                                            FileStatusFilter::All => true,
                                            FileStatusFilter::Completed => f.status == "completed",
                                            FileStatusFilter::InProgress => matches!(f.status.as_str(), "in_progress" | "running"),
                                            FileStatusFilter::Failed => f.status == "failed",
                                            FileStatusFilter::Skipped => f.status == "skipped",
                                        })
                                        .collect();
                                    view! {
                                        <div class="glass bg-surface border border-border rounded-2xl overflow-hidden">
                                            <div class="p-4 border-b border-border">
                                                // Top row: count + system files toggle
                                                <div class="flex items-center justify-between gap-3 mb-3">
                                                    <h3 class="font-semibold text-sm">{format!("Files ({})", files.len())}</h3>
                                                    <div class="flex items-center gap-2">
                                                        {if hidden_count > 0 && !show_system.get() {
                                                            view! {
                                                                <span class="text-xs text-text-secondary">{format!("{} hidden", hidden_count)}</span>
                                                            }.into_any()
                                                        } else { ().into_any() }}
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
                                                </div>
                                                // Status filter pills
                                                <div class="flex flex-wrap gap-1.5">
                                                    {[
                                                        (FileStatusFilter::All,        "All"),
                                                        (FileStatusFilter::Completed,   "Completed"),
                                                        (FileStatusFilter::InProgress,  "In progress"),
                                                        (FileStatusFilter::Failed,      "Failed"),
                                                        (FileStatusFilter::Skipped,     "Skipped"),
                                                    ].into_iter().map(|(f, label)| view! {
                                                        <button
                                                            class=move || if file_status_filter.get() == f {
                                                                "px-2.5 py-1 rounded-full text-xs font-medium bg-primary-tint text-primary border border-primary-glow transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
                                                            } else {
                                                                "px-2.5 py-1 rounded-full text-xs font-medium bg-bg text-text-secondary hover:text-text-primary border border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
                                                            }
                                                            on:click=move |_| file_status_filter.set(f)
                                                        >
                                                            {label}
                                                        </button>
                                                    }).collect_view()}
                                                </div>
                                            </div>

                                            // Desktop table
                                            <div class="hidden md:block overflow-x-auto">
                                                <table class="w-full text-sm">
                                                    <thead>
                                                        <tr class="border-b border-border text-text-secondary">
                                                            <th class="text-left p-3">"File"</th>
                                                            <th class="text-right p-3">"Original size"</th>
                                                            <th class="text-right p-3">"Stored"</th>
                                                            <th class="text-right p-3">"Storage saved"</th>
                                                            <th class="text-right p-3">"Dedup ratio"</th>
                                                            <th class="text-center p-3">"Status"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {files.iter().map(|f| {
                                                            let file_status_class = match f.status.as_str() {
                                                                "completed" => "text-green-400",
                                                                "failed" => "text-red-400",
                                                                _ => "text-yellow-400",
                                                            };
                                                            let row_class = if f.status == "failed" {
                                                                "border-b border-border-alpha bg-error-tint/20"
                                                            } else {
                                                                "border-b border-border-alpha hover:bg-bg/30"
                                                            };
                                                            let basename = file_basename(&f.file_path);
                                                            let parent = file_parent_dir(&f.file_path);
                                                            view! {
                                                                <tr class=row_class>
                                                                    <td class="p-3 max-w-[300px]" title={f.file_path.clone()}>
                                                                        <div class="truncate font-medium">{basename}</div>
                                                                        {if !parent.is_empty() {
                                                                            view! { <div class="truncate text-xs text-text-secondary">{parent}</div> }.into_any()
                                                                        } else { ().into_any() }}
                                                                    </td>
                                                                    <td class="p-3 text-right whitespace-nowrap">{format_bytes(f.original_size as u64)}</td>
                                                                    <td class="p-3 text-right whitespace-nowrap">{format_bytes(f.uploaded_size as u64)}</td>
                                                                    <td class="p-3 text-right whitespace-nowrap text-green-400">{format_bytes((f.original_size - f.uploaded_size).max(0) as u64)}</td>
                                                                    <td class="p-3 text-right whitespace-nowrap" title="Reused from store / total chunks">
                                                                        {format!("{}/{}", f.deduplicated_chunks, f.total_chunks)}
                                                                    </td>
                                                                    <td class={format!("p-3 text-center capitalize {}", file_status_class)}>
                                                                        {display_file_status(f.status.as_str())}
                                                                    </td>
                                                                </tr>
                                                            }
                                                        }).collect_view()}
                                                    </tbody>
                                                </table>
                                            </div>

                                            // Mobile card list
                                            <div class="md:hidden divide-y divide-border-alpha">
                                                {files.into_iter().map(|f| {
                                                    let file_status_class = match f.status.as_str() {
                                                        "completed" => "text-green-400",
                                                        "failed" => "text-red-400",
                                                        _ => "text-yellow-400",
                                                    };
                                                    let basename = file_basename(&f.file_path);
                                                    let parent = file_parent_dir(&f.file_path);
                                                    view! {
                                                        <div class=if f.status == "failed" { "px-4 py-3 bg-error-tint/20" } else { "px-4 py-3" }>
                                                            <div class="font-medium text-sm mb-0.5">{basename}</div>
                                                            {if !parent.is_empty() {
                                                                view! { <div class="text-xs text-text-secondary mb-1">{parent}</div> }.into_any()
                                                            } else { ().into_any() }}
                                                            <div class="flex items-center flex-wrap gap-x-3 gap-y-1 text-xs text-text-secondary">
                                                                <span class={format!("capitalize font-medium {}", file_status_class)}>
                                                                    {display_file_status(f.status.as_str())}
                                                                </span>
                                                                <span>{format!("{} → {}", format_bytes(f.original_size as u64), format_bytes(f.uploaded_size as u64))}</span>
                                                                <span class="text-green-400">{format!("saved {}", format_bytes((f.original_size - f.uploaded_size).max(0) as u64))}</span>
                                                                <span>{format!("{} reused / {} chunks", f.deduplicated_chunks, f.total_chunks)}</span>
                                                            </div>
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        </div>
                                    }
                                }
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

// ── Backup Progress Panel ──────────────────────────────────────────

#[component]
fn BackupProgressPanel() -> impl IntoView {
    let (backup_progress, _) = use_context::<BackupProgressSignal>().unwrap();

    move || {
        let bp = backup_progress.get();
        if !bp.is_running && bp.phase != "Completed" {
            return ().into_any();
        }

        // Completed with zero files — show a clean "up to date" message instead
        // of an empty progress bar with 0/0 stats
        if !bp.is_running && bp.phase == "Completed" && bp.total_files == 0 {
            return view! {
                <div class="glass bg-surface border border-border rounded-2xl p-4 md:p-6 mb-6">
                    <div class="flex items-center gap-3">
                        <div class="w-10 h-10 rounded-xl bg-success-tint flex items-center justify-center flex-shrink-0">
                            <svg class="w-5 h-5 text-success" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                                <polyline points="22 4 12 14.01 9 11.01"/>
                            </svg>
                        </div>
                        <div>
                            <span class="font-semibold">"All files are up to date"</span>
                            <p class="text-xs text-text-secondary">"No new or modified files found since the last backup."</p>
                        </div>
                    </div>
                </div>
            }.into_any();
        }

        let overall_pct = if bp.total_files > 0 {
            (bp.completed_files as f64 / bp.total_files as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        let file_pct = if bp.current_file_size > 0 {
            (bp.current_file_uploaded as f64 / bp.current_file_size as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        let current_file_basename = file_basename(&bp.current_file);

        view! {
            <div class="glass bg-surface border border-border rounded-2xl p-4 md:p-6 mb-6">
                <div class="flex items-center justify-between mb-3">
                    <span class="font-semibold">{bp.phase.clone()}</span>
                </div>

                // Overall progress with file count caption
                <div class="mb-4">
                    <div class="text-xs text-text-secondary mb-1">
                        {format!("Encrypting and uploading {} of {} files", bp.completed_files, bp.total_files)}
                    </div>
                    <div class="h-3 bg-input border border-border rounded-full overflow-hidden" aria-label={format!("{}% complete", overall_pct as u32)}>
                        <div
                            class="h-full bg-gradient-to-r from-accent to-accent-light rounded-full progress-glow transition-all duration-500"
                            style:width=format!("{}%", overall_pct)
                        ></div>
                    </div>
                </div>

                // Current file progress — show basename, full path on hover
                {if bp.is_running && !bp.current_file.is_empty() {
                    view! {
                        <div>
                            <div
                                class="text-xs text-text-secondary mb-1 truncate"
                                title={bp.current_file.clone()}
                            >
                                {current_file_basename}
                            </div>
                            <div class="h-2 bg-input border border-border rounded-full overflow-hidden">
                                <div
                                    class="h-full bg-primary-light rounded-full transition-all duration-300"
                                    style:width=format!("{}%", file_pct)
                                ></div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }}

                // Stats with clearer labels
                <div class="flex flex-wrap gap-4 md:gap-6 mt-4 text-xs md:text-sm text-text-secondary">
                    <span>{format!("Uploaded so far: {}", format_bytes(bp.uploaded_bytes))}</span>
                    <span>{format!("Total size: {}", format_bytes(bp.total_bytes))}</span>
                </div>
            </div>
        }
        .into_any()
    }
}

// ── Backup Config Card (Job Card) ──────────────────────────────────

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

#[component]
fn BackupConfigCard(
    config: DecryptedBackupConfigWithRemoteStorage,
    is_current_device: bool,
) -> impl IntoView {
    let (backup_progress, set_backup_progress) = use_context::<BackupProgressSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (error, set_error) = signal(Option::<String>::None);
    let (btn_loading, set_btn_loading) = signal(false);
    let (has_interrupted_job, set_has_interrupted_job) = signal(false);

    let config_id = config.config_id;
    let display_name = config.display_name.clone();
    let source_dir = config.source_directory.clone();
    let storage_type = format!("{}", config.storage_type);
    let icon = storage_type_icon(&config.storage_type);
    let is_active = config.is_active;
    let device_label = config
        .physical_device_id
        .clone()
        .unwrap_or_else(|| "Unknown device".to_string());

    // On mount, check for a running job for this config.
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();
    if is_current_device && is_active {
        Effect::new(move || {
            if !is_unlocked.get() {
                return;
            }
            spawn_local(async move {
                let req = GetResumableBackupJobRequest {
                    backup_config_id: config_id,
                };
                if let Ok(resp) = tauri_invoke::<_, GetResumableBackupJobResponse>(
                    "get_resumable_job_for_config",
                    "request",
                    req,
                )
                .await
                {
                    if resp
                        .job
                        .as_ref()
                        .map_or(false, |j| j.job.status == "interrupted")
                    {
                        let _ = set_has_interrupted_job.try_set(true);
                    }
                }
            });
        });
    }

    let on_start = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_btn_loading.set(true);
        set_error.set(None);

        let req = StartBackupRequest { config_id };

        spawn_local(async move {
            match tauri_invoke::<_, ()>("start_backup_command", "request", req).await {
                Ok(_) => {
                    set_backup_progress.update(|bp| {
                        bp.is_running = true;
                        bp.active_config_id = Some(config_id);
                        bp.phase = "Starting...".to_string();
                        bp.total_files = 0;
                        bp.total_bytes = 0;
                        bp.completed_files = 0;
                        bp.failed_files = 0;
                        bp.uploaded_bytes = 0;
                        bp.deduplicated_bytes = 0;
                        bp.completed_bytes = 0;
                        bp.completed_original_bytes = 0;
                        bp.current_file = String::new();
                        bp.current_file_size = 0;
                        bp.current_file_uploaded = 0;
                    });
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_btn_loading.try_set(false);
        });
    };

    // Opacity for disabled configs
    let card_opacity = if is_active { "" } else { "opacity-60" };

    view! {
        <div class={format!("glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-4 md:p-5 hover-lift {}", card_opacity)}>
            // Interrupted job warning strip
            {move || has_interrupted_job.get().then(|| view! {
                <div class="flex items-center gap-2 mb-3 px-3 py-2 rounded-xl bg-warning-tint border border-warning-border text-warning text-xs font-medium">
                    <span>"Last backup was interrupted"</span>
                </div>
            })}
            // Header row: icon, name, status badge
            <div class="flex items-start gap-3">
                // Storage type icon
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center flex-shrink-0 mt-0.5"
                    inner_html=icon
                ></div>

                // Config info
                <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2 mb-1">
                        <h3 class="text-sm md:text-base font-semibold truncate">{display_name}</h3>
                        // Active/Disabled badge
                        {if is_active {
                            view! {
                                <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-success-tint text-success flex-shrink-0">
                                    <span class="w-1.5 h-1.5 rounded-full bg-success"></span>
                                    "Active"
                                </span>
                            }.into_any()
                        } else {
                            view! {
                                <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-error-tint text-error flex-shrink-0">
                                    <span class="w-1.5 h-1.5 rounded-full bg-error"></span>
                                    "Disabled"
                                </span>
                            }.into_any()
                        }}
                    </div>

                    // Source directory
                    <div class="text-xs text-text-secondary mb-2">
                        <span class="text-text-secondary/70">"Source: "</span>
                        <span class="md:hidden">{short_path(&source_dir)}</span>
                        <span class="hidden md:inline">{source_dir.clone()}</span>
                    </div>

                    // Metadata row
                    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-secondary">
                        <span>{format!("Storage: {}", storage_type)}</span>
                        <span class="text-border">"·"</span>
                        <span>{format!("Device: {}", &device_label[..device_label.len().min(12)])}</span>
                        <span class="text-border">"·"</span>
                        <span>"Encryption: AES-256-GCM"</span>
                    </div>
                </div>
            </div>

            // Actions row
            <div class="mt-4 pt-3 border-t border-elevation-1-border flex items-center justify-between">
                // Left: Start backup button (current device + active only)
                {if is_current_device && is_active {
                    view! {
                        <div>
                            {move || error.get().map(|e| view! {
                                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm mb-2">{e}</div>
                            })}
                            <form on:submit=on_start class="inline">
                                {move || {
                                    let bp = backup_progress.get();
                                    let this_config_running = bp.active_config_id == Some(config_id);
                                    let any_running = bp.is_running;
                                    if this_config_running {
                                        view! {
                                            <div class="flex items-center gap-2">
                                                <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-warning-tint text-warning border border-warning-border">
                                                    <span class="w-1.5 h-1.5 rounded-full bg-warning animate-pulse"></span>
                                                    "Running"
                                                </span>
                                                <button
                                                    type="button"
                                                    class="text-xs text-text-secondary hover:text-primary-light transition-colors"
                                                    on:click=move |_| set_phase.set(AppPhase::Main(MainView::BackupReports))
                                                >
                                                    "View Report"
                                                </button>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <button
                                                type="submit"
                                                data-testid="backup-run-now"
                                                class="btn-gradient text-white px-5 py-2 rounded-xl font-medium text-xs md:text-sm shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                                                disabled=move || btn_loading.try_get().unwrap_or(false) || any_running
                                            >
                                                {move || {
                                                    if btn_loading.try_get().unwrap_or(false) {
                                                        "Starting..."
                                                    } else if has_interrupted_job.get() {
                                                        "Resume"
                                                    } else {
                                                        "Run Now"
                                                    }
                                                }}
                                            </button>
                                        }.into_any()
                                    }
                                }}
                            </form>
                        </div>
                    }.into_any()
                } else if !is_active {
                    view! {
                        <span class="text-xs text-text-secondary italic">"Config disabled"</span>
                    }.into_any()
                } else {
                    view! {
                        <span class="text-xs text-text-secondary italic">"Not this device"</span>
                    }.into_any()
                }}

                // Right: See Reports link
                <button
                    class="text-xs text-text-secondary hover:text-primary-light transition-colors"
                    on:click=move |_| set_phase.set(AppPhase::Main(MainView::BackupReports))
                >
                    "See Reports →"
                </button>
            </div>
        </div>
    }
}

// ── Restore Jobs List ──────────────────────────────────────────────

#[component]
fn RestoreJobsList(
    configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>>,
    on_select: Callback<Uuid>,
) -> impl IntoView {
    let (jobs, set_jobs) = signal(Vec::<RestoreJobSummary>::new());
    let (total_count, set_total_count) = signal(0i64);
    let (page, set_page) = signal(1i64);
    let (loading, set_loading) = signal(true);
    let search_query: RwSignal<String> = RwSignal::new(String::new());
    let status_filter: RwSignal<ReportStatusFilter> = RwSignal::new(ReportStatusFilter::All);
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();
    let page_size = 10i64;

    let fetch_jobs = move || {
        set_loading.set(true);
        let current_page = page.get_untracked();
        spawn_local(async move {
            let req = ListRestoreJobsRequest {
                page: current_page,
                page_size,
            };
            match tauri_invoke::<_, ListRestoreJobsResponse>("list_restore_jobs", "request", req)
                .await
            {
                Ok(resp) => {
                    set_jobs.set(resp.jobs);
                    set_total_count.set(resp.total_count);
                }
                Err(e) => leptos::logging::warn!("Failed to list restore jobs: {}", e),
            }
            set_loading.set(false);
        });
    };

    Effect::new(move || {
        if !is_unlocked.get() {
            return;
        }
        let _ = page.get();
        fetch_jobs();
    });

    let total_pages = move || ((total_count.get() as f64) / (page_size as f64)).ceil() as i64;

    view! {
        <div class="min-h-[120px]">
            {move || {
                if loading.get() {
                    return view! {
                        <div class="space-y-2">
                            {(0..3).map(|_| view! {
                                <div class="glass bg-surface border border-border rounded-xl p-4 animate-pulse">
                                    <div class="flex items-center justify-between">
                                        <div class="flex items-center gap-3">
                                            <div class="h-5 w-16 bg-bg rounded-full"></div>
                                            <div class="h-4 w-28 bg-bg rounded"></div>
                                        </div>
                                        <div class="h-4 w-32 bg-bg rounded"></div>
                                    </div>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any();
                }

                let job_list = jobs.get();
                if job_list.is_empty() {
                    return view! {
                        <div class="glass bg-surface border border-border rounded-2xl p-6 text-center">
                            <p class="text-text-secondary">"No restore jobs found."</p>
                        </div>
                    }.into_any();
                }

                let config_list = configs.get();
                let query = search_query.get();
                let active_status_filter = status_filter.get();
                let visible_jobs: Vec<_> = job_list
                    .into_iter()
                    .filter(|job| report_status_matches(&job.status, active_status_filter))
                    .filter(|job| {
                        let identity = backup_report_identity(&config_list, job.backup_config_id);
                        let started = format_local_datetime(&job.started_at);
                        backup_report_matches_query(&query, &identity, &job.status, &started, job.id)
                    })
                    .collect();
                let has_visible_jobs = !visible_jobs.is_empty();

                view! {
                    <div class="glass bg-surface border border-border rounded-2xl overflow-hidden">
                        <div class="px-4 md:px-5 py-3 border-b border-border space-y-3">
                            <div class="relative">
                                <svg class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-secondary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                    <circle cx="11" cy="11" r="8" />
                                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                                </svg>
                                <input
                                    type="text"
                                    class="w-full bg-input border border-border rounded-lg pl-9 pr-8 py-2 text-sm focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary-tint transition-all"
                                    placeholder="Search restore reports..."
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
                            <div class="flex flex-wrap items-center gap-1.5">
                                {[
                                    (ReportStatusFilter::All, "All"),
                                    (ReportStatusFilter::Completed, "Completed"),
                                    (ReportStatusFilter::CompletedWithErrors, "With errors"),
                                    (ReportStatusFilter::Failed, "Failed"),
                                    (ReportStatusFilter::Running, "Running"),
                                ].into_iter().map(|(filter, label)| view! {
                                    <button
                                        class=move || if status_filter.get() == filter {
                                            "px-2.5 py-1 rounded-full text-xs font-medium bg-primary text-white transition-colors"
                                        } else {
                                            "px-2.5 py-1 rounded-full text-xs font-medium bg-bg text-text-secondary hover:text-text-primary transition-colors"
                                        }
                                        on:click=move |_| status_filter.set(filter)
                                    >
                                        {label}
                                    </button>
                                }).collect_view()}
                            </div>
                        </div>

                        <div class="hidden md:block overflow-hidden">
                            <table class="w-full table-fixed text-sm">
                                <thead>
                                    <tr class="border-b border-border text-text-secondary">
                                        <th class="text-left p-2 w-[132px]">"Status"</th>
                                        <th class="text-left p-2">"Backup"</th>
                                        <th class="text-left p-2 w-[132px]">"Started"</th>
                                        <th class="text-right p-2 w-[56px]">"Files"</th>
                                        <th class="text-right p-2 w-[92px]">"Restored"</th>
                                        <th class="hidden lg:table-cell text-right p-2 w-[78px]">"Duration"</th>
                                        <th class="hidden xl:table-cell text-right p-2 w-[76px]">"Job"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {visible_jobs.iter().map(|job| {
                                        let job_id = job.id;
                                        let identity = backup_report_identity(&config_list, job.backup_config_id);
                                        let status_class = match job.status.as_str() {
                                            "completed" => "bg-success-tint text-success",
                                            "failed" | "interrupted" => "bg-error-tint text-error",
                                            "completed_with_errors" => "bg-warning-tint text-warning",
                                            _ => "bg-warning-tint text-warning",
                                        };
                                        let started = format_local_datetime(&job.started_at);
                                        let duration = report_duration_label(job.started_at, job.completed_at);
                                        view! {
                                            <tr
                                                data-testid="restore-report-row"
                                                class="border-b border-border-alpha hover:bg-bg/30 cursor-pointer transition-colors"
                                                on:click=move |_| on_select.run(job_id)
                                            >
                                                <td class="p-2 whitespace-nowrap overflow-hidden">
                                                    <span class={format!("px-2 py-0.5 rounded-full text-xs font-medium {}", status_class)}>
                                                        {display_job_status(job.status.as_str())}
                                                    </span>
                                                </td>
                                                <td class="p-2 min-w-0">
                                                    <div class="font-medium truncate">{identity.display_name}</div>
                                                    <div class="text-xs text-text-secondary truncate" title={identity.source_directory.clone()}>
                                                        {format!("{} -> {}", short_path(&identity.source_directory), identity.storage_label)}
                                                    </div>
                                                </td>
                                                <td class="p-2 whitespace-nowrap">{started}</td>
                                                <td class="p-2 text-right whitespace-nowrap">{job.total_files}</td>
                                                <td class="p-2 text-right whitespace-nowrap">{format_bytes(job.restored_bytes as u64)}</td>
                                                <td class="hidden lg:table-cell p-2 text-right whitespace-nowrap">{duration}</td>
                                                <td class="hidden xl:table-cell p-2 text-right whitespace-nowrap text-text-secondary">{short_job_id(job.id)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>

                        <div class="md:hidden divide-y divide-border-alpha">
                            {visible_jobs.into_iter().map(|job| {
                                let job_id = job.id;
                                let identity = backup_report_identity(&config_list, job.backup_config_id);
                                let status_class = match job.status.as_str() {
                                    "completed" => "bg-success-tint text-success",
                                    "failed" | "interrupted" => "bg-error-tint text-error",
                                    "completed_with_errors" => "bg-warning-tint text-warning",
                                    _ => "bg-warning-tint text-warning",
                                };
                                let started = format_local_datetime(&job.started_at);
                                view! {
                                    <button
                                        data-testid="restore-report-row"
                                        class="w-full text-left px-4 py-3 hover:bg-bg/30 active:bg-bg/60 transition-colors"
                                        on:click=move |_| on_select.run(job_id)
                                    >
                                        <div class="flex items-center gap-2 mb-1.5">
                                            <span class={format!("px-2 py-0.5 rounded-full text-xs font-medium {}", status_class)}>
                                                {display_job_status(job.status.as_str())}
                                            </span>
                                            <span class="text-xs text-text-secondary">{started}</span>
                                        </div>
                                        <div class="font-medium text-sm truncate">{identity.display_name}</div>
                                        <div class="text-xs text-text-secondary truncate">{format!("{} • {}", short_path(&identity.source_directory), identity.storage_label)}</div>
                                        <div class="flex items-center flex-wrap gap-x-3 gap-y-1 text-xs text-text-secondary mt-1.5">
                                            <span>{format!("{} files", job.total_files)}</span>
                                            <span>{format!("{} restored", format_bytes(job.restored_bytes as u64))}</span>
                                            <span>{format!("Job {}", short_job_id(job.id))}</span>
                                        </div>
                                    </button>
                                }
                            }).collect_view()}
                        </div>

                        {if !has_visible_jobs {
                            view! {
                                <div class="px-4 py-8 text-center text-sm text-text-secondary">
                                    "No restore reports match the current filters."
                                </div>
                            }.into_any()
                        } else {
                            ().into_any()
                        }}
                    </div>
                }.into_any()
            }}

            // Pagination
            {move || {
                let tp = total_pages();
                if tp <= 1 {
                    return ().into_any();
                }
                let current = page.get();
                let prev_disabled = current <= 1;
                let next_disabled = current >= tp;
                let start = (current - 1) * page_size + 1;
                let end = (current * page_size).min(total_count.get());
                let total = total_count.get();
                let label = format!("Showing {}\u{2013}{} of {}", start, end, total);
                view! {
                    <div class="flex items-center justify-center gap-4 mt-6">
                        <button
                            class="px-4 py-2 rounded-xl text-sm font-medium border border-border hover:border-primary-glow transition-all disabled:opacity-30 disabled:cursor-not-allowed"
                            disabled=prev_disabled
                            on:click=move |_| set_page.update(|p| *p -= 1)
                        >
                            "Prev"
                        </button>
                        <span class="text-sm text-text-secondary">
                            {label}
                        </span>
                        <button
                            class="px-4 py-2 rounded-xl text-sm font-medium border border-border hover:border-primary-glow transition-all disabled:opacity-30 disabled:cursor-not-allowed"
                            disabled=next_disabled
                            on:click=move |_| set_page.update(|p| *p += 1)
                        >
                            "Next"
                        </button>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Restore Job Detail View ────────────────────────────────────────

#[component]
fn RestoreJobDetailView<F: Fn() + 'static>(
    job_id: Uuid,
    configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>>,
    on_back: F,
) -> impl IntoView {
    let (detail, set_detail) = signal(Option::<DecryptedRestoreJobDetail>::None);
    let (loading, set_loading) = signal(true);
    let show_system = RwSignal::new(false);
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    Effect::new(move || {
        if !is_unlocked.get() {
            return;
        }
        spawn_local(async move {
            let req = GetRestoreJobDetailRequest { id: job_id };
            match tauri_invoke::<_, DecryptedRestoreJobDetailResponse>(
                "get_restore_job_detail",
                "request",
                req,
            )
            .await
            {
                Ok(resp) => set_detail.set(Some(resp.job)),
                Err(e) => leptos::logging::warn!("Failed to get restore job detail: {}", e),
            }
            set_loading.set(false);
        });
    });

    view! {
        <div>
            <button
                class="flex items-center gap-1 text-sm text-text-secondary hover:text-text-primary mb-4 transition-colors"
                on:click=move |_| on_back()
            >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                </svg>
                "Back to reports"
            </button>

            {move || {
                if loading.get() {
                    return view! {
                        <div class="text-text-secondary text-sm">"Loading job details..."</div>
                    }.into_any();
                }

                match detail.get() {
                    None => view! {
                        <div class="text-text-secondary text-sm">"Job not found."</div>
                    }.into_any(),
                    Some(job) => {
                        let status_class = match job.status.as_str() {
                            "completed" => "bg-success-tint text-success",
                            "failed" => "bg-error-tint text-error",
                            _ => "bg-warning-tint text-warning",
                        };
                        let started = format_local_datetime_sec(&job.started_at);
                        let completed = job.completed_at.as_ref().map(|t| format_local_datetime_sec(t)).unwrap_or_else(|| "--".to_string());
                        let identity = backup_report_identity(&configs.get(), job.backup_config_id);
                        let job_short_id = short_job_id(job.id);
                        let device_short_label = short_device_label(&identity.device_label);

                        view! {
                            <div>
                                <div class="mb-5">
                                    <h3 class="text-xl font-semibold text-text-primary truncate">{identity.display_name.clone()}</h3>
                                    <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-text-secondary mt-1">
                                        <span>"Restore report"</span>
                                        <span>"•"</span>
                                        <span>{display_job_status(job.status.as_str())}</span>
                                        <span>"•"</span>
                                        <span>{format!("Job {}", job_short_id)}</span>
                                    </div>
                                    <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-text-secondary mt-2">
                                        <span class="truncate max-w-full" title={identity.source_directory.clone()}>{short_path(&identity.source_directory)}</span>
                                        <span class="text-text-muted">"→"</span>
                                        <span>{identity.storage_label.clone()}</span>
                                        <span class="text-text-muted">"•"</span>
                                        <span title={identity.device_label.clone()}>{format!("Device {}", device_short_label)}</span>
                                    </div>
                                </div>

                                // Summary stats
                                <div class="glass bg-surface border border-border rounded-2xl p-5 mb-4">
                                    <div class="flex items-center gap-3 mb-4">
                                        <span class={format!("px-2.5 py-1 rounded-full text-xs font-medium {}", status_class)}>
                                            {display_job_status(job.status.as_str())}
                                        </span>
                                        {job.error_message.clone().map(|e| view! {
                                            <span class="text-sm text-red-400">{e}</span>
                                        })}
                                    </div>

                                    <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Started"</div>
                                            <div>{started}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Completed"</div>
                                            <div>{completed}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Files (OK / Failed)"</div>
                                            <div>{format!("{} / {}", job.total_files_succeeded, job.total_files_failed)}</div>
                                        </div>

                                    </div>

                                    <div class="grid grid-cols-2 md:grid-cols-3 gap-4 text-sm mt-4">
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Original"</div>
                                            <div>{format_bytes(job.original_bytes as u64)}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Restored"</div>
                                            <div>{format_bytes(job.restored_bytes as u64)}</div>
                                        </div>
                                        <div>
                                            <div class="text-text-secondary text-xs mb-1">"Overwrite"</div>
                                            <div class="capitalize">{job.overwrite_behavior.clone()}</div>
                                        </div>
                                    </div>
                                </div>

                                // File list (card layout, no horizontal scroll)
                                {
                                    let job_destination_type = job.destination_type.clone();
                                    let job_destination_storage_id = job.destination_storage_id;
                                    let job_destination_prefix = job.destination_prefix.clone();
                                    let source_directory = identity.source_directory.clone();
                                    let hidden_count = job.files.iter().filter(|f| is_system_file(&f.file_path)).count();
                                    let files: Vec<_> = if show_system.get() {
                                        job.files
                                    } else {
                                        job.files.into_iter().filter(|f| !is_system_file(&f.file_path)).collect()
                                    };
                                    view! {
                                        <div class="glass bg-surface border border-border rounded-2xl overflow-hidden">
                                            <div class="p-4 border-b border-border flex items-center justify-between gap-3">
                                                <h3 class="font-semibold text-sm">{format!("Files ({})", files.len())}</h3>
                                                <div class="flex items-center gap-2">
                                                    {if hidden_count > 0 && !show_system.get() {
                                                        view! {
                                                            <span class="text-xs text-text-secondary">{format!("{} hidden", hidden_count)}</span>
                                                        }.into_any()
                                                    } else { ().into_any() }}
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
                                            </div>
                                            <div>
                                                {files.into_iter().map(|f| {
                                                    let file_status_class = match f.status.as_str() {
                                                        "completed" => "text-green-400",
                                                        "failed" => "text-red-400",
                                                        _ => "text-yellow-400",
                                                    };
                                                    let file_path_full = f.file_path.clone();
                                                    let file_path_short = short_path(&f.file_path);
                                                    let destination_path_open = f.destination_path.clone().unwrap_or_else(|| {
                                                        fallback_restore_destination_path(
                                                            &job_destination_type,
                                                            job_destination_prefix.as_deref(),
                                                            &source_directory,
                                                            &f.file_path,
                                                        )
                                                    });
                                                    let destination_type_open = job_destination_type.clone();
                                                    let destination_storage_id_open = job_destination_storage_id;
                                                    let is_completed = f.status == "completed";
                                            view! {
                                                <div class="px-4 py-3 border-b border-border-alpha hover:bg-bg/30">
                                                    // File path: short on mobile, full on desktop
                                                    <div class="text-sm truncate mb-1.5" title={file_path_full.clone()}>
                                                        <span class="md:hidden">{file_path_short}</span>
                                                        <span class="hidden md:inline">{file_path_full.clone()}</span>
                                                    </div>
                                                    // Status + stats + open on one row
                                                    <div class="flex items-center flex-wrap gap-x-3 gap-y-1 text-xs text-text-secondary">
                                                        <span class={format!("capitalize font-medium {}", file_status_class)}>
                                                            {display_file_status(f.status.as_str())}
                                                        </span>
                                                        <span>{format_bytes(f.original_size as u64)}</span>
                                                        <span>{format!("{}/{} chunks", f.chunks_completed, f.total_chunks)}</span>
                                                        {if is_completed {
                                                            let args = OpenRestoredFileArgs {
                                                                destination_type: destination_type_open.clone(),
                                                                destination_storage_id: destination_storage_id_open,
                                                                destination_path: destination_path_open.clone(),
                                                            };
                                                            Some(view! {
                                                                <button
                                                                    class="ml-auto flex items-center gap-1 text-accent hover:text-accent-light transition-colors text-xs font-medium"
                                                                    on:click={
                                                                        let args = args.clone();
                                                                        move |_| {
                                                                            let args = args.clone();
                                                                            spawn_local(async move {
                                                                                let _ = tauri_invoke_with_args::<_, ()>("open_restored_file", &args).await;
                                                                            });
                                                                        }
                                                                    }
                                                                >
                                                                    "Open"
                                                                    <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                                                                    </svg>
                                                                </button>
                                                            })
                                                        } else {
                                                            None
                                                        }}
                                                    </div>
                                                </div>
                                            }
                                                }).collect_view()}
                                            </div>
                                        </div>
                                    }
                                }
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

fn format_bytes(bytes: u64) -> String {
    shared_ui::utils::format_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::{
        backup_config::CleanupType,
        common::{Base64EncryptedData, EncryptionAlgorithm},
        remote_storage::RemoteStorageType,
    };
    use chrono::TimeZone;

    /// Builds the smallest decrypted config needed to test report identity matching.
    fn test_config(config_id: Uuid) -> DecryptedBackupConfigWithRemoteStorage {
        DecryptedBackupConfigWithRemoteStorage {
            storage_id: Uuid::nil(),
            source_directory: "/Users/rohit/PDF".to_string(),
            config_id,
            display_name: "PDF Files".to_string(),
            storage_type: RemoteStorageType::GoogleDrive,
            config: Base64EncryptedData {
                nonce: String::new(),
                ciphertext: String::new(),
                algorithm: EncryptionAlgorithm::Aes256Gcm,
            },
            local_device_id: Some(Uuid::nil()),
            physical_device_id: Some("MacBook Pro".to_string()),
            is_active: true,
            cleanup_type: CleanupType::NoCleanup,
            exclusion_config: Default::default(),
        }
    }

    #[test]
    fn short_job_id_uses_first_eight_uuid_chars_for_dense_report_tables() {
        let id = Uuid::parse_str("12345678-90ab-cdef-1234-567890abcdef").unwrap();

        assert_eq!(short_job_id(id), "12345678");
    }

    #[test]
    fn short_device_label_keeps_long_device_ids_from_cluttering_headers() {
        assert_eq!(short_device_label("ACF273F5-9C5A-5AFA"), "ACF273F5");
        assert_eq!(short_device_label("My Mac"), "My Mac");
    }

    #[test]
    fn backup_savings_label_does_not_make_failed_zero_uploads_look_successful() {
        assert_eq!(
            backup_savings_label("completed_with_errors", 1_000, 0),
            "No data stored"
        );
        assert_eq!(backup_savings_label("completed", 1_000, 750), "25.0% saved");
    }

    #[test]
    fn report_duration_label_handles_completed_and_active_jobs() {
        let started = chrono::Utc
            .with_ymd_and_hms(2026, 6, 12, 9, 36, 47)
            .unwrap();
        let completed = chrono::Utc
            .with_ymd_and_hms(2026, 6, 12, 9, 38, 27)
            .unwrap();

        assert_eq!(report_duration_label(started, Some(completed)), "1m 40s");
        assert_eq!(report_duration_label(started, None), "In progress");
    }

    #[test]
    fn backup_report_identity_uses_decrypted_client_config_metadata() {
        let config_id = Uuid::parse_str("aaaaaaaa-90ab-cdef-1234-567890abcdef").unwrap();
        let config = test_config(config_id);

        let identity = backup_report_identity(&[config], config_id);

        assert_eq!(identity.display_name, "PDF Files");
        assert_eq!(identity.source_directory, "/Users/rohit/PDF");
        assert_eq!(identity.storage_label, "Google Drive");
        assert_eq!(identity.device_label, "MacBook Pro");
    }

    #[test]
    fn fallback_restore_destination_rebuilds_configured_storage_key_for_old_reports() {
        let path = fallback_restore_destination_path(
            "configured_storage",
            Some("cloudless-restored/user-123"),
            "/Users/rohit/PDF",
            "/Users/rohit/PDF/invoices/june.pdf",
        );

        assert_eq!(
            path, "cloudless-restored/user-123/invoices/june.pdf",
            "old configured-storage report rows should open the restored object key"
        );
    }

    #[test]
    fn fallback_restore_destination_keeps_local_reports_on_local_path() {
        let path = fallback_restore_destination_path(
            "download_folder",
            None,
            "/Users/rohit/PDF",
            "/Users/rohit/PDF/invoices/june.pdf",
        );

        assert_eq!(
            path, "/Users/rohit/PDF/invoices/june.pdf",
            "local reports without stored destination metadata keep the legacy path"
        );
    }

    #[test]
    fn report_search_matches_human_identity_and_short_job_id() {
        let job_id = Uuid::parse_str("abcdef12-90ab-cdef-1234-567890abcdef").unwrap();
        let identity = BackupReportIdentity {
            display_name: "PDF Files".to_string(),
            source_directory: "/Users/rohit/PDF".to_string(),
            storage_label: "Google Drive".to_string(),
            device_label: "MacBook Pro".to_string(),
        };

        assert!(backup_report_matches_query(
            "pdf",
            &identity,
            "completed",
            "2026-06-12 09:36",
            job_id,
        ));
        assert!(backup_report_matches_query(
            "abcdef12",
            &identity,
            "completed",
            "2026-06-12 09:36",
            job_id,
        ));
        assert!(!backup_report_matches_query(
            "sftp",
            &identity,
            "completed",
            "2026-06-12 09:36",
            job_id,
        ));
    }
}
