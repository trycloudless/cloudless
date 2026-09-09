use crate::components::common::tauri_invoke;
use crate::state::*;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use shared_ui::forms::SignupFormView;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, Serialize)]
struct SignupAndLoginRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SignupAndLoginResponse {
    pub user_id: Uuid,
    pub email_verified: bool,
}

#[component]
pub fn SignupForm() -> impl IntoView {
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
        let name_val = form_data.get("name").as_string().unwrap_or_default();
        let email_val = form_data.get("email").as_string().unwrap_or_default();
        let password = form_data.get("password").as_string().unwrap_or_default();

        let name_clone = name_val.clone();
        let email_clone = email_val.clone();

        spawn_local(async move {
            let password_clone = password.clone();
            let req = SignupAndLoginRequest {
                name: name_val,
                email: email_val,
                password,
            };

            match tauri_invoke::<_, SignupAndLoginResponse>("signup_and_login", "request", req)
                .await
            {
                Ok(resp) => {
                    set_session.update(|s| {
                        s.user_id = Some(resp.user_id);
                        s.user_name = name_clone;
                        s.user_email = email_clone;
                        s.is_first_time_setup = resp.email_verified;
                        s.pending_password = if resp.email_verified {
                            String::new()
                        } else {
                            password_clone
                        };
                    });
                    if resp.email_verified {
                        set_phase.set(AppPhase::PolicyAcceptance);
                    } else {
                        set_phase.set(AppPhase::EmailVerification);
                    }
                    return;
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(e.user_message().to_string()));
                }
            }
            let _ = set_loading.try_set(false);
        });
    };

    view! {
        <div on:submit=on_submit>
            <SignupFormView
                error=MaybeProp::derive(move || error.get())
                loading=Signal::derive(move || loading.get())
            />
        </div>
    }
}
