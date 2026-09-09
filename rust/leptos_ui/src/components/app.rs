use crate::components::{
    auth::auth_page::AuthPage,
    auth::email_verification::EmailVerification,
    auth::policy_acceptance::PolicyAcceptance,
    common::{tauri_invoke_no_args, tauri_invoke_with_args, tauri_listen_event},
    main_layout::main_layout::MainLayout,
    setup::setup_wizard::SetupWizard,
};
use crate::state::*;
use api_types::backup::BackupResult;
use api_types::restore::RestoreResult;
use api_types::scheduler::SchedulerEvent;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

#[derive(Serialize)]
struct SetTrayStatusArgs {
    label: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateInfo {
    version: String,
}

#[component]
pub fn App() -> impl IntoView {
    let (phase, set_phase) = signal(AppPhase::Auth);
    let (auth_mode, set_auth_mode) = signal(AuthMode::Login);
    let (backup_progress, set_backup_progress) = signal(BackupProgress::default());
    let (restore_progress, set_restore_progress) = signal(RestoreProgress::default());
    let (session, set_session) = signal(SessionInfo::default());
    let (scheduler_state, set_scheduler_state) = signal(SchedulerState::default());
    let (settings_tab, set_settings_tab) = signal("system");

    provide_context::<PhaseSignal>((phase, set_phase));
    provide_context::<AuthModeSignal>((auth_mode, set_auth_mode));
    provide_context::<BackupProgressSignal>((backup_progress, set_backup_progress));
    provide_context::<RestoreProgressSignal>((restore_progress, set_restore_progress));
    provide_context::<SessionSignal>((session, set_session));
    provide_context::<SchedulerSignal>((scheduler_state, set_scheduler_state));
    provide_context::<SettingsTabSignal>((settings_tab, set_settings_tab));
    let tray_nav_config = RwSignal::new(None::<uuid::Uuid>);
    provide_context::<TrayNavSignal>(tray_nav_config);
    let is_unlocked: IsUnlockedSignal = RwSignal::new(false);
    provide_context::<IsUnlockedSignal>(is_unlocked);

    // ── Startup update check ────────────────────────────────────────
    let (update_version, set_update_version) = signal::<Option<String>>(None);
    let (update_banner_dismissed, set_update_banner_dismissed) = signal(false);

    spawn_local(async move {
        if let Ok(Some(info)) =
            tauri_invoke_no_args::<Option<UpdateInfo>>("check_for_updates").await
        {
            set_update_version.set(Some(info.version));
        }
    });

    // ── Global event listeners (always active) ─────────────────────
    tauri_listen_event::<BackupResult, _>("backup-progress", move |event| {
        // Check before the move into .update() so we can spawn the tray update afterwards.
        let is_completed = matches!(event, BackupResult::Completed { .. });
        set_backup_progress.update(|bp| match event {
            BackupResult::ConfigLoaded { .. } => {
                bp.phase = "Config loaded".to_string();
            }
            BackupResult::IndexUpdated { files_updated, .. } => {
                bp.phase = format!("Synced {} files", files_updated);
            }
            BackupResult::CandidatesFound {
                total_files,
                total_bytes,
            } => {
                bp.phase = "Uploading".to_string();
                bp.total_files = total_files;
                bp.total_bytes = total_bytes;
            }
            BackupResult::FileStarted {
                ref file_path,
                file_size,
                ..
            } => {
                bp.phase = "Uploading".to_string();
                bp.current_file = file_path.clone();
                bp.current_file_size = file_size;
                bp.current_file_uploaded = 0;
            }
            BackupResult::ChunkUploaded {
                ref file_path,
                bytes_uploaded,
                ..
            } => {
                bp.current_file = file_path.clone();
                bp.current_file_uploaded = bytes_uploaded;
                bp.uploaded_bytes = bp.completed_bytes + bytes_uploaded;
            }
            BackupResult::FileCompleted {
                uploaded_bytes,
                deduplicated_bytes,
                ..
            } => {
                bp.completed_files += 1;
                bp.completed_bytes += uploaded_bytes;
                bp.uploaded_bytes = bp.completed_bytes;
                bp.deduplicated_bytes += deduplicated_bytes;
                bp.completed_original_bytes += bp.current_file_size;
                bp.current_file_uploaded = bp.current_file_size;
            }
            BackupResult::FileFailed { .. } => {
                bp.phase = "Uploading".to_string();
                bp.failed_files += 1;
            }
            BackupResult::Completed { .. } => {
                bp.phase = "Completed".to_string();
                // Keep total_files/total_bytes from CandidatesFound (the full candidate set).
                // Completed sends only the succeeded counts — don't overwrite the totals.
                bp.completed_files = bp.total_files - bp.failed_files;
                bp.is_running = false;
                bp.active_config_id = None;
            }
            BackupResult::CleanupCompleted { files_deleted, .. } => {
                if files_deleted > 0 {
                    bp.phase = format!("Cleaned up {} local files", files_deleted);
                }
            }
            BackupResult::Failed { ref reason, .. } => {
                bp.phase = format!("Failed: {}", reason);
                bp.is_running = false;
                bp.active_config_id = None;
            }
        });
        // Update the tray "Last backup" status line after a successful backup.
        if is_completed {
            spawn_local(async move {
                let now = js_sys::Date::new_0();
                let h = now.get_hours();
                let m = now.get_minutes();
                let (h12, ampm) = if h == 0 {
                    (12u32, "AM")
                } else if h < 12 {
                    (h, "AM")
                } else if h == 12 {
                    (12, "PM")
                } else {
                    (h - 12, "PM")
                };
                let label = format!("Last backup: {:02}:{:02} {ampm}", h12, m);
                let _ = tauri_invoke_with_args::<_, ()>(
                    "set_tray_status",
                    &SetTrayStatusArgs { label },
                )
                .await;
            });
        }
    });

    tauri_listen_event::<RestoreResult, _>("restore-progress", move |event| {
        set_restore_progress.update(|rp| match event {
            RestoreResult::ConfigLoaded { .. } => {
                rp.phase = "Config loaded".to_string();
            }
            RestoreResult::VerifyingChunks {
                total_files,
                total_chunks,
            } => {
                rp.phase = format!("Verifying {} chunks", total_chunks);
                rp.total_files = total_files;
            }
            RestoreResult::ChunkVerified { .. } => {}
            RestoreResult::IntegrityCheckComplete { .. } => {
                rp.phase = "Downloading".to_string();
            }
            RestoreResult::FileStarted {
                ref file_path,
                file_size,
                ..
            } => {
                rp.current_file = file_path.clone();
                rp.current_file_size = file_size;
                rp.current_file_restored = 0;
            }
            RestoreResult::ChunkDownloaded {
                ref file_path,
                bytes_restored,
                ..
            } => {
                rp.current_file = file_path.clone();
                rp.current_file_restored = bytes_restored;
            }
            RestoreResult::FileCompleted { restored_bytes, .. } => {
                rp.completed_files += 1;
                rp.restored_bytes += restored_bytes;
                rp.current_file_restored = rp.current_file_size;
            }
            RestoreResult::FileSkipped { .. } => {
                rp.completed_files += 1;
            }
            RestoreResult::Completed {
                total_files,
                total_bytes,
            } => {
                rp.phase = "Completed".to_string();
                rp.total_files = total_files;
                rp.total_bytes = total_bytes;
                rp.completed_files = total_files;
                rp.is_running = false;
            }
            RestoreResult::Failed { ref reason, .. } => {
                rp.phase = format!("Failed: {}", reason);
                rp.is_running = false;
            }
        });
    });

    tauri_listen_event::<SchedulerEvent, _>("scheduler-event", move |event| {
        set_scheduler_state.update(|ss| match event {
            SchedulerEvent::NextBackupAt { timestamp } => {
                ss.next_backup_at = Some(timestamp);
            }
            SchedulerEvent::StateChanged {
                enabled,
                interval_secs,
            } => {
                ss.enabled = enabled;
                ss.interval_minutes = interval_secs / 60;
            }
            _ => {}
        });
    });

    // Navigate directly to a backup config's file browser when the user clicks
    // a config item in the system tray.
    #[derive(Deserialize)]
    struct NavigateToConfigPayload {
        config_id: uuid::Uuid,
    }
    tauri_listen_event::<NavigateToConfigPayload, _>("navigate-to-config", move |event| {
        tray_nav_config.set(Some(event.config_id));
        set_phase.set(AppPhase::Main(MainView::Files));
    });

    // Fetch machine-level constants at launch
    spawn_local(async move {
        if let Ok(machine_id) = tauri_invoke_no_args::<String>("get_machine_id").await {
            set_session.update(|s| s.physical_device_id = machine_id);
        }
        if let Ok(platform) = tauri_invoke_no_args::<String>("get_platform").await {
            set_session.update(|s| s.device_platform = platform);
        }
    });

    // Signal to E2E tests that the Leptos reactive system has fully initialized.
    // Effect::new with no reactive reads runs exactly once after the first render
    // cycle completes — at that point the login page is mounted and interactive,
    // so tests can rely on this attribute instead of polling for specific buttons.
    Effect::new(|_| {
        if let Some(body) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.body())
        {
            let _ = body.set_attribute("data-app-ready", "true");
        }
    });

