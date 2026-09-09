use api_types::backup_config::DecryptedBackupConfigWithRemoteStorage;
use api_types::backup_job::{BackupJobSummary, GetLatestBackupJobResponse};
use api_types::dashboard::GetDashboardStatsResponse;
use api_types::restore_job::{GetLatestRestoreJobResponse, RestoreJobSummary};
use leptos::prelude::*;
use serde::Serialize;
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;

use crate::components::common::{
    abbrev_home_path, file_basename, format_local_datetime, format_relative_time, tauri_invoke,
    tauri_invoke_no_args,
};
use crate::components::main_layout::security_summary::SecurityStatusSummary;
use crate::state::IsUnlockedSignal;
use crate::state::*;

// ── Main Dashboard ─────────────────────────────────────────────────

#[component]
pub fn Dashboard() -> impl IntoView {
    let (session, _) = use_context::<SessionSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    // Fetch latest backup job once, share with ProtectionStatusBar and RecentActivitySection
    let (latest_backup, set_latest_backup) = signal(Option::<BackupJobSummary>::None);
    let (latest_backup_loaded, set_latest_backup_loaded) = signal(false);

    Effect::new(move || {
        if !is_unlocked.get() {
            return;
        }
        spawn_local(async move {
            if let Ok(resp) =
                tauri_invoke_no_args::<GetLatestBackupJobResponse>("get_latest_backup_job").await
            {
                let _ = set_latest_backup.try_set(resp.job);
            }
            let _ = set_latest_backup_loaded.try_set(true);
        });
    });

    view! {
        <div>
            <h2 class="text-2xl md:text-3xl font-display font-bold gradient-text mb-2">"Dashboard"</h2>
            <p class="text-text-secondary mb-6 md:mb-8 text-sm md:text-base">"Your backup status at a glance"</p>

            // 1. Security Status Summary (Level 1)
            <SecurityStatusSummary />

            // 2. Protection Status bar
            <ProtectionStatusBar latest_backup=latest_backup latest_backup_loaded=latest_backup_loaded />

            // 3. Live progress panels (shown only when running)
            <BackupProgressWidget />
            <RestoreProgressWidget />

            // 4. Primary CTA — Quick Actions (Level 1)
            <QuickActionsWidget session=session />

            // 5. Stats row (Level 2)
            <StatsRow />

            // 6. Recent Activity (Level 2)
            <RecentActivitySection latest_backup=latest_backup />

            // 7. Device info (Level 3)
            <LocalDeviceWidget session=session />
        </div>
    }
}

// ── Protection Status Bar ──────────────────────────────────────────

#[component]
fn ProtectionStatusBar(
    latest_backup: ReadSignal<Option<BackupJobSummary>>,
    latest_backup_loaded: ReadSignal<bool>,
) -> impl IntoView {
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();
    let (stats, set_stats) = signal(Option::<GetDashboardStatsResponse>::None);

    Effect::new(move || {
        if !is_unlocked.get() {
            return;
        }
        spawn_local(async move {
            if let Ok(resp) =
                tauri_invoke_no_args::<GetDashboardStatsResponse>("get_dashboard_stats").await
            {
                let _ = set_stats.try_set(Some(resp));
            }
        });
    });

    move || {
        let Some(s) = stats.get() else {
            return ().into_any();
        };

        let is_healthy = s.total_files_protected > 0;

        if !is_healthy {
            // First-run state: no files backed up yet.
            return view! {
                <div class="flex items-start gap-3 px-4 py-3 rounded-xl border border-primary-glow bg-primary-tint mb-6">
                    <span class="text-primary mt-0.5 flex-shrink-0" inner_html=r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>"#></span>
                    <div>
                        <p class="text-sm font-medium text-text-primary">"Your first backup is ready to start"</p>
                        <p class="text-xs text-text-secondary mt-0.5">"Setup is complete. Start your first backup to protect files on this device."</p>
                    </div>
                </div>
            }.into_any();
        }

        let message = format!(
            "{} files protected across {} active configs",
            s.total_files_protected, s.active_backup_configs
        );
        let icon = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><polyline points="9 12 11 14 15 10"/></svg>"#;

        // Last backup timestamp
        let last_backup_text = if latest_backup_loaded.get() {
            match latest_backup.get() {
                Some(job) if job.status == "completed" => {
                    let completed = job.completed_at.unwrap_or(job.started_at);
                    let relative = format_relative_time(&completed);
                    let full = format_local_datetime(&completed);
                    Some((relative, full))
                }
                _ => None,
            }
        } else {
            None
        };

        view! {
            <div class="flex flex-col gap-1.5 px-4 py-2.5 rounded-xl border border-success-tint bg-success-tint mb-6">
                <div class="flex items-center gap-2">
                    <span class="text-success" inner_html=icon></span>
                    <span class="text-sm text-success">{message}</span>
                </div>
                {last_backup_text.map(|(relative, full)| view! {
                    <div class="flex items-center gap-2 ml-6">
                        <svg class="w-3.5 h-3.5 text-text-secondary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="12" cy="12" r="10"/>
                            <polyline points="12 6 12 12 16 14"/>
                        </svg>
                        <span class="text-xs text-text-secondary" title=full>
                            {format!("Last backup: {}", relative)}
                        </span>
                    </div>
                })}
            </div>
        }
        .into_any()
    }
}

