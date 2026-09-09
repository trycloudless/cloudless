use std::collections::HashMap;

use api_types::subscription::SubscriptionStatus;
use async_trait::async_trait;

use crate::{
    core::{
        CoreError, CoreResult,
        ports::{
            payment_provider::*,
            payment_webhook::{
                PaymentEvent, PaymentWebhookVerifier, VerifiedPaymentEvent,
                WebhookPaymentData, WebhookSubscriptionData,
            },
        },
    },
    infra::dodo::types::*,
};

/// HTTP client wrapper for the Dodo Payments REST API.
///
/// Holds a `reqwest::Client`, base URL, API key and optional webhook secret.
/// Implements both `PaymentProvider` (async, network) and `PaymentWebhookVerifier`
/// (sync, CPU-only HMAC) so it can be injected via a single `BillingAdapter` enum.
#[derive(Clone)]
pub struct DodoClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    webhook_secret: Option<String>,
}

impl DodoClient {
    /// Creates a new DodoClient.
    ///
    /// `webhook_secret` is required for verifying incoming webhooks.
    /// If `None`, `verify_and_parse` returns an error.
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        webhook_secret: Option<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            webhook_secret,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn post<Req, Res>(&self, path: &str, body: &Req) -> CoreResult<Res>
    where
        Req: serde::Serialize,
        Res: serde::de::DeserializeOwned,
    {
        let response = self
            .http
            .post(self.url(path))
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .map_err(|e| CoreError::internal(format!("Dodo HTTP error: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CoreError::internal(format!(
                "Dodo API returned {status}: {body}"
            )));
        }

        response
            .json::<Res>()
            .await
            .map_err(|e| CoreError::internal(format!("Dodo response parse error: {e}")))
    }

    async fn get<Res>(&self, path: &str) -> CoreResult<Res>
    where
        Res: serde::de::DeserializeOwned,
    {
        let response = self
            .http
            .get(self.url(path))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| CoreError::internal(format!("Dodo HTTP error: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CoreError::internal(format!(
                "Dodo API returned {status}: {body}"
            )));
        }

        response
            .json::<Res>()
            .await
            .map_err(|e| CoreError::internal(format!("Dodo response parse error: {e}")))
    }
}

#[async_trait]
impl PaymentProvider for DodoClient {
    async fn create_checkout_session(
        &self,
        params: CreateCheckoutParams,
    ) -> CoreResult<CheckoutResult> {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "cloudless_user_id".to_string(),
            params.cloudless_user_id.to_string(),
        );
        metadata.insert("cloudless_tier".to_string(), params.tier.to_string());
        metadata.insert(
            "cloudless_checkout_id".to_string(),
            params.cloudless_checkout_id.to_string(),
        );
        metadata.insert(
            "cloudless_environment".to_string(),
            params.cloudless_environment,
        );

        // For returning customers, link by customer_id.
        // For first-time customers, send email+name so Dodo creates a record.
        let customer = Some(if let Some(existing_id) = params.provider_customer_id {
            CheckoutCustomer {
                customer_id: Some(existing_id),
                email: None,
                name: None,
            }
        } else {
            CheckoutCustomer {
                customer_id: None,
                email: Some(params.customer_email),
                name: Some(params.customer_name),
            }
        });

        let req = CreateCheckoutSessionRequest {
            product_cart: vec![ProductCartItem {
                product_id: params.product_id,
                quantity: 1,
            }],
            customer,
            billing_address: None, // collected by Dodo on the hosted checkout page
            metadata,
            return_url: params.return_url,
            cancel_url: Some(params.cancel_url),
        };

        let res: CreateCheckoutSessionResponse = self.post("/checkouts", &req).await?;

        let checkout_url = res
            .checkout_url
            .ok_or_else(|| CoreError::internal("Dodo checkout response missing checkout_url"))?;

