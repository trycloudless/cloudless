use crate::components::common::tauri_invoke;
use crate::state::*;
use leptos::prelude::*;
use serde::Serialize;
use wasm_bindgen_futures::spawn_local;

const MIN_PASSWORD_LEN: usize = 12;

#[derive(Debug, Clone, Serialize)]
struct SetupEncryptionRequest {
    pub password: String,
}

#[component]
pub fn EncryptionSetup() -> impl IntoView {
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (_, set_session) = use_context::<SessionSignal>().unwrap();

    let (password, set_password) = signal(String::new());
    let (confirm, set_confirm) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let (show_password, set_show_password) = signal(false);
    let (show_confirm, set_show_confirm) = signal(false);
    let (show_recovery_info, set_show_recovery_info) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();

        let pw = password.get_untracked();
        let cf = confirm.get_untracked();

        if pw != cf {
            set_error.set(Some("Passwords do not match".to_string()));
            return;
        }

        if pw.len() < MIN_PASSWORD_LEN {
            set_error.set(Some(format!(
                "Password must be at least {MIN_PASSWORD_LEN} characters"
            )));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            let req = SetupEncryptionRequest { password: pw };

            match tauri_invoke::<_, ()>("setup_encryption", "request", req).await {
                Ok(_) => {
                    let _ = set_session.try_update(|s| {
                        s.encryption_ready_banner = true;
                    });
                    let _ = set_phase.try_set(AppPhase::Setup(SetupStep::Device));
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    // Real-time requirement checks
    let pw_long_enough = move || password.get().len() >= MIN_PASSWORD_LEN;
    let pw_has_upper = move || password.get().chars().any(|c| c.is_uppercase());
    let pw_has_lower = move || password.get().chars().any(|c| c.is_lowercase());
    let pw_has_digit = move || password.get().chars().any(|c| c.is_ascii_digit());
    let pw_has_symbol = move || {
        password
            .get()
            .chars()
            .any(|c| !c.is_alphanumeric() && !c.is_whitespace())
    };

    let pw_nonempty = move || !password.get().is_empty();
    let pw_mismatch = move || {
        let cf = confirm.get();
        let pw = password.get();
        !cf.is_empty() && cf != pw
    };
    let pw_match_ok = move || {
        let cf = confirm.get();
        let pw = password.get();
        !cf.is_empty() && cf == pw && pw.len() >= MIN_PASSWORD_LEN
    };

    // A requirement row: grey dot → green check (met) or red X (field non-empty, unmet).
    let req_row = move |label: &'static str, met: bool, nonempty: bool| {
        if met {
            view! {
                <p class="text-xs flex items-center gap-1.5 text-accent">
                    <svg class="w-3 h-3 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="3">
                        <polyline points="20 6 9 17 4 12"/>
                    </svg>
                    {label}
                </p>
            }.into_any()
        } else if nonempty {
            view! {
                <p class="text-xs flex items-center gap-1.5 text-error">
                    <svg class="w-3 h-3 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="3">
                        <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                    </svg>
                    {label}
                </p>
            }.into_any()
        } else {
            view! {
                <p class="text-xs flex items-center gap-1.5 text-text-secondary">
                    <svg class="w-3 h-3 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
                        <circle cx="12" cy="12" r="3"/>
                    </svg>
                    {label}
                </p>
            }
            .into_any()
        }
    };

    view! {
        <form on:submit=on_submit>
            <h2 class="text-2xl md:text-3xl font-display font-bold gradient-text mb-1">"Set Encryption Password"</h2>
            <p class="text-text-secondary mb-1 text-sm">"This password is never sent to CloudLess and cannot be recovered by support."</p>
            <p class="text-text-secondary mb-6 text-sm">"Your files are encrypted client-side before leaving this device."</p>

                    // "Learn how recovery works" toggle
                    <div class="mb-5">
                        <button
                            type="button"
                            class="text-xs text-accent hover:text-accent-light transition-colors"
                            on:click=move |_| set_show_recovery_info.set(!show_recovery_info.get_untracked())
                        >
                            {move || if show_recovery_info.get() { "▲ Hide recovery explanation" } else { "▼ Learn how recovery works" }}
                        </button>
                        {move || show_recovery_info.get().then(|| view! {
                            <div class="mt-2 bg-primary-tint border border-primary-glow rounded-xl p-3 text-xs text-text-secondary space-y-1.5">
                                <p class="font-medium text-text-primary">"How recovery works"</p>
                                <p>"Your account password signs you in to CloudLess."</p>
                                <p>"This encryption password unlocks your files on this device. CloudLess never receives it."</p>
                                <p>"After setup, save a recovery key in Settings > Security. It can unlock your backup if you forget this password."</p>
                            </div>
                        })}
                    </div>

                    // Password field + live requirements checklist
                    <div class="mb-5">
                        <label class="block text-sm font-medium text-text-secondary mb-2" for="setup-encryption-password">"Encryption Password"</label>
                        <div class="relative">
                            <input
                                type=move || if show_password.get() { "text" } else { "password" }
                                id="setup-encryption-password"
                                aria-label="Setup Encryption Password"
                                placeholder=format!("At least {MIN_PASSWORD_LEN} characters")
                                class="w-full bg-input border border-border rounded-xl px-4 py-3 pr-11 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                                on:input=move |ev| set_password.set(event_target_value(&ev))
                                required
                            />
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
                        </div>

                        // Live requirements checklist
                        {move || pw_nonempty().then(|| {
                            let ne = pw_nonempty();
                            view! {
                                <div class="mt-2 space-y-1 pl-1">
                                    {req_row("At least 12 characters", pw_long_enough(), ne)}
                                    {req_row("One upper-case letter",  pw_has_upper(),   ne)}
                                    {req_row("One lower-case letter",  pw_has_lower(),   ne)}
                                    {req_row("One number",             pw_has_digit(),   ne)}
                                    {req_row("One symbol",             pw_has_symbol(),  ne)}
                                </div>
                            }
                        })}
                    </div>

                    // Confirm field
                    <div class="mb-5">
                        <label class="block text-sm font-medium text-text-secondary mb-2" for="setup-confirm-password">"Confirm Password"</label>
                        <div class="relative">
                            <input
                                type=move || if show_confirm.get() { "text" } else { "password" }
                                id="setup-confirm-password"
                                aria-label="Setup Confirm Password"
                                placeholder="Re-enter your password"
                                class="w-full bg-input border border-border rounded-xl px-4 py-3 pr-11 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                                on:input=move |ev| set_confirm.set(event_target_value(&ev))
                                required
                            />
                            <button
                                type="button"
                                tabindex=-1
                                class="absolute right-3 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text-primary transition-colors"
                                aria-label="Toggle password visibility"
                                on:click=move |ev: leptos::ev::MouseEvent| {
                                    ev.prevent_default();
                                    set_show_confirm.update(|v| *v = !*v);
                                }
                            >
                                {move || if show_confirm.get() {
                                    view! { <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 01-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/></svg> }.into_any()
                                } else {
                                    view! { <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg> }.into_any()
                                }}
                            </button>
                        </div>
                        {move || pw_mismatch().then(|| view! {
                            <p class="text-xs text-error mt-1">"Passwords do not match."</p>
                        })}
                        {move || pw_match_ok().then(|| view! {
                            <p class="text-xs text-accent mt-1">"Passwords match"</p>
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
                        {move || if loading.try_get().unwrap_or(false) { "Securing..." } else { "Continue" }}
                    </button>
        </form>
    }
}
