use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Subscription pricing tier.
///
/// Stored as lowercase snake_case in PostgreSQL (`subscription_tier` enum)
/// and serialized the same way over the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionTier {
    Free,
    Starter,
    Pro,
    Lifetime,
}

impl Default for SubscriptionTier {
    fn default() -> Self {
        Self::Free
    }
}

impl std::fmt::Display for SubscriptionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Free => write!(f, "free"),
            Self::Starter => write!(f, "starter"),
            Self::Pro => write!(f, "pro"),
            Self::Lifetime => write!(f, "lifetime"),
        }
    }
}

impl std::str::FromStr for SubscriptionTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "free" => Ok(Self::Free),
            "starter" => Ok(Self::Starter),
            "pro" => Ok(Self::Pro),
            "lifetime" => Ok(Self::Lifetime),
            other => Err(format!("unknown subscription tier: '{}'", other)),
        }
    }
}

/// Subscription lifecycle status.
///
/// Stored as lowercase snake_case in PostgreSQL (`subscription_status` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    /// Dodo payment retry in progress. User retains paid-tier read-only access
    /// during this grace period until the subscription is resolved or expires.
    OnHold,
    Canceled,
    Expired,
}

impl Default for SubscriptionStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl std::fmt::Display for SubscriptionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::PastDue => write!(f, "past_due"),
            Self::OnHold => write!(f, "on_hold"),
            Self::Canceled => write!(f, "canceled"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

impl std::str::FromStr for SubscriptionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "past_due" => Ok(Self::PastDue),
            "on_hold" => Ok(Self::OnHold),
            "canceled" => Ok(Self::Canceled),
            "expired" => Ok(Self::Expired),
            other => Err(format!("unknown subscription status: '{}'", other)),
        }
    }
}

/// Reason for a subscription tier or status change. Recorded in the
/// `subscription_history` table for every mutation.
///
/// Stored as lowercase snake_case in PostgreSQL (`tier_change_reason` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TierChangeReason {
    Initial,
    Upgrade,
    Downgrade,
    Renewal,
    Cancellation,
    Expiration,
    Reactivation,
    LifetimePurchase,
    AdminOverride,
}

impl std::fmt::Display for TierChangeReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initial => write!(f, "initial"),
            Self::Upgrade => write!(f, "upgrade"),
            Self::Downgrade => write!(f, "downgrade"),
            Self::Renewal => write!(f, "renewal"),
            Self::Cancellation => write!(f, "cancellation"),
            Self::Expiration => write!(f, "expiration"),
            Self::Reactivation => write!(f, "reactivation"),
            Self::LifetimePurchase => write!(f, "lifetime_purchase"),
            Self::AdminOverride => write!(f, "admin_override"),
        }
    }
}

impl std::str::FromStr for TierChangeReason {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "initial" => Ok(Self::Initial),
            "upgrade" => Ok(Self::Upgrade),
            "downgrade" => Ok(Self::Downgrade),
            "renewal" => Ok(Self::Renewal),
            "cancellation" => Ok(Self::Cancellation),
            "expiration" => Ok(Self::Expiration),
            "reactivation" => Ok(Self::Reactivation),
            "lifetime_purchase" => Ok(Self::LifetimePurchase),
            "admin_override" => Ok(Self::AdminOverride),
            other => Err(format!("unknown tier change reason: '{}'", other)),
        }
    }
}

// ---------------------------------------------------------------------------
// Core domain types
// ---------------------------------------------------------------------------

/// A user's subscription record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Uuid,
    pub user_id: Uuid,
    pub tier: SubscriptionTier,
    pub status: SubscriptionStatus,
    pub current_period_start: Option<DateTime<Utc>>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Resource limits derived from a subscription tier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TierLimits {
    /// Maximum number of registered devices. `None` means unlimited.
    pub max_devices: Option<u32>,
    /// Maximum number of backup configurations. `None` means unlimited.
    pub max_backup_configs: Option<u32>,
    /// Whether automatic scheduled backups are allowed.
    pub auto_backup_enabled: bool,
    /// Metadata retention in days. `None` means unlimited.
    pub metadata_retention_days: Option<u32>,
}

