use crate::auth::{RequestEnv, extract_auth, verify_auth};
use axum::response::Html;
use axum_extra::extract::cookie::CookieJar;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::forms::ForgotPasswordFormView;

use crate::components::layout::Layout;

#[derive(Deserialize)]
pub struct ForgotPasswordFormData {
    pub email: String,
}

#[component]
fn ForgotPasswordPage(success: Option<String>, error: Option<String>) -> impl IntoView {
    view! {
        <div class="min-h-[70vh] flex items-center justify-center px-6">
            <div class="w-full max-w-md glass bg-surface border border-border rounded-2xl p-8 animate-slide-up">
                <ForgotPasswordFormView
                    action="/forgot-password".to_string()
                    error=MaybeProp::derive({
                        let error = error.clone();
                        move || error.clone()
                    })
                    success=MaybeProp::derive({
                        let success = success.clone();
                        move || success.clone()
                    })
                >
                    <div class="mt-6 text-center">
                        <a href="/login" class="text-sm text-primary-light hover:underline">"Back to login"</a>
                    </div>
                </ForgotPasswordFormView>
            </div>
        </div>
    }
}

pub async fn forgot_password_handler(RequestEnv(env): RequestEnv, jar: CookieJar) -> Html<String> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    let html = view! {
        <Layout title="Forgot Password - CloudLess".to_string() auth=state description="Reset your CloudLess account password.".to_string()>
            <ForgotPasswordPage success=None error=None />
        </Layout>
    }
    .to_html();
    Html(html)
}

pub async fn forgot_password_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    axum::Form(form): axum::Form<ForgotPasswordFormData>,
) -> Html<String> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;

    let api_base =
        std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let client = reqwest::Client::new();
    let _ = client
        .post(format!("{}/password-reset/request", api_base))
        .json(&api_types::password_reset::PasswordResetRequest { email: form.email })
        .send()
        .await;

    let html = view! {
        <Layout title="Forgot Password - CloudLess".to_string() auth=state description="Reset your CloudLess account password.".to_string()>
            <ForgotPasswordPage
                success=Some("If that email exists, a reset link has been sent. Check your inbox.".to_string())
                error=None
            />
        </Layout>
    }
    .to_html();
    Html(html)
}
