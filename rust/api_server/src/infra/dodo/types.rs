use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Checkout session
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CreateCheckoutSessionRequest {
    pub product_cart: Vec<ProductCartItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer: Option<CheckoutCustomer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_address: Option<BillingAddress>,
    pub metadata: std::collections::HashMap<String, String>,
    pub return_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProductCartItem {
    pub product_id: String,
    pub quantity: u32,
}

/// Sent in checkout requests. Use `customer_id` for returning customers,
/// or `email`+`name` for first-time customers (Dodo creates the record).
#[derive(Debug, Serialize)]
pub struct CheckoutCustomer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BillingAddress {
    pub country: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zipcode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCheckoutSessionResponse {
    pub checkout_url: Option<String>,
    /// Dodo's session ID for this checkout session (field: `session_id`).
    /// Stored as `provider_checkout_session_id` to enable checkout correlation.
    pub session_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Customer portal
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CreateCustomerPortalSessionRequest {
    pub customer_id: String,
    pub return_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCustomerPortalSessionResponse {
    pub link: String,
}

// ---------------------------------------------------------------------------
// Subscription fetch (for reconciliation)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DodoSubscription {
    pub subscription_id: String,
    pub customer: DodoWebhookCustomer,
    pub status: String,
    pub product_id: String,
    pub quantity: Option<u32>,
    pub currency: Option<String>,
    pub previous_billing_date: Option<DateTime<Utc>>,
    pub next_billing_date: Option<DateTime<Utc>>,
    pub trial_period_days: Option<i32>,
}

// ---------------------------------------------------------------------------
// Webhooks
// ---------------------------------------------------------------------------

/// Top-level webhook envelope from Dodo.
#[derive(Debug, Deserialize)]
pub struct DodoWebhookEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: serde_json::Value,
    pub business_id: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
}

/// Nested customer object inside subscription webhook events.
#[derive(Debug, Deserialize)]
pub struct DodoWebhookCustomer {
    pub customer_id: String,
}

/// Parsed subscription data embedded in webhook event `data`.
#[derive(Debug, Deserialize)]
pub struct DodoWebhookSubscriptionData {
    pub subscription_id: String,
    pub customer: DodoWebhookCustomer,
    pub status: String,
    pub product_id: String,
    pub quantity: Option<u32>,
    pub currency: Option<String>,
    /// Start of the current billing period (Dodo calls this `previous_billing_date`).
    pub previous_billing_date: Option<DateTime<Utc>>,
    pub next_billing_date: Option<DateTime<Utc>>,
    pub trial_period_days: Option<i32>,
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
    pub cancel_at_next_billing_date: Option<bool>,
}

/// One item in a payment webhook's product_cart array.
#[derive(Debug, Deserialize)]
pub struct PaymentCartItem {
    pub product_id: String,
}

/// Parsed payment data embedded in payment webhook event `data`.
///
/// One-time purchases may carry the product either as a top-level `product_id`
/// or inside `product_cart`. Both are checked; `product_id` takes priority.
#[derive(Debug, Deserialize)]
pub struct DodoWebhookPaymentData {
    pub payment_id: String,
    pub customer: DodoWebhookCustomer,
    /// Top-level product ID. Present in some Dodo payload shapes.
    pub product_id: Option<String>,
    /// Cart items. Present in one-time purchase payloads per Dodo docs.
    pub product_cart: Option<Vec<PaymentCartItem>>,
    pub status: String,
    pub currency: Option<String>,
    pub total_amount: Option<i64>,
    pub subscription_id: Option<String>,
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}