/// Subscription record bundled with the computed effective tier and limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionResponse {
    pub subscription: Subscription,
    /// The effective tier after applying period-aware cancellation logic.
    /// Use this for UI display and billing action decisions — not `subscription.tier`.
    pub effective_tier: SubscriptionTier,
    pub limits: TierLimits,
    /// Whether the billing system is active on this deployment.
    ///
    /// `false` on self-hosted instances (`BILLING_MODE=disabled`): the server
    /// returns Pro-equivalent limits and the client must hide all upgrade/portal UI.
    /// Defaults to `true` when deserializing old API responses that lack this field.
    #[serde(default = "default_billing_enabled")]
    pub billing_enabled: bool,
}

/// Serde default: treat missing `billing_enabled` as `true` so old API responses
/// don't accidentally hide billing UI for paying customers.
fn default_billing_enabled() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Tier logic
// ---------------------------------------------------------------------------

impl SubscriptionTier {
    /// Returns the resource limits for this tier.
    pub fn limits(&self) -> TierLimits {
        match self {
            Self::Free => TierLimits {
                max_devices: Some(1),
                max_backup_configs: Some(1),
                auto_backup_enabled: false,
                metadata_retention_days: Some(7),
            },
            Self::Starter => TierLimits {
                max_devices: Some(3),
                max_backup_configs: Some(3),
                auto_backup_enabled: true,
                metadata_retention_days: None,
            },
            Self::Pro | Self::Lifetime => TierLimits {
                max_devices: None,
                max_backup_configs: None,
                auto_backup_enabled: true,
                metadata_retention_days: None,
            },
        }
    }

    /// Whether the tier is a paid tier.
    pub fn is_paid(&self) -> bool {
        !matches!(self, Self::Free)
    }

    /// Numeric rank for comparing tiers. Higher = more premium.
    pub fn rank(&self) -> u8 {
        match self {
            Self::Free => 0,
            Self::Starter => 1,
            Self::Pro => 2,
            Self::Lifetime => 3,
        }
    }

    /// Determines the change reason when transitioning between tiers.
    pub fn change_reason(from: &Self, to: &Self) -> TierChangeReason {
        match to.rank().cmp(&from.rank()) {
            std::cmp::Ordering::Greater => {
                if matches!(to, Self::Lifetime) {
                    TierChangeReason::LifetimePurchase
                } else {
                    TierChangeReason::Upgrade
                }
            }
            std::cmp::Ordering::Less => TierChangeReason::Downgrade,
            std::cmp::Ordering::Equal => TierChangeReason::Renewal,
        }
    }

    /// Computes the effective tier based on the subscription status.
    ///
    /// - Lifetime subscriptions are never downgraded.
    /// - Active and past-due (grace period) subscriptions keep their tier.
    /// - Canceled or expired paid subscriptions fall back to free limits.
    pub fn effective_tier(tier: &Self, status: &SubscriptionStatus) -> Self {
        match (tier, status) {
            (Self::Lifetime, _) => Self::Lifetime,
            // OnHold = Dodo payment retry in progress; user retains access during grace period
            (
                _,
                SubscriptionStatus::Active
                | SubscriptionStatus::PastDue
                | SubscriptionStatus::OnHold,
            ) => *tier,
            _ => Self::Free,
        }
    }
}

impl Subscription {
    /// Returns the effective tier for this subscription, accounting for status and period.
    ///
    /// A subscription cancelled with `cancel_at_period_end = true` retains its paid tier
    /// until `current_period_end` passes, matching the architecture's "access until period
    /// end" contract. Once `subscription.expired` fires, status becomes Expired and the
    /// tier falls to Free via the static fallback.
    pub fn effective_tier(&self) -> SubscriptionTier {
        if self.status == SubscriptionStatus::Canceled
            && self.cancel_at_period_end
            && self
                .current_period_end
                .map_or(false, |end| end > Utc::now())
        {
            return self.tier;
        }
        SubscriptionTier::effective_tier(&self.tier, &self.status)
    }

    /// Returns the resource limits for this subscription's effective tier.
    pub fn effective_limits(&self) -> TierLimits {
        self.effective_tier().limits()
    }

