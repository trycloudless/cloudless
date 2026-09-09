use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::{
    extract::Path,
    response::{Html, Redirect},
};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::email_template_api_port::EmailTemplateApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use shared_ui::display::SuccessAlert;

use crate::{components::layout::Layout, error::WebsiteError};
use api_types::email_template::EmailTemplateSummary;

#[component]
fn EmailTemplateListPage(
    templates: Vec<EmailTemplateSummary>,
    success: Option<String>,
) -> impl IntoView {
    view! {
        <div class="max-w-5xl mx-auto px-6 py-16">
            <div class="flex items-center justify-between mb-8">
                <h1 class="text-4xl font-display font-bold gradient-text">"Email Templates"</h1>
                <a
                    href="/create-email-template"
                    class="inline-flex items-center gap-2 btn-gradient text-white px-5 py-2 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all"
                >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                    </svg>
                    "New Template"
                </a>
            </div>

            {success.map(|msg| view! { <SuccessAlert message=msg /> })}

            {if templates.is_empty() {
                view! {
                    <p class="text-center text-text-secondary text-lg py-12">"No email templates yet. Create one to get started."</p>
                }.into_any()
            } else {
                view! {
                    <div class="bg-surface border border-border rounded-xl overflow-hidden">
                        <table class="w-full">
                            <thead>
                                <tr class="border-b border-border text-left text-sm text-text-secondary">
                                    <th class="px-6 py-4 font-medium">"Name"</th>
                                    <th class="px-6 py-4 font-medium">"Subject"</th>
                                    <th class="px-6 py-4 font-medium">"Type"</th>
                                    <th class="px-6 py-4 font-medium">"Updated"</th>
                                    <th class="px-6 py-4 font-medium text-right">"Actions"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {templates.into_iter().map(|t| {
                                    let edit_url = format!("/edit-email-template/{}", t.id);
                                    let delete_url = format!("/delete-email-template/{}", t.id);
                                    let template_type = t.template_type.to_string();
                                    let updated = t.updated_at.format("%Y-%m-%d %H:%M").to_string();
                                    view! {
                                        <tr class="border-b border-border-alpha hover:bg-surface-alpha transition-colors">
                                            <td class="px-6 py-4 text-text-primary font-medium">{t.name}</td>
                                            <td class="px-6 py-4 text-text-secondary">{t.subject}</td>
                                            <td class="px-6 py-4">
                                                <span class="inline-block px-3 py-1 rounded-full text-xs font-medium bg-primary-tint text-primary-light">
                                                    {template_type}
                                                </span>
                                            </td>
                                            <td class="px-6 py-4 text-text-secondary text-sm">{updated}</td>
                                            <td class="px-6 py-4 text-right">
                                                <div class="flex items-center justify-end gap-2">
                                                    <a
                                                        href=edit_url
                                                        class="text-sm text-primary-light hover:underline"
                                                    >
                                                        "Edit"
                                                    </a>
                                                    <form method="POST" action=delete_url style="display:inline" onsubmit="return confirm('Delete this template?')">
                                                        <button type="submit" class="text-sm text-error hover:underline">"Delete"</button>
                                                    </form>
                                                </div>
                                            </td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

pub async fn list_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        let html = view! {
            <Layout title="Unauthorized - CloudLess".to_string() auth=state>
                <div class="max-w-2xl mx-auto px-6 py-24 text-center">
                    <h1 class="text-4xl font-display font-bold text-error mb-4">"Unauthorized"</h1>
                    <p class="text-text-secondary">"Only admins can manage email templates."</p>
                </div>
            </Layout>
        }
        .to_html();
        return Ok(Html(html));
    }

    let list = env.email_template_api().list_all().await?;

    let html = view! {
        <Layout title="Email Templates - CloudLess".to_string() auth=state>
            <EmailTemplateListPage templates=list.templates success=None />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn delete_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(id): Path<uuid::Uuid>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let _ = env.email_template_api().delete(id).await;

    let list = env
        .email_template_api()
        .list_all()
        .await
        .map(|l| l.templates)
        .unwrap_or_default();

    let html = view! {
        <Layout title="Email Templates - CloudLess".to_string() auth=state>
            <EmailTemplateListPage templates=list success=Some("Template deleted successfully.".to_string()) />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