    view! {
        // ── Update available banner ─────────────────────────────────
        {move || {
            let version = update_version.get();
            let dismissed = update_banner_dismissed.get();
            if let (Some(v), false) = (version, dismissed) {
                view! {
                    <div class="fixed top-0 left-0 right-0 z-50 flex items-center justify-between gap-4 px-4 py-2 bg-primary text-white text-sm shadow-lg">
                        <span>
                            "CloudLess v"{v}" is available. "
                            <button
                                class="underline font-semibold hover:opacity-80"
                                on:click=move |_| {
                                    set_settings_tab.set("preferences");
                                    set_phase.set(AppPhase::Main(MainView::Settings));
                                }
                            >
                                "Update in Settings"
                            </button>
                        </span>
                        <button
                            class="opacity-70 hover:opacity-100 transition-opacity"
                            on:click=move |_| set_update_banner_dismissed.set(true)
                            aria-label="Dismiss"
                        >
                            "✕"
                        </button>
                    </div>
                }.into_any()
            } else {
                view! { <span /> }.into_any()
            }
        }}

        {move || {
            match phase.get() {
                AppPhase::Auth => {
                    view! { <AuthPage /> }.into_any()
                }
                AppPhase::EmailVerification => {
                    view! { <EmailVerification /> }.into_any()
                }
                AppPhase::Setup(step) => {
                    view! { <SetupWizard step=step /> }.into_any()
                }
                AppPhase::PolicyAcceptance => {
                    view! { <PolicyAcceptance /> }.into_any()
                }
                AppPhase::Main(active_view) => {
                    view! { <MainLayout active_view=active_view /> }.into_any()
                }
            }
        }}
    }
}