        Ok(CheckoutResult {
            checkout_url,
            provider_checkout_session_id: res.session_id,
        })
    }

    async fn create_customer_portal_session(&self, customer_id: &str) -> CoreResult<String> {
        let path = format!("/customers/{}/customer-portal/session", customer_id);
        let req = CreateCustomerPortalSessionRequest {
            customer_id: customer_id.to_string(),
            return_url: None,
        };
        let res: CreateCustomerPortalSessionResponse = self.post(&path, &req).await?;
        Ok(res.link)
    }

    async fn fetch_subscription(
        &self,
        provider_subscription_id: &str,
    ) -> CoreResult<ProviderSubscriptionState> {
        let path = format!("/subscriptions/{}", provider_subscription_id);
        let sub: DodoSubscription = self.get(&path).await?;

        // Dodo's GET /subscriptions response has no explicit cancel_at_period_end field.
        // Derive it: cancelled with a future next_billing_date means the user still has
        // paid access until that date (same semantics as cancel_at_next_billing_date on webhooks).
        let cancel_at_period_end = matches!(sub.status.as_str(), "cancelled" | "canceled")
            && sub
                .next_billing_date
                .map_or(false, |end| end > chrono::Utc::now());

        let subscription_status =
            dodo_status_to_subscription_status(&sub.status).unwrap_or(SubscriptionStatus::Active);

        Ok(ProviderSubscriptionState {
            provider_subscription_id: sub.subscription_id,
            provider_customer_id: sub.customer.customer_id,
            provider_status: sub.status,
            subscription_status,
            provider_product_id: sub.product_id,
            currency: sub.currency,
            period_start: sub.previous_billing_date,
            period_end: sub.next_billing_date,
            next_billing_date: sub.next_billing_date,
            trial_period_days: sub.trial_period_days,
            cancel_at_period_end,
        })
    }
}

impl PaymentWebhookVerifier for DodoClient {
    /// Verify the Dodo Standard Webhooks signature and parse the body into a `VerifiedPaymentEvent`.
    ///
    /// Extracts the three Standard Webhooks headers, calls the HMAC verifier, then maps
    /// the Dodo-specific event envelope to the provider-neutral `PaymentEvent` enum.
    fn verify_and_parse(
        &self,
        headers: &HashMap<String, String>,
        raw_body: &[u8],
    ) -> CoreResult<VerifiedPaymentEvent> {
        let secret = self
            .webhook_secret
            .as_deref()
            .ok_or_else(|| CoreError::internal("Dodo webhook secret is not configured"))?;

        let webhook_id = headers
            .get("webhook-id")
            .ok_or_else(|| CoreError::authentication("Missing webhook-id header"))?;
        let webhook_timestamp = headers
            .get("webhook-timestamp")
            .ok_or_else(|| CoreError::authentication("Missing webhook-timestamp header"))?;
        let webhook_signature = headers
            .get("webhook-signature")
            .ok_or_else(|| CoreError::authentication("Missing webhook-signature header"))?;

        // Delegate to the HMAC verifier (pub(super) within the dodo module).
        super::webhook::verify_webhook_signature(
            secret,
            webhook_id,
            webhook_timestamp,
            raw_body,
            webhook_signature,
        )
        .map_err(|e| CoreError::authentication(e.to_string()))?;

        let raw_payload: serde_json::Value =
            serde_json::from_slice(raw_body).unwrap_or(serde_json::Value::Null);

        let dodo_event: DodoWebhookEvent = serde_json::from_slice(raw_body)
            .map_err(|e| CoreError::validation(format!("Invalid webhook JSON: {e}")))?;

        let event = dodo_event_to_payment_event(&dodo_event.event_type, dodo_event.data)?;

        Ok(VerifiedPaymentEvent {
            event_type: dodo_event.event_type,
            event,
            raw_payload,
        })
    }

    /// Re-parse a stored webhook payload into a `PaymentEvent` without re-verifying the signature.
    ///
    /// Called by the recovery path where the original request headers are gone.
    /// `payload` is the full raw JSON stored at original delivery time.
    fn parse_stored_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> CoreResult<PaymentEvent> {
        let data = payload.get("data").cloned().unwrap_or_default();
        dodo_event_to_payment_event(event_type, data)
    }
}

// ---------------------------------------------------------------------------
// Dodo → neutral mapping helpers (private to this module)
// ---------------------------------------------------------------------------

