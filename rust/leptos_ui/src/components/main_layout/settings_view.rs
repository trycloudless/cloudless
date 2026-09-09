use crate::components::common::{
    download_text_file, format_bytes, format_local_date, format_local_datetime, read_local_storage,
    tauri_invoke, tauri_invoke_no_args, tauri_invoke_with_args, write_local_storage,
};
use crate::components::main_layout::checkout_policy_modal::CheckoutPolicyModal;
use crate::components::storage::sftp_storage_form::{
    CreateRemoteStorageResponse as SftpCreateRemoteStorageResponse, SftpStorageForm,
};
use crate::state::*;
use api_types::backup_config::{
    BackupExclusionConfig, BackupExclusionEntry, BackupExclusionPreset, BackupExclusionRule,
    BackupExclusionRuleKind, CleanupType, CreateBackupConfigResponse, CreateBackupConfigUiRequest,
    DecryptedBackupConfigWithRemoteStorage, PreviewBackupExclusionsRequest,
    PreviewBackupExclusionsResponse, RenameBackupConfigRequest, RenameBackupConfigResponse,
    ToggleBackupConfigRequest, ToggleBackupConfigResponse, UpdateCleanupTypeRequest,
    UpdateCleanupTypeResponse, UpdateExclusionConfigResponse,
};
use api_types::dashboard::GetDashboardStatsResponse;
use api_types::gc::{GcRunStatus, GcRunSummary, GetRetentionSettingsResponse, ListGcRunsResponse};
use api_types::local_device::{
    DeviceSummary, GetOrCreateLocalDeviceResponse, ListAllDevicesResponse,
};
use api_types::remote_storage::{
    CreateRemoteStorageResponse, ListRemoteStoragesResponse, ReauthRemoteStorageResponse,
    RemoteStorageStatus, RemoteStorageSummary, RemoteStorageType,
};
use api_types::subscription::{SubscriptionResponse, SubscriptionStatus, SubscriptionTier};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use shared_ui::theme;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::wasm_bindgen::JsValue;

// ── Helpers ─────────────────────────────────────────────────────────

async fn sleep_ms(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
            .unwrap();
    });
    JsFuture::from(promise).await.ok();
}

/// Returns a masked UUID showing only the first 8 and last 5 characters.
/// Example: `"019de8d7-…-7ea91"` — safe to display in primary settings views.
fn masked_uuid(id: &Uuid) -> String {
    let s = id.to_string();
    let prefix = &s[..8.min(s.len())];
    let suffix = s.rsplit('-').next().unwrap_or("");
    format!("{}-\u{2026}-{}", prefix, suffix)
}

/// Shortens a path to show at most the last 2 components.
fn short_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.rsplit('/').take(2).collect();
    match parts.len() {
        0 => path.to_string(),
        1 => parts[0].to_string(),
        _ => format!(".../{}/{}", parts[1], parts[0]),
    }
}

fn storage_type_icon(storage_type: &RemoteStorageType) -> &'static str {
    match storage_type {
        RemoteStorageType::Aws => {
            r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/><path d="M3 12c0 1.66 4.03 3 9 3s9-1.34 9-3"/></svg>"#
        }
        RemoteStorageType::GoogleDrive => {
            r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2L2 19.5h7.5L12 14l2.5 5.5H22L12 2z"/><path d="M2 19.5h20"/><path d="M7.5 12L12 2l4.5 10"/></svg>"#
        }
        RemoteStorageType::OneDrive => {
            r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 18.5h9.5a4.5 4.5 0 10-1.08-8.87A6 6 0 004 11.5a3.5 3.5 0 003 7z"/><path d="M8.5 13.5h7"/><path d="M12 10v7"/></svg>"#
        }
        RemoteStorageType::LocalFilesystem => {
            r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12H2"/><path d="M5.45 5.11L2 12v6a2 2 0 002 2h16a2 2 0 002-2v-6l-3.45-6.89A2 2 0 0016.76 4H7.24a2 2 0 00-1.79 1.11z"/><line x1="6" y1="16" x2="6.01" y2="16"/><line x1="10" y1="16" x2="10.01" y2="16"/></svg>"#
        }
        RemoteStorageType::Sftp => {
            r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="14" rx="2"/><path d="M7 20h10"/><path d="M12 18v2"/><path d="M7 8h.01"/><path d="M10 8h.01"/><path d="M13 8h4"/><path d="M7 12h10"/></svg>"#
        }
    }
}

// ── Types for Tauri requests (not in api_types) ────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutoBackupSettings {
    pub enabled: bool,
    pub interval_minutes: u64,
}

#[derive(Debug, Clone, Serialize)]
struct AddRemoteStorageRequest {
    pub storage_name: String,
    pub s3_access_key: String,
    pub s3_secret: String,
    pub s3_region: String,
    pub s3_bucket: String,
}

#[derive(Debug, Clone, Serialize)]
struct AddLocalStorageRequest {
    pub storage_name: String,
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateBackupExclusionsArgs {
    pub id: Uuid,
    pub exclusion_config: BackupExclusionConfig,
}

#[derive(Debug, Clone, Serialize)]
struct TestLocalConnectionRequest {
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize)]
struct TestS3ConnectionRequest {
    pub s3_access_key: String,
    pub s3_secret: String,
    pub s3_region: String,
    pub s3_bucket: String,
}

/// Maps raw storage/network error messages to user-friendly, actionable text.
fn map_storage_error(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("nosuchbucket")
        || lower.contains("no such bucket")
        || lower.contains("bucket not found")
    {
        "Bucket not found — check the bucket name and region.".to_string()
    } else if lower.contains("invalidregion")
        || lower.contains("incorrect region")
        || lower.contains("redirect")
    {
        "Region does not match bucket — check the bucket's actual region.".to_string()
    } else if lower.contains("accessdenied")
        || lower.contains("access denied")
        || lower.contains("forbidden")
    {
        "Access denied — verify the access key has s3:GetObject and s3:PutObject permissions."
            .to_string()
    } else if lower.contains("invalidsignature")
        || lower.contains("invalid access key")
        || lower.contains("invalid secret")
    {
        "Invalid credentials — check the access key and secret key.".to_string()
    } else if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("connection refused")
    {
        "Connection timed out — check your network connectivity.".to_string()
    } else {
        err.to_string()
    }
}

/// Mirrors the GcSummary from cloudless_core (not available in WASM).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct GcSummary {
    pub gc_run_id: uuid::Uuid,
    pub versions_deleted: u64,
    pub chunks_deleted: u64,
    pub storage_freed_bytes: i64,
}

// ── Main component ─────────────────────────────────────────────────

