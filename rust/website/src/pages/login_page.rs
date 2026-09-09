use crate::auth::{self, RequestEnv, extract_auth};
use crate::env::WebsiteEnv;
use api_types::auth::LoginRequest;
use api_types::error::ApiError;
use axum::extract::Query;
use axum::response::{Html, IntoResponse, Redirect};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use cloudless_core::ports::api::ApiClientError;
use cloudless_core::ports::api::user_api_port::UserApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::forms::LoginFormView;
use url::form_urlencoded;

use crate::components::layout::Layout;

#[derive(Deserialize)]
pub struct LoginFormData {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginReturnQuery {
    pub return_url: Option<String>,
}

#[component]
fn LoginPage(error: Option<String>, action: String) -> impl IntoView {
    view! {
        <div class="min-h-[70vh] flex items-center justify-center px-6">
            <div class="w-full max-w-md glass bg-surface border border-border rounded-2xl p-8 animate-slide-up">
                <LoginFormView
                    action=action
                    title="Sign in to CloudLess"
                    subtitle="Manage your account, billing, and backups."
                    error=MaybeProp::derive({
                        let error = error.clone();
                        move || error.clone()
                    })
                />
                <div class="mt-4 text-center">
                    <a href="/forgot-password" class="text-sm text-primary-light hover:underline">"Forgot your password?"</a>
                </div>
            </div>
        </div>
    }
}

fn build_login_action(return_url: Option<&str>) -> String {
    match return_url {
        Some(url) if !url.is_empty() => {
            let encoded: String = form_urlencoded::byte_serialize(url.as_bytes()).collect();
            format!("/login?return_url={}", encoded)
        }
        _ => "/login".to_string(),
    }
}

pub async fn login_page_handler(
    Query(query): Query<LoginReturnQuery>,
    jar: CookieJar,
) -> (CookieJar, Html<String>) {
    let auth = extract_auth(&jar);

    // Clear stale cookies when arriving at login page (e.g. after session expiry redirect)
    let jar = if auth.logged_in {
        jar.remove(Cookie::build("mvs_token").path("/").build())
            .remove(Cookie::build("mvs_refresh").path("/").build())
            .remove(Cookie::build("mvs_role").path("/").build())
            .remove(Cookie::build("mvs_user_name").path("/").build())
    } else {
        jar
    };

    let action = build_login_action(query.return_url.as_deref());
    let html = view! {
        <Layout title="Login - CloudLess".to_string() description="Sign in to your CloudLess account.".to_string()>
            <LoginPage error=None action=action />
        </Layout>
    }
    .to_html();
    (jar, Html(html))
}

pub async fn logout_handler(jar: CookieJar) -> (CookieJar, Redirect) {
    let jar = jar
        .remove(Cookie::build("mvs_token").path("/").build())
        .remove(Cookie::build("mvs_refresh").path("/").build())
        .remove(Cookie::build("mvs_role").path("/").build())
        .remove(Cookie::build("mvs_user_name").path("/").build());
    (jar, Redirect::to("/"))
}

pub async fn login_submit_handler(
    Query(query): Query<LoginReturnQuery>,
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    axum::Form(form): axum::Form<LoginFormData>,
) -> Result<(CookieJar, Redirect), axum::response::Response> {
    let result = env
        .user_api()
        .login(LoginRequest {
            email: form.email,
            password: form.password,
        })
        .await;

    match result {
        Ok(resp) => {
            let jar = jar
                .add(auth::secure_cookie("mvs_token", resp.access_token))
                .add(auth::secure_cookie("mvs_refresh", resp.refresh_token))
                .add(auth::ui_cookie("mvs_role", resp.role.to_string()))
                .add(auth::ui_cookie("mvs_user_name", resp.name));

            // Honor return_url only for relative paths (security guard against open redirect)
            let redirect_to = query
                .return_url
                .filter(|url| url.starts_with('/'))
                .unwrap_or_else(|| "/".to_string());

            Ok((jar, Redirect::to(&redirect_to)))
        }
        Err(e) => {
            let error_msg = match &e {
                ApiClientError::Api(ApiError::Unauthorized) => {
                    "Invalid email or password.".to_string()
                }
                _ => format!("{}", e),
            };
            let action = build_login_action(query.return_url.as_deref());
            let html = view! {
                <Layout title="Login - CloudLess".to_string() description="Sign in to your CloudLess account.".to_string()>
                    <LoginPage error=Some(error_msg) action=action />
                </Layout>
            }
            .to_html();
            Err(Html(html).into_response())
        }
    }
}
