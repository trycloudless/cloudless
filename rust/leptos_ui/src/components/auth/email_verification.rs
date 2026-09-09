use crate::components::common::tauri_invoke;
use crate::state::*;
use api_types::auth::{LoginRequest, LoginResponse};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, Serialize)]
struct VerifyEmailRequest {
    pub email: String,
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
struct VerifyEmailResponse {
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ResendVerificationRequest {
    pub email: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ResendVerificationResponse {
    pub message: String,
}

/// Email verification screen shown after signup.
///
/// Displays a 6-digit code input field and a resend button. On successful
/// verification, transitions to the Setup phase.
#[component]
pub fn EmailVerification() -> impl IntoView {
    let (phase, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (session, set_session) = use_context::<SessionSignal>().unwrap();

    let (error, set_error) = signal(Option::<String>::None);
    let (success, set_success) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let (resending, set_resending) = signal(false);

    // Only render when in EmailVerification phase
    let email = move || session.get().user_email.clone();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

        let form: web_sys::HtmlFormElement = ev.target().unwrap().unchecked_into();
        let form_data = web_sys::FormData::new_with_form(&form).unwrap();
        let code = form_data
            .get("code")
            .as_string()
            .unwrap_or_default()
            .trim()
            .to_string();

        let user_email = session.get_untracked().user_email.clone();

        spawn_local(async move {
            let req = VerifyEmailRequest {
                email: user_email.clone(),
                code,
            };

            match tauri_invoke::<_, VerifyEmailResponse>("verify_email", "request", req).await {
                Ok(resp) if resp.verified => {
                    let pending_password = session.get_untracked().pending_password.clone();
                    let login_req = LoginRequest {
                        email: user_email,
                        password: pending_password,
                    };
                    match tauri_invoke::<_, LoginResponse>("login", "request", login_req).await {
                        Ok(login_resp) => {
                            set_session.update(|s| {
                                s.user_id = Some(login_resp.user_id);
                                s.user_name = login_resp.name.clone();
                                s.is_first_time_setup = true;
                                s.pending_password = String::new();
                            });
                            set_phase.set(AppPhase::PolicyAcceptance);
                        }
                        Err(e) => {
                            let _ = set_error.try_set(Some(e.user_message().to_string()));
                        }
                    }
                    return;
                }
                Ok(_) => {
                    let _ = set_error.try_set(Some("Verification failed. Try again.".to_string()));
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.user_message().to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    let on_resend = move |_| {
        set_resending.set(true);
        set_error.set(None);
        set_success.set(None);

        let user_email = session.get_untracked().user_email.clone();

        spawn_local(async move {
            let req = ResendVerificationRequest { email: user_email };

            match tauri_invoke::<_, ResendVerificationResponse>(
                "resend_verification",
                "request",
                req,
            )
            .await
            {
                Ok(resp) => {
                    let _ = set_success.try_set(Some(resp.message));
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.user_message().to_string()));
                }
            }
            let _ = set_resending.try_set(false);
        });
    };

    view! {
        <div class="min-h-screen flex items-center justify-center p-6 bg-bg">
            <div class="w-full max-w-md glass bg-surface border border-border rounded-2xl p-8">
                <div class="text-center mb-6">
                    <div class="w-16 h-16 bg-primary-tint rounded-2xl flex items-center justify-center mx-auto mb-4">
                        <svg class="w-8 h-8 text-primary-light" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                        </svg>
                    </div>
                    <h2 class="text-3xl font-display font-bold gradient-text mb-2">"Verify Your Email"</h2>
                    <p class="text-text-secondary text-sm">
                        "We sent a 6-digit code to "
                        <span class="text-text-primary font-medium">{email}</span>
                    </p>
                </div>

                {move || error.get().map(|e| view! {
                    <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm mb-4">{e}</div>
                })}

                {move || success.get().map(|msg| view! {
                    <div class="bg-accent-tint border border-accent text-accent rounded-xl px-4 py-3 text-sm mb-4">{msg}</div>
                })}

                <form on:submit=on_submit class="space-y-5">
                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2" for="verification-code">
                            "Verification Code"
                        </label>
                        <input
                            type="text"
                            id="verification-code"
                            name="code"
                            maxlength="6"
                            pattern="[0-9]{6}"
                            inputmode="numeric"
                            autocomplete="one-time-code"
                            placeholder="000000"
                            class="w-full bg-input border border-border rounded-xl px-4 py-4 text-text-primary text-center text-2xl font-mono tracking-[0.5em] focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                            required
                        />
                    </div>

                    <button
                        type="submit"
                        class="w-full btn-gradient text-white py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                        disabled=move || loading.get()
                    >
                        {move || if loading.get() { "Verifying..." } else { "Verify Email" }}
                    </button>
                </form>

                <div class="mt-6 text-center">
                    <p class="text-text-secondary text-sm mb-2">"Didn't receive the code?"</p>
                    <button
                        type="button"
                        class="text-primary-light hover:text-text-primary text-sm font-medium transition-colors"
                        disabled=move || resending.get()
                        on:click=on_resend
                    >
                        {move || if resending.get() { "Sending..." } else { "Resend Code" }}
                    </button>
                </div>

                <div class="mt-4 text-center">
                    <button
                        type="button"
                        class="text-text-secondary hover:text-text-primary text-xs transition-colors"
                        on:click=move |_| set_phase.set(AppPhase::Auth)
                    >
                        "Back to Sign In"
                    </button>
                </div>
            </div>
        </div>
    }
}