/// Map a Dodo event type + raw data JSON to a provider-neutral `PaymentEvent`.
fn dodo_event_to_payment_event(
    event_type: &str,
    data: serde_json::Value,
) -> CoreResult<PaymentEvent> {
    match event_type {
        "subscription.active"
        | "subscription.renewed"
        | "subscription.updated"
        | "subscription.plan_changed" => {
            let d: DodoWebhookSubscriptionData = serde_json::from_value(data).map_err(|e| {
                CoreError::internal(format!("Failed to parse subscription event: {e}"))
            })?;
            Ok(PaymentEvent::SubscriptionActive(dodo_sub_to_normalized(d)))
        }
        "subscription.on_hold" => {
            let d: DodoWebhookSubscriptionData = serde_json::from_value(data).map_err(|e| {
                CoreError::internal(format!("Failed to parse on_hold event: {e}"))
            })?;
            Ok(PaymentEvent::SubscriptionOnHold(dodo_sub_to_normalized(d)))
        }
        "subscription.cancelled" => {
            let d: DodoWebhookSubscriptionData = serde_json::from_value(data).map_err(|e| {
                CoreError::internal(format!("Failed to parse cancelled event: {e}"))
            })?;
            Ok(PaymentEvent::SubscriptionCancelled(dodo_sub_to_normalized(d)))
        }
        "subscription.expired" | "subscription.failed" => {
            let d: DodoWebhookSubscriptionData = serde_json::from_value(data).map_err(|e| {
                CoreError::internal(format!("Failed to parse expired event: {e}"))
            })?;
            Ok(PaymentEvent::SubscriptionExpired(dodo_sub_to_normalized(d)))
        }
        "payment.succeeded" => {
            let d: DodoWebhookPaymentData = serde_json::from_value(data).map_err(|e| {
                CoreError::internal(format!("Failed to parse payment.succeeded: {e}"))
            })?;
            Ok(PaymentEvent::PaymentSucceeded(dodo_payment_to_normalized(d)))
        }
        "payment.failed" => {
            let d: DodoWebhookPaymentData = serde_json::from_value(data).map_err(|e| {
                CoreError::internal(format!("Failed to parse payment.failed: {e}"))
            })?;
            Ok(PaymentEvent::PaymentFailed(dodo_payment_to_normalized(d)))
        }
        other => Ok(PaymentEvent::Unknown {
            event_type: other.to_string(),
        }),
    }
}

/// Convert Dodo subscription webhook data to the provider-neutral struct.
///
/// Field renames applied:
///   previous_billing_date        → period_start
///   next_billing_date            → period_end
///   cancel_at_next_billing_date  → cancel_at_period_end
fn dodo_sub_to_normalized(d: DodoWebhookSubscriptionData) -> WebhookSubscriptionData {
    WebhookSubscriptionData {
        provider_subscription_id: d.subscription_id,
        provider_customer_id: d.customer.customer_id,
        provider_status: d.status,
        product_id: d.product_id,
        period_start: d.previous_billing_date,
        period_end: d.next_billing_date,
        cancel_at_period_end: d.cancel_at_next_billing_date.unwrap_or(false),
        metadata: d.metadata,
    }
}

/// Convert Dodo payment webhook data to the provider-neutral struct.
///
/// Resolves the dual `product_id` / `product_cart` payload shapes into a single
/// `product_id` field: top-level `product_id` takes priority, falling back to
/// `product_cart[0].product_id` for one-time purchase payloads per Dodo docs.
fn dodo_payment_to_normalized(d: DodoWebhookPaymentData) -> WebhookPaymentData {
    let product_id = d.product_id.or_else(|| {
        d.product_cart
            .as_ref()
            .and_then(|cart| cart.first())
            .map(|item| item.product_id.clone())
    });
    WebhookPaymentData {
        provider_payment_id: d.payment_id,
        provider_customer_id: d.customer.customer_id,
        product_id,
        provider_status: d.status,
        provider_subscription_id: d.subscription_id,
        metadata: d.metadata,
    }
}

