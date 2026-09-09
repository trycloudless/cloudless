use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use api_types::subscription::{SubscriptionStatus, SubscriptionTier};
use api_types::user::AdminUserListResponse;
use axum::extract::Query;
use axum::response::Html;
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::user_api_port::UserApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::pagination::Pagination;
use shared_ui::utils::format_date_long;

use crate::{components::layout::Layout, error::WebsiteError};

#[derive(Deserialize, Default)]
pub struct PageQuery {
    pub page: Option<u32>,
}

fn plan_badge(tier: SubscriptionTier) -> impl IntoView {
    let (label, class) = match tier {
        SubscriptionTier::Free => (
            "Free",
            "bg-surface border border-border text-text-secondary",
        ),
        SubscriptionTier::Starter => (
            "Starter",
            "bg-primary-tint text-primary border border-primary-glow",
        ),
        SubscriptionTier::Pro => (
            "Pro",
            "bg-accent-tint text-accent-light border border-accent-light",
        ),
        SubscriptionTier::Lifetime => (
            "Lifetime",
            "bg-warning-tint text-warning border border-warning",
        ),
    };
    view! {
        <span class={format!("inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium {}", class)}>
            {label}
        </span>
    }
}

fn status_badge(status: SubscriptionStatus) -> impl IntoView {
    let (label, class) = match status {
        SubscriptionStatus::Active => ("Active", "bg-accent-tint text-accent-light"),
        SubscriptionStatus::PastDue => ("Past Due", "bg-warning-tint text-warning"),
        SubscriptionStatus::OnHold => ("On Hold", "bg-warning-tint text-warning"),
        SubscriptionStatus::Canceled => ("Canceled", "bg-error-tint text-error"),
        SubscriptionStatus::Expired => (
            "Expired",
            "bg-surface text-text-disabled border border-border",
        ),
    };
    view! {
        <span class={format!("inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium {}", class)}>
            {label}
        </span>
    }
}

#[component]
fn UsersPage(resp: AdminUserListResponse) -> impl IntoView {
    let total_pages = resp.total_count.div_ceil(resp.per_page as u64) as u32;
    let showing_start = ((resp.page - 1) * resp.per_page + 1).min(resp.total_count as u32);
    let showing_end = (resp.page * resp.per_page).min(resp.total_count as u32);

    view! {
        <div class="max-w-6xl mx-auto px-6 py-16">
            <div class="mb-10 flex items-end justify-between">
                <div>
                    <h1 class="text-4xl font-display font-bold gradient-text mb-2">"Users"</h1>
                    <p class="text-text-secondary">
                        {format!("{} registered users", resp.total_count)}
                    </p>
                </div>
                {if resp.total_count > resp.per_page as u64 {
                    view! {
                        <p class="text-sm text-text-secondary">
                            {format!("Showing {}-{} of {}", showing_start, showing_end, resp.total_count)}
                        </p>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </div>

            <div class="glass bg-surface border border-border rounded-2xl overflow-hidden">
                <table class="w-full">
                    <thead>
                        <tr class="border-b border-border text-left text-sm text-text-secondary">
                            <th class="px-6 py-3 font-medium">"Name"</th>
                            <th class="px-6 py-3 font-medium">"Email"</th>
                            <th class="px-6 py-3 font-medium">"Joined Since"</th>
                            <th class="px-6 py-3 font-medium">"Plan"</th>
                            <th class="px-6 py-3 font-medium">"Status"</th>
                            <th class="px-6 py-3 font-medium text-center">"Verified"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {resp.users.into_iter().map(|user| {
                            let joined = format_date_long(&user.created_at);
                            view! {
                                <tr class="border-b border-border-alpha last:border-0 hover:bg-surface-alpha">
                                    <td class="px-6 py-4 text-text-primary font-medium">{user.name}</td>
                                    <td class="px-6 py-4 text-text-secondary text-sm">{user.email}</td>
                                    <td class="px-6 py-4 text-text-secondary text-sm">{joined}</td>
                                    <td class="px-6 py-4">{plan_badge(user.plan)}</td>
                                    <td class="px-6 py-4">{status_badge(user.plan_status)}</td>
                                    <td class="px-6 py-4 text-center">
                                        {if user.email_verified {
                                            view! {
                                                <span class="text-accent-light" title="Verified">
                                                    <svg class="w-4 h-4 inline" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                                        <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                                                        <polyline points="22 4 12 14.01 9 11.01"/>
                                                    </svg>
                                                </span>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <span class="text-text-disabled" title="Not verified">
                                                    <svg class="w-4 h-4 inline" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                                        <circle cx="12" cy="12" r="10"/>
                                                        <line x1="15" y1="9" x2="9" y2="15"/>
                                                        <line x1="9" y1="9" x2="15" y2="15"/>
                                                    </svg>
                                                </span>
                                            }.into_any()
                                        }}
                                    </td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>

            <Pagination
                current_page=resp.page
                total_pages=total_pages
                base_url="/users".to_string()
            />
        </div>
    }
}

pub async fn users_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Query(q): Query<PageQuery>,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;

    if !state.is_admin() {
        let html = view! {
            <Layout title="Unauthorized - CloudLess".to_string() auth=state>
                <div class="max-w-2xl mx-auto px-6 py-24 text-center">
                    <h1 class="text-4xl font-display font-bold text-error mb-4">"Unauthorized"</h1>
                    <p class="text-text-secondary">"You don't have permission to view this page."</p>
                </div>
            </Layout>
        }
        .to_html();
        return Ok(Html(html));
    }

    let token = auth.token.as_deref().unwrap_or_default();
    let page = q.page.unwrap_or(1).max(1);
    let resp = env.user_api().list_all_users(token, page, 20).await?;

    let html = view! {
        <Layout title="Users - CloudLess".to_string() auth=state>
            <UsersPage resp=resp />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
