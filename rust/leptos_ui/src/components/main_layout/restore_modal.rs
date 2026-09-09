use crate::components::common::{tauri_invoke, tauri_invoke_no_args};
use crate::state::*;
use api_types::remote_storage::{
    ListRemoteStoragesResponse, RemoteStorageStatus, RemoteStorageSummary,
};
use api_types::restore_job::{OverwriteBehavior, RestoreDestination};
use leptos::prelude::*;
use serde::Serialize;
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, Serialize)]
struct RestoreFileSelectionDto {
    pub version_id: Uuid,
    pub file_path: String,
    pub size: i64,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize)]
struct StartRestoreRequest {
    pub config_id: Uuid,
    pub file_selections: Vec<RestoreFileSelectionDto>,
    pub destination: RestoreDestination,
    pub overwrite_behavior: OverwriteBehavior,
}

/// Information about a file version selected for restore.
#[derive(Debug, Clone)]
pub struct RestoreSelection {
    pub version_id: Uuid,
    pub file_path: String,
    pub size: i64,
    pub version: u32,
}

#[component]
pub fn RestoreModal(
    config_id: Uuid,
    selections: Vec<RestoreSelection>,
    is_current_device: bool,
    /// Called with `true` when the restore job successfully starts, `false` when dismissed.
    on_close: Callback<bool>,
) -> impl IntoView {
    let (_, set_restore_progress) = use_context::<RestoreProgressSignal>().unwrap();
    let (error, set_error) = signal(Option::<String>::None);
    let (submitting, set_submitting) = signal(false);
    let (confirming, set_confirming) = signal(false);
    let default_dest = if is_current_device {
        "original"
    } else {
        "download"
    };
    let (destination, set_destination) = signal(default_dest.to_string());
    let (custom_path, set_custom_path) = signal(String::new());
    let (remote_storages, set_remote_storages) = signal(Vec::<RemoteStorageSummary>::new());
    let (selected_storage_id, set_selected_storage_id) = signal(String::new());
    let (overwrite, set_overwrite) = signal("overwrite".to_string());

    let total_files = selections.len();
    let total_size: i64 = selections.iter().map(|s| s.size).sum();
    let total_size_str = format_bytes(total_size as u64);
    let total_size_str_confirm = total_size_str.clone();

    // Build DTOs once, store for reuse across reactive closures
    let file_selection_dtos: Vec<RestoreFileSelectionDto> = selections
        .iter()
        .map(|s| RestoreFileSelectionDto {
            version_id: s.version_id,
            file_path: s.file_path.clone(),
            size: s.size,
            version: s.version,
        })
        .collect();
    let stored_dtos = StoredValue::new(file_selection_dtos);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(resp) =
                tauri_invoke_no_args::<ListRemoteStoragesResponse>("list_remote_storages").await
            {
                let active = resp
                    .list
                    .into_iter()
                    .filter(|s| s.status == RemoteStorageStatus::Active)
                    .collect::<Vec<_>>();
                if selected_storage_id.get_untracked().is_empty() {
                    if let Some(first) = active.first() {
                        set_selected_storage_id.set(first.id.to_string());
                    }
                }
                set_remote_storages.set(active);
            }
        });
    });

    let do_restore = move || {
        set_submitting.set(true);
        set_error.set(None);

        let dest = match destination.get_untracked().as_str() {
            "download" => RestoreDestination::DownloadFolder,
            "custom" => RestoreDestination::CustomPath(custom_path.get_untracked()),
            "configured_storage" => {
                let storage_id = Uuid::parse_str(&selected_storage_id.get_untracked())
                    .expect("validated configured storage id");
                RestoreDestination::RemoteStorage { storage_id }
            }
            _ => RestoreDestination::OriginalPath,
        };

        let overwrite_behavior = match overwrite.get_untracked().as_str() {
            "keep_both" => OverwriteBehavior::KeepBoth,
            "skip" => OverwriteBehavior::SkipIfExists,
            _ => OverwriteBehavior::Overwrite,
        };

        let req = StartRestoreRequest {
            config_id,
            file_selections: stored_dtos.get_value(),
            destination: dest,
            overwrite_behavior,
        };

        let set_rp = set_restore_progress;
        let on_close = on_close.clone();

        spawn_local(async move {
            match tauri_invoke::<_, ()>("start_restore_command", "request", req).await {
                Ok(_) => {
                    set_rp.update(|rp| {
                        rp.is_running = true;
                        rp.phase = "Starting...".to_string();
                        rp.completed_files = 0;
                        rp.restored_bytes = 0;
                    });
                    on_close.run(true);
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_submitting.try_set(false);
        });
    };

    view! {
        // Backdrop
        <div
            class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4"
            on:click=move |_| on_close.run(false)
        >
            // Modal
            <div
                class="glass bg-surface border border-border rounded-2xl w-full max-w-md animate-slide-up"
                on:click=move |ev: leptos::ev::MouseEvent| ev.stop_propagation()
            >
                // Header
                <div class="flex items-center justify-between p-5 border-b border-border">
                    <h3 class="text-lg font-semibold">
                        {move || if confirming.get() { "Confirm Restore" } else { "Restore Files" }}
                    </h3>
                    <button
                        class="text-text-secondary hover:text-text-primary transition-colors"
                        on:click=move |_| on_close.run(false)
                    >
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>

                {move || {
                    if confirming.get() {
                        // ── Phase 2: Confirmation Summary ──
                        let dest_label = match destination.get().as_str() {
                            "download" => "Downloads folder".to_string(),
                            "custom" => {
                                let p = custom_path.get();
                                if p.is_empty() { "Custom path".to_string() } else { p }
                            }
                            "configured_storage" => {
                                let selected = selected_storage_id.get();
                                remote_storages
                                    .get()
                                    .iter()
                                    .find(|s| s.id.to_string() == selected)
                                    .map(|s| format!("{} / cloudless-restored", s.name))
                                    .unwrap_or_else(|| "Configured storage / cloudless-restored".to_string())
                            }
                            _ => "Original location".to_string(),
                        };
                        let overwrite_label = match overwrite.get().as_str() {
                            "keep_both" => "Keep both (rename new)",
                            "skip" => "Skip existing",
                            _ => "Overwrite",
                        };
                        let is_overwrite = overwrite.get() == "overwrite";
                        let is_configured_storage = destination.get() == "configured_storage";
                        let total_size_str = total_size_str_confirm.clone();

                        view! {
                            <form on:submit=move |ev: leptos::ev::SubmitEvent| {
                                ev.prevent_default();
                                do_restore();
                            }>
                                <div class="p-5 space-y-4">
                                    // Summary rows
                                    <div class="space-y-3">
                                        <div class="flex justify-between py-2 border-b border-border">
                                            <span class="text-sm text-text-secondary">"Files"</span>
                                            <span class="text-sm font-medium">
                                                {format!("{} file{}", total_files, if total_files != 1 { "s" } else { "" })}
                                            </span>
                                        </div>
                                        <div class="flex justify-between py-2 border-b border-border">
                                            <span class="text-sm text-text-secondary">"Total size"</span>
                                            <span class="text-sm font-medium">{total_size_str}</span>
                                        </div>
                                        <div class="flex justify-between py-2 border-b border-border">
                                            <span class="text-sm text-text-secondary">"Destination"</span>
                                            <span class="text-sm font-medium truncate ml-4 text-right">{dest_label}</span>
                                        </div>
                                        <div class="flex justify-between py-2 border-b border-border">
                                            <span class="text-sm text-text-secondary">"If file exists"</span>
                                            <span class="text-sm font-medium">{overwrite_label}</span>
                                        </div>
                                    </div>

                                    // Warning for overwrite mode
                                    {if is_overwrite {
                                        view! {
                                            <div class="flex items-start gap-2 p-3 bg-warning-tint border border-warning-tint rounded-xl">
                                                <svg class="w-4 h-4 text-warning flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                    <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
                                                    <line x1="12" y1="9" x2="12" y2="13"/>
                                                    <line x1="12" y1="17" x2="12.01" y2="17"/>
                                                </svg>
                                                <p class="text-xs text-warning">"Existing files at the destination will be overwritten. This cannot be undone."</p>
                                            </div>
                                        }.into_any()
                                    } else {
                                        ().into_any()
                                    }}

                                    {if is_configured_storage {
                                        view! {
                                            <div class="flex items-start gap-2 p-3 bg-primary-tint border border-primary-glow rounded-xl">
                                                <p class="text-xs text-primary">"Restored files will be written as normal files to the selected storage under cloudless-restored. CloudLess decrypts locally; the CloudLess server cannot read the files."</p>
                                            </div>
                                        }.into_any()
                                    } else {
                                        ().into_any()
                                    }}

                                    // Error
                                    {move || error.get().map(|e| view! {
                                        <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                                    })}
                                </div>

                                // Footer
                                <div class="flex justify-end gap-3 p-5 border-t border-border">
                                    <button
                                        type="button"
                                        class="px-4 py-2 rounded-xl text-sm font-medium border border-border hover:bg-bg transition-all"
                                        on:click=move |_| set_confirming.set(false)
                                    >
                                        "Back"
                                    </button>
                                    <button
                                        type="submit"
                                        data-testid="confirm-restore-button"
                                        class="btn-gradient text-white px-6 py-2 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint disabled:opacity-50 disabled:cursor-not-allowed"
                                        disabled=move || submitting.get()
                                    >
                                        {move || if submitting.get() { "Restoring..." } else { "Confirm & Restore" }}
                                    </button>
                                </div>
                            </form>
                        }.into_any()
                    } else {
                        // ── Phase 1: Configuration Form ──
                        let total_size_str = total_size_str.clone();

                        view! {
                            <form on:submit=move |ev: leptos::ev::SubmitEvent| {
                                ev.prevent_default();
                                // Validate custom path if selected
                                if destination.get_untracked() == "custom" && custom_path.get_untracked().is_empty() {
                                    set_error.set(Some("Please select a custom destination path.".to_string()));
                                    return;
                                }
                                if destination.get_untracked() == "configured_storage" && selected_storage_id.get_untracked().is_empty() {
                                    set_error.set(Some("Please select a configured storage destination.".to_string()));
                                    return;
                                }
                                set_error.set(None);
                                set_confirming.set(true);
                            }>
                                <div class="p-5 space-y-5">
                                    // Summary
                                    <div class="bg-bg/50 rounded-xl p-3">
                                        <div class="text-sm text-text-secondary mb-1">"Selected"</div>
                                        <div class="text-sm font-medium">
                                            {format!("{} file{}, {}", total_files, if total_files != 1 { "s" } else { "" }, total_size_str)}
                                        </div>
                                    </div>

                                    // Destination
                                    <fieldset>
                                        <legend class="block text-sm font-medium mb-2">"Destination"</legend>
                                        <div class="space-y-2">
                                            {if is_current_device {
                                                view! {
                                                    <label class="flex items-center gap-2 cursor-pointer">
                                                        <input
                                                            type="radio"
                                                            name="destination"
                                                            value="original"
                                                            checked=move || destination.get() == "original"
                                                            on:change=move |_| set_destination.set("original".to_string())
                                                            class="accent-primary"
                                                        />
                                                        <span class="text-sm">"Original location"</span>
                                                    </label>
                                                }.into_any()
                                            } else {
                                                ().into_any()
                                            }}
                                            <label class="flex items-center gap-2 cursor-pointer">
                                                <input
                                                    type="radio"
                                                    name="destination"
                                                    value="download"
                                                    checked=move || destination.get() == "download"
                                                    on:change=move |_| set_destination.set("download".to_string())
                                                    class="accent-primary"
                                                />
                                                <span class="text-sm">"Downloads folder"</span>
                                            </label>
                                            <label class="flex items-center gap-2 cursor-pointer">
                                                <input
                                                    type="radio"
                                                    name="destination"
                                                    value="custom"
                                                    checked=move || destination.get() == "custom"
                                                    on:change=move |_| set_destination.set("custom".to_string())
                                                    class="accent-primary"
                                                />
                                                <span class="text-sm">"Custom path"</span>
                                            </label>
                                            <label class="flex items-center gap-2 cursor-pointer">
                                                <input
                                                    type="radio"
                                                    name="destination"
                                                    value="configured_storage"
                                                    checked=move || destination.get() == "configured_storage"
                                                    on:change=move |_| set_destination.set("configured_storage".to_string())
                                                    class="accent-primary"
                                                />
                                                <span class="text-sm">"Configured storage"</span>
                                            </label>
                                            {move || {
                                                if destination.get() == "custom" {
                                                    view! {
                                                        <div class="flex gap-2 mt-1">
                                                            <input
                                                                type="text"
                                                                placeholder="/path/to/restore"
                                                                class="flex-1 px-3 py-2 bg-input border border-border rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-primary-tint"
                                                                prop:value=move || custom_path.get()
                                                                on:input=move |ev| set_custom_path.set(event_target_value(&ev))
                                                            />
                                                            <button
                                                                type="button"
                                                                class="flex-shrink-0 px-3 py-2 bg-primary-tint border border-primary-glow rounded-xl text-primary text-sm font-medium hover:bg-primary-tint transition-all"
                                                                on:click=move |_| {
                                                                    spawn_local(async move {
                                                                        match tauri_invoke_no_args::<Option<String>>("pick_directory").await {
                                                                            Ok(Some(path)) => set_custom_path.set(path),
                                                                            _ => {}
                                                                        }
                                                                    });
                                                                }
                                                            >
                                                                "Browse"
                                                            </button>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    ().into_any()
                                                }
                                            }}
                                            {move || {
                                                if destination.get() == "configured_storage" {
                                                    let storages = remote_storages.get();
                                                    view! {
                                                        <div class="space-y-2 mt-1">
                                                            <select
                                                                class="w-full px-3 py-2 bg-input border border-border rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-primary-tint"
                                                                prop:value=move || selected_storage_id.get()
                                                                on:change=move |ev| set_selected_storage_id.set(event_target_value(&ev))
                                                            >
                                                                {storages.iter().map(|storage| {
                                                                    let id = storage.id.to_string();
                                                                    let label = format!("{} ({})", storage.name, storage.storage_type);
                                                                    view! {
                                                                        <option value=id>{label}</option>
                                                                    }
                                                                }).collect_view()}
                                                            </select>
                                                            <p class="text-xs text-text-secondary">
                                                                "Restored files will be written as normal files under cloudless-restored."
                                                            </p>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    ().into_any()
                                                }
                                            }}
                                        </div>
                                    </fieldset>

                                    // Overwrite behavior
                                    <fieldset>
                                        <legend class="block text-sm font-medium mb-2">"If file exists"</legend>
                                        <div class="space-y-2">
                                            <label class="flex items-center gap-2 cursor-pointer">
                                                <input
                                                    type="radio"
                                                    name="overwrite"
                                                    value="overwrite"
                                                    checked=move || overwrite.get() == "overwrite"
                                                    on:change=move |_| set_overwrite.set("overwrite".to_string())
                                                    class="accent-primary"
                                                />
                                                <span class="text-sm">"Overwrite"</span>
                                            </label>
                                            <label class="flex items-center gap-2 cursor-pointer">
                                                <input
                                                    type="radio"
                                                    name="overwrite"
                                                    value="keep_both"
                                                    checked=move || overwrite.get() == "keep_both"
                                                    on:change=move |_| set_overwrite.set("keep_both".to_string())
                                                    class="accent-primary"
                                                />
                                                <span class="text-sm">"Keep both (rename new)"</span>
                                            </label>
                                            <label class="flex items-center gap-2 cursor-pointer">
                                                <input
                                                    type="radio"
                                                    name="overwrite"
                                                    value="skip"
                                                    checked=move || overwrite.get() == "skip"
                                                    on:change=move |_| set_overwrite.set("skip".to_string())
                                                    class="accent-primary"
                                                />
                                                <span class="text-sm">"Skip existing"</span>
                                            </label>
                                        </div>
                                        // Inline warning when overwrite is selected
                                        {move || (overwrite.get() == "overwrite").then(|| view! {
                                            <div class="flex items-start gap-2 p-2.5 mt-2 bg-warning-tint border border-warning-tint rounded-lg">
                                                <svg class="w-3.5 h-3.5 text-warning flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                    <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
                                                    <line x1="12" y1="9" x2="12" y2="13"/>
                                                    <line x1="12" y1="17" x2="12.01" y2="17"/>
                                                </svg>
                                                <p class="text-xs text-warning">"Existing files at the destination will be overwritten. This cannot be undone."</p>
                                            </div>
                                        })}
                                    </fieldset>

                                    // Error
                                    {move || error.get().map(|e| view! {
                                        <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-2 text-sm">{e}</div>
                                    })}
                                </div>

                                // Footer
                                <div class="flex justify-end gap-3 p-5 border-t border-border">
                                    <button
                                        type="button"
                                        class="px-4 py-2 rounded-xl text-sm font-medium border border-border hover:bg-bg transition-all"
                                        on:click=move |_| on_close.run(false)
                                    >
                                        "Cancel"
                                    </button>
                                    <button
                                        type="submit"
                                        data-testid="review-restore-button"
                                        class="btn-gradient text-white px-6 py-2 rounded-xl font-medium text-sm shadow-lg shadow-primary-tint"
                                    >
                                        "Review & Restore"
                                    </button>
                                </div>
                            </form>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

fn format_bytes(bytes: u64) -> String {
    shared_ui::utils::format_bytes(bytes)
}
