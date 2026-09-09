use api_types::subscription::{
    CheckoutStatusResponse, CreateCheckoutRequest, CreateCheckoutResponse, SubscriptionTier,
};
use axum::{
    extract::{Path, Query},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::CookieJar;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;

use crate::{
    auth::{RequestEnv, extract_auth, verify_auth},
    components::layout::Layout,
    env::WebsiteEnv,
};

// ---------------------------------------------------------------------------
// Checkout initiation — GET /checkout?tier=<tier>
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CheckoutQuery {
    pub tier: Option<String>,
}

/// Initiates a Dodo hosted checkout by calling the backend API.
///
/// If the user is not authenticated, redirects to /login with a return URL.
/// If the tier is invalid or the API call fails, redirects back to /pricing with
/// an error message rendered in the page (or just back to /pricing on hard errors).
pub async fn checkout_initiate_handler(
    Query(params): Query<CheckoutQuery>,
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Response {
    let auth = extract_auth(&jar);

    if !auth.logged_in {
        let tier_str = params.tier.as_deref().unwrap_or("pro");
        // URL-encode the return path so the nested query string is unambiguous
        return Redirect::to(&format!(
            "/login?return_url=%2Fcheckout%3Ftier%3D{}",
            tier_str
        ))
        .into_response();
    }

    let tier = match params.tier.as_deref() {
        Some("starter") => SubscriptionTier::Starter,
        Some("pro") => SubscriptionTier::Pro,
        Some("lifetime") => SubscriptionTier::Lifetime,
        _ => return Redirect::to("/pricing").into_response(),
    };

    let url = match env.http_api().get_url("api/subscription/checkout") {
        Ok(u) => u,
        Err(_) => return Redirect::to("/pricing").into_response(),
    };

    let req = env
        .http_api()
        .client
        .post(url)
        .json(&CreateCheckoutRequest { tier });

    match env
        .http_api()
        .send_with_auth::<CreateCheckoutResponse>(req)
        .await
    {
        Ok(resp) => Redirect::to(&resp.checkout_url).into_response(),
        Err(e) => {
            let correlation_id = uuid::Uuid::now_v7().to_string()[..8].to_string();
            let message = checkout_error_message(&e);
            tracing::error!(
                correlation_id = %correlation_id,
                error = %e,
                "Checkout initiation failed"
            );
            let auth_state = verify_auth(&env, &auth).await;
            let html = view! {
                <Layout title="Checkout Error - CloudLess".to_string() auth=auth_state>
                    <BillingErrorPage correlation_id=correlation_id message=message />
                </Layout>
            }
            .to_html();
            Html(html).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Billing return page — GET /billing/return/:checkout_id
// ---------------------------------------------------------------------------

/// Shown after the user returns from Dodo checkout.
///
/// If the user has a valid session, fetches the live checkout status from the API
/// and renders status-aware content. Falls back to the generic "processing" page
/// when the user is unauthenticated (e.g., different browser or expired session).
pub async fn billing_return_handler(
    Path(checkout_id): Path<String>,
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Html<String> {
    let auth = extract_auth(&jar);
    let auth_state = verify_auth(&env, &auth).await;

    // Fetch live status when authenticated so the page reflects the actual state.
    let checkout_status: Option<String> = if auth.logged_in {
        if let Ok(checkout_uuid) = uuid::Uuid::parse_str(&checkout_id) {
            let status_url = env.http_api().get_url(&format!(
                "api/subscription/checkout/{}/status",
                checkout_uuid
            ));
            if let Ok(url) = status_url {
                let req = env.http_api().client.get(url);
                env.http_api()
                    .send_with_auth::<CheckoutStatusResponse>(req)
                    .await
                    .ok()
                    .map(|r| r.status)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let html = view! {
        <Layout title="Thank You - CloudLess".to_string() auth=auth_state>
            <BillingReturnPage status=checkout_status />
        </Layout>
    }
    .to_html();

    Html(html)
}

// ---------------------------------------------------------------------------
// Billing cancel page — GET /billing/cancel
// ---------------------------------------------------------------------------

/// Shown when the user abandons the Dodo checkout. Simply redirects back to pricing.
pub async fn billing_cancel_handler() -> Redirect {
    Redirect::to("/pricing")
}

// ---------------------------------------------------------------------------
// Leptos components
// ---------------------------------------------------------------------------

fn checkout_error_message(error: &cloudless_core::ports::api::ApiClientError) -> String {
    match error {
        cloudless_core::ports::api::ApiClientError::Api(
            api_types::error::ApiError::Validation { message },
        )
        | cloudless_core::ports::api::ApiClientError::Api(api_types::error::ApiError::Conflict {
            message,
        }) => message.clone(),
        cloudless_core::ports::api::ApiClientError::Api(
            api_types::error::ApiError::Unauthorized,
        ) => "Your session has expired. Please sign in again before starting checkout.".to_string(),
        _ => "Something went wrong starting your checkout. Please try again or contact support."
            .to_string(),
    }
}

#[component]
fn BillingReturnPage(status: Option<String>) -> impl IntoView {
    match status.as_deref() {
        Some("completed") => view! {
            <div class="max-w-2xl mx-auto px-6 py-24 text-center">
                <div class="text-5xl mb-6">"✅"</div>
                <h1 class="text-3xl font-display font-bold text-text-primary mb-4">
                    "Payment confirmed!"
                </h1>
                <p class="text-text-secondary mb-6">
                    "Your subscription is now active. Open the CloudLess desktop app to see your updated plan."
                </p>
                <a
                    href="/"
                    class="inline-block border border-primary text-primary-light px-6 py-3 rounded-xl font-semibold hover:bg-primary-tint transition-all"
                >
                    "Back to Home"
                </a>
            </div>
        }.into_any(),
        Some("failed") => view! {
            <div class="max-w-2xl mx-auto px-6 py-24 text-center">
                <div class="text-5xl mb-6">"❌"</div>
                <h1 class="text-3xl font-display font-bold text-text-primary mb-4">
                    "Payment failed"
                </h1>
                <p class="text-text-secondary mb-6">
                    "Your payment was not completed. No charge was made. Please try again."
                </p>
                <a
                    href="/pricing"
                    class="inline-block border border-primary text-primary-light px-6 py-3 rounded-xl font-semibold hover:bg-primary-tint transition-all"
                >
                    "Try Again"
                </a>
            </div>
        }.into_any(),
        _ => view! {
            // "pending" or unknown/unauthenticated — show processing state
            <div class="max-w-2xl mx-auto px-6 py-24 text-center">
                <div class="text-5xl mb-6">"⏳"</div>
                <h1 class="text-3xl font-display font-bold text-text-primary mb-4">
                    "Checkout submitted"
                </h1>
                <p class="text-text-secondary mb-6">
                    "Your order is being confirmed. This usually takes a few seconds. "
                    "Open the CloudLess desktop app. Your plan will update automatically once payment is verified."
                </p>
                <p class="text-sm text-text-secondary mb-8">
                    "If your plan does not update within a couple of minutes, try signing out and back in."
                </p>
                <a
                    href="/"
                    class="inline-block border border-primary text-primary-light px-6 py-3 rounded-xl font-semibold hover:bg-primary-tint transition-all"
                >
                    "Back to Home"
                </a>
            </div>
        }.into_any(),
    }
}

#[component]
fn BillingErrorPage(correlation_id: String, message: String) -> impl IntoView {
    view! {
        <div class="max-w-2xl mx-auto px-6 py-24 text-center">
            <div class="text-5xl mb-6">"⚠️"</div>
            <h1 class="text-3xl font-display font-bold text-text-primary mb-4">
                "Checkout failed"
            </h1>
            <p class="text-text-secondary mb-4">
                {message}
            </p>
            <p class="text-sm text-text-secondary mb-4">
                "If this does not look right, "
                <a href="mailto:support@trycloudless.io" class="text-primary-light hover:underline">"contact support"</a>
                "."
            </p>
            <p class="text-xs text-text-secondary/60 font-mono mb-8">
                {format!("ref: {}", correlation_id)}
            </p>
            <a
                href="/pricing"
                class="inline-block border border-primary text-primary-light px-6 py-3 rounded-xl font-semibold hover:bg-primary-tint transition-all"
            >
                "Back to Pricing"
            </a>
        </div>
    }
}