#[component]
pub fn SettingsView() -> impl IntoView {
    let (active_tab, set_active_tab) = use_context::<SettingsTabSignal>().unwrap();

    let tabs = vec![
        ("system", "Backups"),
        ("infrastructure", "Storage"),
        ("security", "Security"),
        ("data", "Retention"),
        ("preferences", "Preferences"),
    ];

    view! {
        <div>
            <h2 class="text-2xl md:text-3xl font-display font-bold gradient-text mb-2">"Settings"</h2>
            <p class="text-text-secondary mb-6 md:mb-8 text-sm md:text-base">"Manage your backup configuration"</p>

            // Tab bar
            <div class="flex gap-1 mb-6 bg-surface border border-border rounded-xl p-1 overflow-x-auto flex-nowrap scrollbar-none">
                {tabs.into_iter().map(|(id, label)| {
                    view! {
                        <button
                            class=move || if active_tab.get() == id {
                                "py-2 px-3 md:px-4 rounded-lg bg-primary-tint text-primary-light font-medium text-xs md:text-sm transition-all whitespace-nowrap"
                            } else {
                                "py-2 px-3 md:px-4 rounded-lg text-text-secondary hover:text-text-primary text-xs md:text-sm transition-all whitespace-nowrap"
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
                "system" => view! { <SystemTab /> }.into_any(),
                "infrastructure" => view! { <InfrastructureTab /> }.into_any(),
                "security" => view! { <SecurityTab /> }.into_any(),
                "data" => view! { <DataManagementTab /> }.into_any(),
                "preferences" => view! { <PreferencesTab /> }.into_any(),
                _ => ().into_any(),
            }}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// SYSTEM TAB — Auto-Backup + Backup Configs
// ═══════════════════════════════════════════════════════════════════

#[component]
fn SystemTab() -> impl IntoView {
    view! {
        <div class="space-y-6">
            <AutoBackupSection />
            <BackupConfigsSection />
        </div>
    }
}

/// Polls `get_subscription` until `effective_tier` changes from `starting_tier`,
/// then updates the backup-configs section signals and shows a banner.
/// Errors from individual polls are silently skipped — only a tier change or
/// timeout ends the loop.
async fn poll_subscription_for_upgrade(
    starting_tier: SubscriptionTier,
    set_polling: WriteSignal<bool>,
    set_banner: WriteSignal<Option<String>>,
    set_max_configs_limit: WriteSignal<Option<u32>>,
    set_current_tier: WriteSignal<SubscriptionTier>,
) {
    const MAX_POLLS: u32 = 100; // ~5 minutes at 3s intervals
    for _ in 0..MAX_POLLS {
        sleep_ms(3_000).await;
        if let Ok(sub) = tauri_invoke_no_args::<SubscriptionResponse>("get_subscription").await {
            if sub.effective_tier != starting_tier {
                let _ = set_max_configs_limit.try_set(sub.limits.max_backup_configs);
                let _ = set_current_tier.try_set(sub.effective_tier);
                let _ = set_banner.try_set(Some(
                    "Upgrade successful! Your plan limits have been updated.".to_string(),
                ));
                let _ = set_polling.try_set(false);
                return;
            }
        }
        // No change yet (or transient error) — keep polling
    }
    let _ = set_banner.try_set(Some(
        "Payment timed out — please check your email or visit billing settings.".to_string(),
    ));
    let _ = set_polling.try_set(false);
}

/// Same as `poll_subscription_for_upgrade` but for the auto-backup section.
async fn poll_subscription_for_upgrade_auto_backup(
    starting_tier: SubscriptionTier,
    set_polling: WriteSignal<bool>,
    set_banner: WriteSignal<Option<String>>,
    set_auto_backup_allowed: WriteSignal<Option<bool>>,
) {
    const MAX_POLLS: u32 = 100;
    for _ in 0..MAX_POLLS {
        sleep_ms(3_000).await;
        if let Ok(sub) = tauri_invoke_no_args::<SubscriptionResponse>("get_subscription").await {
            if sub.effective_tier != starting_tier {
                let _ = set_auto_backup_allowed.try_set(Some(sub.limits.auto_backup_enabled));
                let _ = set_banner.try_set(Some(
                    "Upgrade successful! Your plan limits have been updated.".to_string(),
                ));
                let _ = set_polling.try_set(false);
                return;
            }
        }
    }
    let _ = set_banner.try_set(Some(
        "Payment timed out — please check your email or visit billing settings.".to_string(),
    ));
    let _ = set_polling.try_set(false);
}

// ── Auto-Backup Section ────────────────────────────────────────────

#[component]
fn AutoBackupSection() -> impl IntoView {
    let (scheduler_state, set_scheduler_state) = use_context::<SchedulerSignal>().unwrap();

    let (loading, set_loading) = signal(true);
    let (saving, set_saving) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (enabled, set_enabled) = signal(false);
    let (interval, set_interval) = signal(60u64);
    // None = still loading; Some(true) = allowed; Some(false) = not allowed (free/expired)
    let (auto_backup_allowed, set_auto_backup_allowed) = signal(Option::<bool>::None);
    let (checkout_polling, set_checkout_polling) = signal(false);
    let (checkout_banner, set_checkout_banner) = signal(Option::<String>::None);
    let (show_checkout_modal, set_show_checkout_modal) = signal(false);

    spawn_local(async move {
        let allowed = tauri_invoke_no_args::<SubscriptionResponse>("get_subscription")
            .await
            .map(|s| s.limits.auto_backup_enabled)
            .unwrap_or(false);
        let _ = set_auto_backup_allowed.try_set(Some(allowed));

        match tauri_invoke_no_args::<AutoBackupSettings>("get_auto_backup_settings").await {
            Ok(settings) => {
                let _ = set_enabled.try_set(settings.enabled);
                let _ = set_interval.try_set(settings.interval_minutes);
                set_scheduler_state.update(|ss| {
                    ss.enabled = settings.enabled;
                    ss.interval_minutes = settings.interval_minutes;
                });
            }
            Err(e) => {
                let _ = set_error.try_set(Some(e.to_string()));
            }
        }
        let _ = set_loading.try_set(false);
    });

    let on_toggle = move |_| {
        let new_enabled = !enabled.get_untracked();
        set_saving.set(true);
        set_error.set(None);

        let current_interval = interval.get_untracked();

        spawn_local(async move {
            let result = if new_enabled {
                tauri_invoke::<_, ()>("enable_auto_backup", "interval_minutes", current_interval)
                    .await
            } else {
                tauri_invoke_no_args::<()>("disable_auto_backup").await
            };

            match result {
                Ok(()) => {
                    let _ = set_enabled.try_set(new_enabled);
                    set_scheduler_state.update(|ss| {
                        ss.enabled = new_enabled;
                        ss.interval_minutes = current_interval;
                        if !new_enabled {
                            ss.next_backup_at = None;
                        }
                    });
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_saving.try_set(false);
        });
    };

    let on_interval_change = move |ev: leptos::ev::Event| {
        let val: u64 = event_target_value(&ev).parse().unwrap_or(60);
        set_interval.set(val);

        if enabled.get_untracked() {
            set_saving.set(true);
            set_error.set(None);

            spawn_local(async move {
                match tauri_invoke::<_, ()>("enable_auto_backup", "interval_minutes", val).await {
                    Ok(()) => {
                        set_scheduler_state.update(|ss| {
                            ss.interval_minutes = val;
                        });
                    }
                    Err(e) => {
                        let _ = set_error.try_set(Some(e.to_string()));
                    }
                }
                let _ = set_saving.try_set(false);
            });
        }
    };

    let interval_options: Vec<(u64, &str)> = vec![
        (15, "Every 15 minutes"),
        (30, "Every 30 minutes"),
        (60, "Every hour"),
        (240, "Every 4 hours"),
        (720, "Every 12 hours"),
        (1440, "Every 24 hours"),
    ];

    view! {
        <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-6">
            {move || checkout_banner.get().map(|msg| {
                let is_success = msg.starts_with("Upgrade successful");
                let is_error = msg.starts_with("Failed") || msg.starts_with("Payment failed");
                let banner_class = if is_success {
                    "rounded-xl px-4 py-3 text-sm mb-4 bg-success-tint border border-success text-success"
                } else if is_error {
                    "rounded-xl px-4 py-3 text-sm mb-4 bg-error-tint border border-error-border text-error"
                } else {
                    "rounded-xl px-4 py-3 text-sm mb-4 bg-primary-tint border border-primary-glow text-primary-light"
                };
                view! {
                    <div class=banner_class>
                        {msg}
                        {(!checkout_polling.get()).then(|| view! {
                            <button
                                class="ml-3 text-xs opacity-60 hover:opacity-100 transition-opacity"
                                on:click=move |_| set_checkout_banner.set(None)
                            >"✕"</button>
                        })}
                    </div>
                }
            })}

            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="10" />
                        <polyline points="12 6 12 12 16 14" />
                    </svg>
                </div>
                <div>
                    <h3 class="text-lg font-semibold">"Auto-Backup"</h3>
                    <p class="text-xs text-text-secondary">"Periodically back up all active configs on this device"</p>
                </div>
            </div>

            {move || loading.get().then(|| view! {
                <p class="text-text-secondary text-sm">"Loading..."</p>
            })}

            {move || (!loading.get()).then(|| {
                let is_enabled = enabled.get();
                let is_saving = saving.get();
                let is_allowed = auto_backup_allowed.get().unwrap_or(false);
                view! {
                    <div class="space-y-4">
                        // Toggle row
                        <div class="flex items-center justify-between">
                            <div>
                                <p class="text-sm font-medium">"Enable automatic backups"</p>
                                {(!is_allowed).then(|| view! {
                                    <p class="text-xs text-text-secondary mt-0.5">
                                        "Available on Starter and above. "
                                        <button
                                            class="text-primary-light hover:underline bg-transparent border-0 p-0 cursor-pointer text-xs"
                                            disabled=move || checkout_polling.get()
                                            on:click=move |_| set_show_checkout_modal.set(true)
                                        >
                                            "Upgrade"
                                        </button>
                                    </p>
                                })}
                            </div>
                            <div class="flex items-center gap-3">
                                // Large visual indicator — only when allowed and active
                                {if is_allowed && is_enabled {
                                    view! {
                                        <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-success-tint text-success">
                                            <span class="w-1.5 h-1.5 rounded-full bg-success"></span>
                                            "Active"
                                        </span>
                                    }.into_any()
                                } else if is_allowed {
                                    view! {
                                        <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-bg text-text-secondary">
                                            <span class="w-1.5 h-1.5 rounded-full bg-text-secondary"></span>
                                            "Off"
                                        </span>
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }}

                                <button
                                    class=move || {
                                        let allowed = auto_backup_allowed.get().unwrap_or(false);
                                        if !allowed {
                                            "relative inline-flex h-6 w-11 items-center rounded-full bg-border transition-colors opacity-40 cursor-not-allowed"
                                        } else if enabled.get() {
                                            "relative inline-flex h-6 w-11 items-center rounded-full bg-primary transition-colors"
                                        } else {
                                            "relative inline-flex h-6 w-11 items-center rounded-full bg-border transition-colors"
                                        }
                                    }
                                    on:click=on_toggle
                                    disabled=move || is_saving || !auto_backup_allowed.get().unwrap_or(false)
                                >
                                    <span
                                        class=move || if enabled.get() && auto_backup_allowed.get().unwrap_or(false) {
                                            "inline-block h-4 w-4 rounded-full bg-white transform transition-transform translate-x-6"
                                        } else {
                                            "inline-block h-4 w-4 rounded-full bg-white transform transition-transform translate-x-1"
                                        }
                                    />
                                </button>
                            </div>
                        </div>

                        // Frequency dropdown (visible when enabled)
                        {is_enabled.then(|| {
                            let current_interval = interval.get();
                            let options = interval_options.clone();
                            view! {
                                <div>
                                    <label class="block text-sm font-medium text-text-secondary mb-1.5">"Backup frequency"</label>
                                    <select
                                        class="w-full bg-input border border-border rounded-xl px-4 py-2.5 text-text-primary text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                                        on:change=on_interval_change
                                        disabled=is_saving
                                    >
                                        {options.into_iter().map(|(mins, label)| {
                                            let selected = mins == current_interval;
                                            view! { <option value={mins.to_string()} selected=selected>{label}</option> }
                                        }).collect_view()}
                                    </select>
                                </div>
                            }
                        })}

                        // Status line
                        {is_enabled.then(|| {
                            let next = scheduler_state.get().next_backup_at.clone();
                            view! {
                                <div class="flex items-center gap-2 py-2 px-3 bg-primary-tint border border-primary-tint rounded-xl">
                                    <span class="text-primary text-xs">
                                        {match next {
                                            Some(ts) => format_next_backup(&ts),
                                            None => "Waiting for first scheduled run...".to_string(),
                                        }}
                                    </span>
                                </div>
                            }
                        })}

                        {is_saving.then(|| view! {
                            <p class="text-xs text-text-secondary">"Saving..."</p>
                        })}

                        {move || error.get().map(|e| view! {
                            <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                        })}
                    </div>
                }
            })}

            {move || show_checkout_modal.get().then(|| view! {
                <CheckoutPolicyModal
                    on_close=Callback::new(move |accepted: bool| {
                        set_show_checkout_modal.set(false);
                        if accepted {
                            spawn_local(async move {
                                match tauri_invoke::<_, String>(
                                    "open_checkout",
                                    "tier",
                                    SubscriptionTier::Starter,
                                ).await {
                                    Ok(_) => {
                                        set_checkout_polling.set(true);
                                        set_checkout_banner.set(Some("Payment opened in browser — waiting for confirmation…".to_string()));
                                        spawn_local(async move {
                                            poll_subscription_for_upgrade_auto_backup(
                                                SubscriptionTier::Free,
                                                set_checkout_polling,
                                                set_checkout_banner,
                                                set_auto_backup_allowed,
                                            ).await;
                                        });
                                    }
                                    Err(e) => {
                                        set_checkout_banner.set(Some(format!("Failed to open checkout: {}", e.user_message())));
                                    }
                                }
                            });
                        }
                    })
                />
            })}
        </div>
    }
}

/// Formats an RFC 3339 timestamp into a human-friendly "Next backup in X" string.
fn format_next_backup(timestamp: &str) -> String {
    use chrono::{DateTime, Utc};

    let Ok(next) = timestamp.parse::<DateTime<Utc>>() else {
        return format!("Next backup at {}", timestamp);
    };

    let now = Utc::now();
    let diff = next - now;

    if diff.num_seconds() <= 0 {
        return "Backup starting soon...".to_string();
    }

    let total_minutes = diff.num_minutes();
    if total_minutes < 1 {
        format!("Next backup in {} seconds", diff.num_seconds())
    } else if total_minutes < 60 {
        format!(
            "Next backup in {} minute{}",
            total_minutes,
            if total_minutes == 1 { "" } else { "s" }
        )
    } else {
        let hours = total_minutes / 60;
        let mins = total_minutes % 60;
        if mins == 0 {
            format!(
                "Next backup in {} hour{}",
                hours,
                if hours == 1 { "" } else { "s" }
            )
        } else {
            format!("Next backup in {}h {}m", hours, mins)
        }
    }
}

// ── Backup Configs Section (Job Cards) ──────────────────────────────

#[component]
fn BackupConfigsSection() -> impl IntoView {
    let (session, set_session) = use_context::<SessionSignal>().unwrap();

    let (configs, set_configs) = signal(Vec::<DecryptedBackupConfigWithRemoteStorage>::new());
    let (storages, set_storages) = signal(Vec::<RemoteStorageSummary>::new());
    let (loading, set_loading) = signal(true);
    // None = unlimited, Some(n) = capped at n configs
    let (max_configs_limit, set_max_configs_limit) = signal(Option::<u32>::None);
    let (current_tier, set_current_tier) = signal(SubscriptionTier::Free);
    let can_add_config = Signal::derive(move || match max_configs_limit.get() {
        None => true,
        Some(max) => (configs.get().len() as u32) < max,
    });
    let upgrade_tier = Signal::derive(move || match current_tier.get() {
        SubscriptionTier::Free => SubscriptionTier::Starter,
        SubscriptionTier::Starter => SubscriptionTier::Pro,
        _ => SubscriptionTier::Pro,
    });
    let (checkout_polling, set_checkout_polling) = signal(false);
    let (checkout_banner, set_checkout_banner) = signal(Option::<String>::None);
    let (show_checkout_modal, set_show_checkout_modal) = signal(false);

    // Wizard state (replaces the old flat show_form)
    let (show_wizard, set_show_wizard) = signal(false);
    let (wizard_step, set_wizard_step) = signal(1u8); // 1=Source 2=Storage 3=Exclusions 4=Cleanup 5=Review
    let (form_error, set_form_error) = signal(Option::<String>::None);
    let (form_loading, set_form_loading) = signal(false);
    let (confirming_id, set_confirming_id) = signal(Option::<Uuid>::None);
    let (renaming_id, set_renaming_id) = signal(Option::<Uuid>::None);
    let (rename_value, set_rename_value) = signal(String::new());
    let (rename_loading, set_rename_loading) = signal(false);
    let (editing_cleanup_id, set_editing_cleanup_id) = signal(Option::<Uuid>::None);
    let (editing_cleanup_days, set_editing_cleanup_days) = signal(String::new());
    let (editing_cleanup_enabled, set_editing_cleanup_enabled) = signal(false);
    let (cleanup_edit_loading, set_cleanup_edit_loading) = signal(false);

    // Wizard step 1: Source
    let (config_name, set_config_name) = signal(String::new());
    let (source_dir, set_source_dir) = signal(String::new());
    // Wizard step 2: Storage
    let (selected_storage_id, set_selected_storage_id) = signal(String::new());
    // Wizard step 3: Exclusions
    let (wiz_preset_temp_cache, set_wiz_preset_temp_cache) = signal(false);
    let (wiz_preset_installers, set_wiz_preset_installers) = signal(false);
    let (wiz_preset_dev_deps, set_wiz_preset_dev_deps) = signal(false);
    let (wiz_preset_os_meta, set_wiz_preset_os_meta) = signal(true);
    let (wiz_custom_rules, set_wiz_custom_rules) = signal(Vec::<BackupExclusionRule>::new());
    let (wiz_advanced_globs, set_wiz_advanced_globs) = signal(String::new());
    let (wiz_advanced_expanded, set_wiz_advanced_expanded) = signal(false);
    let (wiz_preview_result, set_wiz_preview_result) =
        signal(Option::<PreviewBackupExclusionsResponse>::None);
    let (wiz_preview_loading, set_wiz_preview_loading) = signal(false);
    let (wiz_preview_error, set_wiz_preview_error) = signal(Option::<String>::None);
    // Wizard step 4: Cleanup
    let (form_cleanup_enabled, set_form_cleanup_enabled) = signal(false);
    let (form_cleanup_days, set_form_cleanup_days) = signal("30".to_string());

    // Per-card edit exclusions state
    let (editing_exclusions_id, set_editing_exclusions_id) = signal(Option::<Uuid>::None);
    let (edit_excl_preset_temp_cache, set_edit_excl_preset_temp_cache) = signal(false);
    let (edit_excl_preset_installers, set_edit_excl_preset_installers) = signal(false);
    let (edit_excl_preset_dev_deps, set_edit_excl_preset_dev_deps) = signal(false);
    let (edit_excl_preset_os_meta, set_edit_excl_preset_os_meta) = signal(false);
    let (edit_excl_custom_rules, set_edit_excl_custom_rules) =
        signal(Vec::<BackupExclusionRule>::new());
    let (edit_excl_globs, set_edit_excl_globs) = signal(String::new());
    let (edit_excl_advanced_expanded, set_edit_excl_advanced_expanded) = signal(false);
    let (edit_excl_loading, set_edit_excl_loading) = signal(false);
    let (edit_excl_error, set_edit_excl_error) = signal(Option::<String>::None);

    spawn_local(async move {
        if let Ok(resp) = tauri_invoke_no_args::<Vec<DecryptedBackupConfigWithRemoteStorage>>(
            "list_all_backup_configs",
        )
        .await
        {
            let _ = set_configs.try_set(resp);
        }
        if let Ok(resp) =
            tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages").await
        {
            let _ = set_storages.try_set(resp.list);
        }
        if let Ok(sub) = tauri_invoke_no_args::<SubscriptionResponse>("get_subscription").await {
            let _ = set_max_configs_limit.try_set(sub.limits.max_backup_configs);
            let _ = set_current_tier.try_set(sub.effective_tier);
        }
        let _ = set_loading.try_set(false);
    });

    let build_wiz_exclusion_config = move || {
        let mut entries = Vec::<BackupExclusionEntry>::new();
        if wiz_preset_temp_cache.get_untracked() {
            entries.push(BackupExclusionEntry::Preset(
                BackupExclusionPreset::TemporaryAndCache,
            ));
        }
        if wiz_preset_installers.get_untracked() {
            entries.push(BackupExclusionEntry::Preset(
                BackupExclusionPreset::InstallersAndArchives,
            ));
        }
        if wiz_preset_dev_deps.get_untracked() {
            entries.push(BackupExclusionEntry::Preset(
                BackupExclusionPreset::DeveloperDependencies,
            ));
        }
        if wiz_preset_os_meta.get_untracked() {
            entries.push(BackupExclusionEntry::Preset(
                BackupExclusionPreset::OsMetadata,
            ));
        }
        for rule in wiz_custom_rules.get_untracked() {
            entries.push(BackupExclusionEntry::Custom(rule));
        }
        for line in wiz_advanced_globs.get_untracked().lines() {
            let g = line.trim();
            if !g.is_empty() {
                entries.push(BackupExclusionEntry::Glob(g.to_string()));
            }
        }
        BackupExclusionConfig { entries }
    };

    let build_edit_exclusion_config = move || {
        let mut entries = Vec::<BackupExclusionEntry>::new();
        if edit_excl_preset_temp_cache.get_untracked() {
            entries.push(BackupExclusionEntry::Preset(
                BackupExclusionPreset::TemporaryAndCache,
            ));
        }
        if edit_excl_preset_installers.get_untracked() {
            entries.push(BackupExclusionEntry::Preset(
                BackupExclusionPreset::InstallersAndArchives,
            ));
        }
        if edit_excl_preset_dev_deps.get_untracked() {
            entries.push(BackupExclusionEntry::Preset(
                BackupExclusionPreset::DeveloperDependencies,
            ));
        }
        if edit_excl_preset_os_meta.get_untracked() {
            entries.push(BackupExclusionEntry::Preset(
                BackupExclusionPreset::OsMetadata,
            ));
        }
        for rule in edit_excl_custom_rules.get_untracked() {
            entries.push(BackupExclusionEntry::Custom(rule));
        }
        for line in edit_excl_globs.get_untracked().lines() {
            let g = line.trim();
            if !g.is_empty() {
                entries.push(BackupExclusionEntry::Glob(g.to_string()));
            }
        }
        BackupExclusionConfig { entries }
    };

    let reset_wizard = move || {
        set_show_wizard.set(false);
        set_wizard_step.set(1);
        set_config_name.set(String::new());
        set_source_dir.set(String::new());
        set_selected_storage_id.set(String::new());
        set_wiz_preset_temp_cache.set(false);
        set_wiz_preset_installers.set(false);
        set_wiz_preset_dev_deps.set(false);
        set_wiz_preset_os_meta.set(true);
        set_wiz_custom_rules.set(Vec::new());
        set_wiz_advanced_globs.set(String::new());
        set_wiz_advanced_expanded.set(false);
        set_wiz_preview_result.set(None);
        set_wiz_preview_error.set(None);
        set_form_cleanup_enabled.set(false);
        set_form_cleanup_days.set("30".to_string());
        set_form_error.set(None);
    };

    let on_add = move || {
        set_form_loading.set(true);
        set_form_error.set(None);

        let s = session.get_untracked();
        let local_device_id = s.local_device_id;

        spawn_local(async move {
            let device_id = match local_device_id {
                Some(id) => id,
                None => {
                    match tauri_invoke::<_, GetOrCreateLocalDeviceResponse>(
                        "get_or_create_device",
                        "display_name",
                        String::new(),
                    )
                    .await
                    {
                        Ok(dev) => {
                            set_session.update(|s| {
                                s.device_display_name = dev.display_name.clone();
                                s.local_device_id = Some(dev.id);
                                s.device_registered = true;
                                s.physical_device_id = dev.physical_device_id.clone();
                            });
                            dev.id
                        }
                        Err(e) => {
                            let _ = set_form_error.try_set(Some(format!(
                                "Failed to register device: {}",
                                e.user_message()
                            )));
                            let _ = set_form_loading.try_set(false);
                            return;
                        }
                    }
                }
            };

            let storage_id_str = selected_storage_id.get_untracked();
            let Ok(storage_id) = storage_id_str.parse::<Uuid>() else {
                let _ = set_form_error.try_set(Some("Please select a remote storage.".to_string()));
                let _ = set_form_loading.try_set(false);
                return;
            };

            let cleanup_type = if form_cleanup_enabled.get_untracked() {
                let days: u16 = form_cleanup_days
                    .get_untracked()
                    .trim()
                    .parse()
                    .unwrap_or(30);
                CleanupType::DaysAfterLastBackup(days)
            } else {
                CleanupType::NoCleanup
            };

            let req = CreateBackupConfigUiRequest {
                storage_id,
                display_name: config_name.get_untracked(),
                local_device_id: device_id,
                source_directory: source_dir.get_untracked(),
                cleanup_type,
                exclusion_config: build_wiz_exclusion_config(),
            };

            match tauri_invoke::<_, CreateBackupConfigResponse>(
                "create_backup_config",
                "request",
                req,
            )
            .await
            {
                Ok(resp) => {
                    set_session.update(|s| {
                        if s.config_id.is_none() {
                            s.config_id = Some(resp.id);
                            s.source_directory = Some(source_dir.get_untracked());
                            s.storage_configured = true;
                        }
                    });
                    reset_wizard();
                    if let Ok(resp) = tauri_invoke_no_args::<
                        Vec<DecryptedBackupConfigWithRemoteStorage>,
                    >("list_all_backup_configs")
                    .await
                    {
                        let _ = set_configs.try_set(resp);
                    }
                }
                Err(e) => {
                    let _ = set_form_error.try_set(Some(e.user_message().to_string()));
                }
            }
            let _ = set_form_loading.try_set(false);
        });
    };

    let input_class = "w-full bg-input border border-border rounded-xl px-4 py-2.5 text-text-primary text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all";
    let label_class = "block text-sm font-medium text-text-secondary mb-1.5";

    view! {
        <div>
            {move || checkout_banner.get().map(|msg| {
                let is_success = msg.starts_with("Upgrade successful");
                let is_error = msg.starts_with("Failed") || msg.starts_with("Payment failed");
                let banner_class = if is_success {
                    "rounded-xl px-4 py-3 text-sm mb-4 bg-success-tint border border-success text-success"
                } else if is_error {
                    "rounded-xl px-4 py-3 text-sm mb-4 bg-error-tint border border-error-border text-error"
                } else {
                    "rounded-xl px-4 py-3 text-sm mb-4 bg-primary-tint border border-primary-glow text-primary-light"
                };
                view! {
                    <div class=banner_class>
                        {msg}
                        {(!checkout_polling.get()).then(|| view! {
                            <button
                                class="ml-3 text-xs opacity-60 hover:opacity-100 transition-opacity"
                                on:click=move |_| set_checkout_banner.set(None)
                            >"✕"</button>
                        })}
                    </div>
                }
            })}

            <div class="flex items-center justify-between mb-4">
                <h3 class="text-lg font-semibold">"Backup Configs"</h3>
                {move || if show_wizard.get() {
                    view! {
                        <button
                            class="text-sm font-medium text-primary-light hover:text-primary transition-colors"
                            on:click=move |_| reset_wizard()
                        >
                            "Cancel"
                        </button>
                    }.into_any()
                } else if can_add_config.get() {
                    view! {
                        <button
                            class="text-sm font-medium text-primary-light hover:text-primary transition-colors"
                            on:click=move |_| {
                                set_show_wizard.set(true);
                                set_wizard_step.set(1);
                                spawn_local(async move {
                                    if let Ok(resp) = tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages").await {
                                        let _ = set_storages.try_set(resp.list);
                                    }
                                });
                            }
                        >
                            "+ Add Config"
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <span class="text-xs text-text-secondary">
                            "Plan limit reached — "
                            <button
                                class="text-primary-light hover:underline bg-transparent border-0 p-0 cursor-pointer text-xs"
                                disabled=move || checkout_polling.get()
                                on:click=move |_| set_show_checkout_modal.set(true)
                            >
                                "Upgrade"
                            </button>
                        </span>
                    }.into_any()
                }}
            </div>

            // 5-step wizard
            {move || show_wizard.get().then(|| {
                let step = wizard_step.get();
                view! {
                    <div class="mb-6 p-4 bg-bg/50 border border-border rounded-xl space-y-4">
                        // Step indicator
                        <div class="flex items-center justify-between text-xs text-text-secondary">
                            {[("1","Source"),("2","Storage"),("3","Exclusions"),("4","Cleanup"),("5","Review")].iter().enumerate().map(|(i, (n, label))| {
                                let s = i as u8 + 1;
                                let active = step == s;
                                let done = step > s;
                                let cls = if active { "font-semibold text-primary" } else if done { "text-text-secondary/60" } else { "text-text-secondary/40" };
                                view! {
                                    <span class=cls>
                                        {*n}"·"{*label}
                                    </span>
                                }
                            }).collect_view()}
                        </div>

                        // Step content
                        {
                            if step == 1 {
                                view! {
                                    <div class="space-y-3">
                                        <div>
                                            <label class=label_class>"Config Name"</label>
                                            <input type="text" class=input_class placeholder="My Backup Config"
                                                prop:value=move || config_name.get()
                                                on:input=move |ev| set_config_name.set(event_target_value(&ev)) />
                                        </div>
                                        <div>
                                            <label class=label_class>"Folder to protect"</label>
                                            <div class="flex gap-2">
                                                <input type="text" class=input_class placeholder="/Users/you/Documents"
                                                    prop:value=move || source_dir.get()
                                                    on:input=move |ev| set_source_dir.set(event_target_value(&ev)) />
                                                <button type="button"
                                                    class="flex-shrink-0 px-3 py-2 bg-primary-tint border border-primary-glow rounded-xl text-primary text-sm font-medium hover:bg-primary-tint transition-all"
                                                    on:click=move |_| {
                                                        spawn_local(async move {
                                                            if let Ok(Some(path)) = tauri_invoke_no_args::<Option<String>>("pick_directory").await {
                                                                set_source_dir.set(path);
                                                            }
                                                        });
                                                    }
                                                >"Browse"</button>
                                            </div>
                                            // Quick picks
                                            <div class="flex flex-wrap gap-1.5 mt-2">
                                                {["Home","Desktop","Documents","Pictures","Downloads"].iter().map(|label| {
                                                    let lbl = *label;
                                                    view! {
                                                        <button type="button"
                                                            class="text-xs px-2 py-1 rounded-lg bg-elevation-1 border border-border text-text-secondary hover:text-primary hover:border-primary-glow transition-all"
                                                            on:click=move |_| {
                                                                let dir = lbl.to_string();
                                                                spawn_local(async move {
                                                                    // Resolve to absolute path without opening any file picker
                                                                    if let Ok(path) = tauri_invoke::<_, String>("resolve_standard_dir", "dir", dir).await {
                                                                        set_source_dir.set(path);
                                                                    }
                                                                });
                                                            }
                                                        >{lbl}</button>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        </div>
                                    </div>
                                }.into_any()
                            } else if step == 2 {
                                let storage_list = storages.get();
                                view! {
                                    <div>
                                        <label class=label_class>"Remote Storage"</label>
                                        {if storage_list.is_empty() {
                                            view! {
                                                <p class="text-sm text-text-secondary">"No storages configured. Add a remote storage in the section above first."</p>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <select class=input_class
                                                    on:change=move |ev| set_selected_storage_id.set(event_target_value(&ev))
                                                >
                                                    <option value="" disabled selected>"Select a storage..."</option>
                                                    {storage_list.into_iter().map(|s| {
                                                        let id = s.id.to_string();
                                                        let label = format!("{} ({})", s.name, s.storage_type);
                                                        let selected = selected_storage_id.get() == id;
                                                        view! { <option value={id} selected=selected>{label}</option> }
                                                    }).collect_view()}
                                                </select>
                                            }.into_any()
                                        }}
                                    </div>
                                }.into_any()
                            } else if step == 3 {
                                view! {
                                    <div class="space-y-4">
                                        <div>
                                            <p class=label_class>"Preset exclusions"</p>
                                            <div class="space-y-1.5">
                                                {[
                                                    (wiz_preset_os_meta, set_wiz_preset_os_meta, "OS metadata (.DS_Store, desktop.ini)"),
                                                    (wiz_preset_temp_cache, set_wiz_preset_temp_cache, "Temporary files & caches"),
                                                    (wiz_preset_installers, set_wiz_preset_installers, "Installers & archives (.dmg, .zip, .pkg…)"),
                                                    (wiz_preset_dev_deps, set_wiz_preset_dev_deps, "Developer dependencies (node_modules, target, .venv…)"),
                                                ].into_iter().map(|(sig, set_sig, lbl)| {
                                                    view! {
                                                        <label class="flex items-center gap-2 cursor-pointer select-none text-sm text-text-secondary">
                                                            <input type="checkbox" class="w-4 h-4 accent-primary"
                                                                prop:checked=move || sig.get()
                                                                on:change=move |ev| {
                                                                    let checked = ev.target().unwrap()
                                                                        .unchecked_into::<web_sys::HtmlInputElement>()
                                                                        .checked();
                                                                    set_sig.set(checked);
                                                                }
                                                            />
                                                            {lbl}
                                                        </label>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        </div>

                                        // Custom rules
                                        <div>
                                            <div class="flex items-center justify-between mb-1.5">
                                                <p class=label_class style="margin-bottom:0">"Custom rules"</p>
                                                <button type="button"
                                                    class="text-xs text-primary-light hover:text-primary transition-colors"
                                                    on:click=move |_| {
                                                        let mut rules = wiz_custom_rules.get_untracked();
                                                        rules.push(BackupExclusionRule {
                                                            id: uuid::Uuid::now_v7().to_string(),
                                                            enabled: true,
                                                            kind: BackupExclusionRuleKind::FileExtension,
                                                            value: String::new(),
                                                        });
                                                        set_wiz_custom_rules.set(rules);
                                                    }
                                                >"+ Add rule"</button>
                                            </div>
                                            {move || {
                                                let rules = wiz_custom_rules.get();
                                                if rules.is_empty() {
                                                    view! { <p class="text-xs text-text-secondary/60">"No custom rules."</p> }.into_any()
                                                } else {
                                                    view! {
                                                        <div class="space-y-2">
                                                            {rules.into_iter().enumerate().map(|(idx, rule)| {
                                                                let rule_id = rule.id.clone();
                                                                let kind_val = format!("{:?}", rule.kind);
                                                                let value_val = rule.value.clone();
                                                                let enabled = rule.enabled;
                                                                view! {
                                                                    <div class="flex items-center gap-2">
                                                                        <input type="checkbox" class="w-4 h-4 accent-primary flex-shrink-0"
                                                                            prop:checked=enabled
                                                                            on:change=move |ev| {
                                                                                let checked = ev.target().unwrap()
                                                                                    .unchecked_into::<web_sys::HtmlInputElement>()
                                                                                    .checked();
                                                                                let mut rules = wiz_custom_rules.get_untracked();
                                                                                if let Some(r) = rules.get_mut(idx) {
                                                                                    r.enabled = checked;
                                                                                }
                                                                                set_wiz_custom_rules.set(rules);
                                                                            }
                                                                        />
                                                                        <select class="bg-input border border-border rounded-lg px-2 py-1.5 text-text-primary text-xs flex-shrink-0"
                                                                            on:change=move |ev| {
                                                                                let val = event_target_value(&ev);
                                                                                let kind = match val.as_str() {
                                                                                    "FolderName" => BackupExclusionRuleKind::FolderName,
                                                                                    "FileOrFolderName" => BackupExclusionRuleKind::FileOrFolderName,
                                                                                    "PathContains" => BackupExclusionRuleKind::PathContains,
                                                                                    _ => BackupExclusionRuleKind::FileExtension,
                                                                                };
                                                                                let mut rules = wiz_custom_rules.get_untracked();
                                                                                if let Some(r) = rules.get_mut(idx) {
                                                                                    r.kind = kind;
                                                                                }
                                                                                set_wiz_custom_rules.set(rules);
                                                                            }
                                                                        >
                                                                            <option value="FileExtension" selected={kind_val == "FileExtension"}>"File ext"</option>
                                                                            <option value="FolderName" selected={kind_val == "FolderName"}>"Folder name"</option>
                                                                            <option value="FileOrFolderName" selected={kind_val == "FileOrFolderName"}>"Name"</option>
                                                                            <option value="PathContains" selected={kind_val == "PathContains"}>"Path contains"</option>
                                                                        </select>
                                                                        <input type="text" class="flex-1 bg-input border border-border rounded-lg px-2 py-1.5 text-text-primary text-xs"
                                                                            placeholder="e.g. log"
                                                                            prop:value=value_val
                                                                            on:input=move |ev| {
                                                                                let val = event_target_value(&ev);
                                                                                let mut rules = wiz_custom_rules.get_untracked();
                                                                                if let Some(r) = rules.get_mut(idx) {
                                                                                    r.value = val;
                                                                                }
                                                                                set_wiz_custom_rules.set(rules);
                                                                            }
                                                                        />
                                                                        <button type="button"
                                                                            class="text-xs text-error hover:text-error/80 transition-colors flex-shrink-0"
                                                                            on:click=move |_| {
                                                                                let rid = rule_id.clone();
                                                                                let mut rules = wiz_custom_rules.get_untracked();
                                                                                rules.retain(|r| r.id != rid);
                                                                                set_wiz_custom_rules.set(rules);
                                                                            }
                                                                        >"✕"</button>
                                                                    </div>
                                                                }
                                                            }).collect_view()}
                                                        </div>
                                                    }.into_any()
                                                }
                                            }}
                                        </div>

                                        // Advanced globs
                                        <div>
                                            <button type="button"
                                                class="text-xs text-text-secondary hover:text-primary transition-colors"
                                                on:click=move |_| set_wiz_advanced_expanded.update(|v| *v = !*v)
                                            >
                                                {move || if wiz_advanced_expanded.get() { "▾ Advanced glob patterns" } else { "▸ Advanced glob patterns" }}
                                            </button>
                                            {move || wiz_advanced_expanded.get().then(|| view! {
                                                <textarea
                                                    class="mt-2 w-full bg-input border border-border rounded-xl px-3 py-2 text-text-primary text-xs font-mono focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all resize-none"
                                                    rows="4"
                                                    placeholder="**/*.log&#10;**/build/**"
                                                    prop:value=move || wiz_advanced_globs.get()
                                                    on:input=move |ev| set_wiz_advanced_globs.set(event_target_value(&ev))
                                                ></textarea>
                                            })}
                                        </div>

                                        // Preview
                                        <div>
                                            <button type="button"
                                                class="text-xs font-medium px-3 py-1.5 rounded-lg bg-primary-tint border border-primary-glow text-primary hover:bg-primary-tint/80 transition-all disabled:opacity-50"
                                                disabled=move || wiz_preview_loading.get() || source_dir.get().is_empty()
                                                on:click=move |_| {
                                                    if source_dir.get_untracked().is_empty() { return; }
                                                    set_wiz_preview_loading.set(true);
                                                    set_wiz_preview_error.set(None);
                                                    let req = PreviewBackupExclusionsRequest {
                                                        source_directory: source_dir.get_untracked(),
                                                        exclusions: build_wiz_exclusion_config(),
                                                        sample_limit: 10,
                                                    };
                                                    spawn_local(async move {
                                                        match tauri_invoke::<_, PreviewBackupExclusionsResponse>(
                                                            "preview_backup_exclusions", "request", req,
                                                        ).await {
                                                            Ok(r) => set_wiz_preview_result.set(Some(r)),
                                                            Err(e) => set_wiz_preview_error.set(Some(e.user_message().to_string())),
                                                        }
                                                        set_wiz_preview_loading.set(false);
                                                    });
                                                }
                                            >
                                                {move || if wiz_preview_loading.get() { "Previewing…" } else { "Preview exclusions" }}
                                            </button>
                                            {move || wiz_preview_error.get().map(|e| view! {
                                                <p class="text-xs text-error mt-1">{e}</p>
                                            })}
                                            {move || wiz_preview_result.get().map(|r| view! {
                                                <div class="mt-2 p-3 rounded-xl bg-elevation-1 border border-elevation-1-border text-xs space-y-1">
                                                    <p class="text-text-primary">
                                                        <span class="text-success font-medium">{r.included_files}" files included"</span>
                                                        " · "
                                                        <span class="text-error font-medium">{r.excluded_files}" excluded"</span>
                                                    </p>
                                                    <p class="text-text-secondary">"Excluded bytes: "{format_bytes(r.excluded_bytes)}</p>
                                                    {(!r.sample_excluded_files.is_empty()).then(|| view! {
                                                        <div class="mt-1">
                                                            <p class="text-text-secondary/70 mb-0.5">"Sample excluded:"</p>
                                                            {r.sample_excluded_files.into_iter().map(|f| view! {
                                                                <p class="truncate text-text-secondary/60 font-mono">{f.path}</p>
                                                            }).collect_view()}
                                                        </div>
                                                    })}
                                                </div>
                                            })}
                                        </div>
                                    </div>
                                }.into_any()
                            } else if step == 4 {
                                view! {
                                    <div>
                                        <label class=label_class>"Local File Cleanup"</label>
                                        <div class="flex items-center gap-3 mb-2">
                                            <label class="flex items-center gap-2 cursor-pointer select-none text-sm text-text-secondary">
                                                <input type="checkbox" class="w-4 h-4 accent-primary"
                                                    prop:checked=move || form_cleanup_enabled.get()
                                                    on:change=move |ev| {
                                                        let checked = ev.target().unwrap()
                                                            .unchecked_into::<web_sys::HtmlInputElement>()
                                                            .checked();
                                                        set_form_cleanup_enabled.set(checked);
                                                    }
                                                />
                                                "Delete local files after backup"
                                            </label>
                                        </div>
                                        {move || form_cleanup_enabled.get().then(|| view! {
                                            <div class="flex items-center gap-2">
                                                <input type="number" min="1" max="3650" class=input_class placeholder="30"
                                                    prop:value=move || form_cleanup_days.get()
                                                    on:input=move |ev| set_form_cleanup_days.set(event_target_value(&ev)) />
                                                <span class="text-sm text-text-secondary flex-shrink-0">"days after backup"</span>
                                            </div>
                                        })}
                                        <p class="text-xs text-text-secondary mt-1">"Files are only deleted once they have been safely backed up for the configured number of days."</p>
                                    </div>
                                }.into_any()
                            } else {
                                // Step 5: Review
                                let name_val = config_name.get();
                                let dir_val = source_dir.get();
                                let storage_val = {
                                    let sid = selected_storage_id.get();
                                    storages.get().into_iter().find(|s| s.id.to_string() == sid)
                                        .map(|s| format!("{} ({})", s.name, s.storage_type))
                                        .unwrap_or_else(|| "—".to_string())
                                };
                                let cleanup_val = if form_cleanup_enabled.get() {
                                    format!("Delete after {} days", form_cleanup_days.get())
                                } else {
                                    "No cleanup".to_string()
                                };
                                let excl_config = BackupExclusionConfig {
                                    entries: {
                                        let mut v = Vec::new();
                                        if wiz_preset_temp_cache.get() { v.push(BackupExclusionEntry::Preset(BackupExclusionPreset::TemporaryAndCache)); }
                                        if wiz_preset_installers.get() { v.push(BackupExclusionEntry::Preset(BackupExclusionPreset::InstallersAndArchives)); }
                                        if wiz_preset_dev_deps.get() { v.push(BackupExclusionEntry::Preset(BackupExclusionPreset::DeveloperDependencies)); }
                                        if wiz_preset_os_meta.get() { v.push(BackupExclusionEntry::Preset(BackupExclusionPreset::OsMetadata)); }
                                        v
                                    }
                                };
                                let excl_label = excl_config.summary_label();
                                let custom_count = wiz_custom_rules.get().len();
                                let excl_summary = if custom_count > 0 {
                                    format!("{}, {} custom rule{}", excl_label, custom_count, if custom_count == 1 { "" } else { "s" })
                                } else {
                                    excl_label
                                };
                                view! {
                                    <div class="space-y-2 text-sm">
                                        <div class="flex gap-2"><span class="text-text-secondary/70 w-24 flex-shrink-0">"Name:"</span><span class="text-text-primary">{name_val}</span></div>
                                        <div class="flex gap-2"><span class="text-text-secondary/70 w-24 flex-shrink-0">"Source:"</span><span class="text-text-primary font-mono text-xs">{dir_val}</span></div>
                                        <div class="flex gap-2"><span class="text-text-secondary/70 w-24 flex-shrink-0">"Storage:"</span><span class="text-text-primary">{storage_val}</span></div>
                                        <div class="flex gap-2"><span class="text-text-secondary/70 w-24 flex-shrink-0">"Exclusions:"</span><span class="text-text-primary">{excl_summary}</span></div>
                                        <div class="flex gap-2"><span class="text-text-secondary/70 w-24 flex-shrink-0">"Cleanup:"</span><span class="text-text-primary">{cleanup_val}</span></div>
                                    </div>
                                }.into_any()
                            }
                        }

                        // Error
                        {move || form_error.try_get().flatten().map(|e| view! {
                            <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                        })}

                        // Navigation
                        <div class="flex items-center gap-3 pt-2">
                            {move || (wizard_step.get() > 1).then(|| view! {
                                <button type="button"
                                    class="text-sm px-4 py-2 rounded-xl border border-border text-text-secondary hover:text-text-primary hover:border-border/80 transition-all"
                                    on:click=move |_| set_wizard_step.update(|s| *s = s.saturating_sub(1))
                                >"Back"</button>
                            })}
                            {move || {
                                let step = wizard_step.get();
                                if step < 5 {
                                    view! {
                                        <button type="button"
                                            class="btn-gradient text-white px-6 py-2 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint"
                                            on:click=move |_| {
                                                let s = wizard_step.get_untracked();
                                                // Validate current step
                                                let err = match s {
                                                    1 => {
                                                        if config_name.get_untracked().trim().is_empty() {
                                                            Some("Config name is required.".to_string())
                                                        } else if source_dir.get_untracked().trim().is_empty() {
                                                            Some("Source directory is required.".to_string())
                                                        } else { None }
                                                    }
                                                    2 => {
                                                        if selected_storage_id.get_untracked().is_empty() {
                                                            Some("Please select a remote storage.".to_string())
                                                        } else { None }
                                                    }
                                                    _ => None,
                                                };
                                                if let Some(e) = err {
                                                    set_form_error.set(Some(e));
                                                } else {
                                                    set_form_error.set(None);
                                                    set_wizard_step.update(|s| *s = (*s + 1).min(5));
                                                }
                                            }
                                        >"Continue"</button>
                                    }.into_any()
                                } else {
                                    view! {
                                        <button type="button"
                                            class="btn-gradient text-white px-6 py-2 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint disabled:opacity-50"
                                            disabled=move || form_loading.try_get().unwrap_or(false)
                                            on:click=move |_| on_add()
                                        >
                                            {move || if form_loading.try_get().unwrap_or(false) { "Creating…" } else { "Create Config" }}
                                        </button>
                                    }.into_any()
                                }
                            }}
                        </div>
                    </div>
                }
            })}

            // Config cards (job card style, consistent with backup page)
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
                                            <div class="h-3 bg-bg rounded w-2/3"></div>
                                        </div>
                                    </div>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any();
                }

                let physical_device_id = session.get().physical_device_id.clone();
                let mut list = configs.get();

                if list.is_empty() {
                    return view! { <p class="text-text-secondary text-sm">"No backup configs yet."</p> }.into_any();
                }

                list.sort_by(|a, b| {
                    let a_current = a.physical_device_id.as_deref() == Some(physical_device_id.as_str());
                    let b_current = b.physical_device_id.as_deref() == Some(physical_device_id.as_str());
                    b_current.cmp(&a_current)
                        .then(b.is_active.cmp(&a.is_active))
                });

                view! {
                    <div class="space-y-4">
                        {list.into_iter().map(|c| {
                            let is_active = c.is_active;
                            let config_id = c.config_id;
                            let display_name = c.display_name.clone();
                            let display_name2 = display_name.clone();
                            let source_dir = c.source_directory.clone();
                            let storage_type = format!("{}", c.storage_type);
                            let icon = storage_type_icon(&c.storage_type);
                            let cleanup_type = c.cleanup_type.clone();
                            let cleanup_type_for_edit = cleanup_type.clone();
                            let device_label = c.physical_device_id.clone().unwrap_or_else(|| "Unknown".to_string());
                            let card_opacity = if is_active { "" } else { "opacity-60" };
                            let exclusion_summary_label = c.exclusion_config.summary_label();
                            let c_excl = c.exclusion_config.clone();

                            view! {
                                <div class={format!("glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-4 md:p-5 hover-lift {}", card_opacity)}>
                                    // Header: icon + name + badge
                                    <div class="flex items-start gap-3">
                                        <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center flex-shrink-0 mt-0.5"
                                            inner_html=icon
                                        ></div>
                                        <div class="flex-1 min-w-0">
                                            <div class="flex items-center gap-2 mb-1">
                                                {move || {
                                                    let dn = display_name2.clone();
                                                    if renaming_id.get() == Some(config_id) {
                                                        view! {
                                                            <form
                                                                class="flex items-center gap-1.5 flex-1 min-w-0"
                                                                on:submit=move |ev| {
                                                                    ev.prevent_default();
                                                                    let new_name = rename_value.get_untracked();
                                                                    if new_name.trim().is_empty() { return; }
                                                                    set_rename_loading.set(true);
                                                                    spawn_local(async move {
                                                                        let req = RenameBackupConfigRequest {
                                                                            id: config_id,
                                                                            display_name: new_name,
                                                                        };
                                                                        if let Ok(_) = tauri_invoke::<_, RenameBackupConfigResponse>(
                                                                            "rename_backup_config",
                                                                            "request",
                                                                            req,
                                                                        ).await {
                                                                            if let Ok(resp) = tauri_invoke_no_args::<Vec<DecryptedBackupConfigWithRemoteStorage>>(
                                                                                "list_all_backup_configs",
                                                                            ).await {
                                                                                let _ = set_configs.try_set(resp);
                                                                            }
                                                                        }
                                                                        let _ = set_rename_loading.try_set(false);
                                                                        let _ = set_renaming_id.try_set(None);
                                                                    });
                                                                }
                                                            >
                                                                <input
                                                                    type="text"
                                                                    class="flex-1 min-w-0 bg-input border border-primary rounded-lg px-2 py-0.5 text-sm font-semibold focus:outline-none focus:ring-2 focus:ring-primary-tint"
                                                                    prop:value=move || rename_value.get()
                                                                    on:input=move |ev| set_rename_value.set(event_target_value(&ev))
                                                                    autofocus
                                                                />
                                                                <button
                                                                    type="submit"
                                                                    class="text-xs px-2 py-1 rounded-lg bg-primary text-white font-medium hover:opacity-90 disabled:opacity-50 flex-shrink-0"
                                                                    disabled=move || rename_loading.get()
                                                                >
                                                                    {move || if rename_loading.get() { "..." } else { "Save" }}
                                                                </button>
                                                                <button
                                                                    type="button"
                                                                    class="text-xs px-2 py-1 rounded-lg text-text-secondary hover:bg-elevation-1-border flex-shrink-0"
                                                                    on:click=move |_| { set_renaming_id.set(None); }
                                                                >
                                                                    "Cancel"
                                                                </button>
                                                            </form>
                                                        }.into_any()
                                                    } else {
                                                        let dn2 = dn.clone();
                                                        view! {
                                                            <h4
                                                                class="text-sm md:text-base font-semibold truncate cursor-pointer hover:text-primary transition-colors"
                                                                title="Click to rename"
                                                                on:click=move |_| {
                                                                    set_rename_value.set(dn2.clone());
                                                                    set_renaming_id.set(Some(config_id));
                                                                }
                                                            >
                                                                {dn}
                                                            </h4>
                                                        }.into_any()
                                                    }
                                                }}
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
                                            <div class="text-xs text-text-secondary mb-2">
                                                <span class="text-text-secondary/70">"Source: "</span>
                                                <span class="md:hidden">{short_path(&source_dir)}</span>
                                                <span class="hidden md:inline">{source_dir}</span>
                                            </div>
                                            <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-secondary">
                                                <span>{format!("Storage: {}", storage_type)}</span>
                                                <span class="text-border">"·"</span>
                                                <span>{format!("Device: {}", &device_label[..device_label.len().min(12)])}</span>
                                                <span class="text-border">"·"</span>
                                                <span>"Encryption: AES-256-GCM"</span>
                                            </div>
                                            // Cleanup row
                                            <div class="mt-1.5">
                                        {move || {
                                            if editing_cleanup_id.get() == Some(config_id) {
                                                view! {
                                                    <div class="flex flex-wrap items-center gap-2">
                                                        <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer select-none">
                                                            <input
                                                                type="checkbox"
                                                                class="w-3.5 h-3.5 accent-primary"
                                                                prop:checked=move || editing_cleanup_enabled.get()
                                                                on:change=move |ev| {
                                                                    use wasm_bindgen::JsCast;
                                                                    let checked = ev.target().unwrap()
                                                                        .unchecked_into::<web_sys::HtmlInputElement>()
                                                                        .checked();
                                                                    set_editing_cleanup_enabled.set(checked);
                                                                }
                                                            />
                                                            "Delete after"
                                                        </label>
                                                        {move || editing_cleanup_enabled.get().then(|| view! {
                                                            <input
                                                                type="number"
                                                                min="1"
                                                                max="3650"
                                                                class="w-16 bg-input border border-border rounded-lg px-2 py-0.5 text-xs focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary-tint"
                                                                prop:value=move || editing_cleanup_days.get()
                                                                on:input=move |ev| set_editing_cleanup_days.set(event_target_value(&ev))
                                                            />
                                                            <span class="text-xs text-text-secondary">"days"</span>
                                                        })}
                                                        <button
                                                            type="button"
                                                            class="text-xs px-2 py-1 rounded-lg bg-primary text-white font-medium hover:opacity-90 disabled:opacity-50"
                                                            disabled=move || cleanup_edit_loading.get()
                                                            on:click=move |_| {
                                                                let new_cleanup = if editing_cleanup_enabled.get_untracked() {
                                                                    let days: u16 = editing_cleanup_days.get_untracked().trim().parse().unwrap_or(30);
                                                                    CleanupType::DaysAfterLastBackup(days)
                                                                } else {
                                                                    CleanupType::NoCleanup
                                                                };
                                                                set_cleanup_edit_loading.set(true);
                                                                spawn_local(async move {
                                                                    let req = UpdateCleanupTypeRequest {
                                                                        id: config_id,
                                                                        cleanup_type: new_cleanup,
                                                                    };
                                                                    if let Ok(_) = tauri_invoke::<_, UpdateCleanupTypeResponse>(
                                                                        "update_cleanup_type",
                                                                        "request",
                                                                        req,
                                                                    ).await {
                                                                        if let Ok(resp) = tauri_invoke_no_args::<Vec<DecryptedBackupConfigWithRemoteStorage>>(
                                                                            "list_all_backup_configs",
                                                                        ).await {
                                                                            let _ = set_configs.try_set(resp);
                                                                        }
                                                                    }
                                                                    let _ = set_cleanup_edit_loading.try_set(false);
                                                                    let _ = set_editing_cleanup_id.try_set(None);
                                                                });
                                                            }
                                                        >
                                                            {move || if cleanup_edit_loading.get() { "..." } else { "Save" }}
                                                        </button>
                                                        <button
                                                            type="button"
                                                            class="text-xs px-2 py-1 rounded-lg text-text-secondary hover:bg-elevation-1-border"
                                                            on:click=move |_| set_editing_cleanup_id.set(None)
                                                        >
                                                            "Cancel"
                                                        </button>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                let cleanup_label = match &cleanup_type {
                                                    CleanupType::NoCleanup => "No cleanup".to_string(),
                                                    CleanupType::DaysAfterLastBackup(d) => format!("Delete after {} days", d),
                                                };
                                                let ct = cleanup_type_for_edit.clone();
                                                view! {
                                                    <div class="flex items-center gap-2">
                                                        <span class="text-xs text-text-secondary">
                                                            <span class="text-text-secondary/70">"Cleanup: "</span>
                                                            {cleanup_label}
                                                        </span>
                                                        <button
                                                            type="button"
                                                            class="text-xs text-primary-light hover:text-primary transition-colors"
                                                            on:click=move |_| {
                                                                match &ct {
                                                                    CleanupType::NoCleanup => {
                                                                        set_editing_cleanup_enabled.set(false);
                                                                        set_editing_cleanup_days.set("30".to_string());
                                                                    }
                                                                    CleanupType::DaysAfterLastBackup(d) => {
                                                                        set_editing_cleanup_enabled.set(true);
                                                                        set_editing_cleanup_days.set(d.to_string());
                                                                    }
                                                                }
                                                                set_editing_cleanup_id.set(Some(config_id));
                                                            }
                                                        >
                                                            "Edit"
                                                        </button>
                                                    </div>
                                                }.into_any()
                                            }
                                        }}
                                            </div>
                                            // Exclusion row
                                            <div class="mt-1.5">
                                                {move || {
                                                    if editing_exclusions_id.get() == Some(config_id) {
                                                        // Edit panel
                                                        view! {
                                                            <div class="space-y-3 p-3 bg-elevation-0 rounded-xl border border-border">
                                                                <p class="text-xs font-medium text-text-primary">"Edit Exclusions"</p>
                                                                // Presets
                                                                <div class="space-y-1.5">
                                                                    <p class="text-xs text-text-secondary/70">"Presets"</p>
                                                                    <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer select-none">
                                                                        <input type="checkbox" class="w-3.5 h-3.5 accent-primary"
                                                                            prop:checked=move || edit_excl_preset_temp_cache.get()
                                                                            on:change=move |ev| {
                                                                                let checked = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().checked();
                                                                                set_edit_excl_preset_temp_cache.set(checked);
                                                                            }
                                                                        />
                                                                        "Temp & cache files"
                                                                    </label>
                                                                    <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer select-none">
                                                                        <input type="checkbox" class="w-3.5 h-3.5 accent-primary"
                                                                            prop:checked=move || edit_excl_preset_installers.get()
                                                                            on:change=move |ev| {
                                                                                let checked = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().checked();
                                                                                set_edit_excl_preset_installers.set(checked);
                                                                            }
                                                                        />
                                                                        "Installers & archives"
                                                                    </label>
                                                                    <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer select-none">
                                                                        <input type="checkbox" class="w-3.5 h-3.5 accent-primary"
                                                                            prop:checked=move || edit_excl_preset_dev_deps.get()
                                                                            on:change=move |ev| {
                                                                                let checked = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().checked();
                                                                                set_edit_excl_preset_dev_deps.set(checked);
                                                                            }
                                                                        />
                                                                        "Developer dependencies"
                                                                    </label>
                                                                    <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer select-none">
                                                                        <input type="checkbox" class="w-3.5 h-3.5 accent-primary"
                                                                            prop:checked=move || edit_excl_preset_os_meta.get()
                                                                            on:change=move |ev| {
                                                                                let checked = ev.target().unwrap().unchecked_into::<web_sys::HtmlInputElement>().checked();
                                                                                set_edit_excl_preset_os_meta.set(checked);
                                                                            }
                                                                        />
                                                                        "OS metadata"
                                                                    </label>
                                                                </div>
                                                                // Custom rules
                                                                <div class="space-y-1.5">
                                                                    <div class="flex items-center justify-between">
                                                                        <p class="text-xs text-text-secondary/70">"Custom rules"</p>
                                                                        <button type="button"
                                                                            class="text-xs text-primary hover:text-primary/80 transition-colors"
                                                                            on:click=move |_| {
                                                                                let new_rule = BackupExclusionRule {
                                                                                    id: Uuid::now_v7().to_string(),
                                                                                    enabled: true,
                                                                                    kind: BackupExclusionRuleKind::FileExtension,
                                                                                    value: String::new(),
                                                                                };
                                                                                set_edit_excl_custom_rules.update(|rules| rules.push(new_rule));
                                                                            }
                                                                        >"+ Add rule"</button>
                                                                    </div>
                                                                    {move || {
                                                                        let rules = edit_excl_custom_rules.get();
                                                                        if rules.is_empty() {
                                                                            view! { <p class="text-xs text-text-secondary/50 italic">"No custom rules"</p> }.into_any()
                                                                        } else {
                                                                            rules.into_iter().enumerate().map(|(idx, rule)| {
                                                                                let rule_id = rule.id.clone();
                                                                                let kind_val = format!("{:?}", rule.kind);
                                                                                let value_val = rule.value.clone();
                                                                                view! {
                                                                                    <div class="flex items-center gap-2">
                                                                                        <select class="bg-input border border-border rounded-lg px-2 py-1 text-xs text-text-primary flex-shrink-0"
                                                                                            on:change=move |ev| {
                                                                                                let val = event_target_value(&ev);
                                                                                                let kind = match val.as_str() {
                                                                                                    "FolderName" => BackupExclusionRuleKind::FolderName,
                                                                                                    "FileOrFolderName" => BackupExclusionRuleKind::FileOrFolderName,
                                                                                                    "PathContains" => BackupExclusionRuleKind::PathContains,
                                                                                                    _ => BackupExclusionRuleKind::FileExtension,
                                                                                                };
                                                                                                let mut rules = edit_excl_custom_rules.get_untracked();
                                                                                                if let Some(r) = rules.get_mut(idx) { r.kind = kind; }
                                                                                                set_edit_excl_custom_rules.set(rules);
                                                                                            }
                                                                                        >
                                                                                            <option value="FileExtension" selected={kind_val == "FileExtension"}>"File ext"</option>
                                                                                            <option value="FolderName" selected={kind_val == "FolderName"}>"Folder name"</option>
                                                                                            <option value="FileOrFolderName" selected={kind_val == "FileOrFolderName"}>"Name"</option>
                                                                                            <option value="PathContains" selected={kind_val == "PathContains"}>"Path contains"</option>
                                                                                        </select>
                                                                                        <input type="text" class="flex-1 bg-input border border-border rounded-lg px-2 py-1 text-xs text-text-primary"
                                                                                            placeholder="e.g. log"
                                                                                            prop:value=value_val
                                                                                            on:input=move |ev| {
                                                                                                let val = event_target_value(&ev);
                                                                                                let mut rules = edit_excl_custom_rules.get_untracked();
                                                                                                if let Some(r) = rules.get_mut(idx) { r.value = val; }
                                                                                                set_edit_excl_custom_rules.set(rules);
                                                                                            }
                                                                                        />
                                                                                        <button type="button"
                                                                                            class="text-xs text-error hover:text-error/80 flex-shrink-0"
                                                                                            on:click=move |_| {
                                                                                                let rid = rule_id.clone();
                                                                                                let mut rules = edit_excl_custom_rules.get_untracked();
                                                                                                rules.retain(|r| r.id != rid);
                                                                                                set_edit_excl_custom_rules.set(rules);
                                                                                            }
                                                                                        >"✕"</button>
                                                                                    </div>
                                                                                }
                                                                            }).collect_view().into_any()
                                                                        }
                                                                    }}
                                                                </div>
                                                                // Advanced globs
                                                                <div>
                                                                    <button type="button"
                                                                        class="text-xs text-text-secondary hover:text-primary transition-colors"
                                                                        on:click=move |_| set_edit_excl_advanced_expanded.update(|v| *v = !*v)
                                                                    >
                                                                        {move || if edit_excl_advanced_expanded.get() { "▾ Advanced glob patterns" } else { "▸ Advanced glob patterns" }}
                                                                    </button>
                                                                    {move || edit_excl_advanced_expanded.get().then(|| view! {
                                                                        <textarea
                                                                            class="mt-2 w-full bg-input border border-border rounded-xl px-3 py-2 text-text-primary text-xs font-mono focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all resize-none"
                                                                            rows="3"
                                                                            placeholder="**/*.log&#10;**/build/**"
                                                                            prop:value=move || edit_excl_globs.get()
                                                                            on:input=move |ev| set_edit_excl_globs.set(event_target_value(&ev))
                                                                        ></textarea>
                                                                    })}
                                                                </div>
                                                                // Error
                                                                {move || edit_excl_error.get().map(|e| view! {
                                                                    <p class="text-xs text-error">{e}</p>
                                                                })}
                                                                // Save / Cancel
                                                                <div class="flex items-center gap-2">
                                                                    <button type="button"
                                                                        class="text-xs px-3 py-1.5 rounded-lg bg-primary text-white font-medium hover:opacity-90 disabled:opacity-50"
                                                                        disabled=move || edit_excl_loading.get()
                                                                        on:click=move |_| {
                                                                            let exclusion_config = build_edit_exclusion_config();
                                                                            set_edit_excl_loading.set(true);
                                                                            set_edit_excl_error.set(None);
                                                                            spawn_local(async move {
                                                                                let args = UpdateBackupExclusionsArgs {
                                                                                    id: config_id,
                                                                                    exclusion_config,
                                                                                };
                                                                                match tauri_invoke_with_args::<_, UpdateExclusionConfigResponse>(
                                                                                    "update_backup_exclusions",
                                                                                    &args,
                                                                                ).await {
                                                                                    Ok(_) => {
                                                                                        if let Ok(resp) = tauri_invoke_no_args::<Vec<DecryptedBackupConfigWithRemoteStorage>>(
                                                                                            "list_all_backup_configs",
                                                                                        ).await {
                                                                                            let _ = set_configs.try_set(resp);
                                                                                        }
                                                                                        let _ = set_editing_exclusions_id.try_set(None);
                                                                                    }
                                                                                    Err(e) => {
                                                                                        let _ = set_edit_excl_error.try_set(Some(e.user_message().to_string()));
                                                                                    }
                                                                                }
                                                                                let _ = set_edit_excl_loading.try_set(false);
                                                                            });
                                                                        }
                                                                    >
                                                                        {move || if edit_excl_loading.get() { "Saving…" } else { "Save" }}
                                                                    </button>
                                                                    <button type="button"
                                                                        class="text-xs px-3 py-1.5 rounded-lg text-text-secondary hover:bg-elevation-1-border"
                                                                        on:click=move |_| {
                                                                            set_editing_exclusions_id.set(None);
                                                                            set_edit_excl_error.set(None);
                                                                        }
                                                                    >"Cancel"</button>
                                                                </div>
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        // Summary row
                                                        let excl_summary = exclusion_summary_label.clone();
                                                        let c_excl_for_click = c_excl.clone();
                                                        view! {
                                                            <div class="flex items-center gap-2">
                                                                <span class="text-xs text-text-secondary">
                                                                    <span class="text-text-secondary/70">"Exclusions: "</span>
                                                                    {excl_summary}
                                                                </span>
                                                                <button
                                                                    type="button"
                                                                    class="text-xs text-primary-light hover:text-primary transition-colors"
                                                                    on:click=move |_| {
                                                                        let mut p_tc = false;
                                                                        let mut p_inst = false;
                                                                        let mut p_dev = false;
                                                                        let mut p_os = false;
                                                                        let mut custom = Vec::<BackupExclusionRule>::new();
                                                                        let mut globs = String::new();
                                                                        for entry in &c_excl_for_click.entries {
                                                                            match entry {
                                                                                BackupExclusionEntry::Preset(p) => match p {
                                                                                    BackupExclusionPreset::TemporaryAndCache => p_tc = true,
                                                                                    BackupExclusionPreset::InstallersAndArchives => p_inst = true,
                                                                                    BackupExclusionPreset::DeveloperDependencies => p_dev = true,
                                                                                    BackupExclusionPreset::OsMetadata => p_os = true,
                                                                                },
                                                                                BackupExclusionEntry::Custom(r) => custom.push(r.clone()),
                                                                                BackupExclusionEntry::Glob(g) => {
                                                                                    if !globs.is_empty() { globs.push('\n'); }
                                                                                    globs.push_str(g);
                                                                                }
                                                                            }
                                                                        }
                                                                        set_edit_excl_preset_temp_cache.set(p_tc);
                                                                        set_edit_excl_preset_installers.set(p_inst);
                                                                        set_edit_excl_preset_dev_deps.set(p_dev);
                                                                        set_edit_excl_preset_os_meta.set(p_os);
                                                                        set_edit_excl_custom_rules.set(custom);
                                                                        set_edit_excl_globs.set(globs);
                                                                        set_edit_excl_error.set(None);
                                                                        set_edit_excl_advanced_expanded.set(false);
                                                                        set_editing_exclusions_id.set(Some(config_id));
                                                                    }
                                                                >"Edit"</button>
                                                            </div>
                                                        }.into_any()
                                                    }
                                                }}
                                            </div>
                                        </div>
                                    </div>

                                    // Actions row: Enable/Disable with inline confirm
                                    <div class="mt-4 pt-3 border-t border-elevation-1-border">
                                        {move || {
                                            let is_confirming = confirming_id.get() == Some(config_id);
                                            if is_confirming {
                                                let label = if is_active { "Confirm Disable?" } else { "Confirm Enable?" };
                                                let btn_class = if is_active {
                                                    "text-xs font-medium px-3 py-1.5 rounded-lg bg-error-tint text-error hover:bg-error-border transition-all"
                                                } else {
                                                    "text-xs font-medium px-3 py-1.5 rounded-lg bg-accent-tint text-accent hover:bg-accent-tint transition-all"
                                                };
                                                view! {
                                                    <button
                                                        class=btn_class
                                                        on:click=move |_| {
                                                            set_confirming_id.set(None);
                                                            spawn_local(async move {
                                                                let req = ToggleBackupConfigRequest {
                                                                    id: config_id,
                                                                    is_active: !is_active,
                                                                };
                                                                if let Ok(_) = tauri_invoke::<_, ToggleBackupConfigResponse>(
                                                                    "toggle_backup_config",
                                                                    "request",
                                                                    req,
                                                                ).await {
                                                                    if let Ok(resp) = tauri_invoke_no_args::<Vec<DecryptedBackupConfigWithRemoteStorage>>(
                                                                        "list_all_backup_configs",
                                                                    ).await {
                                                                        let _ = set_configs.try_set(resp);
                                                                    }
                                                                }
                                                            });
                                                        }
                                                        on:focusout=move |_| set_confirming_id.set(None)
                                                    >
                                                        {label}
                                                    </button>
                                                }.into_any()
                                            } else {
                                                let label = if is_active { "Disable" } else { "Enable" };
                                                let btn_class = if is_active {
                                                    "text-xs font-medium px-3 py-1.5 rounded-lg text-text-secondary hover:bg-error-tint hover:text-error transition-all"
                                                } else {
                                                    "text-xs font-medium px-3 py-1.5 rounded-lg text-text-secondary hover:bg-accent-tint hover:text-accent transition-all"
                                                };
                                                view! {
                                                    <button
                                                        class=btn_class
                                                        on:click=move |_| {
                                                            set_confirming_id.set(Some(config_id));
                                                            let window = web_sys::window().unwrap();
                                                            let cb = wasm_bindgen::closure::Closure::once(move || {
                                                                if confirming_id.get_untracked() == Some(config_id) {
                                                                    set_confirming_id.set(None);
                                                                }
                                                            });
                                                            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                                                                cb.as_ref().unchecked_ref(),
                                                                3000,
                                                            );
                                                            cb.forget();
                                                        }
                                                    >
                                                        {label}
                                                    </button>
                                                }.into_any()
                                            }
                                        }}
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}

            {move || show_checkout_modal.get().then(|| {
                let tier = upgrade_tier.get();
                let starting_tier = current_tier.get();
                view! {
                    <CheckoutPolicyModal
                        on_close=Callback::new(move |accepted: bool| {
                            set_show_checkout_modal.set(false);
                            if accepted {
                                spawn_local(async move {
                                    match tauri_invoke::<_, String>("open_checkout", "tier", tier).await {
                                        Ok(_) => {
                                            set_checkout_polling.set(true);
                                            set_checkout_banner.set(Some("Payment opened in browser — waiting for confirmation…".to_string()));
                                            spawn_local(async move {
                                                poll_subscription_for_upgrade(
                                                    starting_tier,
                                                    set_checkout_polling,
                                                    set_checkout_banner,
                                                    set_max_configs_limit,
                                                    set_current_tier,
                                                ).await;
                                            });
                                        }
                                        Err(e) => {
                                            set_checkout_banner.set(Some(format!("Failed to open checkout: {}", e.user_message())));
                                        }
                                    }
                                });
                            }
                        })
                    />
                }
            })}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// INFRASTRUCTURE TAB — Devices + Storage
// ═══════════════════════════════════════════════════════════════════

#[component]
fn InfrastructureTab() -> impl IntoView {
    view! {
        <div class="space-y-6">
            <DevicesSection />
            <StorageSection />
        </div>
    }
}

// ── Devices Section ────────────────────────────────────────────────

#[component]
fn DevicesSection() -> impl IntoView {
    let (session, _) = use_context::<SessionSignal>().unwrap();
    let (devices, set_devices) = signal(Vec::<DeviceSummary>::new());
    let (loading, set_loading) = signal(true);

    spawn_local(async move {
        if let Ok(resp) = tauri_invoke_no_args::<ListAllDevicesResponse>("list_all_devices").await {
            let _ = set_devices.try_set(resp.devices);
        }
        let _ = set_loading.try_set(false);
    });

    view! {
        <div class="glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                    </svg>
                </div>
                <h3 class="text-lg font-semibold">"Devices"</h3>
            </div>

            {move || {
                if loading.get() {
                    return view! { <p class="text-text-secondary text-sm">"Loading..."</p> }.into_any();
                }

                let physical_device_id = session.get().physical_device_id.clone();
                let mut list = devices.get();

                if list.is_empty() {
                    return view! { <p class="text-text-secondary text-sm">"No devices registered yet."</p> }.into_any();
                }

                list.sort_by(|a, b| {
                    let a_current = a.physical_device_id == physical_device_id;
                    let b_current = b.physical_device_id == physical_device_id;
                    b_current.cmp(&a_current)
                });

                view! {
                    <div class="space-y-2">
                        {list.into_iter().map(|d| {
                            let is_current = d.physical_device_id == physical_device_id;
                            let name = d.display_name.clone().unwrap_or_else(|| d.physical_device_id.clone());
                            let created = format_local_date(&d.created_at);
                            view! {
                                <div class={if is_current {
                                    "flex items-center justify-between py-3 px-4 bg-primary-tint border border-primary-glow rounded-xl"
                                } else {
                                    "flex items-center justify-between py-3 px-4 bg-bg/30 border border-border rounded-xl"
                                }}>
                                    <div class="min-w-0">
                                        <div class="flex items-center gap-2 flex-wrap">
                                            <span class="font-medium text-sm">{name}</span>
                                            {if is_current {
                                                view! {
                                                    <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-success-tint text-success">
                                                        <span class="w-1.5 h-1.5 rounded-full bg-success"></span>
                                                        "This device"
                                                    </span>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <span class="inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-medium bg-bg text-text-secondary">
                                                        "Registered"
                                                    </span>
                                                }.into_any()
                                            }}
                                        </div>
                                        <div class="flex items-center gap-1.5 text-xs text-text-secondary mt-0.5">
                                            <span>{format!("{} \u{00b7} {}", d.platform, masked_uuid(&d.id))}</span>
                                            {let full_id = d.id.to_string(); view! {
                                                <button
                                                    class="text-text-secondary/60 hover:text-text-primary transition-colors"
                                                    title="Copy full device ID"
                                                    aria-label="Copy full device ID"
                                                    on:click=move |_| copy_to_clipboard(&full_id)
                                                >
                                                    <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
                                                </button>
                                            }}
                                        </div>
                                    </div>
                                    <div class="text-right flex-shrink-0 ml-3">
                                        <div class="text-xs text-text-secondary">"Added"</div>
                                        <div class="text-xs font-medium">{created}</div>
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Storage Section ────────────────────────────────────────────────

#[component]
fn StorageSection() -> impl IntoView {
    let (storages, set_storages) = signal(Vec::<RemoteStorageSummary>::new());
    let (loading, set_loading) = signal(true);
    let (show_form, set_show_form) = signal(false);
    let (form_error, set_form_error) = signal(Option::<String>::None);
    let (form_loading, set_form_loading) = signal(false);
    let (test_status, set_test_status) = signal(Option::<Result<String, String>>::None);
    let (testing, set_testing) = signal(false);

    let (provider, set_provider) = signal("s3".to_string());

    // S3 form fields
    let (storage_name, set_storage_name) = signal(String::new());
    let (access_key, set_access_key) = signal(String::new());
    let (secret, set_secret) = signal(String::new());
    let (region, set_region) = signal("us-east-1".to_string());
    let (bucket, set_bucket) = signal(String::new());

    // Google Drive form fields
    let (gdrive_name, set_gdrive_name) = signal(String::new());
    let (onedrive_name, set_onedrive_name) = signal(String::new());

    // Local filesystem form fields
    let (local_name, set_local_name) = signal(String::new());
    let (local_path, set_local_path) = signal(String::new());

    let refresh_storages = move || {
        spawn_local(async move {
            let _ = set_loading.try_set(true);
            if let Ok(resp) =
                tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages").await
            {
                let _ = set_storages.try_set(resp.list);
            }
            let _ = set_loading.try_set(false);
        });
    };

    refresh_storages();

    let on_add_s3 = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_form_loading.set(true);
        set_form_error.set(None);

        spawn_local(async move {
            let req = AddRemoteStorageRequest {
                storage_name: storage_name.get_untracked(),
                s3_access_key: access_key.get_untracked(),
                s3_secret: secret.get_untracked(),
                s3_region: region.get_untracked(),
                s3_bucket: bucket.get_untracked(),
            };

            match tauri_invoke::<_, CreateRemoteStorageResponse>(
                "add_remote_storage",
                "request",
                req,
            )
            .await
            {
                Ok(_) => {
                    let _ = set_show_form.try_set(false);
                    let _ = set_storage_name.try_set(String::new());
                    let _ = set_access_key.try_set(String::new());
                    let _ = set_secret.try_set(String::new());
                    let _ = set_region.try_set("us-east-1".to_string());
                    let _ = set_bucket.try_set(String::new());
                    if let Ok(resp) =
                        tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages")
                            .await
                    {
                        let _ = set_storages.try_set(resp.list);
                    }
                }
                Err(e) => {
                    let _ = set_form_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_form_loading.try_set(false);
        });
    };

    let on_add_gdrive = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_form_loading.set(true);
        set_form_error.set(None);

        spawn_local(async move {
            match tauri_invoke::<_, CreateRemoteStorageResponse>(
                "add_google_drive_storage",
                "storage_name",
                gdrive_name.get_untracked(),
            )
            .await
            {
                Ok(_) => {
                    let _ = set_show_form.try_set(false);
                    let _ = set_gdrive_name.try_set(String::new());
                    if let Ok(resp) =
                        tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages")
                            .await
                    {
                        let _ = set_storages.try_set(resp.list);
                    }
                }
                Err(e) => {
                    let _ = set_form_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_form_loading.try_set(false);
        });
    };

    let on_add_onedrive = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_form_loading.set(true);
        set_form_error.set(None);

        spawn_local(async move {
            match tauri_invoke::<_, CreateRemoteStorageResponse>(
                "add_onedrive_storage",
                "storage_name",
                onedrive_name.get_untracked(),
            )
            .await
            {
                Ok(_) => {
                    let _ = set_show_form.try_set(false);
                    let _ = set_onedrive_name.try_set(String::new());
                    if let Ok(resp) =
                        tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages")
                            .await
                    {
                        let _ = set_storages.try_set(resp.list);
                    }
                }
                Err(e) => {
                    let _ = set_form_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_form_loading.try_set(false);
        });
    };

    let browse_local_path = move |_| {
        spawn_local(async move {
            match tauri_invoke_no_args::<Option<String>>("pick_directory").await {
                Ok(Some(path)) => set_local_path.set(path),
                _ => {}
            }
        });
    };

    let test_s3_connection = move |_: leptos::ev::MouseEvent| {
        set_testing.set(true);
        set_test_status.set(None);

        spawn_local(async move {
            let req = TestS3ConnectionRequest {
                s3_access_key: access_key.get_untracked(),
                s3_secret: secret.get_untracked(),
                s3_region: region.get_untracked(),
                s3_bucket: bucket.get_untracked(),
            };

            match tauri_invoke::<_, bool>("test_s3_connection", "request", req).await {
                Ok(_) => {
                    let _ = set_test_status.try_set(Some(Ok("s3".to_string())));
                }
                Err(e) => {
                    let _ = set_test_status.try_set(Some(Err(map_storage_error(&e.to_string()))));
                }
            }
            let _ = set_testing.try_set(false);
        });
    };

    let test_local_connection = move |_: leptos::ev::MouseEvent| {
        set_testing.set(true);
        set_test_status.set(None);

        spawn_local(async move {
            let req = TestLocalConnectionRequest {
                root_path: local_path.get_untracked(),
            };

            match tauri_invoke::<_, bool>("test_local_connection", "request", req).await {
                Ok(_) => {
                    let _ = set_test_status.try_set(Some(Ok("local".to_string())));
                }
                Err(e) => {
                    let _ = set_test_status.try_set(Some(Err(map_storage_error(&e.to_string()))));
                }
            }
            let _ = set_testing.try_set(false);
        });
    };

    let on_submit_local = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_form_loading.set(true);
        set_form_error.set(None);

        spawn_local(async move {
            let req = AddLocalStorageRequest {
                storage_name: local_name.get_untracked(),
                root_path: local_path.get_untracked(),
            };

            match tauri_invoke::<_, CreateRemoteStorageResponse>(
                "add_local_storage",
                "request",
                req,
            )
            .await
            {
                Ok(_) => {
                    let _ = set_show_form.try_set(false);
                    let _ = set_local_name.try_set(String::new());
                    let _ = set_local_path.try_set(String::new());
                    if let Ok(resp) =
                        tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages")
                            .await
                    {
                        let _ = set_storages.try_set(resp.list);
                    }
                }
                Err(e) => {
                    let _ = set_form_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_form_loading.try_set(false);
        });
    };

    let input_class = "w-full bg-input border border-border rounded-xl px-4 py-2.5 text-text-primary text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all";
    let label_class = "block text-sm font-medium text-text-secondary mb-1.5";

    view! {
        <div class="glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl p-6">
            <div class="flex items-center justify-between mb-4">
                <div class="flex items-center gap-3">
                    <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                        <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <ellipse cx="12" cy="5" rx="9" ry="3"/>
                            <path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/>
                            <path d="M3 12c0 1.66 4.03 3 9 3s9-1.34 9-3"/>
                        </svg>
                    </div>
                    <h3 class="text-lg font-semibold">"Remote Storages"</h3>
                </div>
                <button
                    class="text-sm font-medium text-primary-light hover:text-primary transition-colors"
                    on:click=move |_| set_show_form.update(|v| *v = !*v)
                >
                    {move || if show_form.get() { "Cancel" } else { "+ Add Storage" }}
                </button>
            </div>

            // Add storage form
            {move || show_form.get().then(|| {
                let current_provider = provider.get();
                view! {
                    <div class="mb-6 p-4 bg-bg/50 border border-border rounded-xl space-y-3">
                        // Provider selector
                        <div>
                            <label class=label_class>"Choose backup storage"</label>
                            <p class="text-xs text-text-secondary mb-2">"Pick where encrypted backups will be stored."</p>
                            <div class="flex gap-2">
                                <button
                                    type="button"
                                    class=move || if provider.get() == "s3" {
                                        "flex-1 py-2 px-3 rounded-lg bg-primary-tint text-primary-light font-medium text-sm border border-primary-glow transition-all"
                                    } else {
                                        "flex-1 py-2 px-3 rounded-lg text-text-secondary text-sm border border-border hover:border-primary-glow transition-all"
                                    }
                                    on:click=move |_| {
                                        set_provider.set("s3".to_string());
                                        set_form_error.set(None);
                                        set_test_status.set(None);
                                    }
                                >
                                    "Amazon S3"
                                </button>
                                <button
                                    type="button"
                                    class=move || if provider.get() == "gdrive" {
                                        "flex-1 py-2 px-3 rounded-lg bg-primary-tint text-primary-light font-medium text-sm border border-primary-glow transition-all"
                                    } else {
                                        "flex-1 py-2 px-3 rounded-lg text-text-secondary text-sm border border-border hover:border-primary-glow transition-all"
                                    }
                                    on:click=move |_| {
                                        set_provider.set("gdrive".to_string());
                                        set_form_error.set(None);
                                        set_test_status.set(None);
                                    }
                                >
                                    "Google Drive"
                                </button>
                                <button
                                    type="button"
                                    class=move || if provider.get() == "onedrive" {
                                        "flex-1 py-2 px-3 rounded-lg bg-primary-tint text-primary-light font-medium text-sm border border-primary-glow transition-all"
                                    } else {
                                        "flex-1 py-2 px-3 rounded-lg text-text-secondary text-sm border border-border hover:border-primary-glow transition-all"
                                    }
                                    on:click=move |_| {
                                        set_provider.set("onedrive".to_string());
                                        set_form_error.set(None);
                                        set_test_status.set(None);
                                    }
                                >
                                    "OneDrive"
                                </button>
                                <button
                                    type="button"
                                    class=move || if provider.get() == "sftp" {
                                        "flex-1 py-2 px-3 rounded-lg bg-primary-tint text-primary-light font-medium text-sm border border-primary-glow transition-all"
                                    } else {
                                        "flex-1 py-2 px-3 rounded-lg text-text-secondary text-sm border border-border hover:border-primary-glow transition-all"
                                    }
                                    on:click=move |_| {
                                        set_provider.set("sftp".to_string());
                                        set_form_error.set(None);
                                        set_test_status.set(None);
                                    }
                                >
                                    "SFTP Server"
                                </button>
                                <button
                                    type="button"
                                    class=move || if provider.get() == "local" {
                                        "flex-1 py-2 px-3 rounded-lg bg-primary-tint text-primary-light font-medium text-sm border border-primary-glow transition-all"
                                    } else {
                                        "flex-1 py-2 px-3 rounded-lg text-text-secondary text-sm border border-border hover:border-primary-glow transition-all"
                                    }
                                    on:click=move |_| {
                                        set_provider.set("local".to_string());
                                        set_form_error.set(None);
                                        set_test_status.set(None);
                                    }
                                >
                                    "External Folder"
                                </button>
                            </div>
                        </div>

                        // S3 form
                        {(current_provider == "s3").then(|| view! {
                            <form on:submit=on_add_s3 class="space-y-3">
                                <div>
                                    <label class=label_class>"Storage Name"</label>
                                    <input type="text" class=input_class placeholder="My S3 Bucket"
                                        prop:value=move || storage_name.get()
                                        on:input=move |ev| set_storage_name.set(event_target_value(&ev)) required />
                                </div>
                                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                    <div>
                                        <label class=label_class>"Access Key"</label>
                                        <input type="text" class=input_class placeholder="AKIA..."
                                            prop:value=move || access_key.get()
                                            on:input=move |ev| set_access_key.set(event_target_value(&ev)) required />
                                        <p class="text-xs text-text-secondary mt-1">"Used only to authenticate writes to your bucket."</p>
                                    </div>
                                    <div>
                                        <label class=label_class>"Secret Key"</label>
                                        <input type="password" class=input_class placeholder="secret"
                                            prop:value=move || secret.get()
                                            on:input=move |ev| set_secret.set(event_target_value(&ev)) required />
                                        <p class="text-xs text-text-secondary mt-1">"Stored encrypted on this device."</p>
                                    </div>
                                </div>
                                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                    <div>
                                        <label class=label_class>"Region"</label>
                                        <input type="text" class=input_class
                                            prop:value=move || region.try_get().unwrap_or_default()
                                            on:input=move |ev| set_region.set(event_target_value(&ev)) required />
                                        <p class="text-xs text-text-secondary mt-1">"Must match the bucket's region."</p>
                                    </div>
                                    <div>
                                        <label class=label_class>"Bucket"</label>
                                        <input type="text" class=input_class placeholder="my-bucket"
                                            prop:value=move || bucket.get()
                                            on:input=move |ev| set_bucket.set(event_target_value(&ev)) required />
                                        <p class="text-xs text-text-secondary mt-1">"CloudLess will only write inside this bucket."</p>
                                    </div>
                                </div>

                                <div>
                                    <button
                                        type="button"
                                        class="text-sm font-medium text-accent hover:text-accent-light transition-colors disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                                        on:click=test_s3_connection
                                        disabled=move || testing.try_get().unwrap_or(false)
                                    >
                                        {move || if testing.try_get().unwrap_or(false) { "Testing..." } else { "Test Connection" }}
                                    </button>
                                    {move || test_status.try_get().flatten().map(|r| match r {
                                        Ok(_) => view! {
                                            <div class="mt-2 space-y-1">
                                                <p class="text-xs text-accent">"✓ Connection verified"</p>
                                                <p class="text-xs text-accent">"✓ Write access confirmed"</p>
                                                <p class="text-xs text-accent">"✓ Bucket reachable"</p>
                                            </div>
                                        }.into_any(),
                                        Err(e) => view! {
                                            <p class="mt-2 text-sm text-error">{e}</p>
                                        }.into_any(),
                                    })}
                                </div>

                                {move || form_error.try_get().flatten().map(|e| view! {
                                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                                })}

                                <button type="submit"
                                    class="btn-gradient text-white px-6 py-2 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint disabled:opacity-50"
                                    disabled=move || form_loading.try_get().unwrap_or(false)
                                >
                                    {move || if form_loading.try_get().unwrap_or(false) { "Adding..." } else { "Add Storage" }}
                                </button>
                            </form>
                        })}

                        // Google Drive form
                        {(current_provider == "gdrive").then(|| view! {
                            <form on:submit=on_add_gdrive class="space-y-3">
                                <div>
                                    <label class=label_class>"Storage Name"</label>
                                    <input type="text" class=input_class placeholder="My Google Drive"
                                        on:input=move |ev| set_gdrive_name.set(event_target_value(&ev)) required />
                                </div>

                                <p class="text-xs text-text-secondary">
                                    "Clicking Connect will open Google's consent screen in your browser. "
                                    "CloudLess will only request access to files it creates (drive.file scope)."
                                </p>

                                {move || form_error.try_get().flatten().map(|e| view! {
                                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                                })}

                                <button type="submit"
                                    class="btn-gradient text-white px-6 py-2 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint disabled:opacity-50"
                                    disabled=move || form_loading.try_get().unwrap_or(false)
                                >
                                    {move || if form_loading.try_get().unwrap_or(false) { "Connecting..." } else { "Connect Google Drive" }}
                                </button>
                            </form>
                        })}

                        // Microsoft OneDrive form
                        {(current_provider == "onedrive").then(|| view! {
                            <form on:submit=on_add_onedrive class="space-y-3">
                                <div>
                                    <label class=label_class>"Storage Name"</label>
                                    <input type="text" class=input_class placeholder="My OneDrive"
                                        on:input=move |ev| set_onedrive_name.set(event_target_value(&ev)) required />
                                </div>

                                <p class="text-xs text-text-secondary">
                                    "Clicking Connect will open Microsoft's consent screen in your browser. "
                                    "CloudLess will only request access to its own OneDrive app folder."
                                </p>

                                {move || form_error.try_get().flatten().map(|e| view! {
                                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                                })}

                                <button type="submit"
                                    class="btn-gradient text-white px-6 py-2 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint disabled:opacity-50"
                                    disabled=move || form_loading.try_get().unwrap_or(false)
                                >
                                    {move || if form_loading.try_get().unwrap_or(false) { "Connecting..." } else { "Connect OneDrive" }}
                                </button>
                            </form>
                        })}

                        // SFTP form
                        {(current_provider == "sftp").then(|| view! {
                            <SftpStorageForm
                                id_prefix="settings-sftp"
                                submit_label="Add Storage"
                                loading_label="Adding..."
                                on_success=Callback::new(move |_resp: SftpCreateRemoteStorageResponse| {
                                    let _ = set_show_form.try_set(false);
                                    spawn_local(async move {
                                        if let Ok(resp) =
                                            tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages")
                                                .await
                                        {
                                            let _ = set_storages.try_set(resp.list);
                                        }
                                    });
                                })
                            />
                        })}

                        // Local filesystem form
                        {(current_provider == "local").then(|| view! {
                            <form on:submit=on_submit_local class="space-y-3">
                                <div>
                                    <label class=label_class>"Storage Name"</label>
                                    <input type="text" id="settings-local-storage-name" class=input_class placeholder="My Local Backup"
                                        prop:value=move || local_name.get()
                                        on:input=move |ev| set_local_name.set(event_target_value(&ev)) required />
                                </div>
                                <div>
                                    <label class=label_class>"Storage Directory"</label>
                                    <div class="flex gap-2">
                                        <input type="text" id="settings-local-root-path" class=input_class placeholder="/mnt/external/backups"
                                            prop:value=move || local_path.get()
                                            on:input=move |ev| set_local_path.set(event_target_value(&ev)) required />
                                        <button
                                            type="button"
                                            class="flex-shrink-0 px-3 py-2 bg-primary-tint border border-primary-glow rounded-xl text-primary text-sm font-medium hover:bg-primary-tint transition-all"
                                            on:click=browse_local_path
                                        >
                                            "Browse"
                                        </button>
                                    </div>
                                    <p class="text-xs text-text-secondary mt-1">"Choose a folder on an external drive, NAS mount, or separate disk."</p>
                                </div>

                                <p class="text-xs text-warning">
                                    "Warning: Backing up to the same disk as your source files does not protect against disk failure. "
                                    "Use an external or network-attached drive for best results."
                                </p>

                                {move || form_error.try_get().flatten().map(|e| view! {
                                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                                })}

                                <div>
                                    <button
                                        type="button"
                                        class="text-sm font-medium text-accent hover:text-accent-light transition-colors disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                                        on:click=test_local_connection
                                        disabled=move || testing.try_get().unwrap_or(false)
                                    >
                                        {move || if testing.try_get().unwrap_or(false) { "Testing..." } else { "Test Connection" }}
                                    </button>
                                    {move || test_status.try_get().flatten().map(|r| match r {
                                        Ok(_) => view! {
                                            <div class="mt-2 space-y-1">
                                                <p class="text-xs text-accent">"✓ Connection verified"</p>
                                                <p class="text-xs text-accent">"✓ Path accessible"</p>
                                                <p class="text-xs text-accent">"✓ Write access confirmed"</p>
                                            </div>
                                        }.into_any(),
                                        Err(e) => view! {
                                            <p class="mt-2 text-sm text-error">{e}</p>
                                        }.into_any(),
                                    })}
                                </div>

                                <div class="flex gap-2">
                                    <button type="submit"
                                        class="btn-gradient text-white px-6 py-2 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint disabled:opacity-50"
                                        disabled=move || form_loading.try_get().unwrap_or(false)
                                    >
                                        {move || if form_loading.try_get().unwrap_or(false) { "Adding..." } else { "Add Storage" }}
                                    </button>
                                </div>
                            </form>
                        })}
                    </div>
                }
            })}

            // Storage list
            {move || {
                if loading.get() {
                    view! { <p class="text-text-secondary text-sm">"Loading..."</p> }.into_any()
                } else {
                    let list = storages.get();
                    if list.is_empty() {
                        view! { <p class="text-text-secondary text-sm">"No remote storages registered yet."</p> }.into_any()
                    } else {
                        view! {
                            <div class="space-y-2">
                                {list.into_iter().map(|s| {
                                    let icon = storage_type_icon(&s.storage_type);
                                    let created = format_local_date(&s.created_at);
                                    let is_expired = s.status == RemoteStorageStatus::AuthTokenExpired;
                                    let is_google_drive = matches!(s.storage_type, RemoteStorageType::GoogleDrive);
                                    let is_onedrive = matches!(s.storage_type, RemoteStorageType::OneDrive);
                                    let is_oauth_storage = is_google_drive || is_onedrive;
                                    let storage_id = s.id;

                                    // Border and icon styling changes for expired storages
                                    let card_class = if is_expired {
                                        "flex flex-col gap-2 py-3 px-4 bg-error-tint/20 border border-error-border rounded-xl"
                                    } else {
                                        "flex flex-col gap-2 py-3 px-4 bg-bg/30 border border-border rounded-xl"
                                    };
                                    let icon_bg = if is_expired {
                                        "w-8 h-8 rounded-lg bg-error-tint flex items-center justify-center flex-shrink-0 text-error"
                                    } else {
                                        "w-8 h-8 rounded-lg bg-primary-tint flex items-center justify-center flex-shrink-0 text-primary"
                                    };

                                    view! {
                                        <div class=card_class>
                                            <div class="flex items-center justify-between">
                                                <div class="flex items-center gap-3 min-w-0">
                                                    <div class=icon_bg
                                                        inner_html=icon
                                                    ></div>
                                                    <div class="min-w-0">
                                                        <div class="flex items-center gap-2">
                                                            <span class="font-medium text-sm">{s.name.clone()}</span>
                                                            {is_expired.then(|| view! {
                                                                <span class="text-xs font-medium text-error bg-error-tint px-2 py-0.5 rounded-full">"Auth Expired"</span>
                                                            })}
                                                        </div>
                                                        <div class="flex items-center gap-1.5 text-xs text-text-secondary">
                                                            <span>{format!("{} \u{00b7} {}", s.storage_type, masked_uuid(&s.id))}</span>
                                                            {let full_id = s.id.to_string(); view! {
                                                                <button
                                                                    class="text-text-secondary/60 hover:text-text-primary transition-colors"
                                                                    title="Copy full storage ID"
                                                                    aria-label="Copy full storage ID"
                                                                    on:click=move |_| copy_to_clipboard(&full_id)
                                                                >
                                                                    <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
                                                                </button>
                                                            }}
                                                        </div>
                                                    </div>
                                                </div>
                                                <div class="text-right flex-shrink-0 ml-3">
                                                    <div class="text-xs text-text-secondary">"Added"</div>
                                                    <div class="text-xs font-medium">{created}</div>
                                                </div>
                                            </div>

                                            // Expired storage: reconnect button + help text
                                            {(is_expired && is_oauth_storage).then(|| {
                                                let set_storages = set_storages.clone();
                                                let (reauth_loading, set_reauth_loading) = signal(false);
                                                let (reauth_error, set_reauth_error) = signal(Option::<String>::None);
                                                let command_name = if is_onedrive {
                                                    "reauth_onedrive_storage"
                                                } else {
                                                    "reauth_google_drive_storage"
                                                };
                                                let provider_label = if is_onedrive {
                                                    "OneDrive"
                                                } else {
                                                    "Google Drive"
                                                };

                                                view! {
                                                    <div class="pl-11 space-y-2">
                                                        <p class="text-xs text-warning">
                                                            {format!("Your {} authorization has expired. Please reconnect using the same account to prevent restore failures.", provider_label)}
                                                        </p>

                                                        {move || reauth_error.get().map(|e| view! {
                                                            <div class="bg-error-tint border border-error-border text-error rounded-lg px-3 py-1.5 text-xs">{e}</div>
                                                        })}

                                                        <button
                                                            class="text-sm font-medium text-primary-light hover:text-primary bg-primary-tint px-4 py-1.5 rounded-lg transition-colors disabled:opacity-50"
                                                            disabled=move || reauth_loading.get()
                                                            on:click=move |_| {
                                                                set_reauth_loading.set(true);
                                                                set_reauth_error.set(None);
                                                                let set_storages = set_storages.clone();
                                                                spawn_local(async move {
                                                                    match tauri_invoke::<_, ReauthRemoteStorageResponse>(
                                                                        command_name,
                                                                        "storage_id",
                                                                        storage_id.to_string(),
                                                                    ).await {
                                                                        Ok(_) => {
                                                                            // Refresh the storage list to show updated status
                                                                            if let Ok(resp) = tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages").await {
                                                                                let _ = set_storages.try_set(resp.list);
                                                                            }
                                                                        }
                                                                        Err(e) => {
                                                                            let _ = set_reauth_error.try_set(Some(e.to_string()));
                                                                        }
                                                                    }
                                                                    let _ = set_reauth_loading.try_set(false);
                                                                });
                                                            }
                                                        >
                                                            {move || if reauth_loading.get() { "Reconnecting...".to_string() } else { format!("Reconnect {}", provider_label) }}
                                                        </button>
                                                    </div>
                                                }
                                            })}
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// SECURITY TAB — Account + Encryption
// ═══════════════════════════════════════════════════════════════════

#[component]
fn SecurityTab() -> impl IntoView {
    view! {
        <div class="space-y-6">
            <EncryptionSection />
            <BackupSecurityStats />
            <DeviceSecuritySection />
            <AccountSection />
        </div>
    }
}

// ── Encryption Section ─────────────────────────────────────────────

const ICON_LOCK_LARGE: &str = r#"<svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>"#;

const ICON_CHECK_CIRCLE: &str = r#"<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>"#;

/// Which modal is currently showing in the encryption section.
#[derive(Clone, PartialEq)]
enum EncryptionModal {
    None,
    ExportRecoveryKey,
    ChangeRecoveryKey,
    ChangePassword,
}

/// Response type matching Tauri's ExportRecoveryKeyResponse.
#[derive(Debug, Clone, Deserialize)]
struct ExportRecoveryKeyResponse {
    pub recovery_key: String,
}

/// Request type matching Tauri's ChangeEncryptionPasswordRequest.
#[derive(Debug, Clone, Serialize)]
struct ChangeEncryptionPasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Mirrors the Tauri BiometricStatusResponse for deserialization.
#[derive(Debug, Clone, Deserialize)]
struct BiometricStatusResponse {
    pub is_available: bool,
}

#[component]
fn EncryptionSection() -> impl IntoView {
    let (has_rk, set_has_rk) = signal(false);
    let (rk_loading, set_rk_loading) = signal(true);
    let (active_modal, set_active_modal) = signal(EncryptionModal::None);

    // Biometric unlock state
    let (bio_available, set_bio_available) = signal(false);
    let (bio_enabled, set_bio_enabled) = signal(false);
    let (bio_loading, set_bio_loading) = signal(true);
    let (bio_action_loading, set_bio_action_loading) = signal(false);
    let (bio_error, set_bio_error) = signal(Option::<String>::None);

    // Check if recovery key exists on load
    spawn_local(async move {
        match tauri_invoke_no_args::<bool>("has_recovery_key").await {
            Ok(exists) => {
                let _ = set_has_rk.try_set(exists);
            }
            Err(_) => {}
        }
        let _ = set_rk_loading.try_set(false);
    });

    // Check biometric availability + enabled state on load
    spawn_local(async move {
        let available = tauri_invoke_no_args::<BiometricStatusResponse>("is_biometric_available")
            .await
            .map(|s| s.is_available)
            .unwrap_or(false);
        let _ = set_bio_available.try_set(available);

        if available {
            let enabled = tauri_invoke_no_args::<bool>("is_biometric_enabled")
                .await
                .unwrap_or(false);
            let _ = set_bio_enabled.try_set(enabled);
        }

        let _ = set_bio_loading.try_set(false);
    });

    view! {
        <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-6">
            // Header
            <div class="flex items-center gap-4 mb-5">
                <div class="w-14 h-14 rounded-2xl bg-primary-tint flex items-center justify-center text-primary"
                    inner_html=ICON_LOCK_LARGE
                ></div>
                <div>
                    <h3 class="text-lg font-semibold">"End-to-End Encryption"</h3>
                    <p class="text-sm text-text-secondary">"Your data is encrypted before it leaves this device. The server never sees your plaintext."</p>
                </div>
            </div>

            // Status rows
            <div class="border-t border-elevation-1-border pt-4 space-y-3">
                <EncryptionRow label="Algorithm" value="AES-256-GCM" />
                <EncryptionRow label="Key Derivation" value="Argon2id" />
                <EncryptionRow label="Key Storage" value="Local Device Only" />
                <EncryptionRow label="Key Status" value="Active" />
                {move || {
                    if rk_loading.get() {
                        view! {
                            <div class="flex items-center justify-between py-1.5">
                                <span class="text-sm text-text-secondary">"Recovery Key"</span>
                                <span class="text-sm text-text-secondary">"Checking..."</span>
                            </div>
                        }.into_any()
                    } else if has_rk.get() {
                        let verified_label = read_local_storage("cloudless_rk_verified_at")
                            .and_then(|iso| {
                                chrono::DateTime::parse_from_rfc3339(&iso).ok()
                            })
                            .map(|dt| format!("Verified {}", format_local_date(&dt.with_timezone(&chrono::Utc))))
                            .unwrap_or_else(|| "Exported".to_string());
                        view! {
                            <div class="flex items-center justify-between py-1.5">
                                <span class="text-sm text-text-secondary">"Recovery Key"</span>
                                <div class="flex items-center gap-2">
                                    <span class="text-sm font-medium">{verified_label}</span>
                                    <span class="text-success" inner_html=ICON_CHECK_CIRCLE></span>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="flex items-center justify-between py-1.5">
                                <span class="text-sm text-text-secondary">"Recovery Key"</span>
                                <div class="flex items-center gap-2">
                                    <span class="text-sm font-medium text-warning">"Not exported"</span>
                                    <svg class="w-4 h-4 text-warning" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
                                        <line x1="12" y1="9" x2="12" y2="13"/>
                                        <line x1="12" y1="17" x2="12.01" y2="17"/>
                                    </svg>
                                </div>
                            </div>
                        }.into_any()
                    }
                }}
                // Biometric Unlock status row (shown only when hardware is available)
                {move || {
                    if bio_loading.get() {
                        view! {
                            <div class="flex items-center justify-between py-1.5">
                                <span class="text-sm text-text-secondary">"Biometric Unlock"</span>
                                <span class="text-sm text-text-secondary">"Checking..."</span>
                            </div>
                        }.into_any()
                    } else if bio_available.get() {
                        if bio_enabled.get() {
                            view! {
                                <div class="flex items-center justify-between py-1.5">
                                    <span class="text-sm text-text-secondary">"Biometric Unlock"</span>
                                    <div class="flex items-center gap-2">
                                        <span class="text-sm font-medium">"Enabled"</span>
                                        <span class="text-success" inner_html=ICON_CHECK_CIRCLE></span>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="flex items-center justify-between py-1.5">
                                    <span class="text-sm text-text-secondary">"Biometric Unlock"</span>
                                    <span class="text-sm font-medium text-text-secondary">"Available"</span>
                                </div>
                            }.into_any()
                        }
                    } else {
                        // Not available on this device — hide the row entirely
                        ().into_any()
                    }
                }}
            </div>

            // Actions
            <div class="border-t border-elevation-1-border mt-4 pt-4">
                <h4 class="text-sm font-medium text-text-secondary mb-3">"Actions"</h4>
                <div class="flex flex-wrap gap-2">
                    <button
                        class="px-4 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                        on:click=move |_| set_active_modal.set(EncryptionModal::ExportRecoveryKey)
                    >
                        {move || if has_rk.get() { "Show Recovery Key" } else { "Save Recovery Key" }}
                    </button>
                    {move || has_rk.get().then(|| view! {
                        <button
                            class="px-4 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                            on:click=move |_| set_active_modal.set(EncryptionModal::ChangeRecoveryKey)
                        >
                            "Rotate Recovery Key"
                        </button>
                    })}
                    <button
                        class="px-4 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                        on:click=move |_| set_active_modal.set(EncryptionModal::ChangePassword)
                    >
                        "Change Password"
                    </button>
                    // Biometric enable/disable button (shown only when hardware is available)
                    {move || {
                        if !bio_available.get() || bio_loading.get() {
                            return ().into_any();
                        }
                        if bio_enabled.get() {
                            view! {
                                <button
                                    class="px-4 py-2 rounded-xl text-sm font-medium border border-error-border text-error hover:bg-error-tint transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                                    disabled=move || bio_action_loading.get()
                                    on:click=move |_| {
                                        set_bio_action_loading.set(true);
                                        let _ = set_bio_error.try_set(None);
                                        spawn_local(async move {
                                            match tauri_invoke_no_args::<()>("disable_biometric_unlock").await {
                                                Ok(_) => {
                                                    let _ = set_bio_enabled.try_set(false);
                                                }
                                                Err(e) => {
                                                    let _ = set_bio_error.try_set(Some(format!("Failed to disable: {}", e)));
                                                }
                                            }
                                            let _ = set_bio_action_loading.try_set(false);
                                        });
                                    }
                                >
                                    {move || if bio_action_loading.get() { "Disabling..." } else { "Disable Biometric Unlock" }}
                                </button>
                            }.into_any()
                        } else {
                            view! {
                                <button
                                    class="px-4 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                                    disabled=move || bio_action_loading.get()
                                    on:click=move |_| {
                                        set_bio_action_loading.set(true);
                                        let _ = set_bio_error.try_set(None);
                                        spawn_local(async move {
                                            match tauri_invoke_no_args::<()>("enable_biometric_unlock").await {
                                                Ok(_) => {
                                                    let _ = set_bio_enabled.try_set(true);
                                                }
                                                Err(e) => {
                                                    let _ = set_bio_error.try_set(Some(format!("Failed to enable: {}", e)));
                                                }
                                            }
                                            let _ = set_bio_action_loading.try_set(false);
                                        });
                                    }
                                >
                                    {move || if bio_action_loading.get() { "Enabling..." } else { "Enable Biometric Unlock" }}
                                </button>
                            }.into_any()
                        }
                    }}
                </div>
            </div>

            // Biometric error
            {move || bio_error.get().map(|e| view! {
                <div class="mt-3 flex items-center gap-2 p-3 bg-error-tint border border-error-border rounded-xl">
                    <p class="text-xs text-error">{e}</p>
                </div>
            })}

            // Warning (only shown if no recovery key)
            {move || (!rk_loading.get() && !has_rk.get()).then(|| view! {
                <div class="mt-4 flex items-start gap-2 p-3 bg-warning-tint border border-warning-tint rounded-xl">
                    <svg class="w-4 h-4 text-warning flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
                        <line x1="12" y1="9" x2="12" y2="13"/>
                        <line x1="12" y1="17" x2="12.01" y2="17"/>
                    </svg>
                    <p class="text-xs text-warning">"Recovery key not saved. If you forget your password and lose this device, your backup cannot be unlocked."</p>
                </div>
            })}
        </div>

        // Modals
        {move || match active_modal.get() {
            EncryptionModal::ExportRecoveryKey => view! {
                <RecoveryKeyModal
                    is_rotation=false
                    on_close=Callback::new(move |exported| {
                        set_active_modal.set(EncryptionModal::None);
                        if exported {
                            write_local_storage("cloudless_rk_verified_at", &js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default());
                            set_has_rk.set(true);
                        }
                    })
                />
            }.into_any(),
            EncryptionModal::ChangeRecoveryKey => view! {
                <RecoveryKeyModal
                    is_rotation=true
                    on_close=Callback::new(move |_| {
                        set_active_modal.set(EncryptionModal::None);
                    })
                />
            }.into_any(),
            EncryptionModal::ChangePassword => view! {
                <ChangePasswordModal
                    on_close=Callback::new(move |_| {
                        set_active_modal.set(EncryptionModal::None);
                    })
                />
            }.into_any(),
            EncryptionModal::None => ().into_any(),
        }}
    }
}

// ── Recovery Key Modal (Export / Rotate) ────────────────────────────

/// Phases of the recovery key export/rotate flow.
#[derive(Clone, PartialEq)]
enum RecoveryKeyPhase {
    /// Generating the key (loading state).
    Generating,
    /// Phase 1: Display the key for user to copy.
    Display,
    /// Phase 2: User confirms by re-entering the key.
    Confirm,
    /// Phase 3: Success.
    Success,
}

/// Copies text to clipboard using the Clipboard API.
fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(navigator) = js_sys::Reflect::get(&window, &JsValue::from_str("navigator")) {
            if let Ok(clipboard) = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard"))
            {
                let _ = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText")).and_then(
                    |write_fn| {
                        let write_fn: js_sys::Function = write_fn.unchecked_into();
                        write_fn.call1(&clipboard, &JsValue::from_str(text))
                    },
                );
            }
        }
    }
}

#[component]
fn RecoveryKeyModal(
    is_rotation: bool,
    /// Callback with `true` if key was exported successfully, `false` if cancelled.
    on_close: Callback<bool>,
) -> impl IntoView {
    let (phase, set_phase) = signal(RecoveryKeyPhase::Generating);
    let (recovery_key, set_recovery_key) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (confirm_input, set_confirm_input) = signal(String::new());
    let (confirm_error, set_confirm_error) = signal(Option::<String>::None);
    let (copied, set_copied) = signal(false);
    let (acknowledged, set_acknowledged) = signal(false);
    let (confirming_rotation, set_confirming_rotation) = signal(false);

    // If rotation, show confirmation dialog first
    if is_rotation {
        set_confirming_rotation.set(true);
        set_phase.set(RecoveryKeyPhase::Generating);
    }

    let generate_key = move |cmd: &'static str| {
        set_error.set(None);
        spawn_local(async move {
            match tauri_invoke_no_args::<ExportRecoveryKeyResponse>(cmd).await {
                Ok(resp) => {
                    let _ = set_recovery_key.try_set(resp.recovery_key);
                    let _ = set_phase.try_set(RecoveryKeyPhase::Display);
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
        });
    };

    // For non-rotation, start generating immediately
    if !is_rotation {
        generate_key("export_recovery_key");
    }

    let title = if is_rotation {
        "Rotate Recovery Key"
    } else {
        "Save Recovery Key"
    };

    /// Normalizes a recovery key string for comparison: uppercase, strip non-alphanumeric.
    fn normalize_rk(s: &str) -> String {
        s.to_uppercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect()
    }

    view! {
        // Modal overlay
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
            <div class="bg-surface border border-border rounded-2xl shadow-2xl w-full max-w-lg max-h-[90vh] overflow-y-auto">
                // Header
                <div class="flex items-center justify-between p-6 pb-0">
                    <h3 class="text-lg font-semibold">{title}</h3>
                    <button
                        class="w-8 h-8 rounded-lg flex items-center justify-center text-text-secondary hover:bg-bg hover:text-text-primary transition-all"
                        on:click=move |_| on_close.run(false)
                    >
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <line x1="18" y1="6" x2="6" y2="18"/>
                            <line x1="6" y1="6" x2="18" y2="18"/>
                        </svg>
                    </button>
                </div>

                <div class="p-6 space-y-4">
                    // Rotation confirmation
                    {move || (is_rotation && confirming_rotation.get()).then(|| view! {
                        <div class="space-y-4">
                            <div class="flex items-start gap-2 p-3 bg-warning-tint border border-warning-tint rounded-xl">
                                <svg class="w-4 h-4 text-warning flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                    <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
                                    <line x1="12" y1="9" x2="12" y2="13"/>
                                    <line x1="12" y1="17" x2="12.01" y2="17"/>
                                </svg>
                                <p class="text-xs text-warning">"This will invalidate your current recovery key. You will need to save a new one. Your existing backups will not be affected."</p>
                            </div>
                            <div class="flex gap-3">
                                <button
                                    class="flex-1 py-2.5 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-bg transition-all"
                                    on:click=move |_| on_close.run(false)
                                >
                                    "Cancel"
                                </button>
                                <button
                                    class="flex-1 py-2.5 rounded-xl text-sm font-medium bg-warning text-white hover:opacity-90 transition-all"
                                    on:click=move |_| {
                                        set_confirming_rotation.set(false);
                                        generate_key("rotate_recovery_key");
                                    }
                                >
                                    "Continue"
                                </button>
                            </div>
                        </div>
                    })}

                    // Error display
                    {move || error.get().map(|e| view! {
                        <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                    })}

                    // Phase: Generating
                    {move || (phase.get() == RecoveryKeyPhase::Generating && !confirming_rotation.get()).then(|| view! {
                        <div class="flex items-center justify-center py-8">
                            <div class="text-center">
                                <div class="animate-spin w-8 h-8 border-2 border-primary border-t-transparent rounded-full mx-auto mb-3"></div>
                                <p class="text-sm text-text-secondary">"Generating recovery key..."</p>
                            </div>
                        </div>
                    })}

                    // Phase 1: Display
                    {move || (phase.get() == RecoveryKeyPhase::Display).then(|| {
                        let key = recovery_key.get();
                        let key_for_copy = key.clone();
                        let key_for_download = key.clone();
                        view! {
                            <div class="space-y-4">
                                <p class="text-sm text-text-secondary">
                                    "Write down or save this recovery key somewhere safe. You will need to confirm it in the next step."
                                </p>

                                // Recovery key display
                                <div class="bg-bg border border-border rounded-xl p-4">
                                    <pre class="font-mono text-sm text-text-primary whitespace-pre-wrap break-all leading-relaxed select-all">{key}</pre>
                                </div>

                                // Copy + Download row
                                <div class="flex gap-2">
                                    <button
                                        class=move || if copied.get() {
                                            "flex-1 py-2.5 rounded-xl text-sm font-medium bg-success-tint text-success border border-success transition-all"
                                        } else {
                                            "flex-1 py-2.5 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                                        }
                                        on:click=move |_| {
                                            copy_to_clipboard(&key_for_copy);
                                            set_copied.set(true);
                                            let window = web_sys::window().unwrap();
                                            let cb = wasm_bindgen::closure::Closure::once(move || {
                                                let _ = set_copied.try_set(false);
                                            });
                                            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                                                cb.as_ref().unchecked_ref(),
                                                2000,
                                            );
                                            cb.forget();
                                        }
                                    >
                                        {move || if copied.get() { "Copied — paste it somewhere safe" } else { "Copy to Clipboard" }}
                                    </button>
                                    <button
                                        class="px-4 py-2.5 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                                        aria-label="Download recovery key as text file"
                                        on:click=move |_| {
                                            download_text_file("cloudless-recovery-key.txt", &key_for_download);
                                        }
                                    >
                                        "Download .txt"
                                    </button>
                                </div>

                                // Warning
                                <div class="flex items-start gap-2 p-3 bg-warning-tint border border-warning-tint rounded-xl">
                                    <svg class="w-4 h-4 text-warning flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
                                        <line x1="12" y1="9" x2="12" y2="13"/>
                                        <line x1="12" y1="17" x2="12.01" y2="17"/>
                                    </svg>
                                    <p class="text-xs text-warning">
                                        "You must remember this password. If you lose it and do not have your recovery key, your backup cannot be decrypted."
                                    </p>
                                </div>

                                // Acknowledgment checkbox
                                <label class="flex items-start gap-3 cursor-pointer">
                                    <input
                                        type="checkbox"
                                        class="mt-0.5 w-4 h-4 rounded border-border text-primary focus:ring-primary-tint cursor-pointer"
                                        prop:checked=move || acknowledged.get()
                                        on:change=move |ev| {
                                            use wasm_bindgen::JsCast;
                                            let checked = ev.target()
                                                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                                .map(|i| i.checked())
                                                .unwrap_or(false);
                                            set_acknowledged.set(checked);
                                        }
                                    />
                                    <span class="text-sm text-text-secondary">
                                        "I understand that CloudLess cannot recover this key for me."
                                    </span>
                                </label>

                                // Continue button — disabled until acknowledged
                                <button
                                    class="w-full btn-gradient text-white py-2.5 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint disabled:opacity-50 disabled:cursor-not-allowed"
                                    disabled=move || !acknowledged.get()
                                    on:click=move |_| {
                                        set_confirm_input.set(String::new());
                                        set_confirm_error.set(None);
                                        set_phase.set(RecoveryKeyPhase::Confirm);
                                    }
                                >
                                    "I've saved it — Continue"
                                </button>
                            </div>
                        }
                    })}

                    // Phase 2: Confirm
                    {move || (phase.get() == RecoveryKeyPhase::Confirm).then(|| view! {
                        <div class="space-y-4">
                            <p class="text-sm text-text-secondary">
                                "Enter your recovery key below to confirm you've saved it correctly."
                            </p>

                            <textarea
                                class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary text-sm font-mono focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all resize-none"
                                rows=3
                                placeholder="CLRK-XXXX-XXXX-..."
                                prop:value=move || confirm_input.get()
                                on:input=move |ev| {
                                    set_confirm_input.set(event_target_value(&ev));
                                    set_confirm_error.set(None);
                                }
                            ></textarea>

                            {move || confirm_error.get().map(|e| view! {
                                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                            })}

                            <div class="flex gap-3">
                                <button
                                    class="flex-1 py-2.5 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-bg transition-all"
                                    on:click=move |_| set_phase.set(RecoveryKeyPhase::Display)
                                >
                                    "Back"
                                </button>
                                <button
                                    class="flex-1 btn-gradient text-white py-2.5 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint"
                                    on:click=move |_| {
                                        let input = normalize_rk(&confirm_input.get());
                                        let expected = normalize_rk(&recovery_key.get());
                                        if input == expected {
                                            set_phase.set(RecoveryKeyPhase::Success);
                                        } else {
                                            set_confirm_error.set(Some("Recovery key does not match. Please try again.".to_string()));
                                        }
                                    }
                                >
                                    "Verify & Save"
                                </button>
                            </div>
                        </div>
                    })}

                    // Phase 3: Success
                    {move || (phase.get() == RecoveryKeyPhase::Success).then(|| view! {
                        <div class="space-y-4 text-center py-4">
                            <div class="w-16 h-16 rounded-full bg-success-tint flex items-center justify-center mx-auto">
                                <svg class="w-8 h-8 text-success" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                    <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                                    <polyline points="22 4 12 14.01 9 11.01"/>
                                </svg>
                            </div>
                            <div>
                                <h4 class="text-lg font-semibold">"Recovery key verified and saved"</h4>
                                <p class="text-sm text-text-secondary mt-1">"You can use this key to recover your data if you forget your password."</p>
                            </div>
                            <button
                                class="w-full btn-gradient text-white py-2.5 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint"
                                on:click=move |_| on_close.run(true)
                            >
                                "Done"
                            </button>
                        </div>
                    })}
                </div>
            </div>
        </div>
    }
}

// ── Change Password Modal ──────────────────────────────────────────

#[component]
fn ChangePasswordModal(on_close: Callback<()>) -> impl IntoView {
    let (current_password, set_current_password) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    let (confirm_password, set_confirm_password) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let (success, set_success) = signal(false);
    let (show_current, set_show_current) = signal(false);
    let (show_new, set_show_new) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_error.set(None);

        let new_pw = new_password.get_untracked();
        let confirm_pw = confirm_password.get_untracked();

        if new_pw != confirm_pw {
            set_error.set(Some("New passwords do not match.".to_string()));
            return;
        }

        if new_pw.is_empty() {
            set_error.set(Some("New password cannot be empty.".to_string()));
            return;
        }

        set_loading.set(true);

        let current_pw = current_password.get_untracked();

        spawn_local(async move {
            let req = ChangeEncryptionPasswordRequest {
                current_password: current_pw,
                new_password: new_pw,
            };
            match tauri_invoke::<_, ()>("change_encryption_password", "request", req).await {
                Ok(()) => {
                    let _ = set_success.try_set(true);
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    let password_toggle_btn = move |show: ReadSignal<bool>, set_show: WriteSignal<bool>| {
        view! {
            <button
                type="button"
                class="absolute right-3 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text-primary transition-colors"
                on:click=move |_| set_show.set(!show.get_untracked())
                tabindex=-1
            >
                {move || if show.get() {
                    view! {
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 11-4.24-4.24"/>
                            <line x1="1" y1="1" x2="23" y2="23"/>
                        </svg>
                    }.into_any()
                } else {
                    view! {
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                            <circle cx="12" cy="12" r="3"/>
                        </svg>
                    }.into_any()
                }}
            </button>
        }
    };

    let input_class = "w-full bg-input border border-border rounded-xl px-4 py-2.5 pr-10 text-text-primary text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all";

    view! {
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
            <div class="bg-surface border border-border rounded-2xl shadow-2xl w-full max-w-md">
                // Header
                <div class="flex items-center justify-between p-6 pb-0">
                    <h3 class="text-lg font-semibold">"Change Encryption Password"</h3>
                    <button
                        class="w-8 h-8 rounded-lg flex items-center justify-center text-text-secondary hover:bg-bg hover:text-text-primary transition-all"
                        on:click=move |_| on_close.run(())
                    >
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <line x1="18" y1="6" x2="6" y2="18"/>
                            <line x1="6" y1="6" x2="18" y2="18"/>
                        </svg>
                    </button>
                </div>

                <div class="p-6">
                    {move || if success.get() {
                        view! {
                            <div class="space-y-4 text-center py-4">
                                <div class="w-16 h-16 rounded-full bg-success-tint flex items-center justify-center mx-auto">
                                    <svg class="w-8 h-8 text-success" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                                        <polyline points="22 4 12 14.01 9 11.01"/>
                                    </svg>
                                </div>
                                <div>
                                    <h4 class="text-lg font-semibold">"Password changed"</h4>
                                    <p class="text-sm text-text-secondary mt-1">"Your encryption password has been updated successfully."</p>
                                </div>
                                <button
                                    class="w-full btn-gradient text-white py-2.5 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint"
                                    on:click=move |_| on_close.run(())
                                >
                                    "Done"
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <form on:submit=on_submit class="space-y-4">
                                <p class="text-sm text-text-secondary">
                                    "Enter your current password and choose a new one. Your recovery key (if exported) will remain valid."
                                </p>

                                <div>
                                    <label class="block text-sm font-medium text-text-secondary mb-1.5">"Current Password"</label>
                                    <div class="relative">
                                        <input
                                            type=move || if show_current.get() { "text" } else { "password" }
                                            class=input_class
                                            placeholder="Enter current password"
                                            prop:value=move || current_password.get()
                                            on:input=move |ev| set_current_password.set(event_target_value(&ev))
                                            required
                                        />
                                        {password_toggle_btn(show_current, set_show_current)}
                                    </div>
                                </div>

                                <div>
                                    <label class="block text-sm font-medium text-text-secondary mb-1.5">"New Password"</label>
                                    <div class="relative">
                                        <input
                                            type=move || if show_new.get() { "text" } else { "password" }
                                            class=input_class
                                            placeholder="Enter new password"
                                            prop:value=move || new_password.get()
                                            on:input=move |ev| set_new_password.set(event_target_value(&ev))
                                            required
                                        />
                                        {password_toggle_btn(show_new, set_show_new)}
                                    </div>
                                </div>

                                <div>
                                    <label class="block text-sm font-medium text-text-secondary mb-1.5">"Confirm New Password"</label>
                                    <div class="relative">
                                        <input
                                            type=move || if show_new.get() { "text" } else { "password" }
                                            class=input_class
                                            placeholder="Confirm new password"
                                            prop:value=move || confirm_password.get()
                                            on:input=move |ev| set_confirm_password.set(event_target_value(&ev))
                                            required
                                        />
                                    </div>
                                </div>

                                {move || error.get().map(|e| view! {
                                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                                })}

                                <div class="flex gap-3 pt-2">
                                    <button
                                        type="button"
                                        class="flex-1 py-2.5 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-bg transition-all"
                                        on:click=move |_| on_close.run(())
                                    >
                                        "Cancel"
                                    </button>
                                    <button
                                        type="submit"
                                        class="flex-1 btn-gradient text-white py-2.5 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint disabled:opacity-50"
                                        disabled=move || loading.get()
                                    >
                                        {move || if loading.get() { "Changing..." } else { "Change Password" }}
                                    </button>
                                </div>
                            </form>
                        }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

#[component]
fn EncryptionRow(label: &'static str, value: &'static str) -> impl IntoView {
    view! {
        <div class="flex items-center justify-between py-1.5">
            <span class="text-sm text-text-secondary">{label}</span>
            <div class="flex items-center gap-2">
                <span class="text-sm font-medium">{value}</span>
                <span class="text-success" inner_html=ICON_CHECK_CIRCLE></span>
            </div>
        </div>
    }
}

// ── Backup Security Stats ──────────────────────────────────────────

#[component]
fn BackupSecurityStats() -> impl IntoView {
    let (stats, set_stats) = signal(Option::<GetDashboardStatsResponse>::None);

    spawn_local(async move {
        if let Ok(resp) =
            tauri_invoke_no_args::<GetDashboardStatsResponse>("get_dashboard_stats").await
        {
            let _ = set_stats.try_set(Some(resp));
        }
    });

    view! {
        <div class="glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                    </svg>
                </div>
                <h3 class="text-lg font-semibold">"Backup Security"</h3>
            </div>
            {move || {
                match stats.get() {
                    None => view! {
                        <div class="space-y-3 animate-pulse">
                            {(0..4).map(|_| view! {
                                <div class="flex justify-between py-2">
                                    <div class="h-4 bg-bg rounded w-1/3"></div>
                                    <div class="h-4 bg-bg rounded w-1/4"></div>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any(),
                    Some(s) => {
                        let savings_pct = if s.total_original_bytes > 0 {
                            let saved = s.total_original_bytes - s.total_uploaded_bytes;
                            format!("{:.1}%", (saved as f64 / s.total_original_bytes as f64 * 100.0).max(0.0))
                        } else {
                            "--".to_string()
                        };

                        view! {
                            <div class="space-y-3">
                                <div class="flex justify-between py-2 border-b border-border">
                                    <span class="text-sm text-text-secondary">"Encrypted files"</span>
                                    <span class="text-sm font-medium">{s.total_files_protected.to_string()}</span>
                                </div>
                                <div class="flex justify-between py-2 border-b border-border">
                                    <span class="text-sm text-text-secondary">"Original data"</span>
                                    <span class="text-sm font-medium">{format_bytes(s.total_original_bytes as u64)}</span>
                                </div>
                                <div class="flex justify-between py-2 border-b border-border">
                                    <span class="text-sm text-text-secondary">"Storage used"</span>
                                    <span class="text-sm font-medium">{format_bytes(s.total_uploaded_bytes as u64)}</span>
                                </div>
                                <div class="flex justify-between py-2 border-b border-border">
                                    <span class="text-sm text-text-secondary">"Dedup savings"</span>
                                    <span class="text-sm font-medium">{savings_pct}</span>
                                </div>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

// ── Device Security Section ────────────────────────────────────────

#[component]
fn DeviceSecuritySection() -> impl IntoView {
    let (session, _) = use_context::<SessionSignal>().unwrap();

    view! {
        <div class="glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="5" y="2" width="14" height="20" rx="2" ry="2"/>
                        <line x1="12" y1="18" x2="12.01" y2="18"/>
                    </svg>
                </div>
                <h3 class="text-lg font-semibold">"This Device"</h3>
            </div>
            <div class="space-y-3">
                <div class="flex justify-between py-2 border-b border-border">
                    <span class="text-sm text-text-secondary">"Platform"</span>
                    <span class="text-sm font-medium capitalize">{move || session.get().device_platform.clone()}</span>
                </div>
                <div class="flex justify-between py-2 border-b border-border">
                    <span class="text-sm text-text-secondary">"Display name"</span>
                    <span class="text-sm font-medium">{move || {
                        session.get().device_display_name.clone().unwrap_or_else(|| "\u{2014}".to_string())
                    }}</span>
                </div>
                <div class="flex justify-between py-2 border-b border-border">
                    <span class="text-sm text-text-secondary">"Device ID"</span>
                    <span class="text-sm font-mono truncate ml-4" title=move || session.get().physical_device_id.clone()>
                        {move || {
                            let id = session.get().physical_device_id.clone();
                            if id.len() > 16 { format!("{}...", &id[..16]) } else { id }
                        }}
                    </span>
                </div>
                <div class="flex justify-between py-2 border-b border-border">
                    <span class="text-sm text-text-secondary">"Registered"</span>
                    <span class="text-sm font-medium">{move || {
                        if session.get().device_registered {
                            view! { <span class="text-success">"Yes"</span> }.into_any()
                        } else {
                            view! { <span class="text-warning">"No"</span> }.into_any()
                        }
                    }}</span>
                </div>
            </div>
        </div>
    }
}

// ── Account Section ────────────────────────────────────────────────

const WEBSITE_BASE_URL: &str = match option_env!("WEBSITE_BASE_URL") {
    Some(url) => url,
    None => "http://localhost:3000",
};

#[component]
fn AccountSection() -> impl IntoView {
    let (session, set_session) = use_context::<SessionSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (_, set_auth_mode) = use_context::<AuthModeSignal>().unwrap();

    let (subscription, set_subscription) = signal(Option::<SubscriptionResponse>::None);
    let (portal_error, set_portal_error) = signal(Option::<String>::None);

    spawn_local(async move {
        if let Ok(resp) = tauri_invoke_no_args::<SubscriptionResponse>("get_subscription").await {
            let _ = set_subscription.try_set(Some(resp));
        }
    });

    let on_lock = move |_: leptos::ev::MouseEvent| {
        set_auth_mode.set(AuthMode::UnlockEncryption);
        set_phase.set(AppPhase::Auth);
    };

    let on_logout = move |_: leptos::ev::MouseEvent| {
        set_session.set(SessionInfo::default());
        set_auth_mode.set(AuthMode::Login);
        set_phase.set(AppPhase::Auth);
    };

    view! {
        <div class="glass bg-elevation-2 border border-elevation-2-border shadow-elevation-2 rounded-2xl p-6">
            <h3 class="text-lg font-semibold mb-4">"Account"</h3>
            <div class="space-y-3">
                <div class="flex justify-between py-2 border-b border-border">
                    <span class="text-text-secondary text-sm">"Name"</span>
                    <span class="text-sm">{move || {
                        let s = session.get();
                        if s.user_name.is_empty() { "\u{2014}".to_string() } else { s.user_name.clone() }
                    }}</span>
                </div>
                <div class="flex justify-between py-2 border-b border-border">
                    <span class="text-text-secondary text-sm">"Email"</span>
                    <span class="text-sm">{move || {
                        let s = session.get();
                        if s.user_email.is_empty() { "\u{2014}".to_string() } else { s.user_email.clone() }
                    }}</span>
                </div>
                <div class="flex justify-between py-2 border-b border-border">
                    <span class="text-text-secondary text-sm">"User ID"</span>
                    <div class="flex items-center gap-1.5 text-sm font-mono">
                        {move || match session.get().user_id {
                            None => view! {
                                <span>"\u{2014}"</span>
                            }.into_any(),
                            Some(id) => {
                                let full_id = id.to_string();
                                let display = masked_uuid(&id);
                                view! {
                                    <span>{display}</span>
                                    <button
                                        class="text-text-secondary/60 hover:text-text-primary transition-colors"
                                        title="Copy full user ID"
                                        aria-label="Copy full user ID"
                                        on:click=move |_| copy_to_clipboard(&full_id)
                                    >
                                        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
                                    </button>
                                }.into_any()
                            }
                        }}
                    </div>
                </div>
                <div class="flex justify-between items-center py-2 border-b border-border">
                    <span class="text-text-secondary text-sm">"Plan"</span>
                    <div class="flex items-center gap-2">
                        {move || {
                            let Some(sub) = subscription.get() else {
                                return view! { <span class="text-sm text-text-secondary">"\u{2014}"</span> }.into_any();
                            };
                            // Show the effective tier as the plan name so expired/past-period
                            // users see their current access level, not a stale subscribed tier.
                            let tier_label = match sub.effective_tier {
                                SubscriptionTier::Free => "Free",
                                SubscriptionTier::Starter => "Starter",
                                SubscriptionTier::Pro => "Pro",
                                SubscriptionTier::Lifetime => "Lifetime",
                            };
                            // For gracefully-cancelled subscriptions (still within paid period),
                            // show an amber "Cancels at period end" badge rather than a red
                            // "Canceled" badge — access is still active until the period expires.
                            let is_graceful_cancel = sub.subscription.status == SubscriptionStatus::Canceled
                                && sub.subscription.cancel_at_period_end
                                && sub.effective_tier != SubscriptionTier::Free;
                            let status_badge = if is_graceful_cancel {
                                Some(("Cancels at period end", "bg-warning-tint text-warning border-warning-border"))
                            } else {
                                match sub.subscription.status {
                                    SubscriptionStatus::Active => None,
                                    SubscriptionStatus::OnHold => Some(("Payment issue", "bg-warning-tint text-warning border-warning-border")),
                                    SubscriptionStatus::PastDue => Some(("Past due", "bg-warning-tint text-warning border-warning-border")),
                                    SubscriptionStatus::Canceled => Some(("Canceled", "bg-error-tint text-error border-error-border")),
                                    SubscriptionStatus::Expired => Some(("Expired", "bg-error-tint text-error border-error-border")),
                                }
                            };
                            view! {
                                <div class="flex items-center gap-2">
                                    <span class="text-sm font-medium">{tier_label}</span>
                                    {status_badge.map(|(label, cls)| view! {
                                        <span class=format!("text-xs px-2 py-0.5 rounded-full border {}", cls)>{label}</span>
                                    })}
                                </div>
                            }.into_any()
                        }}
                        {move || {
                            // Hidden on self-hosted instances where billing is disabled.
                            // Use effective_tier (not raw tier) so expired users see "Upgrade"
                            // and cancellation-grace users keep "Manage Billing".
                            let Some(sub) = subscription.get() else {
                                return ().into_any();
                            };
                            if !sub.billing_enabled {
                                return ().into_any();
                            }
                            match sub.effective_tier {
                                SubscriptionTier::Free => view! {
                                    <a
                                        href=format!("{}/pricing", WEBSITE_BASE_URL)
                                        target="_blank"
                                        class="text-xs text-primary-light hover:underline"
                                    >
                                        "Upgrade"
                                    </a>
                                }.into_any(),
                                SubscriptionTier::Lifetime => ().into_any(),
                                _ => view! {
                                    <button
                                        class="text-xs text-primary-light hover:underline"
                                        on:click=move |_| {
                                            set_portal_error.set(None);
                                            spawn_local(async move {
                                                if let Err(e) = tauri_invoke_no_args::<()>("open_billing_portal").await {
                                                    let _ = set_portal_error.try_set(Some(e.to_string()));
                                                }
                                            });
                                        }
                                    >
                                        "Manage Billing"
                                    </button>
                                }.into_any(),
                            }
                        }}
                    </div>
                </div>
            </div>
            {move || portal_error.get().map(|e| view! {
                <p class="mt-2 text-xs text-error">{e}</p>
            })}

            // Lock & Sign Out actions
            <div class="mt-6 pt-4 border-t border-border flex flex-wrap gap-3">
                <button
                    class="flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-primary-tint hover:text-primary hover:border-primary-glow transition-all"
                    on:click=on_lock
                >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
                        <path d="M7 11V7a5 5 0 0110 0v4"/>
                    </svg>
                    "Lock"
                </button>
                <button
                    class="flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-error-tint hover:text-error hover:border-error-border transition-all"
                    on:click=on_logout
                >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4"/>
                        <polyline points="16 17 21 12 16 7"/>
                        <line x1="21" y1="12" x2="9" y2="12"/>
                    </svg>
                    "Sign Out"
                </button>
            </div>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// PREFERENCES TAB — Appearance
// ═══════════════════════════════════════════════════════════════════

#[component]
fn PreferencesTab() -> impl IntoView {
    view! {
        <div class="space-y-6">
            <AppearanceSection />
            <DateFormatSection />
            <DisplayDensitySection />
            <UpdateSection />
        </div>
    }
}

// ── Update Section ─────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
struct UpdateInfo {
    version: String,
    body: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(String, Option<String>), // (version, notes)
    Installing,
    Error(String),
}

#[component]
fn UpdateSection() -> impl IntoView {
    let (status, set_status) = signal(UpdateStatus::Idle);

    let check = move |_| {
        set_status.set(UpdateStatus::Checking);
        spawn_local(async move {
            match tauri_invoke_no_args::<Option<UpdateInfo>>("check_for_updates").await {
                Ok(Some(info)) => {
                    set_status.set(UpdateStatus::Available(info.version, info.body));
                }
                Ok(None) => {
                    set_status.set(UpdateStatus::UpToDate);
                }
                Err(e) => {
                    set_status.set(UpdateStatus::Error(e.to_string()));
                }
            }
        });
    };

    let install = move |_| {
        set_status.set(UpdateStatus::Installing);
        spawn_local(async move {
            match tauri_invoke_no_args::<()>("install_update").await {
                Ok(_) => {}
                Err(e) => {
                    set_status.set(UpdateStatus::Error(e.to_string()));
                }
            }
        });
    };

    view! {
        <div class="glass bg-elevation-3 border border-elevation-3-border shadow-elevation-3 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/>
                        <polyline points="7 10 12 15 17 10"/>
                        <line x1="12" y1="15" x2="12" y2="3"/>
                    </svg>
                </div>
                <div>
                    <h3 class="text-lg font-semibold">"Software Update"</h3>
                    <p class="text-xs text-text-secondary">"CloudLess v"{env!("CARGO_PKG_VERSION")}</p>
                </div>
            </div>

            {move || match status.get() {
                UpdateStatus::Idle | UpdateStatus::UpToDate => {
                    let label = if status.get() == UpdateStatus::UpToDate {
                        "You're up to date"
                    } else {
                        "Check for updates"
                    };
                    view! {
                        <div class="flex items-center justify-between">
                            {if status.get() == UpdateStatus::UpToDate {
                                view! {
                                    <p class="text-sm text-green-400">"You're up to date."</p>
                                }.into_any()
                            } else {
                                view! { <span /> }.into_any()
                            }}
                            <button
                                class="ml-auto px-4 py-2 text-sm font-medium rounded-xl border border-primary text-primary-light hover:bg-primary-tint transition-colors"
                                on:click=check
                            >
                                {label}
                            </button>
                        </div>
                    }.into_any()
                }
                UpdateStatus::Checking => view! {
                    <div class="flex items-center gap-2 text-sm text-text-secondary">
                        <span class="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin inline-block"></span>
                        "Checking for updates..."
                    </div>
                }.into_any(),
                UpdateStatus::Available(version, notes) => view! {
                    <div class="space-y-3">
                        <div class="p-3 rounded-xl bg-primary-tint border border-primary-ring text-sm">
                            <p class="font-medium text-primary-light">"Update available: v"{version.clone()}</p>
                            {notes.as_ref().map(|n| view! {
                                <p class="text-text-secondary mt-1 text-xs">{n.clone()}</p>
                            })}
                        </div>
                        <div class="flex gap-2">
                            <button
                                class="px-4 py-2 text-sm font-medium rounded-xl btn-gradient text-white shadow-lg shadow-primary-tint hover:shadow-xl transition-all"
                                on:click=install
                            >
                                "Install Now"
                            </button>
                            <button
                                class="px-4 py-2 text-sm font-medium rounded-xl border border-border text-text-secondary hover:text-text-primary transition-colors"
                                on:click=move |_| set_status.set(UpdateStatus::Idle)
                            >
                                "Later"
                            </button>
                        </div>
                    </div>
                }.into_any(),
                UpdateStatus::Installing => view! {
                    <div class="flex items-center gap-2 text-sm text-text-secondary">
                        <span class="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin inline-block"></span>
                        "Downloading and installing update... The app will restart shortly."
                    </div>
                }.into_any(),
                UpdateStatus::Error(msg) => view! {
                    <div class="flex items-center justify-between">
                        <p class="text-sm text-red-400">{msg}</p>
                        <button
                            class="px-4 py-2 text-sm font-medium rounded-xl border border-border text-text-secondary hover:text-text-primary transition-colors"
                            on:click=move |_| set_status.set(UpdateStatus::Idle)
                        >
                            "Retry"
                        </button>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}

// ── Appearance Section ─────────────────────────────────────────────

#[component]
fn AppearanceSection() -> impl IntoView {
    let (preference, set_preference) = signal(theme::get_theme_preference());

    let on_select = move |value: &'static str| {
        theme::set_theme(value);
        set_preference.set(value.to_string());
    };

    let btn_class = move |value: &str| {
        if preference.get() == value {
            "px-4 py-2 text-sm font-medium rounded-full bg-primary text-white transition-colors"
        } else {
            "px-4 py-2 text-sm font-medium rounded-full bg-surface-elevated text-text-secondary hover:text-text-primary transition-colors"
        }
    };

    view! {
        <div class="glass bg-elevation-3 border border-elevation-3-border shadow-elevation-3 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="5"/>
                        <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/>
                    </svg>
                </div>
                <h3 class="text-lg font-semibold">"Appearance"</h3>
            </div>

            <div class="flex items-center justify-between">
                <div>
                    <p class="text-sm font-medium">"Theme"</p>
                    <p class="text-xs text-text-secondary">"Choose your preferred color scheme"</p>
                </div>
                <div class="flex gap-1 bg-bg rounded-full p-1">
                    <button
                        class=move || btn_class("auto")
                        on:click=move |_| on_select("auto")
                    >
                        "Auto"
                    </button>
                    <button
                        class=move || btn_class("light")
                        on:click=move |_| on_select("light")
                    >
                        "Light"
                    </button>
                    <button
                        class=move || btn_class("dark")
                        on:click=move |_| on_select("dark")
                    >
                        "Dark"
                    </button>
                </div>
            </div>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════
// DATA MANAGEMENT TAB — Retention Settings + GC + GC History
// ═══════════════════════════════════════════════════════════════════

#[component]
fn DataManagementTab() -> impl IntoView {
    view! {
        <div class="space-y-6">
            <RetentionSettingsSection />
            <GarbageCollectionSection />
            <GcHistorySection />
        </div>
    }
}

// ── Retention Settings Section ─────────────────────────────────────

#[component]
fn RetentionSettingsSection() -> impl IntoView {
    let loading = RwSignal::new(true);
    let saving = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let retention_days = RwSignal::new(30i32);

    spawn_local(async move {
        match tauri_invoke_no_args::<GetRetentionSettingsResponse>("get_retention_settings").await {
            Ok(resp) => {
                let _ = retention_days.try_set(resp.bin_retention_days);
            }
            Err(e) => {
                let _ = error.try_set(Some(e.to_string()));
            }
        }
        let _ = loading.try_set(false);
    });

    let on_save = move |_| {
        saving.set(true);
        error.set(None);
        let days = retention_days.get_untracked();

        spawn_local(async move {
            match tauri_invoke::<_, ()>("update_retention_settings", "bin_retention_days", days)
                .await
            {
                Ok(()) => {}
                Err(e) => {
                    let _ = error.try_set(Some(e.to_string()));
                }
            }
            let _ = saving.try_set(false);
        });
    };

    let retention_options: Vec<(i32, &str)> = vec![
        (7, "7 days"),
        (14, "14 days"),
        (30, "30 days"),
        (60, "60 days"),
        (90, "90 days"),
        (180, "180 days"),
        (365, "365 days"),
    ];

    view! {
        <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="10" />
                        <polyline points="12 6 12 12 16 14" />
                    </svg>
                </div>
                <div>
                    <h3 class="text-lg font-semibold">"Bin Retention"</h3>
                    <p class="text-xs text-text-secondary">"How long deleted versions stay in the bin before permanent removal"</p>
                </div>
            </div>

            {move || loading.get().then(|| view! {
                <p class="text-text-secondary text-sm">"Loading..."</p>
            })}

            {move || (!loading.get()).then(|| {
                let current_days = retention_days.get();
                let options = retention_options.clone();
                view! {
                    <div class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium text-text-secondary mb-1.5">"Retention period"</label>
                            <div class="flex items-center gap-3">
                                <select
                                    class="flex-1 bg-input border border-border rounded-xl px-4 py-2.5 text-text-primary text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                                    on:change=move |ev| {
                                        let val: i32 = event_target_value(&ev).parse().unwrap_or(30);
                                        retention_days.set(val);
                                    }
                                >
                                    {options.into_iter().map(|(days, label)| {
                                        let selected = days == current_days;
                                        view! { <option value={days.to_string()} selected=selected>{label}</option> }
                                    }).collect_view()}
                                </select>
                                <button
                                    class="px-4 py-2.5 rounded-xl text-sm font-medium bg-primary-tint text-primary border border-primary-glow hover:bg-primary hover:text-white transition-all disabled:opacity-50"
                                    on:click=on_save
                                    disabled=move || saving.get()
                                >
                                    {move || if saving.get() { "Saving..." } else { "Save" }}
                                </button>
                            </div>
                        </div>

                        <p class="text-xs text-text-secondary">
                            "Versions moved to the bin will be permanently deleted after this period. Garbage collection must run to finalize deletion."
                        </p>

                        {move || error.get().map(|e| view! {
                            <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                        })}
                    </div>
                }
            })}
        </div>
    }
}

// ── Garbage Collection Section ─────────────────────────────────────

#[component]
fn GarbageCollectionSection() -> impl IntoView {
    let running = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let result = RwSignal::new(Option::<GcSummary>::None);

    let on_run_gc = move |_| {
        running.set(true);
        error.set(None);
        result.set(None);

        spawn_local(async move {
            match tauri_invoke_no_args::<GcSummary>("run_garbage_collection").await {
                Ok(summary) => {
                    let _ = result.try_set(Some(summary));
                }
                Err(e) => {
                    let _ = error.try_set(Some(e.to_string()));
                }
            }
            let _ = running.try_set(false);
        });
    };

    view! {
        <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <polyline points="3 6 5 6 21 6"/>
                        <path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/>
                    </svg>
                </div>
                <div>
                    <h3 class="text-lg font-semibold">"Garbage Collection"</h3>
                    <p class="text-xs text-text-secondary">"Permanently delete expired bin items and free storage space"</p>
                </div>
            </div>

            <div class="space-y-4">
                <button
                    class="w-full py-2.5 rounded-xl text-sm font-medium bg-primary-tint text-primary border border-primary-glow hover:bg-primary hover:text-white transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                    on:click=on_run_gc
                    disabled=move || running.get()
                >
                    {move || if running.get() {
                        "Running Garbage Collection..."
                    } else {
                        "Run Garbage Collection Now"
                    }}
                </button>

                {move || running.get().then(|| view! {
                    <div class="flex items-center gap-2 py-2 px-3 bg-primary-tint border border-primary-tint rounded-xl">
                        <div class="animate-spin w-4 h-4 border-2 border-primary border-t-transparent rounded-full"></div>
                        <span class="text-primary text-xs">"Collecting expired versions and cleaning up orphaned chunks..."</span>
                    </div>
                })}

                {move || result.get().map(|summary| {
                    let freed = format_bytes(summary.storage_freed_bytes as u64);
                    view! {
                        <div class="flex items-start gap-2 p-3 bg-success-tint border border-success rounded-xl">
                            <svg class="w-4 h-4 text-success flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                                <polyline points="22 4 12 14.01 9 11.01"/>
                            </svg>
                            <div class="text-xs text-success">
                                <p class="font-medium">"Garbage collection complete"</p>
                                <p class="mt-1">
                                    {format!(
                                        "{} version{} deleted, {} chunk{} removed, {} freed",
                                        summary.versions_deleted,
                                        if summary.versions_deleted == 1 { "" } else { "s" },
                                        summary.chunks_deleted,
                                        if summary.chunks_deleted == 1 { "" } else { "s" },
                                        freed,
                                    )}
                                </p>
                            </div>
                        </div>
                    }
                })}

                {move || error.get().map(|e| view! {
                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                })}
            </div>
        </div>
    }
}

// ── GC History Section ─────────────────────────────────────────────

#[component]
fn GcHistorySection() -> impl IntoView {
    let loading = RwSignal::new(true);
    let runs = RwSignal::new(Vec::<GcRunSummary>::new());
    let error = RwSignal::new(Option::<String>::None);

    spawn_local(async move {
        match tauri_invoke::<_, ListGcRunsResponse>("list_gc_runs", "limit", 10i32).await {
            Ok(resp) => {
                let _ = runs.try_set(resp.runs);
            }
            Err(e) => {
                let _ = error.try_set(Some(e.to_string()));
            }
        }
        let _ = loading.try_set(false);
    });

    view! {
        <div class="glass bg-elevation-1 border border-elevation-1-border shadow-elevation-1 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/>
                        <polyline points="14 2 14 8 20 8"/>
                        <line x1="16" y1="13" x2="8" y2="13"/>
                        <line x1="16" y1="17" x2="8" y2="17"/>
                    </svg>
                </div>
                <h3 class="text-lg font-semibold">"GC History"</h3>
            </div>

            {move || {
                if loading.get() {
                    return view! {
                        <div class="space-y-2">
                            {(0..3).map(|_| view! {
                                <div class="flex items-center justify-between py-3 px-4 bg-bg/30 border border-border rounded-xl animate-pulse">
                                    <div class="flex items-center gap-3">
                                        <div class="h-5 w-16 bg-bg rounded-full"></div>
                                        <div class="h-4 w-32 bg-bg rounded"></div>
                                    </div>
                                    <div class="h-4 w-24 bg-bg rounded"></div>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any();
                }

                if let Some(e) = error.get() {
                    return view! {
                        <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                    }.into_any();
                }

                let list = runs.get();
                if list.is_empty() {
                    return view! {
                        <p class="text-text-secondary text-sm">"No garbage collection runs yet."</p>
                    }.into_any();
                }

                view! {
                    <div class="space-y-2">
                        {list.into_iter().map(|run| {
                            let status_class = match run.status {
                                GcRunStatus::Completed => "bg-success-tint text-success",
                                GcRunStatus::Failed => "bg-error-tint text-error",
                                GcRunStatus::Running => "bg-warning-tint text-warning",
                            };
                            let status_label = match run.status {
                                GcRunStatus::Completed => "Completed",
                                GcRunStatus::Failed => "Failed",
                                GcRunStatus::Running => "Running",
                            };
                            let freed = format_bytes(run.storage_freed_bytes as u64);
                            let started = format_local_datetime(&run.started_at);
                            let duration = run.completed_at.map(|c| {
                                let diff = c - run.started_at;
                                let secs = diff.num_seconds();
                                if secs < 60 {
                                    format!("{}s", secs)
                                } else {
                                    format!("{}m {}s", secs / 60, secs % 60)
                                }
                            });

                            view! {
                                <div class="flex items-center justify-between py-3 px-4 bg-bg/30 border border-border rounded-xl">
                                    <div class="flex items-center gap-3 min-w-0">
                                        <span class={format!("inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-medium {}", status_class)}>
                                            {status_label}
                                        </span>
                                        <div class="min-w-0">
                                            <div class="text-sm font-medium">
                                                {format!(
                                                    "{} version{}, {} chunk{}",
                                                    run.versions_deleted,
                                                    if run.versions_deleted == 1 { "" } else { "s" },
                                                    run.chunks_deleted,
                                                    if run.chunks_deleted == 1 { "" } else { "s" },
                                                )}
                                            </div>
                                            <div class="text-xs text-text-secondary">
                                                {started}
                                                {duration.map(|d| format!(" ({d})"))}
                                            </div>
                                        </div>
                                    </div>
                                    <div class="text-right flex-shrink-0 ml-3">
                                        <div class="text-sm font-medium">{freed}</div>
                                        <div class="text-xs text-text-secondary">"freed"</div>
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── localStorage preference helpers ────────────────────────────────

fn get_pref(key: &str, default: &str) -> String {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|ls| ls.get_item(key).ok().flatten())
        .unwrap_or_else(|| default.to_string())
}

fn set_pref(key: &str, value: &str) {
    if let Some(ls) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = ls.set_item(key, value);
    }
}

// ── Date Format Section ────────────────────────────────────────────

#[component]
fn DateFormatSection() -> impl IntoView {
    let stored = get_pref("date_format_24h", "false");
    let (is_24h, set_is_24h) = signal(stored == "true");

    let toggle = move |_| {
        let new_val = !is_24h.get_untracked();
        set_is_24h.set(new_val);
        set_pref("date_format_24h", if new_val { "true" } else { "false" });
    };

    view! {
        <div class="glass bg-elevation-3 border border-elevation-3-border shadow-elevation-3 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="10"/>
                        <polyline points="12 6 12 12 16 14"/>
                    </svg>
                </div>
                <h3 class="text-lg font-semibold">"Date & Time"</h3>
            </div>

            <div class="flex items-center justify-between">
                <div>
                    <p class="text-sm font-medium">"Time format"</p>
                    <p class="text-xs text-text-secondary">"Choose 12-hour or 24-hour clock"</p>
                </div>
                <div class="flex gap-1 bg-bg rounded-full p-1">
                    <button
                        class=move || if !is_24h.get() {
                            "px-4 py-2 text-sm font-medium rounded-full bg-primary text-white transition-colors"
                        } else {
                            "px-4 py-2 text-sm font-medium rounded-full bg-surface-elevated text-text-secondary hover:text-text-primary transition-colors"
                        }
                        on:click=move |_| if is_24h.get_untracked() { toggle(()) }
                    >
                        "12h"
                    </button>
                    <button
                        class=move || if is_24h.get() {
                            "px-4 py-2 text-sm font-medium rounded-full bg-primary text-white transition-colors"
                        } else {
                            "px-4 py-2 text-sm font-medium rounded-full bg-surface-elevated text-text-secondary hover:text-text-primary transition-colors"
                        }
                        on:click=move |_| if !is_24h.get_untracked() { toggle(()) }
                    >
                        "24h"
                    </button>
                </div>
            </div>
        </div>
    }
}

// ── Display Density Section ────────────────────────────────────────

#[component]
fn DisplayDensitySection() -> impl IntoView {
    let stored = get_pref("file_display_density", "comfortable");
    let (density, set_density) = signal(stored);

    let on_select = move |value: &'static str| {
        set_density.set(value.to_string());
        set_pref("file_display_density", value);
    };

    let btn_class = move |value: &str| {
        if density.get() == value {
            "px-4 py-2 text-sm font-medium rounded-full bg-primary text-white transition-colors"
        } else {
            "px-4 py-2 text-sm font-medium rounded-full bg-surface-elevated text-text-secondary hover:text-text-primary transition-colors"
        }
    };

    view! {
        <div class="glass bg-elevation-3 border border-elevation-3-border shadow-elevation-3 rounded-2xl p-6">
            <div class="flex items-center gap-3 mb-4">
                <div class="w-10 h-10 rounded-xl bg-primary-tint flex items-center justify-center">
                    <svg class="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <line x1="8" y1="6" x2="21" y2="6"/>
                        <line x1="8" y1="12" x2="21" y2="12"/>
                        <line x1="8" y1="18" x2="21" y2="18"/>
                        <line x1="3" y1="6" x2="3.01" y2="6"/>
                        <line x1="3" y1="12" x2="3.01" y2="12"/>
                        <line x1="3" y1="18" x2="3.01" y2="18"/>
                    </svg>
                </div>
                <h3 class="text-lg font-semibold">"Display Density"</h3>
            </div>

            <div class="flex items-center justify-between">
                <div>
                    <p class="text-sm font-medium">"File list spacing"</p>
                    <p class="text-xs text-text-secondary">"Adjust the spacing between items in file lists"</p>
                </div>
                <div class="flex gap-1 bg-bg rounded-full p-1">
                    <button
                        class=move || btn_class("comfortable")
                        on:click=move |_| on_select("comfortable")
                    >
                        "Comfortable"
                    </button>
                    <button
                        class=move || btn_class("compact")
                        on:click=move |_| on_select("compact")
                    >
                        "Compact"
                    </button>
                </div>
            </div>
        </div>
    }
}
