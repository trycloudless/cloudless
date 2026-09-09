use crate::components::common::{TauriClientError, tauri_invoke};
use crate::state::*;
use api_types::auth::{LoginRequest, LoginResponse};
use leptos::prelude::*;
use shared_ui::forms::LoginFormView;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn LoginForm() -> impl IntoView {
    let (_, set_auth_mode) = use_context::<AuthModeSignal>().unwrap();
    let (_, set_phase) = use_context::<PhaseSignal>().unwrap();
    let (_, set_session) = use_context::<SessionSignal>().unwrap();

    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);

        let form: web_sys::HtmlFormElement = ev.target().unwrap().unchecked_into();
        let form_data = web_sys::FormData::new_with_form(&form).unwrap();
        let email = form_data.get("email").as_string().unwrap_or_default();
        let password = form_data.get("password").as_string().unwrap_or_default();

        spawn_local(async move {
            let password_clone = password.clone();
            let req = LoginRequest {
                email: email.clone(),
                password,
            };

            match tauri_invoke::<_, LoginResponse>("login", "request", req).await {
                Ok(resp) => {
                    let needs_setup = resp.encryption_setup_required;
                    set_session.update(|s| {
                        s.user_id = Some(resp.user_id);
                        s.user_name = resp.name.clone();
                        s.user_email = resp.email.clone();
                    });
                    if needs_setup {
                        set_phase.set(AppPhase::Setup(SetupStep::EncryptionPassword));
                    } else {
                        set_auth_mode.set(AuthMode::UnlockEncryption);
                    }
                    return;
                }
                Err(e) if matches!(e, TauriClientError::Validation { .. }) => {
                    set_session.update(|s| {
                        s.user_email = email.clone();
                        s.pending_password = password_clone;
                    });
                    set_phase.set(AppPhase::EmailVerification);
                    return;
                }
                Err(TauriClientError::Unauthorized { .. }) => {
                    let _ = set_error.try_set(Some("Invalid email or password.".to_string()));
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.user_message().to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    let on_forgot = move |_| {
        set_auth_mode.set(AuthMode::ForgotPassword);
    };

    view! {
        <div on:submit=on_submit>
            <LoginFormView
                error=MaybeProp::derive(move || error.get())
                loading=Signal::derive(move || loading.get())
            >
                <div class="mb-5 text-right">
                    <button
                        type="button"
                        class="text-sm text-primary-light hover:underline"
                        on:click=on_forgot
                    >
                        "Forgot your password?"
                    </button>
                </div>
            </LoginFormView>
        </div>
    }
}
