use crate::auth::{RequestEnv, extract_auth, verify_auth};
use axum::{extract::Query, response::Html};
use axum_extra::extract::cookie::CookieJar;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::display::{ErrorAlert, SuccessAlert};

use crate::components::layout::Layout;

#[derive(Deserialize)]
pub struct ResetPasswordQuery {
    pub token: Option<String>,
}

#[derive(Deserialize)]
pub struct ResetPasswordFormData {
    pub token: String,
    pub new_password: String,
    pub confirm_password: String,
}

#[component]
fn ResetPasswordPage(
    token: String,
    success: Option<String>,
    error: Option<String>,
) -> impl IntoView {
    view! {
        <div class="min-h-[70vh] flex items-center justify-center px-6">
            <div class="w-full max-w-md glass bg-surface border border-border rounded-2xl p-8 animate-slide-up">
                <h2 class="text-4xl font-display font-bold gradient-text mb-2">"Reset Password"</h2>
                <p class="text-text-secondary mb-8">"Enter your new password below."</p>

                {success.map(|msg| view! {
                    <div>
                        <SuccessAlert message=msg />
                        <div class="mt-4 text-center">
                            <a href="/login" class="text-sm text-primary-light hover:underline">"Go to login"</a>
                        </div>
                    </div>
                })}
                {error.map(|e| view! { <ErrorAlert message=e /> })}

                <form method="POST" action="/reset-password" class="space-y-5">
                    <input type="hidden" name="token" value=token />

                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2">"New Password"</label>
                        <input
                            type="password"
                            name="new_password"
                            placeholder="\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                            required
                        />
                    </div>

                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2">"Confirm Password"</label>
                        <input
                            type="password"
                            name="confirm_password"
                            placeholder="\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                            required
                        />
                    </div>

                    <button
                        type="submit"
                        class="w-full btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                    >
                        "Reset Password"
                    </button>
                </form>
            </div>
        </div>
    }
}

pub async fn reset_password_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Query(query): Query<ResetPasswordQuery>,
) -> Html<String> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    let token = query.token.unwrap_or_default();

    let html = view! {
        <Layout title="Reset Password - CloudLess".to_string() auth=state>
            <ResetPasswordPage token=token success=None error=None />
        </Layout>
    }
    .to_html();
    Html(html)
}

pub async fn reset_password_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    axum::Form(form): axum::Form<ResetPasswordFormData>,
) -> Html<String> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;

    if form.new_password != form.confirm_password {
        let html = view! {
            <Layout title="Reset Password - CloudLess".to_string() auth=state>
                <ResetPasswordPage
                    token=form.token
                    success=None
                    error=Some("Passwords do not match.".to_string())
                />
            </Layout>
        }
        .to_html();
        return Html(html);
    }

    let api_base =
        std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let client = reqwest::Client::new();
    let result = client
        .post(format!("{}/password-reset/confirm", api_base))
        .json(&api_types::password_reset::PasswordResetConfirmRequest {
            token: form.token.clone(),
            new_password: form.new_password,
        })
        .send()
        .await;

    let (success, error) = match result {
        Ok(resp) if resp.status().is_success() => (
            Some("Password has been reset successfully. You can now log in.".to_string()),
            None,
        ),
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            (None, Some(format!("Reset failed: {}", body)))
        }
        Err(e) => (None, Some(format!("Request failed: {}", e))),
    };

    let html = view! {
        <Layout title="Reset Password - CloudLess".to_string() auth=state>
            <ResetPasswordPage token=form.token success=success error=error />
        </Layout>
    }
    .to_html();
    Html(html)
}