    /// Builds a `SubscriptionResponse` using the effective tier's limits.
    ///
    /// Always sets `billing_enabled: true`; callers that need `false`
    /// (self-hosted early return) use struct update syntax to override.
    pub fn to_response(&self) -> SubscriptionResponse {
        let effective_tier = self.effective_tier();
        SubscriptionResponse {
            subscription: self.clone(),
            effective_tier,
            limits: effective_tier.limits(),
            billing_enabled: true,
        }
    }
}

impl Subscription {
    /// Returns a placeholder free-tier active subscription (nil IDs).
    pub fn default_free() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            tier: SubscriptionTier::Free,
            status: SubscriptionStatus::Active,
            current_period_start: None,
            current_period_end: None,
            cancel_at_period_end: false,
            created_at: now,
            updated_at: now,
        }
    }
}

impl Default for SubscriptionResponse {
    /// Returns a free-tier active subscription response with hardcoded limits.
    /// Prefer fetching limits from the database via the application layer.
    fn default() -> Self {
        Subscription::default_free().to_response()
    }
}

// ---------------------------------------------------------------------------
// Subscription history types
// ---------------------------------------------------------------------------

/// A single entry in the subscription change history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionHistoryEntry {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub user_id: Uuid,
    pub previous_tier: Option<SubscriptionTier>,
    pub new_tier: SubscriptionTier,
    pub previous_status: Option<SubscriptionStatus>,
    pub new_status: SubscriptionStatus,
    pub reason: TierChangeReason,
    pub external_event_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub changed_at: DateTime<Utc>,
}

/// Response for the subscription history endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionHistoryResponse {
    pub entries: Vec<SubscriptionHistoryEntry>,
}

// ---------------------------------------------------------------------------
// Usage tracking (for over-limit detection on downgrade)
// ---------------------------------------------------------------------------

/// Current resource usage alongside the tier limits.
///
/// Used by the feature gate middleware to determine whether a user is at
/// or over their plan's limits after a downgrade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TierUsage {
    pub limits: TierLimits,
    pub current_devices: u32,
    pub current_configs: u32,
    /// Whether the user currently exceeds the device limit (e.g. after downgrade).
    pub devices_over_limit: bool,
    /// Whether the user currently exceeds the backup config limit.
    pub configs_over_limit: bool,
}

impl TierUsage {
    /// Builds a `TierUsage` from the current resource counts and plan limits.
    pub fn new(limits: TierLimits, current_devices: u32, current_configs: u32) -> Self {
        let devices_over_limit = limits
            .max_devices
            .map_or(false, |max| current_devices > max);
        let configs_over_limit = limits
            .max_backup_configs
            .map_or(false, |max| current_configs > max);
        Self {
            limits,
            current_devices,
            current_configs,
            devices_over_limit,
            configs_over_limit,
        }
    }

    /// Whether the user can add another device.
    pub fn can_add_device(&self) -> bool {
        self.limits
            .max_devices
            .map_or(true, |max| self.current_devices < max)
    }

    /// Whether the user can add another backup configuration.
    pub fn can_add_config(&self) -> bool {
        self.limits
            .max_backup_configs
            .map_or(true, |max| self.current_configs < max)
    }
}

// ---------------------------------------------------------------------------
// Plan change request
// ---------------------------------------------------------------------------

/// Request to change the current subscription plan mid-cycle.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChangePlanRequest {
    pub new_tier: SubscriptionTier,
}

// ---------------------------------------------------------------------------
// Checkout / webhook request/response types
// ---------------------------------------------------------------------------

/// Request to create a payment checkout session for a given tier.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCheckoutRequest {
    pub tier: SubscriptionTier,
}

/// Response containing the hosted checkout URL and the internal checkout session ID.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCheckoutResponse {
    pub checkout_url: String,
    pub checkout_id: Uuid,
}

/// Response containing the Dodo customer portal URL.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePortalResponse {
    pub portal_url: String,
}