// ── Stats Row ──────────────────────────────────────────────────────

#[component]
fn StatsRow() -> impl IntoView {
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();
    let (stats, set_stats) = signal(Option::<GetDashboardStatsResponse>::None);
    let (loading, set_loading) = signal(true);

    Effect::new(move || {
        if !is_unlocked.get() {
            return;
        }
        spawn_local(async move {
            match tauri_invoke_no_args::<GetDashboardStatsResponse>("get_dashboard_stats").await {
                Ok(resp) => set_stats.set(Some(resp)),
                Err(e) => leptos::logging::warn!("Failed to fetch dashboard stats: {}", e),
            }
            set_loading.set(false);
        });
    });

    move || {
        if loading.get() {
            return view! {
                <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-6 min-h-[120px]">
                    {(0..4).map(|_| view! {
                        <div class="glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl p-4 animate-pulse">
                            <div class="h-4 bg-bg rounded w-1/2 mb-2"></div>
                            <div class="h-6 bg-bg rounded w-3/4"></div>
                        </div>
                    }).collect_view()}
                </div>
            }.into_any();
        }

        match stats.get() {
            None => view! {
                <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-6">
                    <StatCard icon=ICON_SHIELD label="Files Protected" value="--".to_string() color="text-primary" />
                    <StatCard icon=ICON_DATABASE label="Storage Used" value="--".to_string() color="text-blue-400" />
                    <StatCard icon=ICON_SAVINGS label="Space Saved" value="--".to_string() color="text-green-400" />
                    <StatCard icon=ICON_CONFIGS label="Active Configs" value="--".to_string() color="text-purple-400" />
                </div>
            }.into_any(),
            Some(s) => {
                let files_val = if s.total_files_protected == 0 {
                    "0 files protected".to_string()
                } else {
                    s.total_files_protected.to_string()
                };

                let storage_val = if s.total_uploaded_bytes == 0 {
                    "Not available yet".to_string()
                } else {
                    format_bytes(s.total_uploaded_bytes as u64)
                };

                let savings_val = if s.total_original_bytes > 0 {
                    let saved = s.total_original_bytes - s.total_uploaded_bytes;
                    let pct = (saved as f64 / s.total_original_bytes as f64 * 100.0).max(0.0);
                    format!("{:.1}%", pct)
                } else {
                    "Not available yet".to_string()
                };

                view! {
                    <div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-6">
                        <StatCard icon=ICON_SHIELD label="Files Protected" value=files_val color="text-primary" />
                        <StatCard icon=ICON_DATABASE label="Storage Used" value=storage_val color="text-blue-400" />
                        <StatCard icon=ICON_SAVINGS label="Space Saved" value=savings_val color="text-green-400" />
                        <StatCard icon=ICON_CONFIGS label="Active Configs" value=s.active_backup_configs.to_string() color="text-purple-400" />
                    </div>
                }.into_any()
            }
        }
    }
}

#[component]
fn StatCard(
    icon: &'static str,
    label: &'static str,
    value: String,
    color: &'static str,
) -> impl IntoView {
    view! {
        <div class="glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl p-4">
            <div class="flex items-center gap-2 mb-2">
                <div class={format!("w-8 h-8 rounded-lg bg-primary-tint flex items-center justify-center {}", color)}
                    inner_html=icon
                ></div>
            </div>
            <div class="text-xl md:text-2xl font-bold">{value}</div>
            <div class="text-xs text-text-secondary mt-0.5">{label}</div>
        </div>
    }
}

