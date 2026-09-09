use crate::components::common::{
    DeviceResolutionResult, TauriClientError, tauri_invoke, tauri_invoke_no_args,
    tauri_invoke_with_args,
};
use crate::state::*;
use api_types::backup_config::DecryptedBackupConfigWithRemoteStorage;
use api_types::local_device::GetOrCreateLocalDeviceResponse;
use api_types::policy::GetPendingPoliciesResponse;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

/// Mirrors the Tauri BiometricStatusResponse for deserialization.
#[derive(Debug, Clone, Deserialize)]
struct BiometricStatusResponse {
    pub is_available: bool,
}

#[derive(Debug, Clone, Serialize)]
struct UnlockEncryptionRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
struct UnlockWithRecoveryKeyRequest {
    pub recovery_key: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChangePasswordAfterRecoveryRequest {
    pub new_password: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnlockMode {
    Password,
    RecoveryKey,
    ChangePassword,
}

fn is_mobile_platform(platform: &str) -> bool {
    platform == "android" || platform == "ios"
}

#[derive(Serialize)]
struct TrayConfigItemDto {
    id: uuid::Uuid,
    name: String,
}

#[derive(Serialize)]
struct SetTrayConfigsArgs {
    configs: Vec<TrayConfigItemDto>,
}

/// Fetches backup configs for the given device and applies them to session state.
/// Then checks for pending policy acceptances before navigating.
async fn load_configs_and_navigate(
    physical_device_id: String,
    set_session: WriteSignal<SessionInfo>,
    set_phase: WriteSignal<AppPhase>,
    is_unlocked: IsUnlockedSignal,
) {
    if let Ok(configs) = tauri_invoke::<_, Vec<DecryptedBackupConfigWithRemoteStorage>>(
        "list_backup_configs",
        "physical_device_id",
        physical_device_id,
    )
    .await
    {
        // Populate tray menu with config names (fire-and-forget; failure is non-critical).
        let tray_items: Vec<TrayConfigItemDto> = configs
            .iter()
            .map(|c| TrayConfigItemDto {
                id: c.config_id,
                name: c.display_name.clone(),
            })
            .collect();
        let _ = tauri_invoke_with_args::<_, ()>(
            "set_tray_configs",
            &SetTrayConfigsArgs {
                configs: tray_items,
            },
        )
        .await;

        if let Some(first) = configs.first() {
            set_session.update(|s| {
                s.config_id = Some(first.config_id);
                s.source_directory = Some(first.source_directory.clone());
                s.storage_type = Some(first.storage_type.to_string());
                s.storage_configured = true;
            });
        }
    }

    // Check for pending policy acceptances before proceeding
    if let Ok(pending) =
        tauri_invoke_no_args::<GetPendingPoliciesResponse>("get_pending_policies").await
    {
        if !pending.list.is_empty() {
            set_phase.set(AppPhase::PolicyAcceptance);
            return;
        }
    }

    is_unlocked.set(true);
    set_phase.set(AppPhase::Main(MainView::Dashboard));
}

#[component]
pub fn UnlockForm() -> impl IntoView {
    let (session, set_session) = use_context::<SessionSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let is_unlocked = use_context::<IsUnlockedSignal>().unwrap();

    let (mode, set_mode) = signal(UnlockMode::Password);
    let (password, set_password) = signal(String::new());
    let (recovery_key_input, set_recovery_key_input) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    let (confirm_password, set_confirm_password) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let (show_password, set_show_password) = signal(false);
    // Biometric unlock: true when hardware is available AND user has enabled it
    let (biometric_ready, set_biometric_ready) = signal(false);
    let (biometric_loading, set_biometric_loading) = signal(false);
    let password_ref = NodeRef::<leptos::html::Input>::new();

    // Check biometric availability + enabled state on mount
    spawn_local({
        let set_biometric_ready = set_biometric_ready.clone();
        async move {
            // Both conditions must be true: hardware available AND user opted in
            let available =
                tauri_invoke_no_args::<BiometricStatusResponse>("is_biometric_available")
                    .await
                    .map(|s| s.is_available)
                    .unwrap_or(false);

            if available {
                let enabled = tauri_invoke_no_args::<bool>("is_biometric_enabled")
                    .await
                    .unwrap_or(false);
                let _ = set_biometric_ready.try_set(enabled);
            }
        }
    });

    Effect::new(move || {
        if let Some(el) = password_ref.get() {
            let _ = el.focus();
        }
    });

    /// After DEK is unlocked (by password or recovery key), resolve device and navigate.
    async fn post_unlock_navigate(
        session: ReadSignal<SessionInfo>,
        set_session: WriteSignal<SessionInfo>,
        set_phase: WriteSignal<AppPhase>,
        set_error: WriteSignal<Option<String>>,
        set_loading: WriteSignal<bool>,
        is_unlocked: IsUnlockedSignal,
    ) {
        // Fire-and-forget: mark any orphaned running jobs as interrupted.
        spawn_local(async move {
            let _ = tauri_invoke_no_args::<api_types::backup_job::AbandonStaleJobsResponse>(
                "abandon_stale_backup_jobs",
            )
            .await;
        });

        let platform = session.get_untracked().device_platform.clone();

        if is_mobile_platform(&platform) {
            match tauri_invoke_no_args::<DeviceResolutionResult>("resolve_mobile_device").await {
                Ok(DeviceResolutionResult::Resolved { device }) => {
                    set_session.update(|s| {
                        s.device_display_name = device.display_name.clone();
                        s.local_device_id = Some(device.id);
                        s.device_registered = true;
                        s.physical_device_id = device.physical_device_id.clone();
                    });
                    load_configs_and_navigate(
                        device.physical_device_id,
                        set_session,
                        set_phase,
                        is_unlocked,
                    )
                    .await;
                }
                Ok(DeviceResolutionResult::MultipleFound { .. })
                | Ok(DeviceResolutionResult::NoneFound) => {
                    set_session.update(|s| {
                        s.returning_user = true;
                    });
                    set_phase.set(AppPhase::Setup(SetupStep::Device));
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(format!(
                        "Failed to resolve device: {}",
                        e.user_message()
                    )));
                    let _ = set_loading.try_set(false);
                }
            }
        } else {
            let hostname = tauri_invoke_no_args::<String>("get_hostname")
                .await
                .unwrap_or_else(|_| "My Device".to_string());

            let dev = match tauri_invoke::<_, GetOrCreateLocalDeviceResponse>(
                "get_or_create_device",
                "display_name",
                hostname,
            )
            .await
            {
                Ok(dev) => dev,
                Err(e) => {
                    let _ = set_error.try_set(Some(format!(
                        "Failed to register device: {}",
                        e.user_message()
                    )));
                    let _ = set_loading.try_set(false);
                    return;
                }
            };

            set_session.update(|s| {
                s.device_display_name = dev.display_name.clone();
                s.local_device_id = Some(dev.id);
                s.device_registered = true;
                s.physical_device_id = dev.physical_device_id.clone();
            });

            load_configs_and_navigate(dev.physical_device_id, set_session, set_phase, is_unlocked)
                .await;
        }
    }

    // Password unlock handler
    let on_password_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            let req = UnlockEncryptionRequest {
                password: password.get_untracked(),
            };

            if let Err(e) = tauri_invoke::<_, ()>("unlock_encryption", "request", req).await {
                if matches!(e, TauriClientError::NotFound { .. }) {
                    set_phase.set(AppPhase::Setup(SetupStep::EncryptionPassword));
                    return;
                }
                let _ = set_error.try_set(Some(e.user_message().to_string()));
                let _ = set_loading.try_set(false);
                return;
            }

            post_unlock_navigate(
                session,
                set_session,
                set_phase,
                set_error,
                set_loading,
                is_unlocked,
            )
            .await;
        });
    };

    // Biometric unlock handler
    let on_biometric_unlock = move |_: leptos::ev::MouseEvent| {
        set_biometric_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            if let Err(e) = tauri_invoke_no_args::<()>("biometric_unlock").await {
                let _ = set_error.try_set(Some(e.user_message().to_string()));
                let _ = set_biometric_loading.try_set(false);
                return;
            }

            post_unlock_navigate(
                session,
                set_session,
                set_phase,
                set_error,
                set_loading,
                is_unlocked,
            )
            .await;
            let _ = set_biometric_loading.try_set(false);
        });
    };

    // Recovery key unlock handler
    let on_recovery_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            let req = UnlockWithRecoveryKeyRequest {
                recovery_key: recovery_key_input.get_untracked(),
            };

            if let Err(e) = tauri_invoke::<_, ()>("unlock_with_recovery_key", "request", req).await
            {
                let _ = set_error.try_set(Some(e.user_message().to_string()));
                let _ = set_loading.try_set(false);
                return;
            }

            let _ = set_loading.try_set(false);
            set_mode.set(UnlockMode::ChangePassword);
        });
    };

    // Change password after recovery handler
    let on_change_password_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();

        let new_pw = new_password.get_untracked();
        let confirm_pw = confirm_password.get_untracked();

        if new_pw != confirm_pw {
            set_error.set(Some("Passwords do not match.".to_string()));
            return;
        }

        if new_pw.len() < 8 {
            set_error.set(Some("Password must be at least 8 characters.".to_string()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            let req = ChangePasswordAfterRecoveryRequest {
                new_password: new_pw,
            };

            if let Err(e) =
                tauri_invoke::<_, ()>("change_password_after_recovery", "request", req).await
            {
                let _ = set_error.try_set(Some(e.user_message().to_string()));
                let _ = set_loading.try_set(false);
                return;
            }

            post_unlock_navigate(
                session,
                set_session,
                set_phase,
                set_error,
                set_loading,
                is_unlocked,
            )
            .await;
        });
    };

    let error_view = move || {
        error.try_get().flatten().map(|e| {
            view! {
                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-5">{e}</div>
            }
        })
    };

    let password_toggle_button = move || {
        view! {
            <button
                type="button"
                tabindex=-1
                class="absolute right-3 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text-primary transition-colors"
                aria-label="Toggle password visibility"
                on:click=move |ev: leptos::ev::MouseEvent| {
                    ev.prevent_default();
                    set_show_password.update(|v| *v = !*v);
                }
            >
                {move || if show_password.get() {
                    view! { <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 01-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/></svg> }.into_any()
                } else {
                    view! { <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg> }.into_any()
                }}
            </button>
        }
    };

    view! {
        {move || match mode.get() {
            UnlockMode::Password => view! {
                <form on:submit=on_password_submit>
                    <h2 class="text-3xl md:text-4xl font-display font-bold gradient-text mb-2">"Unlock Encryption"</h2>
                    <p class="text-text-secondary mb-6 md:mb-8">"Enter your encryption password to access your data"</p>

                    // Biometric unlock button (shown only when available + enabled)
                    {move || biometric_ready.get().then(|| view! {
                        <div class="mb-5">
                            <button
                                type="button"
                                class="w-full flex items-center justify-center gap-3 py-3 rounded-xl font-semibold text-lg border-2 border-primary text-primary hover:bg-primary-tint active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                                disabled=move || biometric_loading.get()
                                on:click=on_biometric_unlock
                            >
                                // Fingerprint icon
                                <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                    <path d="M2 12C2 6.5 6.5 2 12 2a10 10 0 018 4"/>
                                    <path d="M5 19.5C5.5 18 6 15 6 12c0-.7.12-1.37.34-2"/>
                                    <path d="M17.29 21.02c.12-.6.43-2.3.5-3.02"/>
                                    <path d="M12 10a2 2 0 00-2 2c0 1.02-.1 2.51-.26 4"/>
                                    <path d="M8.65 22c.21-.66.45-1.32.57-2"/>
                                    <path d="M14 13.12c0 2.38 0 6.38-1 8.88"/>
                                    <path d="M2 16h.01"/>
                                    <path d="M21.8 16c.2-2 .131-5.354 0-6"/>
                                    <path d="M9 6.8a6 6 0 019 5.2c0 .47 0 1.17-.02 2"/>
                                </svg>
                                {move || if biometric_loading.get() { "Authenticating..." } else { "Unlock with Biometrics" }}
                            </button>
                        </div>
                        // "or" divider between biometric and password
                        <div class="flex items-center gap-3 mb-5">
                            <div class="flex-1 border-t border-border"></div>
                            <span class="text-xs text-text-secondary uppercase tracking-wider">"or"</span>
                            <div class="flex-1 border-t border-border"></div>
                        </div>
                    })}

                    <div class="mb-5">
                        <label class="block text-sm font-medium text-text-secondary mb-2" for="unlock-password">"Encryption Password"</label>
                        <div class="relative">
                            <input
                                type=move || if show_password.get() { "text" } else { "password" }
                                id="unlock-password"
                                data-testid="unlock-password-input"
                                aria-label="Unlock Password"
                                placeholder="Enter your encryption password"
                                class="w-full bg-input border border-border rounded-xl px-4 py-3 pr-11 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                                node_ref=password_ref
                                on:input=move |ev| set_password.set(event_target_value(&ev))
                                required
                            />
                            {password_toggle_button}
                        </div>
                    </div>

                    {error_view}

                    <button
                        type="submit"
                        data-testid="unlock-submit-button"
                        class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                        disabled=move || loading.try_get().unwrap_or(false)
                    >
                        {move || if loading.try_get().unwrap_or(false) { "Unlocking..." } else { "Unlock" }}
                    </button>

                    <div class="mt-4 text-center">
                        <button
                            type="button"
                            class="text-sm text-primary hover:text-primary-hover transition-colors underline"
                            on:click=move |_| {
                                set_error.set(None);
                                set_mode.set(UnlockMode::RecoveryKey);
                            }
                        >
                            "Forgot password? Use Recovery Key"
                        </button>
                    </div>
                </form>
            }.into_any(),

            UnlockMode::RecoveryKey => view! {
                <form on:submit=on_recovery_submit>
                    <h2 class="text-3xl md:text-4xl font-display font-bold gradient-text mb-2">"Recovery Key"</h2>
                    <p class="text-text-secondary mb-6 md:mb-8">"Enter your recovery key to unlock your encryption"</p>

                    <div class="mb-5">
                        <label class="block text-sm font-medium text-text-secondary mb-2" for="recovery-key">"Recovery Key"</label>
                        <textarea
                            id="recovery-key"
                            aria-label="Recovery Key"
                            placeholder="CLRK-XXXX-XXXX-XXXX-..."
                            rows="3"
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary font-mono text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all resize-none"
                            on:input=move |ev| set_recovery_key_input.set(event_target_value(&ev))
                            required
                        />
                    </div>

                    {error_view}

                    <button
                        type="submit"
                        class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                        disabled=move || loading.try_get().unwrap_or(false)
                    >
                        {move || if loading.try_get().unwrap_or(false) { "Recovering..." } else { "Recover" }}
                    </button>

                    <div class="mt-4 text-center">
                        <button
                            type="button"
                            class="text-sm text-primary hover:text-primary-hover transition-colors underline"
                            on:click=move |_| {
                                set_error.set(None);
                                set_mode.set(UnlockMode::Password);
                            }
                        >
                            "Back to password unlock"
                        </button>
                    </div>
                </form>
            }.into_any(),

            UnlockMode::ChangePassword => view! {
                <form on:submit=on_change_password_submit>
                    <h2 class="text-3xl md:text-4xl font-display font-bold gradient-text mb-2">"Set New Password"</h2>
                    <p class="text-text-secondary mb-6 md:mb-8">"Recovery successful. Please set a new encryption password."</p>

                    <div class="mb-5">
                        <label class="block text-sm font-medium text-text-secondary mb-2" for="new-password">"New Password"</label>
                        <div class="relative">
                            <input
                                type=move || if show_password.get() { "text" } else { "password" }
                                id="new-password"
                                aria-label="New Password"
                                placeholder="Enter new encryption password"
                                class="w-full bg-input border border-border rounded-xl px-4 py-3 pr-11 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                                on:input=move |ev| set_new_password.set(event_target_value(&ev))
                                required
                            />
                            {password_toggle_button}
                        </div>
                    </div>

                    <div class="mb-5">
                        <label class="block text-sm font-medium text-text-secondary mb-2" for="confirm-password">"Confirm Password"</label>
                        <input
                            type=move || if show_password.get() { "text" } else { "password" }
                            id="confirm-password"
                            aria-label="Confirm Password"
                            placeholder="Confirm new encryption password"
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                            on:input=move |ev| set_confirm_password.set(event_target_value(&ev))
                            required
                        />
                    </div>

                    {error_view}

                    <button
                        type="submit"
                        class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                        disabled=move || loading.try_get().unwrap_or(false)
                    >
                        {move || if loading.try_get().unwrap_or(false) { "Setting password..." } else { "Set Password & Continue" }}
                    </button>
                </form>
            }.into_any(),
        }}
    }
}
