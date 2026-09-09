use crate::state::*;
use api_types::password_reset::PasswordResetRequest;
use leptos::prelude::*;
use shared_ui::forms::ForgotPasswordFormView;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

const API_BASE_URL: &str = match option_env!("API_BASE_URL") {
    Some(url) => url,
    None => "http://localhost:9000",
};

async fn request_password_reset(email: &str) -> Result<(), String> {
    let url = format!("{}/password-reset/request", API_BASE_URL);

    let req = PasswordResetRequest {
        email: email.to_string(),
    };
    let body = serde_wasm_bindgen::to_value(&req).map_err(|e| e.to_string())?;
    let body = js_sys::JSON::stringify(&body)
        .map_err(|e| format!("{:?}", e))?
        .as_string()
        .ok_or("Failed to stringify JSON")?;

    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body));

    let headers = web_sys::Headers::new().map_err(|e| format!("{:?}", e))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{:?}", e))?;
    opts.set_headers(&headers);

    let request =
        web_sys::Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{:?}", e))?;

    let window = web_sys::window().ok_or("No window")?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{:?}", e))?;

    let resp: web_sys::Response = resp_value.unchecked_into();
    if !resp.ok() {
        return Err("Request failed".to_string());
    }

    Ok(())
}

#[component]
pub fn ForgotPasswordForm() -> impl IntoView {
    let (_, set_auth_mode) = use_context::<AuthModeSignal>().unwrap();

    let (error, set_error) = signal(Option::<String>::None);
    let (success, set_success) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);
        set_error.set(None);
        set_success.set(None);

        let form: web_sys::HtmlFormElement = ev.target().unwrap().unchecked_into();
        let form_data = web_sys::FormData::new_with_form(&form).unwrap();
        let email = form_data.get("email").as_string().unwrap_or_default();

        spawn_local(async move {
            // Always show success to prevent email enumeration
            let _ = request_password_reset(&email).await;
            let _ = set_success.try_set(Some(
                "If that email exists, a reset link has been sent. Check your inbox.".to_string(),
            ));
            let _ = set_loading.try_set(false);
        });
    };

    let on_back = move |_| {
        set_auth_mode.set(AuthMode::Login);
    };

    view! {
        <div on:submit=on_submit>
            <ForgotPasswordFormView
                error=MaybeProp::derive(move || error.get())
                success=MaybeProp::derive(move || success.get())
                loading=Signal::derive(move || loading.get())
            >
                <div class="mt-6 text-center">
                    <button
                        type="button"
                        class="text-sm text-primary-light hover:underline"
                        on:click=on_back
                    >
                        "Back to login"
                    </button>
                </div>
            </ForgotPasswordFormView>
        </div>
    }
}
