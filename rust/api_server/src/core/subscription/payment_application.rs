use std::collections::HashMap;

use api_types::subscription::{
    CreateCheckoutResponse, CreatePortalResponse, Subscription, SubscriptionStatus,
    SubscriptionTier, TierChangeReason,
};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::core::{
    CoreError, CoreResult,
    ports::{
        CreateCheckoutSessionRecord, RecordWebhookEventParams, SubscriptionChangeParams,
        SubscriptionRepo,
        payment_provider::{CreateCheckoutParams, PaymentProvider},
        payment_webhook::{PaymentEvent, PaymentWebhookVerifier, VerifiedPaymentEvent, WebhookPaymentData, WebhookSubscriptionData},
    },
    subscription::env::{PaymentEnv, SubscriptionEnv},
};

// ---------------------------------------------------------------------------
// Checkout
// ---------------------------------------------------------------------------

/// Create a hosted checkout session for the requested tier.
///
/// Validates that:
/// - the requested tier is a paid tier
/// - the user does not already have an equal-or-higher active subscription
/// - a product ID is configured for the tier
pub async fn create_checkout<E>(
    env: &E,
    user_id: Uuid,
    customer_email: String,
    customer_name: String,
    requested_tier: SubscriptionTier,
) -> CoreResult<CreateCheckoutResponse>
where
    E: SubscriptionEnv + PaymentEnv,
{
    if !requested_tier.is_paid() {
        return Err(CoreError::validation("Free tier cannot be purchased"));
    }

    // Reject if the user's effective tier is already >= the requested tier.
    // Uses effective_tier() rather than raw status so that a Canceled user who
    // still has paid access within their current_period_end is correctly blocked
    // from purchasing the same plan again and creating a duplicate subscription.
    if let Some(existing) = env.subscription_repo().get_by_user_id(user_id).await? {
        if existing.effective_tier().rank() >= requested_tier.rank() {
            return Err(CoreError::validation(
                "You already have an equal or higher active subscription",
            ));
        }
    }

    // Map tier → configured product ID; fail early if unconfigured
    let product_id = env.product_id_for_tier(requested_tier).ok_or_else(|| {
        CoreError::internal(format!(
            "Product ID not configured for tier: {}",
            requested_tier
        ))
    })?;

    let checkout_id = Uuid::now_v7();
    let return_url = format!(
        "{}/{}",
        env.return_url_base().trim_end_matches('/'),
        checkout_id
    );
    let cancel_url = env.cancel_url_base().to_string();

    // Re-use existing provider customer ID if available
    let provider_customer_id =
        if let Some(sub) = env.subscription_repo().get_by_user_id(user_id).await? {
            env.subscription_repo()
                .get_provider_fields(sub.id)
                .await?
                .and_then(|f| f.provider_customer_id)
        } else {
            None
        };

    let checkout_params = CreateCheckoutParams {
        cloudless_user_id: user_id,
        cloudless_checkout_id: checkout_id,
        tier: requested_tier,
        product_id: product_id.clone(),
        customer_email,
        customer_name,
        provider_customer_id,
        return_url: return_url.clone(),
        cancel_url: cancel_url.clone(),
        cloudless_environment: env.payment_environment().to_string(),
    };

    let result = env
        .payment_provider()
        .create_checkout_session(checkout_params)
        .await?;

    // If this DB insert fails the provider session already exists. The user's payment will
    // still be granted via webhook (cloudless_user_id in metadata), but the local ops
    // trail is missing. Log prominently so support can recover.
    env.subscription_repo()
        .create_checkout_session_record(CreateCheckoutSessionRecord {
            id: checkout_id,
            user_id,
            tier: requested_tier,
            provider: "dodo".to_string(),
            provider_checkout_session_id: result.provider_checkout_session_id,
            provider_product_id: product_id,
            checkout_url: result.checkout_url.clone(),
            return_url,
            cancel_url,
            metadata: json!({}),
        })
        .await
        .map_err(|e| {
            tracing::error!(
                %user_id,
                %checkout_id,
                checkout_url = %result.checkout_url,
                error = %e,
                "Checkout session created but local DB record insert failed. \
                 The checkout URL above allows the user to complete payment; \
                 entitlement will still be granted via webhook. \
                 Operator action: share the checkout URL with the user if needed."
            );
            e
        })?;

    tracing::info!(user_id = %user_id, tier = %requested_tier, "Checkout session created");

    Ok(CreateCheckoutResponse {
        checkout_url: result.checkout_url,
        checkout_id,
    })
}

// ---------------------------------------------------------------------------
// Customer portal
// ---------------------------------------------------------------------------

/// Create a customer portal session for the authenticated user.
pub async fn create_portal<E>(env: &E, user_id: Uuid) -> CoreResult<CreatePortalResponse>
where
    E: SubscriptionEnv + PaymentEnv,
{
    let sub = env
        .subscription_repo()
        .get_by_user_id(user_id)
        .await?
        .ok_or_else(|| CoreError::validation("No subscription found"))?;

    let provider_fields = env
        .subscription_repo()
        .get_provider_fields(sub.id)
        .await?
        .ok_or_else(|| CoreError::validation("No subscription found"))?;

    let customer_id = provider_fields.provider_customer_id.ok_or_else(|| {
        CoreError::validation("No billing account found — complete a purchase first")
    })?;

    let portal_url = env
        .payment_provider()
        .create_customer_portal_session(&customer_id)
        .await?;

    Ok(CreatePortalResponse { portal_url })
}

// ---------------------------------------------------------------------------
// Webhook handling
// ---------------------------------------------------------------------------