// ── Backup Progress Widget ─────────────────────────────────────────

#[component]
fn BackupProgressWidget() -> impl IntoView {
    let (backup_progress, _) = use_context::<BackupProgressSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (_, set_settings_tab) = use_context::<SettingsTabSignal>().unwrap();

    move || {
        let bp = backup_progress.get();
        if !bp.is_running && bp.phase != "Completed" && !bp.phase.starts_with("Failed") {
            return ().into_any();
        }

        // Completed with zero files — show a clean "up to date" message
        if !bp.is_running && bp.phase == "Completed" && bp.total_files == 0 {
            return view! {
                <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-4 md:p-6 mb-6">
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

        let is_failed = bp.phase.starts_with("Failed");
        let is_auth_error = bp.phase.contains("Auth token expired");
        let card_class = if is_failed {
            "glass bg-error-tint border border-error-border shadow-elevation-1 rounded-2xl p-4 md:p-6 mb-6"
        } else {
            "glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-4 md:p-6 mb-6"
        };

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
        let current_file_full = bp.current_file.clone();

        view! {
            <div class=card_class>
                <div class="flex items-center justify-between mb-3">
                    <span class="font-semibold">{bp.phase.clone()}</span>
                </div>

                {if is_auth_error {
                    view! {
                        <div class="mt-1 mb-2">
                            <p class="text-xs text-text-secondary mb-2">
                                "Your storage authorization has expired. Reconnect it in Settings to resume backups."
                            </p>
                            <button
                                class="text-sm font-medium text-primary-light hover:text-primary bg-primary-tint px-4 py-1.5 rounded-lg transition-colors"
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
                    view! {
                        <div class="mb-4">
                            <div class="text-xs text-text-secondary mb-1">
                                {format!("Encrypting and uploading {} of {} files", bp.completed_files, bp.total_files)}
                            </div>
                            <div class="h-3 bg-input border border-border rounded-full overflow-hidden"
                                aria-label={format!("{}% complete", overall_pct as u32)}>
                                <div
                                    class="h-full bg-gradient-to-r from-accent to-accent-light rounded-full progress-glow transition-all duration-500"
                                    style:width=format!("{}%", overall_pct)
                                ></div>
                            </div>
                        </div>
                    }.into_any()
                }}

                {if bp.is_running && !bp.current_file.is_empty() {
                    view! {
                        <div>
                            <div class="text-xs text-text-secondary mb-1 truncate"
                                title={current_file_full}>
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

                {if !is_auth_error {
                    view! {
                        <div class="flex flex-wrap gap-4 md:gap-6 mt-4 text-xs md:text-sm text-text-secondary">
                            <span>{format!("Uploaded so far: {}", format_bytes(bp.uploaded_bytes))}</span>
                            <span>{format!("Total size: {}", format_bytes(bp.total_bytes))}</span>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }}
            </div>
        }.into_any()
    }
}

// ── Restore Progress Widget ────────────────────────────────────────

#[component]
fn RestoreProgressWidget() -> impl IntoView {
    let (restore_progress, _) = use_context::<RestoreProgressSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (_, set_settings_tab) = use_context::<SettingsTabSignal>().unwrap();

    move || {
        let rp = restore_progress.get();
        if !rp.is_running && rp.phase != "Completed" && !rp.phase.starts_with("Failed") {
            return ().into_any();
        }

        let overall_pct = if rp.total_files > 0 {
            (rp.completed_files as f64 / rp.total_files as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        let file_pct = if rp.current_file_size > 0 {
            (rp.current_file_restored as f64 / rp.current_file_size as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        let is_failed = rp.phase.starts_with("Failed");
        let is_auth_error = rp.phase.contains("Auth token expired");
        let card_class = if is_failed {
            "glass bg-error-tint border border-error-border shadow-elevation-1 rounded-2xl p-4 md:p-6 mb-6"
        } else {
            "glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-4 md:p-6 mb-6"
        };
        let icon_class = if is_failed {
            "w-5 h-5 text-error"
        } else {
            "w-5 h-5 text-accent"
        };

        view! {
            <div class=card_class>
                <div class="flex items-center justify-between mb-4">
                    <div class="flex items-center gap-2">
                        <svg class=icon_class fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        <span class="font-semibold">{format!("Restore: {}", rp.phase)}</span>
                    </div>
                    <span class="text-sm text-text-secondary">
                        {format!("{} / {} files", rp.completed_files, rp.total_files)}
                    </span>
                </div>

                {if is_auth_error {
                    view! {
                        <div class="mt-1 mb-2">
                            <p class="text-xs text-text-secondary mb-2">
                                "Your storage authorization has expired. Reconnect it in Settings to resume restores."
                            </p>
                            <button
                                class="text-sm font-medium text-primary-light hover:text-primary bg-primary-tint px-4 py-1.5 rounded-lg transition-colors"
                                on:click=move |_| {
                                    set_settings_tab.set("infrastructure");
                                    set_phase.set(AppPhase::Main(MainView::Settings));
                                }
                            >
                                "Reconnect Storage"
                            </button>
                        </div>
                    }.into_any()
                } else if !is_failed {
                    view! {
                        <div class="mb-4">
                            <div class="text-xs text-text-secondary mb-1">"Overall"</div>
                            <div class="h-3 bg-input border border-border rounded-full overflow-hidden">
                                <div
                                    class="h-full bg-gradient-to-r from-accent to-accent-light rounded-full progress-glow transition-all duration-500"
                                    style:width=format!("{}%", overall_pct)
                                ></div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }}

                {if rp.is_running && !rp.current_file.is_empty() {
                    view! {
                        <div>
                            <div class="text-xs text-text-secondary mb-1 truncate">{rp.current_file.clone()}</div>
                            <div class="h-2 bg-input border border-border rounded-full overflow-hidden">
                                <div
                                    class="h-full bg-accent rounded-full transition-all duration-300"
                                    style:width=format!("{}%", file_pct)
                                ></div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }}

                {if !is_auth_error {
                    view! {
                        <div class="flex flex-wrap gap-4 md:gap-6 mt-4 text-xs md:text-sm text-text-secondary">
                            <span>{format!("{} restored", format_bytes(rp.restored_bytes))}</span>
                            <span>{format!("{} total", format_bytes(rp.total_bytes))}</span>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }}
            </div>
        }.into_any()
    }
}

// ── Quick Actions (Primary CTA) ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct StartBackupRequest {
    pub config_id: Uuid,
}

#[component]
fn QuickActionsWidget(session: ReadSignal<SessionInfo>) -> impl IntoView {
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();
    let configs: RwSignal<Vec<DecryptedBackupConfigWithRemoteStorage>> = RwSignal::new(Vec::new());
    let loading: RwSignal<bool> = RwSignal::new(true);

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
                Ok(resp) => configs.set(resp),
                Err(e) => leptos::logging::warn!("Dashboard: failed to load configs: {}", e),
            }
            loading.set(false);
        });
    });

    view! {
        <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-5 mb-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-accent-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                            d="M13 10V3L4 14h7v7l9-11h-7z" />
                    </svg>
                </div>
                <div>
                    <div class="text-text-secondary text-xs uppercase tracking-wider">"Quick Actions"</div>
                    <div class="text-lg font-semibold">"Start Backup"</div>
                </div>
            </div>

            {move || {
                if loading.get() {
                    return view! {
                        <div class="text-sm text-text-secondary">"Loading configs..."</div>
                    }.into_any();
                }

                let physical_device_id = session.get().physical_device_id.clone();
                let current_configs: Vec<_> = configs.get().into_iter()
                    .filter(|c| c.is_active && c.physical_device_id.as_deref() == Some(&physical_device_id))
                    .collect();

                if current_configs.is_empty() {
                    let navigate_to_settings = move |_: leptos::ev::MouseEvent| {
                        set_phase.set(AppPhase::Main(MainView::Settings));
                    };
                    return view! {
                        <div class="text-center py-2">
                            <p class="text-sm text-text-secondary mb-3">
                                "Set up your first backup configuration to get started."
                            </p>
                            <button
                                class="px-5 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                                on:click=navigate_to_settings
                            >
                                "Go to Settings"
                            </button>
                        </div>
                    }.into_any();
                }

                view! {
                    <div class="space-y-2">
                        {current_configs.into_iter().map(|cfg| {
                            view! { <QuickBackupButton config=cfg /> }
                        }).collect_view()}
                    </div>
                    <div class="mt-3 text-center">
                        <button
                            class="px-4 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                            on:click=move |_| set_phase.set(AppPhase::Main(MainView::BackupReports))
                        >
                            "Backup History"
                        </button>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn QuickBackupButton(config: DecryptedBackupConfigWithRemoteStorage) -> impl IntoView {
    let (backup_progress, set_backup_progress) = use_context::<BackupProgressSignal>().unwrap();
    let (error, set_error) = signal(Option::<String>::None);
    let (btn_loading, set_btn_loading) = signal(false);

    let config_id = config.config_id;
    let display_name = config.display_name.clone();
    let source_dir = config.source_directory.clone();

    let on_start = move |ev: leptos::ev::MouseEvent| {
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
                        bp.completed_files = 0;
                        bp.uploaded_bytes = 0;
                        bp.completed_bytes = 0;
                    });
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_btn_loading.try_set(false);
        });
    };

    view! {
        <div class="flex items-center gap-3 p-3 bg-bg/30 border border-border rounded-xl">
            <div class="flex-1 min-w-0">
                <div class="text-sm font-medium truncate">{display_name}</div>
                <div class="text-xs text-text-secondary truncate" title=source_dir.clone()>{abbrev_home_path(&source_dir)}</div>
            </div>
            <button
                class="flex-shrink-0 btn-gradient text-white px-4 md:px-6 py-2 rounded-lg font-medium text-xs md:text-sm shadow-lg shadow-primary-tint disabled:opacity-50 disabled:cursor-not-allowed transition-all"
                data-testid="quick-backup-button"
                disabled=move || btn_loading.try_get().unwrap_or(false) || backup_progress.get().is_running
                on:click=on_start
            >
                {move || {
                    let bp = backup_progress.get();
                    // Only show "Running..." on the button whose config is actively running,
                    // not on every button just because *some* backup is in progress.
                    if bp.is_running && bp.active_config_id == Some(config_id) {
                        "Running..."
                    } else if btn_loading.try_get().unwrap_or(false) {
                        "Starting..."
                    } else {
                        "Backup"
                    }
                }}
            </button>
        </div>
        {move || error.get().map(|e| view! {
            <div class="bg-error-tint border border-error-border text-error rounded-xl px-3 py-1.5 text-xs mt-1">{e}</div>
        })}
    }
}

// ── Recent Activity ────────────────────────────────────────────────

#[component]
fn RecentActivitySection(latest_backup: ReadSignal<Option<BackupJobSummary>>) -> impl IntoView {
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();
    let (latest_restore, set_latest_restore) = signal(Option::<RestoreJobSummary>::None);
    let (loading, set_loading) = signal(true);

    Effect::new(move || {
        if !is_unlocked.get() {
            return;
        }
        spawn_local(async move {
            if let Ok(resp) =
                tauri_invoke_no_args::<GetLatestRestoreJobResponse>("get_latest_restore_job").await
            {
                let _ = set_latest_restore.try_set(resp.job);
            }
            let _ = set_loading.try_set(false);
        });
    });

    view! {
        <div class="glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl p-5 mb-6">
            <div class="flex items-center justify-between mb-4">
                <h3 class="text-sm font-semibold uppercase tracking-wider text-text-secondary">"Recent Activity"</h3>
                <button
                    class="text-xs text-text-secondary hover:text-primary-light transition-colors"
                    on:click=move |_| set_phase.set(AppPhase::Main(MainView::BackupReports))
                >
                    "View All →"
                </button>
            </div>

            {move || {
                if loading.get() {
                    return view! {
                        <div class="space-y-3 min-h-[80px]">
                            {(0..2).map(|_| view! {
                                <div class="flex items-center gap-3 animate-pulse">
                                    <div class="w-8 h-8 rounded-lg bg-bg"></div>
                                    <div class="flex-1">
                                        <div class="h-4 bg-bg rounded w-1/3 mb-1"></div>
                                        <div class="h-3 bg-bg rounded w-1/2"></div>
                                    </div>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any();
                }

                let backup = latest_backup.get();
                let restore = latest_restore.get();

                if backup.is_none() && restore.is_none() {
                    return view! {
                        <div class="text-sm text-text-secondary text-center py-4">
                            "No activity yet. Run your first backup to see it here."
                        </div>
                    }.into_any();
                }

                view! {
                    <div class="space-y-3">
                        {backup.map(|j| {
                            let status_class = match j.status.as_str() {
                                "completed" => "text-success",
                                "failed" => "text-error",
                                _ => "text-warning",
                            };
                            let icon_bg = match j.status.as_str() {
                                "completed" => "bg-success-tint",
                                "failed" => "bg-error-tint",
                                _ => "bg-warning-tint",
                            };
                            view! {
                                <div class="flex items-center gap-3">
                                    <div class={format!("w-8 h-8 rounded-lg flex items-center justify-center {}", icon_bg)}>
                                        <svg class={format!("w-4 h-4 {}", status_class)} fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                            <polyline points="16 16 12 12 8 16"/>
                                            <line x1="12" y1="12" x2="12" y2="21"/>
                                            <path d="M20.39 18.39A5 5 0 0018 9h-1.26A8 8 0 103 16.3"/>
                                        </svg>
                                    </div>
                                    <div class="flex-1 min-w-0">
                                        <div class="flex items-center gap-2 text-sm">
                                            <span class="font-medium capitalize">{format!("Backup {}", j.status)}</span>
                                            <span class="text-text-secondary">"·"</span>
                                            <span class="text-text-secondary">{format!("{} files · {} uploaded", j.total_files, format_bytes(j.uploaded_bytes as u64))}</span>
                                        </div>
                                        <div class="text-xs text-text-secondary">{format_local_datetime(&j.started_at)}</div>
                                    </div>
                                </div>
                            }
                        })}
                        {restore.map(|j| {
                            let status_class = match j.status.as_str() {
                                "completed" => "text-success",
                                "failed" => "text-error",
                                _ => "text-warning",
                            };
                            let icon_bg = match j.status.as_str() {
                                "completed" => "bg-success-tint",
                                "failed" => "bg-error-tint",
                                _ => "bg-warning-tint",
                            };
                            view! {
                                <div class="flex items-center gap-3">
                                    <div class={format!("w-8 h-8 rounded-lg flex items-center justify-center {}", icon_bg)}>
                                        <svg class={format!("w-4 h-4 {}", status_class)} fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                            <path d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                                        </svg>
                                    </div>
                                    <div class="flex-1 min-w-0">
                                        <div class="flex items-center gap-2 text-sm">
                                            <span class="font-medium capitalize">{format!("Restore {}", j.status)}</span>
                                            <span class="text-text-secondary">"·"</span>
                                            <span class="text-text-secondary">{format!("{} files, {}", j.total_files, format_bytes(j.restored_bytes as u64))}</span>
                                        </div>
                                        <div class="text-xs text-text-secondary">{format_local_datetime(&j.started_at)}</div>
                                    </div>
                                </div>
                            }
                        })}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Local Device Widget ────────────────────────────────────────────

#[component]
fn LocalDeviceWidget(session: ReadSignal<SessionInfo>) -> impl IntoView {
    view! {
        <div class="glass bg-elevation-3 border border-elevation-3-border shadow-elevation-3 rounded-2xl p-5">
            <div class="flex items-center gap-3 mb-3">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                            d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                    </svg>
                </div>
                <div>
                    <div class="text-text-secondary text-xs uppercase tracking-wider">"This Device"</div>
                    <div class="text-lg font-semibold">{move || {
                        let s = session.get();
                        s.device_display_name.clone().unwrap_or_else(|| "Unknown".to_string())
                    }}</div>
                </div>
            </div>
            <div class="text-sm text-text-secondary">
                {move || {
                    let s = session.get();
                    let platform = if s.device_platform.is_empty() { "Unknown".to_string() } else { s.device_platform.clone() };
                    format!("Platform: {}", platform)
                }}
            </div>
        </div>
    }
}

// ── Icons ──────────────────────────────────────────────────────────

const ICON_SHIELD: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>"#;

const ICON_DATABASE: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/><path d="M3 12c0 1.66 4.03 3 9 3s9-1.34 9-3"/></svg>"#;

const ICON_SAVINGS: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 6 13.5 15.5 8.5 10.5 1 18"/><polyline points="17 6 23 6 23 12"/></svg>"#;

const ICON_CONFIGS: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-2 2 2 2 0 01-2-2v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83 0 2 2 0 010-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 01-2-2 2 2 0 012-2h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 010-2.83 2 2 0 012.83 0l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 012-2 2 2 0 012 2v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 0 2 2 0 010 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 012 2 2 2 0 01-2 2h-.09a1.65 1.65 0 00-1.51 1z"/></svg>"#;

// ── Helpers ────────────────────────────────────────────────────────

fn format_bytes(bytes: u64) -> String {
    shared_ui::utils::format_bytes(bytes)
}
