use crate::components::common::tauri_invoke;
use crate::components::storage::sftp_storage_form::{
    CreateRemoteStorageResponse as SftpCreateRemoteStorageResponse, SftpStorageForm,
};
use crate::state::*;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;

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
struct TestLocalConnectionRequest {
    pub root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CreateRemoteStorageResponse {
    pub id: Uuid,
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

#[component]
pub fn RemoteStorageConfig() -> impl IntoView {
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (_, set_session) = use_context::<SessionSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    // Provider selector: "s3", "gdrive", "onedrive", "sftp", or "local"
    let (provider, set_provider) = signal("s3".to_string());

    // S3 fields
    let (storage_name, set_storage_name) = signal(String::new());
    let (access_key, set_access_key) = signal(String::new());
    let (secret, set_secret) = signal(String::new());
    let (region, set_region) = signal("us-east-1".to_string());
    let (bucket, set_bucket) = signal(String::new());
    let (show_secret, set_show_secret) = signal(false);

    // Google Drive fields
    let (gdrive_name, set_gdrive_name) = signal(String::new());
    // GDrive OAuth handoff state
    let (gdrive_awaiting, set_gdrive_awaiting) = signal(false);
    let (gdrive_timed_out, set_gdrive_timed_out) = signal(false);

    // Microsoft OneDrive fields
    let (onedrive_name, set_onedrive_name) = signal(String::new());

    // Local filesystem fields
    let (local_name, set_local_name) = signal(String::new());
    let (local_path, set_local_path) = signal(String::new());

    // Shared state
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let (test_status, set_test_status) = signal(Option::<Result<String, String>>::None);
    let (testing, set_testing) = signal(false);

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

    let browse_local_path = move |_: leptos::ev::MouseEvent| {
        spawn_local(async move {
            match tauri_invoke::<_, Option<String>>("pick_directory", "", ()).await {
                Ok(Some(path)) => set_local_path.set(path),
                Ok(None) => {}
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
        });
    };

    let on_submit_s3 = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

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
                Ok(resp) => {
                    set_session.update(|s| {
                        s.storage_id = Some(resp.id);
                    });
                    set_phase.set(AppPhase::Setup(SetupStep::BackupConfig));
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    let on_submit_gdrive = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);
        let _ = set_gdrive_awaiting.try_set(true);
        let _ = set_gdrive_timed_out.try_set(false);

        // After 10s with no callback, surface the "still waiting" message.
        if let Some(window) = web_sys::window() {
            let cb = Closure::once(move || {
                if gdrive_awaiting.get_untracked() {
                    let _ = set_gdrive_timed_out.try_set(true);
                }
            });
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                10_000,
            );
            cb.forget();
        }

        spawn_local(async move {
            match tauri_invoke::<_, CreateRemoteStorageResponse>(
                "add_google_drive_storage",
                "storage_name",
                gdrive_name.get_untracked(),
            )
            .await
            {
                Ok(resp) => {
                    set_session.update(|s| {
                        s.storage_id = Some(resp.id);
                    });
                    set_phase.set(AppPhase::Setup(SetupStep::BackupConfig));
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_gdrive_awaiting.try_set(false);
            let _ = set_loading.try_set(false);
        });
    };

    let on_submit_onedrive = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            match tauri_invoke::<_, CreateRemoteStorageResponse>(
                "add_onedrive_storage",
                "storage_name",
                onedrive_name.get_untracked(),
            )
            .await
            {
                Ok(resp) => {
                    set_session.update(|s| {
                        s.storage_id = Some(resp.id);
                    });
                    set_phase.set(AppPhase::Setup(SetupStep::BackupConfig));
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    // Re-opens the browser authorization page without resetting the awaiting overlay.
    let retry_gdrive_auth = move |_: leptos::ev::MouseEvent| {
        let _ = set_gdrive_timed_out.try_set(false);
        let name = gdrive_name.get_untracked();
        spawn_local(async move {
            match tauri_invoke::<_, CreateRemoteStorageResponse>(
                "add_google_drive_storage",
                "storage_name",
                name,
            )
            .await
            {
                Ok(resp) => {
                    set_session.update(|s| {
                        s.storage_id = Some(resp.id);
                    });
                    set_phase.set(AppPhase::Setup(SetupStep::BackupConfig));
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                    let _ = set_gdrive_awaiting.try_set(false);
                    let _ = set_loading.try_set(false);
                }
            }
        });
    };

    // Cancels the GDrive OAuth wait and returns the user to the provider form.
    let cancel_gdrive_auth = move |_: leptos::ev::MouseEvent| {
        let _ = set_gdrive_awaiting.try_set(false);
        let _ = set_gdrive_timed_out.try_set(false);
        set_loading.set(false);
        set_error.set(None);
    };

    let on_submit_local = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

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
                Ok(resp) => {
                    set_session.update(|s| {
                        s.storage_id = Some(resp.id);
                    });
                    set_phase.set(AppPhase::Setup(SetupStep::BackupConfig));
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    let input_class = "w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all";
    let label_class = "block text-sm font-medium text-text-secondary mb-1";
    let helper_class = "text-xs text-text-secondary mt-1 mb-3";

    let active_class = "flex-1 py-2.5 px-4 rounded-xl bg-primary-tint text-primary-light font-medium text-sm border border-primary-glow transition-all";
    let inactive_class = "flex-1 py-2.5 px-4 rounded-xl text-text-secondary text-sm border border-border hover:border-primary-glow transition-all";

    let on_skip = move |_: leptos::ev::MouseEvent| {
        is_unlocked.set(true);
        set_phase.set(AppPhase::Main(MainView::Dashboard));
    };

    view! {
        <div>
            <h2 class="text-2xl md:text-3xl font-display font-bold gradient-text mb-2">"Choose backup storage"</h2>
            <p class="text-text-secondary mb-6 text-sm md:text-base">"Pick where encrypted backups will be stored"</p>

            // Provider selector
            <div class="mb-6">
                <label class=label_class>"Storage provider"</label>
                <div class="flex gap-2 mt-2">
                    <button
                        type="button"
                        class=move || if provider.get() == "s3" { active_class } else { inactive_class }
                        on:click=move |_| { set_provider.set("s3".to_string()); set_error.set(None); set_test_status.set(None); }
                    >
                        "Amazon S3"
                    </button>
                    <button
                        type="button"
                        class=move || if provider.get() == "gdrive" { active_class } else { inactive_class }
                        on:click=move |_| { set_provider.set("gdrive".to_string()); set_error.set(None); set_test_status.set(None); }
                    >
                        "Google Drive"
                    </button>
                    <button
                        type="button"
                        class=move || if provider.get() == "onedrive" { active_class } else { inactive_class }
                        on:click=move |_| { set_provider.set("onedrive".to_string()); set_error.set(None); set_test_status.set(None); }
                    >
                        "OneDrive"
                    </button>
                    <button
                        type="button"
                        class=move || if provider.get() == "sftp" { active_class } else { inactive_class }
                        on:click=move |_| { set_provider.set("sftp".to_string()); set_error.set(None); set_test_status.set(None); }
                    >
                        "SFTP Server"
                    </button>
                    <button
                        type="button"
                        class=move || if provider.get() == "local" { active_class } else { inactive_class }
                        on:click=move |_| { set_provider.set("local".to_string()); set_error.set(None); set_test_status.set(None); }
                    >
                        "External Folder"
                    </button>
                </div>
            </div>

            // ── AWS S3 form ───────────────────────────────────────────
            {move || (provider.get() == "s3").then(|| view! {
                <form on:submit=on_submit_s3>
                    <div class="mb-4">
                        <label class=label_class for="s3-storage-name">"Storage Name"</label>
                        <input type="text" id="s3-storage-name" aria-label="S3 Storage Name" class=input_class placeholder="My Backup Storage"
                            on:input=move |ev| set_storage_name.set(event_target_value(&ev)) required />
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
                        <div>
                            <label class=label_class for="s3-access-key">"Access Key"</label>
                            <input type="text" id="s3-access-key" aria-label="S3 Access Key" class=input_class placeholder="AKIA..."
                                on:input=move |ev| set_access_key.set(event_target_value(&ev)) required />
                            <p class=helper_class>"Used only to authenticate writes to your bucket."</p>
                        </div>
                        <div>
                            <label class=label_class for="s3-secret-key">"Secret Key"</label>
                            <div class="relative">
                                <input
                                    type=move || if show_secret.get() { "text" } else { "password" }
                                    id="s3-secret-key" aria-label="S3 Secret Key" class=input_class placeholder="••••••••"
                                    on:input=move |ev| set_secret.set(event_target_value(&ev)) required />
                                <button
                                    type="button"
                                    aria-label=move || if show_secret.get() { "Hide secret key" } else { "Show secret key" }
                                    class="absolute right-3 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text-primary transition-colors"
                                    on:click=move |_| set_show_secret.set(!show_secret.get_untracked())
                                    tabindex=-1
                                >
                                    {move || if show_secret.get() {
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
                            </div>
                            <p class=helper_class>"Stored encrypted on this device."</p>
                        </div>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
                        <div>
                            <label class=label_class for="s3-region">"Region"</label>
                            <input type="text" id="s3-region" aria-label="S3 Region" class=input_class
                                prop:value=move || region.try_get().unwrap_or_default()
                                on:input=move |ev| set_region.set(event_target_value(&ev)) required />
                            <p class=helper_class>"Must match the bucket's region."</p>
                        </div>
                        <div>
                            <label class=label_class for="s3-bucket">"Bucket"</label>
                            <input type="text" id="s3-bucket" aria-label="S3 Bucket" class=input_class placeholder="my-backup-bucket"
                                on:input=move |ev| set_bucket.set(event_target_value(&ev)) required />
                            <p class=helper_class>"CloudLess will only write inside this bucket."</p>
                        </div>
                    </div>

                    <p class="text-xs text-text-secondary mb-4">
                        "CloudLess only uses the bucket you configure here."
                    </p>

                    // Test Connection
                    <div class="mb-4">
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

                    {move || error.try_get().flatten().map(|e| view! {
                        <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
                    })}

                    <button
                        type="submit"
                        class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                        disabled=move || loading.try_get().unwrap_or(false)
                    >
                        {move || if loading.try_get().unwrap_or(false) { "Saving..." } else { "Continue" }}
                    </button>
                </form>
            })}

            // ── Google Drive form / OAuth handoff overlay ────────────
            {move || (provider.get() == "gdrive").then(|| {
                if gdrive_awaiting.get() {
                    // OAuth handoff overlay — shown while waiting for the browser callback
                    view! {
                        <div class="space-y-5 text-center py-4">
                            <div class="animate-spin w-10 h-10 border-2 border-primary border-t-transparent rounded-full mx-auto"></div>

                            {move || if gdrive_timed_out.get() {
                                view! {
                                    <div>
                                        <p class="font-medium">"Still waiting for authorization"</p>
                                        <p class="text-sm text-text-secondary mt-1">
                                            "CloudLess is still waiting for Google to finish connecting this storage."
                                        </p>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div>
                                        <p class="font-medium">"Open Google in your browser"</p>
                                        <p class="text-sm text-text-secondary mt-1">
                                            "CloudLess is opening your browser so you can connect Google Drive securely."
                                        </p>
                                    </div>
                                }.into_any()
                            }}

                            {move || error.try_get().flatten().map(|e| view! {
                                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm text-left">{e}</div>
                            })}

                            <div class="flex flex-col gap-2">
                                <button
                                    type="button"
                                    class="w-full py-2.5 rounded-xl text-sm font-medium border border-primary-glow text-primary hover:bg-primary-tint transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
                                    on:click=retry_gdrive_auth
                                >
                                    "Open browser again"
                                </button>
                                <button
                                    type="button"
                                    class="w-full py-2.5 rounded-xl text-sm font-medium border border-border text-text-secondary hover:bg-bg transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border"
                                    on:click=cancel_gdrive_auth
                                >
                                    "Use another storage option"
                                </button>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    // Normal Google Drive form
                    view! {
                        <form on:submit=on_submit_gdrive>
                            <div class="mb-4">
                                <label class=label_class for="gdrive-storage-name">"Storage Name"</label>
                                <input type="text" id="gdrive-storage-name" aria-label="Google Drive Storage Name" class=input_class placeholder="My Google Drive"
                                    on:input=move |ev| set_gdrive_name.set(event_target_value(&ev)) required />
                            </div>

                            <p class="text-text-secondary text-sm mb-5">
                                "Clicking Connect will open Google's consent screen in your browser. "
                                "CloudLess only requests access to files it creates (drive.file scope) — your existing Drive files are not accessible."
                            </p>

                            {move || error.try_get().flatten().map(|e| view! {
                                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
                            })}

                            <button
                                type="submit"
                                class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                                disabled=move || loading.try_get().unwrap_or(false)
                            >
                                "Connect Google Drive"
                            </button>
                        </form>
                    }.into_any()
                }
            })}

            // ── Microsoft OneDrive form ─────────────────────────────
            {move || (provider.get() == "onedrive").then(|| view! {
                <form on:submit=on_submit_onedrive>
                    <div class="mb-4">
                        <label class=label_class for="onedrive-storage-name">"Storage Name"</label>
                        <input type="text" id="onedrive-storage-name" aria-label="OneDrive Storage Name" class=input_class placeholder="My OneDrive"
                            on:input=move |ev| set_onedrive_name.set(event_target_value(&ev)) required />
                    </div>

                    <p class="text-text-secondary text-sm mb-5">
                        "Clicking Connect will open Microsoft's consent screen in your browser. "
                        "CloudLess only requests access to its own OneDrive app folder."
                    </p>

                    {move || error.try_get().flatten().map(|e| view! {
                        <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
                    })}

                    <button
                        type="submit"
                        class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                        disabled=move || loading.try_get().unwrap_or(false)
                    >
                        {move || if loading.try_get().unwrap_or(false) { "Connecting..." } else { "Connect OneDrive" }}
                    </button>
                </form>
            })}

            // ── SFTP form ────────────────────────────────────────────
            {move || (provider.get() == "sftp").then(|| view! {
                <SftpStorageForm
                    id_prefix="sftp"
                    submit_label="Continue"
                    loading_label="Saving..."
                    on_success=Callback::new(move |resp: SftpCreateRemoteStorageResponse| {
                        set_session.update(|s| {
                            s.storage_id = Some(resp.id);
                        });
                        set_phase.set(AppPhase::Setup(SetupStep::BackupConfig));
                    })
                />
            })}

            // ── Local Filesystem form ─────────────────────────────────
            {move || (provider.get() == "local").then(|| view! {
                <form on:submit=on_submit_local>
                    <div class="mb-4">
                        <label class=label_class for="local-storage-name">"Storage Name"</label>
                        <input type="text" id="local-storage-name" aria-label="Local Storage Name" class=input_class placeholder="My USB Drive"
                            on:input=move |ev| set_local_name.set(event_target_value(&ev)) required />
                    </div>

                    <div class="mb-4">
                        <label class=label_class for="local-root-path">"Storage Directory"</label>
                        <div class="flex gap-2">
                            <input type="text" id="local-root-path" aria-label="Local Storage Path" class=input_class placeholder="/Volumes/Backup"
                                prop:value=move || local_path.try_get().unwrap_or_default()
                                on:input=move |ev| set_local_path.set(event_target_value(&ev)) required />
                            <button
                                type="button"
                                class="flex-shrink-0 px-4 py-3 bg-primary-tint border border-primary-glow rounded-xl text-primary-light font-medium text-sm hover:bg-primary-tint/80 transition-all"
                                on:click=browse_local_path
                            >
                                "Browse"
                            </button>
                        </div>
                    </div>

                    <p class="text-text-secondary text-sm mb-4">
                        "Choose a folder on an external drive, NAS mount, or separate disk. "
                        "Using the same disk as your source files provides no protection against hardware failure."
                    </p>
                    <p class="text-xs text-text-secondary mb-4">
                        "CloudLess only writes inside the folder you configure here."
                    </p>

                    // Test Connection
                    <div class="mb-4">
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

                    {move || error.try_get().flatten().map(|e| view! {
                        <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
                    })}

                    <button
                        type="submit"
                        class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                        disabled=move || loading.try_get().unwrap_or(false)
                    >
                        {move || if loading.try_get().unwrap_or(false) { "Saving..." } else { "Continue" }}
                    </button>
                </form>
            })}

            <div class="mt-4 text-center">
                <button
                    type="button"
                    class="text-sm text-text-secondary hover:text-text-primary transition-colors"
                    on:click=on_skip
                >
                    "Skip for now"
                </button>
            </div>
        </div>
    }
}