/// Status of a checkout session, returned by the public status endpoint.
/// Clients use this to show real-time feedback on the billing return page.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CheckoutStatusResponse {
    /// "pending" | "completed" | "failed"
    pub status: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_free_tier_limits() {
        let limits = SubscriptionTier::Free.limits();
        assert_eq!(limits.max_devices, Some(1));
        assert_eq!(limits.max_backup_configs, Some(1));
        assert!(!limits.auto_backup_enabled);
        assert_eq!(limits.metadata_retention_days, Some(7));
    }

    #[test]
    fn test_starter_tier_limits() {
        let limits = SubscriptionTier::Starter.limits();
        assert_eq!(limits.max_devices, Some(3));
        assert_eq!(limits.max_backup_configs, Some(3));
        assert!(limits.auto_backup_enabled);
        assert_eq!(limits.metadata_retention_days, None);
    }

    #[test]
    fn test_pro_tier_limits() {
        let limits = SubscriptionTier::Pro.limits();
        assert_eq!(limits.max_devices, None);
        assert!(limits.auto_backup_enabled);
    }

    #[test]
    fn test_lifetime_tier_limits() {
        assert_eq!(
            SubscriptionTier::Lifetime.limits(),
            SubscriptionTier::Pro.limits()
        );
    }

    #[test]
    fn test_effective_tier_active_paid() {
        assert_eq!(
            SubscriptionTier::effective_tier(&SubscriptionTier::Pro, &SubscriptionStatus::Active),
            SubscriptionTier::Pro
        );
    }

    #[test]
    fn test_effective_tier_past_due_keeps_tier() {
        assert_eq!(
            SubscriptionTier::effective_tier(
                &SubscriptionTier::Starter,
                &SubscriptionStatus::PastDue
            ),
            SubscriptionTier::Starter
        );
    }

    #[test]
    fn test_effective_tier_on_hold_keeps_tier() {
        assert_eq!(
            SubscriptionTier::effective_tier(&SubscriptionTier::Pro, &SubscriptionStatus::OnHold),
            SubscriptionTier::Pro
        );
        assert_eq!(
            SubscriptionTier::effective_tier(
                &SubscriptionTier::Starter,
                &SubscriptionStatus::OnHold
            ),
            SubscriptionTier::Starter
        );
    }

    #[test]
    fn test_effective_tier_canceled_falls_to_free() {
        assert_eq!(
            SubscriptionTier::effective_tier(&SubscriptionTier::Pro, &SubscriptionStatus::Canceled),
            SubscriptionTier::Free
        );
    }

    #[test]
    fn test_effective_tier_expired_falls_to_free() {
        assert_eq!(
            SubscriptionTier::effective_tier(
                &SubscriptionTier::Starter,
                &SubscriptionStatus::Expired
            ),
            SubscriptionTier::Free
        );
    }

    #[test]
    fn test_effective_tier_lifetime_never_downgrades() {
        assert_eq!(
            SubscriptionTier::effective_tier(
                &SubscriptionTier::Lifetime,
                &SubscriptionStatus::Canceled
            ),
            SubscriptionTier::Lifetime
        );
        assert_eq!(
            SubscriptionTier::effective_tier(
                &SubscriptionTier::Lifetime,
                &SubscriptionStatus::Expired
            ),
            SubscriptionTier::Lifetime
        );
    }

    #[test]
    fn test_is_paid() {
        assert!(!SubscriptionTier::Free.is_paid());
        assert!(SubscriptionTier::Starter.is_paid());
        assert!(SubscriptionTier::Pro.is_paid());
        assert!(SubscriptionTier::Lifetime.is_paid());
    }

    #[test]
    fn test_tier_from_str() {
        assert_eq!(
            "free".parse::<SubscriptionTier>().unwrap(),
            SubscriptionTier::Free
        );
        assert_eq!(
            "starter".parse::<SubscriptionTier>().unwrap(),
            SubscriptionTier::Starter
        );
        assert_eq!(
            "pro".parse::<SubscriptionTier>().unwrap(),
            SubscriptionTier::Pro
        );
        assert_eq!(
            "lifetime".parse::<SubscriptionTier>().unwrap(),
            SubscriptionTier::Lifetime
        );
        assert!("invalid".parse::<SubscriptionTier>().is_err());
    }

    #[test]
    fn test_status_from_str() {
        assert_eq!(
            "active".parse::<SubscriptionStatus>().unwrap(),
            SubscriptionStatus::Active
        );
        assert_eq!(
            "past_due".parse::<SubscriptionStatus>().unwrap(),
            SubscriptionStatus::PastDue
        );
        assert_eq!(
            "on_hold".parse::<SubscriptionStatus>().unwrap(),
            SubscriptionStatus::OnHold
        );
        assert_eq!(
            "canceled".parse::<SubscriptionStatus>().unwrap(),
            SubscriptionStatus::Canceled
        );
        assert_eq!(
            "expired".parse::<SubscriptionStatus>().unwrap(),
            SubscriptionStatus::Expired
        );
        assert!("invalid".parse::<SubscriptionStatus>().is_err());
    }

    #[test]
    fn test_tier_display_roundtrip() {
        for tier in [
            SubscriptionTier::Free,
            SubscriptionTier::Starter,
            SubscriptionTier::Pro,
            SubscriptionTier::Lifetime,
        ] {
            assert_eq!(tier.to_string().parse::<SubscriptionTier>().unwrap(), tier);
        }
    }

    #[test]
    fn test_status_display_roundtrip() {
        for status in [
            SubscriptionStatus::Active,
            SubscriptionStatus::PastDue,
            SubscriptionStatus::OnHold,
            SubscriptionStatus::Canceled,
            SubscriptionStatus::Expired,
        ] {
            assert_eq!(
                status.to_string().parse::<SubscriptionStatus>().unwrap(),
                status
            );
        }
    }

    #[test]
    fn test_serde_tier_roundtrip() {
        let tier = SubscriptionTier::Starter;
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, "\"starter\"");
        let parsed: SubscriptionTier = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tier);
    }

    #[test]
    fn test_serde_status_roundtrip() {
        let status = SubscriptionStatus::PastDue;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"past_due\"");
        let parsed: SubscriptionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    // --- Tier ranking & change reason ---

    #[test]
    fn test_tier_rank_ordering() {
        assert!(SubscriptionTier::Free.rank() < SubscriptionTier::Starter.rank());
        assert!(SubscriptionTier::Starter.rank() < SubscriptionTier::Pro.rank());
        assert!(SubscriptionTier::Pro.rank() < SubscriptionTier::Lifetime.rank());
    }

    #[test]
    fn test_change_reason_upgrade() {
        assert_eq!(
            SubscriptionTier::change_reason(&SubscriptionTier::Free, &SubscriptionTier::Starter),
            TierChangeReason::Upgrade
        );
        assert_eq!(
            SubscriptionTier::change_reason(&SubscriptionTier::Starter, &SubscriptionTier::Pro),
            TierChangeReason::Upgrade
        );
    }

    #[test]
    fn test_change_reason_downgrade() {
        assert_eq!(
            SubscriptionTier::change_reason(&SubscriptionTier::Pro, &SubscriptionTier::Starter),
            TierChangeReason::Downgrade
        );
        assert_eq!(
            SubscriptionTier::change_reason(&SubscriptionTier::Starter, &SubscriptionTier::Free),
            TierChangeReason::Downgrade
        );
    }

    #[test]
    fn test_change_reason_renewal_same_tier() {
        assert_eq!(
            SubscriptionTier::change_reason(&SubscriptionTier::Pro, &SubscriptionTier::Pro),
            TierChangeReason::Renewal
        );
    }

    #[test]
    fn test_change_reason_lifetime_purchase() {
        assert_eq!(
            SubscriptionTier::change_reason(&SubscriptionTier::Free, &SubscriptionTier::Lifetime),
            TierChangeReason::LifetimePurchase
        );
        assert_eq!(
            SubscriptionTier::change_reason(&SubscriptionTier::Pro, &SubscriptionTier::Lifetime),
            TierChangeReason::LifetimePurchase
        );
    }

    // --- TierChangeReason FromStr/Display ---

    #[test]
    fn test_tier_change_reason_roundtrip() {
        let reasons = [
            TierChangeReason::Initial,
            TierChangeReason::Upgrade,
            TierChangeReason::Downgrade,
            TierChangeReason::Renewal,
            TierChangeReason::Cancellation,
            TierChangeReason::Expiration,
            TierChangeReason::Reactivation,
            TierChangeReason::LifetimePurchase,
            TierChangeReason::AdminOverride,
        ];
        for reason in reasons {
            assert_eq!(
                reason.to_string().parse::<TierChangeReason>().unwrap(),
                reason
            );
        }
    }

    #[test]
    fn test_tier_change_reason_from_str_invalid() {
        assert!("invalid".parse::<TierChangeReason>().is_err());
    }

    // --- TierUsage ---

    #[test]
    fn test_tier_usage_under_limit() {
        let usage = TierUsage::new(SubscriptionTier::Starter.limits(), 1, 2);
        assert!(!usage.devices_over_limit);
        assert!(!usage.configs_over_limit);
        assert!(usage.can_add_device());
        assert!(usage.can_add_config());
    }

    #[test]
    fn test_tier_usage_at_limit() {
        let usage = TierUsage::new(SubscriptionTier::Starter.limits(), 3, 3);
        assert!(!usage.devices_over_limit);
        assert!(!usage.configs_over_limit);
        assert!(!usage.can_add_device());
        assert!(!usage.can_add_config());
    }

    #[test]
    fn test_tier_usage_over_limit_after_downgrade() {
        let usage = TierUsage::new(SubscriptionTier::Free.limits(), 3, 5);
        assert!(usage.devices_over_limit);
        assert!(usage.configs_over_limit);
        assert!(!usage.can_add_device());
        assert!(!usage.can_add_config());
    }

    #[test]
    fn test_tier_usage_unlimited() {
        let usage = TierUsage::new(SubscriptionTier::Pro.limits(), 100, 50);
        assert!(!usage.devices_over_limit);
        assert!(!usage.configs_over_limit);
        assert!(usage.can_add_device());
        assert!(usage.can_add_config());
    }

    // ── Subscription::effective_tier() — period-aware cancellation ───

    fn make_sub(
        tier: SubscriptionTier,
        status: SubscriptionStatus,
        cancel_at_period_end: bool,
        period_end: Option<DateTime<Utc>>,
    ) -> Subscription {
        Subscription {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            tier,
            status,
            current_period_start: None,
            current_period_end: period_end,
            cancel_at_period_end,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_canceled_with_future_period_end_keeps_paid_tier() {
        let future = Utc::now() + chrono::Duration::days(10);
        let sub = make_sub(
            SubscriptionTier::Pro,
            SubscriptionStatus::Canceled,
            true,
            Some(future),
        );
        // User cancelled but still within paid period — must retain Pro limits
        assert_eq!(sub.effective_tier(), SubscriptionTier::Pro);
    }

    #[test]
    fn test_canceled_with_past_period_end_falls_to_free() {
        let past = Utc::now() - chrono::Duration::days(1);
        let sub = make_sub(
            SubscriptionTier::Pro,
            SubscriptionStatus::Canceled,
            true,
            Some(past),
        );
        // Period has elapsed — falls to Free
        assert_eq!(sub.effective_tier(), SubscriptionTier::Free);
    }

    #[test]
    fn test_canceled_without_cancel_at_period_end_falls_to_free_immediately() {
        let future = Utc::now() + chrono::Duration::days(10);
        // cancel_at_period_end = false: no grace period, revoke immediately
        let sub = make_sub(
            SubscriptionTier::Starter,
            SubscriptionStatus::Canceled,
            false,
            Some(future),
        );
        assert_eq!(sub.effective_tier(), SubscriptionTier::Free);
    }

    #[test]
    fn test_canceled_with_no_period_end_falls_to_free() {
        let sub = make_sub(
            SubscriptionTier::Starter,
            SubscriptionStatus::Canceled,
            true,
            None,
        );
        // cancel_at_period_end but no period_end recorded — fall to Free conservatively
        assert_eq!(sub.effective_tier(), SubscriptionTier::Free);
    }

    #[test]
    fn test_active_subscription_keeps_tier_regardless_of_cancel_flag() {
        let future = Utc::now() + chrono::Duration::days(10);
        let sub = make_sub(
            SubscriptionTier::Pro,
            SubscriptionStatus::Active,
            true,
            Some(future),
        );
        assert_eq!(sub.effective_tier(), SubscriptionTier::Pro);
    }
}