/// Verify, persist, and enqueue a webhook event for async processing.
///
/// Returns `Ok(())` quickly; heavy state-machine processing runs in a spawned task.
/// If the server restarts before processing completes, the row stays `pending` for
/// recovery via `recover_pending_webhooks`.
pub async fn handle_webhook<E>(
    env: &E,
    headers: HashMap<String, String>,
    raw_body: &[u8],
) -> CoreResult<()>
where
    E: SubscriptionEnv + PaymentEnv + Clone + Send + Sync + 'static,
{
    // 1. Verify signature and parse — returns 401 on failure
    let verified: VerifiedPaymentEvent = env
        .payment_webhook_verifier()
        .verify_and_parse(&headers, raw_body)?;

    let raw_headers = {
        let mut m = serde_json::Map::new();
        for (k, v) in &headers {
            m.insert(k.clone(), serde_json::Value::String(v.clone()));
        }
        serde_json::Value::Object(m)
    };

    // 2. Resolve subscription_id for the event row (None when no match yet)
    let subscription_id = resolve_subscription_id(env, &verified).await;

    // 3. Persist event — returns false if webhook_id already seen (idempotent)
    let webhook_id = headers
        .get("webhook-id")
        .cloned()
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    let is_new = env
        .subscription_repo()
        .record_webhook_event(RecordWebhookEventParams {
            subscription_id,
            webhook_id: &webhook_id,
            event_type: &verified.event_type,
            provider: "dodo",
            raw_headers,
            payload: verified.raw_payload.clone(),
            verification_status: "verified",
        })
        .await?;

    if !is_new {
        tracing::info!(webhook_id, "Duplicate webhook delivery — skipping");
        return Ok(());
    }

    // 4. Async processing — state machine runs in background
    let env_clone = env.clone();
    let webhook_id_owned = webhook_id.clone();
    let event = verified.event;

    tokio::spawn(async move {
        let result = process_webhook_event(&env_clone, &webhook_id_owned, event).await;
        let err_str = result.as_ref().err().map(|e| e.to_string());
        let _ = env_clone
            .subscription_repo()
            .mark_event_processed(&webhook_id_owned, err_str.as_deref())
            .await;
        if let Err(e) = result {
            tracing::error!(webhook_id = %webhook_id_owned, error = %e, "Webhook processing failed");
        } else {
            tracing::info!(webhook_id = %webhook_id_owned, "Webhook processed successfully");
        }
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// State machine: PaymentEvent → internal subscription state
// ---------------------------------------------------------------------------

/// Dispatch a verified payment event to the appropriate subscription handler.
///
/// All provider-specific parsing has already happened; this function only
/// contains state-machine logic and repository calls.
async fn process_webhook_event<E>(
    env: &E,
    webhook_id: &str,
    event: PaymentEvent,
) -> CoreResult<()>
where
    E: SubscriptionEnv + PaymentEnv,
{
    match event {
        PaymentEvent::SubscriptionActive(data) => {
            handle_subscription_active(env, webhook_id, "subscription.active", data).await
        }
        PaymentEvent::SubscriptionOnHold(data) => {
            handle_subscription_on_hold(env, webhook_id, data).await
        }
        PaymentEvent::SubscriptionCancelled(data) => {
            handle_subscription_cancelled(env, webhook_id, data).await
        }
        PaymentEvent::SubscriptionExpired(data) => {
            handle_subscription_expired(env, webhook_id, data).await
        }
        PaymentEvent::PaymentSucceeded(data) => {
            handle_payment_succeeded(env, webhook_id, data).await
        }
        PaymentEvent::PaymentFailed(data) => handle_payment_failed(env, webhook_id, data).await,
        PaymentEvent::Unknown { event_type } => {
            tracing::info!(webhook_id, event_type, "Unhandled webhook event type");
            Ok(())
        }
    }
}

async fn handle_subscription_active<E>(
    env: &E,
    webhook_id: &str,
    event_type: &str,
    data: WebhookSubscriptionData,
) -> CoreResult<()>
where
    E: SubscriptionEnv + PaymentEnv,
{
    // Try to find an already-linked subscription first. On first activation the
    // provider_subscription_id isn't set yet, so fall back to cloudless_user_id
    // embedded in the checkout metadata.
    let sub = match env
        .subscription_repo()
        .get_by_provider_subscription_id(&data.provider_subscription_id)
        .await?
    {
        Some(s) => s,
        None => {
            let user_id = parse_user_id_from_metadata(&data.metadata);
            if user_id == Uuid::nil() {
                tracing::warn!(
                    webhook_id,
                    provider_subscription_id = %data.provider_subscription_id,
                    "subscription.active: no match and no cloudless_user_id in metadata"
                );
                return Ok(());
            }
            let Some(s) = env.subscription_repo().get_by_user_id(user_id).await? else {
                tracing::warn!(webhook_id, %user_id, "subscription.active: no subscription row for user");
                return Ok(());
            };
            s
        }
    };

    // Link provider IDs (idempotent — safe to call on renewals too).
    env.subscription_repo()
        .set_provider_ids(
            sub.id,
            Some(&data.provider_subscription_id),
            &data.provider_customer_id,
        )
        .await?;

    // product_id is required and authoritative on all subscription events.
    // An unrecognized product_id is a configuration error — fail the event so it
    // appears in failed-event monitoring rather than being silently marked 'done'.
    let new_tier = match tier_from_product_id(env, &data.product_id) {
        Some(t) => t,
        None => {
            tracing::error!(
                webhook_id,
                product_id = %data.product_id,
                event_type,
                "Unrecognized product_id on subscription event. \
                 Check product ID configuration."
            );
            return Err(CoreError::internal(format!(
                "Unrecognized product_id '{}' on {} event — check product ID configuration",
                data.product_id, event_type
            )));
        }
    };

    let reason = match event_type {
        "subscription.renewed" => TierChangeReason::Renewal,
        "subscription.plan_changed" => SubscriptionTier::change_reason(&sub.tier, &new_tier),
        _ if matches!(
            sub.status,
            SubscriptionStatus::Canceled | SubscriptionStatus::Expired
        ) =>
        {
            TierChangeReason::Reactivation
        }
        _ => SubscriptionTier::change_reason(&sub.tier, &new_tier),
    };

    apply_subscription_change(
        env,
        &sub,
        new_tier,
        SubscriptionStatus::Active,
        reason,
        data.period_start,
        data.period_end,
        data.cancel_at_period_end,
        webhook_id,
        &data.provider_status,
        Some(&data.product_id),
        data.period_end,
        None,
    )
    .await?;

    // Mark the originating checkout session complete. Non-fatal: entitlement is
    // already committed above; this is ops visibility only.
    if let Some(checkout_id) = parse_checkout_id_from_metadata(&data.metadata) {
        if let Err(e) = env
            .subscription_repo()
            .complete_checkout_session_by_id(checkout_id)
            .await
        {
            tracing::warn!(webhook_id, %checkout_id, error = %e,
                "Failed to mark checkout session complete — non-fatal");
        }
    }

    Ok(())
}

async fn handle_subscription_on_hold<E>(
    env: &E,
    webhook_id: &str,
    data: WebhookSubscriptionData,
) -> CoreResult<()>
where
    E: SubscriptionEnv,
{
    let Some(sub) = env
        .subscription_repo()
        .get_by_provider_subscription_id(&data.provider_subscription_id)
        .await?
    else {
        return Ok(());
    };

    // on_hold: grace period — keep paid tier, move to OnHold status
    apply_subscription_change(
        env,
        &sub,
        sub.tier,
        SubscriptionStatus::OnHold,
        TierChangeReason::Renewal,
        sub.current_period_start,
        sub.current_period_end,
        false,
        webhook_id,
        &data.provider_status,
        None,
        None,
        None,
    )
    .await
}

async fn handle_subscription_cancelled<E>(
    env: &E,
    webhook_id: &str,
    data: WebhookSubscriptionData,
) -> CoreResult<()>
where
    E: SubscriptionEnv,
{
    let Some(sub) = env
        .subscription_repo()
        .get_by_provider_subscription_id(&data.provider_subscription_id)
        .await?
    else {
        return Ok(());
    };

    let period_end = data.period_end;

    apply_subscription_change(
        env,
        &sub,
        sub.tier,
        SubscriptionStatus::Canceled,
        TierChangeReason::Cancellation,
        sub.current_period_start,
        period_end,
        true,
        webhook_id,
        &data.provider_status,
        None,
        None,
        period_end,
    )
    .await
}

async fn handle_subscription_expired<E>(
    env: &E,
    webhook_id: &str,
    data: WebhookSubscriptionData,
) -> CoreResult<()>
where
    E: SubscriptionEnv,
{
    let Some(sub) = env
        .subscription_repo()
        .get_by_provider_subscription_id(&data.provider_subscription_id)
        .await?
    else {
        return Ok(());
    };

    apply_subscription_change(
        env,
        &sub,
        sub.tier,
        SubscriptionStatus::Expired,
        TierChangeReason::Expiration,
        sub.current_period_start,
        sub.current_period_end,
        false,
        webhook_id,
        &data.provider_status,
        None,
        None,
        None,
    )
    .await
}

async fn handle_payment_succeeded<E>(
    env: &E,
    webhook_id: &str,
    data: WebhookPaymentData,
) -> CoreResult<()>
where
    E: SubscriptionEnv + PaymentEnv,
{
    // Recurring subscription payment — handled by SubscriptionActive/Renewed; skip.
    if data.provider_subscription_id.is_some() {
        return Ok(());
    }

    // One-time lifetime purchase: product_id resolved by the verifier at parse time.
    let Some(ref product_id) = data.product_id else {
        tracing::warn!(
            webhook_id,
            "payment.succeeded: no product_id resolved — ignoring"
        );
        return Ok(());
    };

    // Reject payment.succeeded events for products other than the configured Lifetime product.
    // Without this check any signed one-time payment with cloudless_user_id in metadata
    // would silently grant Lifetime regardless of which product was purchased.
    if tier_from_product_id(env, product_id) != Some(SubscriptionTier::Lifetime) {
        tracing::warn!(
            webhook_id,
            %product_id,
            "payment.succeeded: product_id does not match configured Lifetime product — ignoring"
        );
        return Ok(());
    }

    let user_id = parse_user_id_from_metadata(&data.metadata);
    if user_id == Uuid::nil() {
        tracing::warn!(
            webhook_id,
            customer_id = %data.provider_customer_id,
            "payment.succeeded: missing cloudless_user_id in metadata"
        );
        return Ok(());
    }

    let Some(sub) = env.subscription_repo().get_by_user_id(user_id).await? else {
        tracing::warn!(webhook_id, %user_id, "payment.succeeded: no subscription found");
        return Ok(());
    };

    // Lifetime is a one-time payment — there is no recurring subscription_id.
    env.subscription_repo()
        .set_provider_ids(sub.id, None, &data.provider_customer_id)
        .await?;

    apply_subscription_change(
        env,
        &sub,
        SubscriptionTier::Lifetime,
        SubscriptionStatus::Active,
        TierChangeReason::LifetimePurchase,
        None,
        None,
        false,
        webhook_id,
        "active",
        Some(product_id.as_str()),
        None,
        None,
    )
    .await?;

    // Mark the originating checkout session complete. Non-fatal.
    if let Some(checkout_id) = parse_checkout_id_from_metadata(&data.metadata) {
        if let Err(e) = env
            .subscription_repo()
            .complete_checkout_session_by_id(checkout_id)
            .await
        {
            tracing::warn!(webhook_id, %checkout_id, error = %e,
                "Failed to mark checkout session complete — non-fatal");
        }
    }

    Ok(())
}

async fn handle_payment_failed<E>(
    env: &E,
    webhook_id: &str,
    data: WebhookPaymentData,
) -> CoreResult<()>
where
    E: SubscriptionEnv,
{
    tracing::info!(webhook_id, "Payment failed event received");

    // Mark the associated checkout session as failed so ops can distinguish
    // abandoned/failed purchases from still-processing ones.
    if let Some(checkout_id) = parse_checkout_id_from_metadata(&data.metadata) {
        if let Err(e) = env
            .subscription_repo()
            .fail_checkout_session_by_id(checkout_id)
            .await
        {
            tracing::warn!(webhook_id, %checkout_id, error = %e,
                "Failed to mark checkout session as failed — non-fatal");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn apply_subscription_change<E>(
    env: &E,
    sub: &Subscription,
    new_tier: SubscriptionTier,
    new_status: SubscriptionStatus,
    reason: TierChangeReason,
    period_start: Option<chrono::DateTime<Utc>>,
    period_end: Option<chrono::DateTime<Utc>>,
    cancel_at_period_end: bool,
    webhook_id: &str,
    provider_status: &str,
    provider_product_id: Option<&str>,
    next_billing_date: Option<chrono::DateTime<Utc>>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> CoreResult<()>
where
    E: SubscriptionEnv,
{
    env.subscription_repo()
        .apply_subscription_change_atomic(SubscriptionChangeParams {
            subscription_id: sub.id,
            user_id: sub.user_id,
            new_tier,
            new_status,
            period_start,
            period_end,
            cancel_at_period_end,
            provider_status,
            provider_product_id,
            next_billing_date,
            expires_at,
            previous_tier: Some(sub.tier),
            previous_status: Some(sub.status),
            reason,
            webhook_id: Some(webhook_id),
            metadata: None,
            last_webhook_at: Some(Utc::now()),
            last_synced_at: None,
        })
        .await?;

    tracing::info!(
        user_id = %sub.user_id,
        from_tier = %sub.tier,
        to_tier = %new_tier,
        from_status = %sub.status,
        to_status = %new_status,
        reason = %reason,
        webhook_id,
        "Subscription state updated from webhook"
    );

    Ok(())
}

/// Resolve the local subscription row ID from the event's provider subscription ID.
///
/// Used to pre-associate the DB webhook row with a subscription before processing.
async fn resolve_subscription_id<E>(env: &E, event: &VerifiedPaymentEvent) -> Option<Uuid>
where
    E: SubscriptionEnv,
{
    let provider_sub_id = event.event.provider_subscription_id()?;
    env.subscription_repo()
        .get_by_provider_subscription_id(provider_sub_id)
        .await
        .ok()
        .flatten()
        .map(|sub| sub.id)
}

fn parse_checkout_id_from_metadata(
    metadata: &Option<std::collections::HashMap<String, serde_json::Value>>,
) -> Option<Uuid> {
    metadata
        .as_ref()
        .and_then(|m| m.get("cloudless_checkout_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn tier_from_product_id<E: PaymentEnv>(env: &E, product_id: &str) -> Option<SubscriptionTier> {
    for tier in [
        SubscriptionTier::Starter,
        SubscriptionTier::Pro,
        SubscriptionTier::Lifetime,
    ] {
        if env.product_id_for_tier(tier).as_deref() == Some(product_id) {
            return Some(tier);
        }
    }
    None
}

fn parse_user_id_from_metadata(
    metadata: &Option<std::collections::HashMap<String, serde_json::Value>>,
) -> Uuid {
    metadata
        .as_ref()
        .and_then(|m| m.get("cloudless_user_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil())
}

// ---------------------------------------------------------------------------
// Pending webhook recovery
// ---------------------------------------------------------------------------

/// Scan for webhook events that are still `pending` after 5+ minutes and reprocess them.
///
/// The verifier's `parse_stored_event` is used to reconstruct `PaymentEvent` from stored
/// JSON without re-verifying signatures (originals headers are gone).
/// Safe to call repeatedly — `mark_event_processed` is idempotent.
pub async fn recover_pending_webhooks<E>(env: &E) -> CoreResult<()>
where
    E: SubscriptionEnv + PaymentEnv + Clone + Send + Sync + 'static,
{
    let pending = env
        .subscription_repo()
        .list_pending_webhook_events(300)
        .await?;

    for event in pending {
        tracing::info!(
            webhook_id = %event.webhook_id,
            event_type = %event.event_type,
            "Recovering pending webhook event"
        );

        // Re-parse from stored JSON without signature check.
        let parse_result = env
            .payment_webhook_verifier()
            .parse_stored_event(&event.event_type, event.payload.clone());

        let env_clone = env.clone();
        let webhook_id = event.webhook_id.clone();

        tokio::spawn(async move {
            let result = match parse_result {
                Ok(payment_event) => {
                    process_webhook_event(&env_clone, &webhook_id, payment_event).await
                }
                Err(e) => Err(e),
            };
            let err_str = result.as_ref().err().map(|e| e.to_string());
            let _ = env_clone
                .subscription_repo()
                .mark_event_processed(&webhook_id, err_str.as_deref())
                .await;
            if let Err(e) = result {
                tracing::error!(webhook_id, error = %e, "Recovered webhook processing failed");
            } else {
                tracing::info!(webhook_id, "Recovered webhook processed successfully");
            }
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Admin: reconcile a single user's subscription against the provider
// ---------------------------------------------------------------------------

/// Fetch the current subscription state from the provider and repair any drift.
///
/// Uses `subscription_status` on `ProviderSubscriptionState` (already mapped by the
/// provider adapter) rather than raw status strings.
pub async fn reconcile_subscription<E>(env: &E, user_id: Uuid) -> CoreResult<()>
where
    E: SubscriptionEnv + PaymentEnv,
{
    let sub = env
        .subscription_repo()
        .get_by_user_id(user_id)
        .await?
        .ok_or_else(|| CoreError::not_found("No subscription found for user"))?;

    let provider_fields = env
        .subscription_repo()
        .get_provider_fields(sub.id)
        .await?
        .ok_or_else(|| CoreError::not_found("No provider fields found"))?;

    let provider_sub_id = provider_fields.provider_subscription_id.ok_or_else(|| {
        CoreError::validation("Subscription has no provider ID — not a paid subscription")
    })?;

    let remote = env
        .payment_provider()
        .fetch_subscription(&provider_sub_id)
        .await?;

    let local_status = provider_fields.provider_status.as_deref().unwrap_or("");

    // Resolve the remote tier from product_id. An unrecognized product means we
    // cannot safely reconcile — fail loudly so ops can investigate config drift.
    let remote_tier = match tier_from_product_id(env, &remote.provider_product_id) {
        Some(t) => t,
        None => {
            tracing::error!(
                user_id = %user_id,
                provider_product_id = %remote.provider_product_id,
                "Reconciliation: unrecognized provider product_id — cannot reconcile tier. \
                 Check product ID configuration."
            );
            return Err(CoreError::internal(format!(
                "Unrecognized product_id '{}' during reconciliation — check configuration",
                remote.provider_product_id
            )));
        }
    };

    let status_drifted = remote.provider_status != local_status;
    let tier_drifted = remote_tier != sub.tier;

    if !status_drifted && !tier_drifted {
        tracing::info!(user_id = %user_id, "Reconciliation: no drift detected");
        return Ok(());
    }

    // subscription_status is already normalized by the provider adapter.
    // No provider-specific status string mapping needed here.
    let new_status = remote.subscription_status;

    tracing::warn!(
        user_id = %user_id,
        local_status,
        remote_status = %remote.provider_status,
        local_tier = %sub.tier,
        remote_tier = %remote_tier,
        "Reconciliation: drift detected — repairing from provider"
    );

    // Apply the full entitlement repair atomically, setting last_synced_at (not
    // last_webhook_at) so ops can distinguish reconciliation from webhook delivery.
    env.subscription_repo()
        .apply_subscription_change_atomic(SubscriptionChangeParams {
            subscription_id: sub.id,
            user_id: sub.user_id,
            new_tier: remote_tier,
            new_status,
            period_start: remote.period_start,
            period_end: remote.period_end,
            cancel_at_period_end: remote.cancel_at_period_end,
            provider_status: &remote.provider_status,
            provider_product_id: Some(&remote.provider_product_id),
            next_billing_date: remote.next_billing_date,
            expires_at: None,
            previous_tier: Some(sub.tier),
            previous_status: Some(sub.status),
            reason: SubscriptionTier::change_reason(&sub.tier, &remote_tier),
            webhook_id: Some("reconciliation"),
            metadata: None,
            last_webhook_at: None,
            last_synced_at: Some(Utc::now()),
        })
        .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{
            ports::{
                CreateCheckoutSessionRecord, PendingWebhookEvent, RecordWebhookEventParams,
                SubscriptionChangeParams, SubscriptionProviderFields,
            },
            subscription::env::SubscriptionEnv,
        },
        infra::billing::disabled::DisabledPaymentProvider,
    };
    use api_types::subscription::{
        Subscription, SubscriptionHistoryEntry, SubscriptionStatus, TierChangeReason, TierLimits,
    };
    use std::collections::HashMap as StdHashMap;

    // ---------------------------------------------------------------------------
    // Minimal mock repo that tracks atomic-change and checkout calls.
    // All other methods panic if unexpectedly invoked.
    // ---------------------------------------------------------------------------

    #[derive(Clone)]
    struct MockWebhookRepo {
        subscription_by_provider_id: StdHashMap<String, Subscription>,
        subscription_by_user_id: StdHashMap<Uuid, Subscription>,
        atomic_call_count: std::sync::Arc<std::sync::Mutex<u32>>,
        fail_checkout_call_count: std::sync::Arc<std::sync::Mutex<u32>>,
        complete_checkout_call_count: std::sync::Arc<std::sync::Mutex<u32>>,
        /// Captures the `cancel_at_period_end` field from the last atomic change call.
        last_atomic_cancel_at_period_end: std::sync::Arc<std::sync::Mutex<Option<bool>>>,
        /// Local provider status used by get_provider_fields (for drift detection).
        local_provider_status: String,
    }

    impl MockWebhookRepo {
        fn new(sub: Subscription, provider_id: &str) -> Self {
            let mut by_provider = StdHashMap::new();
            by_provider.insert(provider_id.to_string(), sub.clone());
            let mut by_user = StdHashMap::new();
            by_user.insert(sub.user_id, sub);
            Self {
                subscription_by_provider_id: by_provider,
                subscription_by_user_id: by_user,
                atomic_call_count: Default::default(),
                fail_checkout_call_count: Default::default(),
                complete_checkout_call_count: Default::default(),
                last_atomic_cancel_at_period_end: Default::default(),
                local_provider_status: "active".to_string(),
            }
        }

        fn atomic_calls(&self) -> u32 {
            *self.atomic_call_count.lock().unwrap()
        }
        fn fail_checkout_calls(&self) -> u32 {
            *self.fail_checkout_call_count.lock().unwrap()
        }
        fn last_cancel_at_period_end(&self) -> Option<bool> {
            *self.last_atomic_cancel_at_period_end.lock().unwrap()
        }
    }

    #[async_trait::async_trait]
    impl crate::core::ports::SubscriptionRepo for MockWebhookRepo {
        async fn get_by_user_id(&self, uid: Uuid) -> CoreResult<Option<Subscription>> {
            Ok(self.subscription_by_user_id.get(&uid).cloned())
        }
        async fn get_by_provider_subscription_id(
            &self,
            id: &str,
        ) -> CoreResult<Option<Subscription>> {
            Ok(self.subscription_by_provider_id.get(id).cloned())
        }
        async fn get_tier_limits(&self, _: SubscriptionTier) -> CoreResult<TierLimits> {
            Ok(SubscriptionTier::Free.limits())
        }
        async fn set_provider_ids(&self, _: Uuid, _: Option<&str>, _: &str) -> CoreResult<()> {
            Ok(())
        }
        async fn apply_subscription_change_atomic(
            &self,
            params: SubscriptionChangeParams<'_>,
        ) -> CoreResult<()> {
            *self.atomic_call_count.lock().unwrap() += 1;
            *self.last_atomic_cancel_at_period_end.lock().unwrap() =
                Some(params.cancel_at_period_end);
            Ok(())
        }
        async fn complete_checkout_session_by_id(&self, _: Uuid) -> CoreResult<()> {
            *self.complete_checkout_call_count.lock().unwrap() += 1;
            Ok(())
        }
        async fn fail_checkout_session_by_id(&self, _: Uuid) -> CoreResult<()> {
            *self.fail_checkout_call_count.lock().unwrap() += 1;
            Ok(())
        }
        async fn record_webhook_event(&self, _: RecordWebhookEventParams<'_>) -> CoreResult<bool> {
            Ok(true)
        }
        async fn mark_event_processed(&self, _: &str, _: Option<&str>) -> CoreResult<()> {
            Ok(())
        }

        // Remaining methods unused in these tests
        async fn create(
            &self,
            _: Uuid,
            _: SubscriptionTier,
            _: SubscriptionStatus,
        ) -> CoreResult<Subscription> {
            unimplemented!()
        }
        async fn update_tier_and_status(
            &self,
            _: Uuid,
            _: SubscriptionTier,
            _: SubscriptionStatus,
            _: Option<chrono::DateTime<Utc>>,
            _: Option<chrono::DateTime<Utc>>,
            _: bool,
        ) -> CoreResult<Subscription> {
            unimplemented!()
        }
        async fn update_provider_fields(
            &self,
            _: Uuid,
            _: &str,
            _: Option<&str>,
            _: Option<chrono::DateTime<Utc>>,
            _: Option<chrono::DateTime<Utc>>,
            _: Option<chrono::DateTime<Utc>>,
        ) -> CoreResult<()> {
            Ok(())
        }
        async fn get_provider_fields(
            &self,
            sub_id: Uuid,
        ) -> CoreResult<Option<SubscriptionProviderFields>> {
            Ok(Some(SubscriptionProviderFields {
                subscription_id: sub_id,
                provider: "dodo".to_string(),
                provider_subscription_id: Some("sub_test".to_string()),
                provider_customer_id: Some("cus_test".to_string()),
                provider_status: Some(self.local_provider_status.clone()),
                provider_product_id: Some("prod_pro".to_string()),
                last_webhook_at: None,
                last_synced_at: None,
            }))
        }
        async fn complete_checkout_session(&self, _: &str) -> CoreResult<()> {
            Ok(())
        }
        async fn get_checkout_session_status(
            &self,
            _: Uuid,
            _: Uuid,
        ) -> CoreResult<Option<String>> {
            unimplemented!()
        }
        async fn create_checkout_session_record(
            &self,
            _: CreateCheckoutSessionRecord,
        ) -> CoreResult<()> {
            Ok(())
        }
        async fn record_history(
            &self,
            _: Uuid,
            _: Uuid,
            _: Option<SubscriptionTier>,
            _: SubscriptionTier,
            _: Option<SubscriptionStatus>,
            _: SubscriptionStatus,
            _: TierChangeReason,
            _: Option<&str>,
            _: Option<serde_json::Value>,
        ) -> CoreResult<()> {
            Ok(())
        }
        async fn get_history(&self, _: Uuid) -> CoreResult<Vec<SubscriptionHistoryEntry>> {
            unimplemented!()
        }
        async fn list_pending_webhook_events(
            &self,
            _: i64,
        ) -> CoreResult<Vec<PendingWebhookEvent>> {
            unimplemented!()
        }
    }

    // ---------------------------------------------------------------------------
    // Mock env wrapping the repo + configured product IDs
    // ---------------------------------------------------------------------------

    #[derive(Clone)]
    struct MockWebhookEnv {
        repo: MockWebhookRepo,
    }

    impl MockWebhookEnv {
        fn new(repo: MockWebhookRepo) -> Self {
            Self { repo }
        }
    }

    impl SubscriptionEnv for MockWebhookEnv {
        type Repo = MockWebhookRepo;
        fn subscription_repo(&self) -> &Self::Repo {
            &self.repo
        }
    }

    impl crate::core::subscription::env::PaymentEnv for MockWebhookEnv {
        type Provider = DisabledPaymentProvider;
        type Verifier = DisabledPaymentProvider;

        fn payment_provider(&self) -> &Self::Provider {
            unimplemented!()
        }
        fn payment_webhook_verifier(&self) -> &Self::Verifier {
            unimplemented!()
        }
        fn product_id_for_tier(&self, tier: SubscriptionTier) -> Option<String> {
            match tier {
                SubscriptionTier::Starter => Some("prod_starter".to_string()),
                SubscriptionTier::Pro => Some("prod_pro".to_string()),
                SubscriptionTier::Lifetime => Some("prod_lifetime".to_string()),
                SubscriptionTier::Free => None,
            }
        }
        fn payment_environment(&self) -> &str {
            "test"
        }
        fn return_url_base(&self) -> &str {
            "http://localhost"
        }
        fn cancel_url_base(&self) -> &str {
            "http://localhost"
        }
    }

    // ---------------------------------------------------------------------------
    // tier_from_product_id test env
    // ---------------------------------------------------------------------------

    struct MockPaymentEnvForTier {
        starter_id: Option<String>,
        pro_id: Option<String>,
        lifetime_id: Option<String>,
    }

    impl crate::core::subscription::env::PaymentEnv for MockPaymentEnvForTier {
        type Provider = DisabledPaymentProvider;
        type Verifier = DisabledPaymentProvider;

        fn payment_provider(&self) -> &Self::Provider {
            unimplemented!()
        }
        fn payment_webhook_verifier(&self) -> &Self::Verifier {
            unimplemented!()
        }
        fn product_id_for_tier(&self, tier: SubscriptionTier) -> Option<String> {
            match tier {
                SubscriptionTier::Starter => self.starter_id.clone(),
                SubscriptionTier::Pro => self.pro_id.clone(),
                SubscriptionTier::Lifetime => self.lifetime_id.clone(),
                SubscriptionTier::Free => None,
            }
        }
        fn payment_environment(&self) -> &str {
            "test"
        }
        fn return_url_base(&self) -> &str {
            "http://localhost"
        }
        fn cancel_url_base(&self) -> &str {
            "http://localhost"
        }
    }

    // ---------------------------------------------------------------------------
    // Fixture helpers
    // ---------------------------------------------------------------------------

    fn test_subscription(user_id: Uuid, tier: SubscriptionTier) -> Subscription {
        Subscription {
            id: Uuid::now_v7(),
            user_id,
            tier,
            status: SubscriptionStatus::Active,
            current_period_start: None,
            current_period_end: None,
            cancel_at_period_end: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Build a WebhookSubscriptionData for testing (replaces the old JSON payload builders).
    fn sub_event_data(product_id: &str, provider_sub_id: &str) -> WebhookSubscriptionData {
        let mut metadata = StdHashMap::new();
        metadata.insert(
            "cloudless_checkout_id".to_string(),
            serde_json::json!(Uuid::now_v7().to_string()),
        );
        WebhookSubscriptionData {
            provider_subscription_id: provider_sub_id.to_string(),
            provider_customer_id: "cus_test".to_string(),
            provider_status: "active".to_string(),
            product_id: product_id.to_string(),
            period_start: None,
            period_end: None,
            cancel_at_period_end: false,
            metadata: Some(metadata),
        }
    }

    /// Build a WebhookPaymentData for lifetime purchase tests.
    fn payment_succeeded_data(
        product_id: Option<&str>,
        user_id: Uuid,
        with_sub_id: bool,
    ) -> WebhookPaymentData {
        let mut metadata = StdHashMap::new();
        metadata.insert(
            "cloudless_user_id".to_string(),
            serde_json::json!(user_id.to_string()),
        );
        metadata.insert(
            "cloudless_checkout_id".to_string(),
            serde_json::json!(Uuid::now_v7().to_string()),
        );
        WebhookPaymentData {
            provider_payment_id: "pay_test".to_string(),
            provider_customer_id: "cus_test".to_string(),
            product_id: product_id.map(str::to_string),
            provider_status: "succeeded".to_string(),
            provider_subscription_id: if with_sub_id {
                Some("sub_recurring".to_string())
            } else {
                None
            },
            metadata: Some(metadata),
        }
    }

    fn payment_failed_data(user_id: Uuid) -> WebhookPaymentData {
        let mut metadata = StdHashMap::new();
        metadata.insert(
            "cloudless_user_id".to_string(),
            serde_json::json!(user_id.to_string()),
        );
        metadata.insert(
            "cloudless_checkout_id".to_string(),
            serde_json::json!(Uuid::now_v7().to_string()),
        );
        WebhookPaymentData {
            provider_payment_id: "pay_failed".to_string(),
            provider_customer_id: "cus_test".to_string(),
            product_id: None,
            provider_status: "failed".to_string(),
            provider_subscription_id: None,
            metadata: Some(metadata),
        }
    }

    // ---------------------------------------------------------------------------
    // tier_from_product_id tests
    // ---------------------------------------------------------------------------

    /// Configured product IDs must resolve to the correct tiers.
    #[test]
    fn test_tier_from_product_id_matches_configured_products() {
        let env = MockPaymentEnvForTier {
            starter_id: Some("prod_starter".to_string()),
            pro_id: Some("prod_pro".to_string()),
            lifetime_id: Some("prod_lifetime".to_string()),
        };
        assert_eq!(
            tier_from_product_id(&env, "prod_starter"),
            Some(SubscriptionTier::Starter)
        );
        assert_eq!(
            tier_from_product_id(&env, "prod_pro"),
            Some(SubscriptionTier::Pro)
        );
        assert_eq!(
            tier_from_product_id(&env, "prod_lifetime"),
            Some(SubscriptionTier::Lifetime)
        );
    }

    /// Unknown or unconfigured product IDs must return None (triggers bail-out in handler).
    #[test]
    fn test_tier_from_product_id_unknown_returns_none() {
        let env = MockPaymentEnvForTier {
            starter_id: Some("prod_starter".to_string()),
            pro_id: None,
            lifetime_id: Some("prod_lifetime".to_string()),
        };
        assert_eq!(tier_from_product_id(&env, "prod_unknown"), None);
        assert_eq!(tier_from_product_id(&env, ""), None);
        // Unconfigured tier (pro) also returns None
        assert_eq!(tier_from_product_id(&env, "prod_pro"), None);
    }

    // ---------------------------------------------------------------------------
    // process_webhook_event tests (now using PaymentEvent structs directly)
    // ---------------------------------------------------------------------------

    /// subscription.active for a known product triggers an entitlement change.
    #[tokio::test]
    async fn test_subscription_active_known_product_triggers_entitlement_change() {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Free);
        let repo = MockWebhookRepo::new(sub, "sub_001");
        let env = MockWebhookEnv::new(repo.clone());

        let event = PaymentEvent::SubscriptionActive(sub_event_data("prod_starter", "sub_001"));
        let result = process_webhook_event(&env, "wh_001", event).await;

        assert!(result.is_ok(), "expected Ok but got {:?}", result);
        assert_eq!(
            repo.atomic_calls(),
            1,
            "atomic change should be called once"
        );
    }

    /// subscription.active for an unknown product must return an error (no silent corruption).
    #[tokio::test]
    async fn test_subscription_active_unknown_product_returns_err() {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Free);
        let repo = MockWebhookRepo::new(sub, "sub_001");
        let env = MockWebhookEnv::new(repo.clone());

        let event =
            PaymentEvent::SubscriptionActive(sub_event_data("prod_unknown_xyz", "sub_001"));
        let result = process_webhook_event(&env, "wh_002", event).await;

        assert!(result.is_err(), "unknown product_id should return Err");
        assert_eq!(
            repo.atomic_calls(),
            0,
            "no entitlement change on unknown product"
        );
    }

    /// payment.succeeded with resolved product_id grants Lifetime entitlement.
    #[tokio::test]
    async fn test_payment_succeeded_lifetime_via_product_id() {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Free);
        let repo = MockWebhookRepo::new(sub, "");
        let env = MockWebhookEnv::new(repo.clone());

        let event = PaymentEvent::PaymentSucceeded(payment_succeeded_data(
            Some("prod_lifetime"),
            user_id,
            false,
        ));
        let result = process_webhook_event(&env, "wh_003", event).await;

        assert!(result.is_ok(), "Lifetime payment should succeed");
        assert_eq!(repo.atomic_calls(), 1, "should grant Lifetime entitlement");
    }

    /// payment.succeeded with a subscription_id is a recurring payment — must be skipped.
    #[tokio::test]
    async fn test_payment_succeeded_recurring_is_skipped() {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Starter);
        let repo = MockWebhookRepo::new(sub, "");
        let env = MockWebhookEnv::new(repo.clone());

        // subscription_id present → recurring payment, handled by SubscriptionActive/Renewed
        let event = PaymentEvent::PaymentSucceeded(payment_succeeded_data(
            Some("prod_starter"),
            user_id,
            true,
        ));
        let result = process_webhook_event(&env, "wh_005", event).await;

        assert!(
            result.is_ok(),
            "recurring payment should be silently skipped"
        );
        assert_eq!(
            repo.atomic_calls(),
            0,
            "no entitlement change for recurring payment"
        );
    }

    /// payment.succeeded for an unrecognized product must be skipped gracefully.
    #[tokio::test]
    async fn test_payment_succeeded_unrecognized_product_skipped() {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Free);
        let repo = MockWebhookRepo::new(sub, "");
        let env = MockWebhookEnv::new(repo.clone());

        let event = PaymentEvent::PaymentSucceeded(payment_succeeded_data(
            Some("prod_not_in_config"),
            user_id,
            false,
        ));
        let result = process_webhook_event(&env, "wh_006", event).await;

        assert!(
            result.is_ok(),
            "unrecognized product should be skipped gracefully"
        );
        assert_eq!(
            repo.atomic_calls(),
            0,
            "no entitlement change for unknown product"
        );
    }

    /// payment.succeeded with no product_id resolved must be skipped gracefully.
    #[tokio::test]
    async fn test_payment_succeeded_no_product_skipped() {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Free);
        let repo = MockWebhookRepo::new(sub, "");
        let env = MockWebhookEnv::new(repo.clone());

        let event =
            PaymentEvent::PaymentSucceeded(payment_succeeded_data(None, user_id, false));
        let result = process_webhook_event(&env, "wh_007", event).await;

        assert!(
            result.is_ok(),
            "no product field should be skipped gracefully"
        );
        assert_eq!(
            repo.atomic_calls(),
            0,
            "no entitlement change when product absent"
        );
    }

    /// payment.failed must mark the checkout session as failed.
    #[tokio::test]
    async fn test_payment_failed_marks_checkout_failed() {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Free);
        let repo = MockWebhookRepo::new(sub, "");
        let env = MockWebhookEnv::new(repo.clone());

        let event = PaymentEvent::PaymentFailed(payment_failed_data(user_id));
        let result = process_webhook_event(&env, "wh_008", event).await;

        assert!(result.is_ok(), "payment.failed should return Ok");
        assert_eq!(
            repo.fail_checkout_calls(),
            1,
            "checkout session should be marked failed"
        );
        assert_eq!(
            repo.atomic_calls(),
            0,
            "no entitlement change on payment failure"
        );
    }

    // ---------------------------------------------------------------------------
    // reconcile_subscription: cancel_at_period_end propagation
    // ---------------------------------------------------------------------------

    /// Mock payment provider that returns a configurable subscription state.
    #[derive(Clone)]
    struct MockReconcileProvider {
        remote_status: String,
        remote_subscription_status: SubscriptionStatus,
        remote_product_id: String,
        remote_period_end: Option<chrono::DateTime<Utc>>,
        remote_cancel_at_period_end: bool,
    }

    #[async_trait::async_trait]
    impl crate::core::ports::payment_provider::PaymentProvider for MockReconcileProvider {
        async fn create_checkout_session(
            &self,
            _: crate::core::ports::payment_provider::CreateCheckoutParams,
        ) -> CoreResult<crate::core::ports::payment_provider::CheckoutResult> {
            unimplemented!()
        }
        async fn create_customer_portal_session(&self, _: &str) -> CoreResult<String> {
            unimplemented!()
        }
        async fn fetch_subscription(
            &self,
            _: &str,
        ) -> CoreResult<crate::core::ports::payment_provider::ProviderSubscriptionState> {
            Ok(
                crate::core::ports::payment_provider::ProviderSubscriptionState {
                    provider_subscription_id: "sub_test".to_string(),
                    provider_customer_id: "cus_test".to_string(),
                    provider_status: self.remote_status.clone(),
                    subscription_status: self.remote_subscription_status,
                    provider_product_id: self.remote_product_id.clone(),
                    currency: None,
                    period_start: None,
                    period_end: self.remote_period_end,
                    next_billing_date: self.remote_period_end,
                    trial_period_days: None,
                    cancel_at_period_end: self.remote_cancel_at_period_end,
                },
            )
        }
    }

    #[derive(Clone)]
    struct MockReconcileEnv {
        repo: MockWebhookRepo,
        provider: MockReconcileProvider,
    }

    impl SubscriptionEnv for MockReconcileEnv {
        type Repo = MockWebhookRepo;
        fn subscription_repo(&self) -> &Self::Repo {
            &self.repo
        }
    }

    impl crate::core::subscription::env::PaymentEnv for MockReconcileEnv {
        type Provider = MockReconcileProvider;
        type Verifier = DisabledPaymentProvider;

        fn payment_provider(&self) -> &Self::Provider {
            &self.provider
        }
        fn payment_webhook_verifier(&self) -> &Self::Verifier {
            unimplemented!()
        }
        fn product_id_for_tier(&self, tier: SubscriptionTier) -> Option<String> {
            match tier {
                SubscriptionTier::Starter => Some("prod_starter".to_string()),
                SubscriptionTier::Pro => Some("prod_pro".to_string()),
                SubscriptionTier::Lifetime => Some("prod_lifetime".to_string()),
                SubscriptionTier::Free => None,
            }
        }
        fn payment_environment(&self) -> &str {
            "test"
        }
        fn return_url_base(&self) -> &str {
            "http://localhost"
        }
        fn cancel_url_base(&self) -> &str {
            "http://localhost"
        }
    }

    /// Cancelled subscription with a future period_end must set cancel_at_period_end=true
    /// so effective_tier() retains paid access until the period ends.
    #[tokio::test]
    async fn test_reconciliation_cancelled_with_future_period_end_sets_cancel_at_period_end_true() {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Pro);
        let mut repo = MockWebhookRepo::new(sub, "sub_test");
        repo.local_provider_status = "active".to_string();

        let future = Utc::now() + chrono::Duration::days(15);
        let env = MockReconcileEnv {
            repo: repo.clone(),
            provider: MockReconcileProvider {
                remote_status: "cancelled".to_string(),
                remote_subscription_status: SubscriptionStatus::Canceled,
                remote_product_id: "prod_pro".to_string(),
                remote_period_end: Some(future),
                remote_cancel_at_period_end: true,
            },
        };

        let result = reconcile_subscription(&env, user_id).await;
        assert!(
            result.is_ok(),
            "reconciliation should succeed: {:?}",
            result
        );
        assert_eq!(
            repo.atomic_calls(),
            1,
            "drift should trigger an entitlement update"
        );
        assert_eq!(
            repo.last_cancel_at_period_end().unwrap(),
            true,
            "cancel_at_period_end must be true so effective_tier() retains paid access until period_end"
        );
    }

    /// Cancelled subscription with a past period_end must set cancel_at_period_end=false.
    #[tokio::test]
    async fn test_reconciliation_cancelled_without_future_period_end_sets_cancel_at_period_end_false(
    ) {
        let user_id = Uuid::now_v7();
        let sub = test_subscription(user_id, SubscriptionTier::Pro);
        let mut repo = MockWebhookRepo::new(sub, "sub_test");
        repo.local_provider_status = "active".to_string();

        let past = Utc::now() - chrono::Duration::days(1);
        let env = MockReconcileEnv {
            repo: repo.clone(),
            provider: MockReconcileProvider {
                remote_status: "cancelled".to_string(),
                remote_subscription_status: SubscriptionStatus::Canceled,
                remote_product_id: "prod_pro".to_string(),
                remote_period_end: Some(past),
                remote_cancel_at_period_end: false,
            },
        };

        let result = reconcile_subscription(&env, user_id).await;
        assert!(result.is_ok());
        assert_eq!(repo.atomic_calls(), 1);
        assert_eq!(
            repo.last_cancel_at_period_end().unwrap(),
            false,
            "cancel_at_period_end should be false when the grace period has elapsed"
        );
    }
}