/// Map a Dodo provider status string to the normalized `SubscriptionStatus`.
///
/// Returns `None` for unrecognised strings so callers can log-and-skip safely.
fn dodo_status_to_subscription_status(status: &str) -> Option<SubscriptionStatus> {
    match status {
        "active" => Some(SubscriptionStatus::Active),
        "on_hold" | "on-hold" | "past_due" => Some(SubscriptionStatus::OnHold),
        "cancelled" | "canceled" => Some(SubscriptionStatus::Canceled),
        "expired" | "failed" => Some(SubscriptionStatus::Expired),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// No-op provider for test/local environments without Dodo credentials
// ---------------------------------------------------------------------------

/// No-op payment provider for test environments where Dodo is not configured.
#[derive(Clone)]
pub struct NoDodoClient;

#[async_trait]
impl PaymentProvider for NoDodoClient {
    async fn create_checkout_session(&self, _: CreateCheckoutParams) -> CoreResult<CheckoutResult> {
        Err(CoreError::internal(
            "Dodo payments not configured (DODO_API_KEY is not set)",
        ))
    }

    async fn create_customer_portal_session(&self, _: &str) -> CoreResult<String> {
        Err(CoreError::internal(
            "Dodo payments not configured (DODO_API_KEY is not set)",
        ))
    }

    async fn fetch_subscription(&self, _: &str) -> CoreResult<ProviderSubscriptionState> {
        Err(CoreError::internal(
            "Dodo payments not configured (DODO_API_KEY is not set)",
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── dodo_status_to_subscription_status ──────────────────────────────────

    /// Known Dodo status strings must map to the correct internal status.
    #[test]
    fn test_status_mapping_known_values() {
        assert_eq!(
            dodo_status_to_subscription_status("active"),
            Some(SubscriptionStatus::Active)
        );
        assert_eq!(
            dodo_status_to_subscription_status("on_hold"),
            Some(SubscriptionStatus::OnHold)
        );
        assert_eq!(
            dodo_status_to_subscription_status("past_due"),
            Some(SubscriptionStatus::OnHold)
        );
        assert_eq!(
            dodo_status_to_subscription_status("cancelled"),
            Some(SubscriptionStatus::Canceled)
        );
        assert_eq!(
            dodo_status_to_subscription_status("canceled"),
            Some(SubscriptionStatus::Canceled)
        );
        assert_eq!(
            dodo_status_to_subscription_status("expired"),
            Some(SubscriptionStatus::Expired)
        );
        assert_eq!(
            dodo_status_to_subscription_status("failed"),
            Some(SubscriptionStatus::Expired)
        );
    }

    /// Unknown status strings must return None so callers can log-and-skip safely.
    #[test]
    fn test_status_mapping_unknown_returns_none() {
        assert_eq!(dodo_status_to_subscription_status("pending"), None);
        assert_eq!(dodo_status_to_subscription_status("unknown_status"), None);
        assert_eq!(dodo_status_to_subscription_status(""), None);
    }

    // ── dodo_payment_to_normalized: product_id resolution ───────────────────

    fn make_payment_data(
        product_id: Option<&str>,
        cart: Option<Vec<&str>>,
    ) -> DodoWebhookPaymentData {
        DodoWebhookPaymentData {
            payment_id: "pay_test".to_string(),
            customer: DodoWebhookCustomer {
                customer_id: "cus_test".to_string(),
            },
            product_id: product_id.map(str::to_string),
            product_cart: cart.map(|ids| {
                ids.into_iter()
                    .map(|id| PaymentCartItem {
                        product_id: id.to_string(),
                    })
                    .collect()
            }),
            status: "succeeded".to_string(),
            currency: None,
            total_amount: None,
            subscription_id: None,
            metadata: None,
        }
    }

    /// Top-level product_id takes priority over product_cart.
    #[test]
    fn test_product_id_uses_top_level_when_present() {
        let d = make_payment_data(Some("prod_lifetime"), Some(vec!["prod_other"]));
        assert_eq!(
            dodo_payment_to_normalized(d).product_id.as_deref(),
            Some("prod_lifetime")
        );
    }

    /// Falls back to the first cart item when product_id is absent.
    #[test]
    fn test_product_id_falls_back_to_cart() {
        let d = make_payment_data(None, Some(vec!["prod_lifetime"]));
        assert_eq!(
            dodo_payment_to_normalized(d).product_id.as_deref(),
            Some("prod_lifetime")
        );
    }

    /// Only the first cart item is used.
    #[test]
    fn test_product_id_uses_first_cart_item_only() {
        let d = make_payment_data(None, Some(vec!["prod_a", "prod_b"]));
        assert_eq!(
            dodo_payment_to_normalized(d).product_id.as_deref(),
            Some("prod_a")
        );
    }

    /// None when both top-level and cart are absent.
    #[test]
    fn test_product_id_none_when_both_absent() {
        let d = make_payment_data(None, None);
        assert_eq!(dodo_payment_to_normalized(d).product_id, None);
    }

    /// None when cart is present but empty.
    #[test]
    fn test_product_id_none_when_cart_empty() {
        let d = make_payment_data(None, Some(vec![]));
        assert_eq!(dodo_payment_to_normalized(d).product_id, None);
    }

    // ── dodo_event_to_payment_event ──────────────────────────────────────────

    fn sub_json(sub_id: &str, status: &str, product_id: &str) -> serde_json::Value {
        json!({
            "subscription_id": sub_id,
            "customer": { "customer_id": "cus_test" },
            "status": status,
            "product_id": product_id,
            "previous_billing_date": null,
            "next_billing_date": null,
            "cancel_at_next_billing_date": false
        })
    }

    /// subscription.active maps to SubscriptionActive with correct field names.
    #[test]
    fn test_subscription_active_event_parsed() {
        let data = sub_json("sub_001", "active", "prod_starter");
        let event = dodo_event_to_payment_event("subscription.active", data).unwrap();
        let PaymentEvent::SubscriptionActive(d) = event else {
            panic!("expected SubscriptionActive");
        };
        assert_eq!(d.provider_subscription_id, "sub_001");
        assert_eq!(d.product_id, "prod_starter");
        assert!(!d.cancel_at_period_end);
    }

    /// subscription.renewed maps to SubscriptionActive (same state machine handler).
    #[test]
    fn test_subscription_renewed_maps_to_active() {
        let data = sub_json("sub_002", "active", "prod_pro");
        let event = dodo_event_to_payment_event("subscription.renewed", data).unwrap();
        assert!(matches!(event, PaymentEvent::SubscriptionActive(_)));
    }

    /// subscription.on_hold maps to SubscriptionOnHold.
    #[test]
    fn test_subscription_on_hold_event_parsed() {
        let data = sub_json("sub_003", "on_hold", "prod_pro");
        let event = dodo_event_to_payment_event("subscription.on_hold", data).unwrap();
        assert!(matches!(event, PaymentEvent::SubscriptionOnHold(_)));
    }

    /// subscription.cancelled maps to SubscriptionCancelled.
    #[test]
    fn test_subscription_cancelled_event_parsed() {
        let data = sub_json("sub_004", "cancelled", "prod_pro");
        let event = dodo_event_to_payment_event("subscription.cancelled", data).unwrap();
        assert!(matches!(event, PaymentEvent::SubscriptionCancelled(_)));
    }

    /// subscription.expired maps to SubscriptionExpired.
    #[test]
    fn test_subscription_expired_event_parsed() {
        let data = sub_json("sub_005", "expired", "prod_pro");
        let event = dodo_event_to_payment_event("subscription.expired", data).unwrap();
        assert!(matches!(event, PaymentEvent::SubscriptionExpired(_)));
    }

    /// Unrecognized event types map to Unknown — never an error.
    #[test]
    fn test_unknown_event_type() {
        let event =
            dodo_event_to_payment_event("some.new.event", serde_json::Value::Null).unwrap();
        let PaymentEvent::Unknown { event_type } = event else {
            panic!("expected Unknown");
        };
        assert_eq!(event_type, "some.new.event");
    }

    /// cancel_at_next_billing_date in the Dodo payload maps to cancel_at_period_end.
    #[test]
    fn test_cancel_at_next_billing_date_mapped_to_cancel_at_period_end() {
        let data = json!({
            "subscription_id": "sub_006",
            "customer": { "customer_id": "cus_test" },
            "status": "cancelled",
            "product_id": "prod_pro",
            "previous_billing_date": null,
            "next_billing_date": null,
            "cancel_at_next_billing_date": true
        });
        let event = dodo_event_to_payment_event("subscription.cancelled", data).unwrap();
        let PaymentEvent::SubscriptionCancelled(d) = event else {
            panic!("expected SubscriptionCancelled");
        };
        assert!(
            d.cancel_at_period_end,
            "cancel_at_next_billing_date=true must become cancel_at_period_end=true"
        );
    }
}
