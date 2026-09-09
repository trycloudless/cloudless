use api_types::gc::{
    ConfirmChunkDeletionsRequest, ConfirmChunkDeletionsResponse, GcCollectResponse,
    GcRunDetailResponse, GetRetentionSettingsResponse, ListGcRunsResponse,
};
use api_types::policy::{
    AcceptPolicyRequest, AcceptPolicyResponse, CreatePolicyRequest, CreatePolicyResponse,
    CreatePolicyVersionRequest, CreatePolicyVersionResponse, GetCheckoutPolicyResponse,
    GetPendingPoliciesResponse, ListPoliciesResponse, ListPolicyVersionsResponse, PolicyResponse,
    PolicyType, PolicyVersionResponse, PublicPolicyResponse, PublishPolicyVersionRequest,
    SkipPolicyRequest, SkipPolicyResponse, UpdatePolicyRequest, UpdatePolicyVersionRequest,
};
use api_types::security_event::{
    ListSecurityEventsResponse, StoreSecurityEventRequest, StoreSecurityEventResponse,
};
use api_types::subscription::{
    Subscription, SubscriptionHistoryEntry, SubscriptionStatus, SubscriptionTier, TierChangeReason,
    TierLimits,
};
use api_types::{
    backup_config::{
        BackupConfig, BackupConfigWithRemoteStorage, CleanupType, CreateBackupConfigRequest,
        CreateBackupConfigResponse, RenameBackupConfigResponse, ToggleBackupConfigResponse,
        UpdateCleanupTypeResponse, UpdateExclusionConfigResponse,
    },
    blog::{BlogPostResponse, BlogPostSummary, CreateBlogPostRequest, UpdateBlogPostRequest},
    chunk::{CreateChunkRequest, CreateChunkResponse, UpdateChunkStorageMetaRequest},
    common::Base64EncryptedData,
    email_template::{
        CreateEmailTemplateRequest, EmailTemplateResponse, EmailTemplateSummary, EmailTemplateType,
        UpdateEmailTemplateRequest,
    },
    encrypted_dek::{
        DekKeyType, GetEncryptedDekResponse, StoreEncryptedDekRequest, StoreEncryptedDekResponse,
    },
    local_device::{
        CreateLocalDeviceRequest, CreateLocalDeviceResponse, GetLocalDeviceByPhysicalIdResponse,
        ListAllDevicesResponse, ListDevicesByPlatformResponse,
    },
    remote_file_version::{
        CreateFileVersionRequest, CreateFileVersionResponse, ListAllVersionsRequest,
        ListAllVersionsResponse, ListBackedUpFilesRequest, ListBackedUpFilesResponse,
        ListBinVersionsRequest, ListBinVersionsResponse, MoveAllVersionsToBinRequest,
    },
    remote_storage::{
        CreateRemoteStorageRequest, CreateRemoteStorageResponse, RemoteStorageEntity,
        RemoteStorageStatus, RemoteStorageSummary,
    },
    restore_file_info::{GetFileVersionChunksRequest, GetFileVersionChunksResponse},
    user::{AdminUserInfo, RefreshToken, User, UserCreateRequest, UserCreateResponse, UserInfo},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::core::CoreResult;

#[async_trait]
pub trait DashboardRepo {
    /// Returns aggregated dashboard statistics for a user.
    async fn get_stats(
        &self,
        user_id: Uuid,
    ) -> CoreResult<api_types::dashboard::GetDashboardStatsResponse>;
}

#[async_trait]
pub trait BackupJobRepo {
    async fn create_job(
        &self,
        user_id: Uuid,
        req: api_types::backup_job::CreateBackupJobRequest,
    ) -> CoreResult<api_types::backup_job::CreateBackupJobResponse>;

    async fn create_job_file(
        &self,
        user_id: Uuid,
        req: api_types::backup_job::CreateBackupJobFileRequest,
    ) -> CoreResult<api_types::backup_job::CreateBackupJobFileResponse>;

    async fn complete_job_file(
        &self,
        user_id: Uuid,
        req: api_types::backup_job::CompleteBackupJobFileRequest,
    ) -> CoreResult<()>;

    async fn complete_job(
        &self,
        user_id: Uuid,
        req: api_types::backup_job::CompleteBackupJobRequest,
    ) -> CoreResult<()>;

    async fn list_jobs(
        &self,
        user_id: Uuid,
        req: api_types::backup_job::ListBackupJobsRequest,
    ) -> CoreResult<api_types::backup_job::ListBackupJobsResponse>;

    async fn get_job_detail(
        &self,
        user_id: Uuid,
        job_id: Uuid,
    ) -> CoreResult<api_types::backup_job::GetBackupJobDetailResponse>;

    async fn get_latest_job(
        &self,
        user_id: Uuid,
    ) -> CoreResult<api_types::backup_job::GetLatestBackupJobResponse>;

    async fn get_resumable_job(
        &self,
        user_id: Uuid,
        req: api_types::backup_job::GetResumableBackupJobRequest,
    ) -> CoreResult<api_types::backup_job::GetResumableBackupJobResponse>;

    async fn log_cleanup_files(
        &self,
        user_id: Uuid,
        req: api_types::backup_job::LogCleanupFilesRequest,
    ) -> CoreResult<()>;

    async fn abandon_stale_jobs(
        &self,
        user_id: Uuid,
    ) -> CoreResult<api_types::backup_job::AbandonStaleJobsResponse>;
}

#[async_trait]
pub trait ChunkRepo {
    async fn create(
        &self,
        user_id: Uuid,
        chunk: CreateChunkRequest,
    ) -> CoreResult<CreateChunkResponse>;

    /// Returns the ordered list of chunks belonging to a file version.
    /// Used during restore to know which chunks to download from remote storage.
    async fn get_chunks_for_version(
        &self,
        user_id: Uuid,
        request: GetFileVersionChunksRequest,
    ) -> CoreResult<GetFileVersionChunksResponse>;

    /// Updates a chunk's storage metadata (e.g. after re-encrypting a stale S3 object).
    async fn update_storage_meta(
        &self,
        user_id: Uuid,
        request: UpdateChunkStorageMetaRequest,
    ) -> CoreResult<()>;
}

#[async_trait]
pub trait UserRepo {
    async fn create_user(&self, user: UserCreateRequest) -> CoreResult<UserCreateResponse>;
    async fn find_by_id(&self, id: Uuid) -> CoreResult<UserInfo>;
    async fn list_all_paginated(
        &self,
        page: u32,
        per_page: u32,
    ) -> CoreResult<(Vec<AdminUserInfo>, u64)>;
    async fn update_password_hash(&self, user_id: Uuid, password_hash: String) -> CoreResult<()>;
    async fn set_email_verified(&self, user_id: Uuid, verified: bool) -> CoreResult<()>;
}

#[async_trait]
pub trait RemoteStorageRepo {
    async fn create(
        &self,
        user_id: Uuid,
        storage: CreateRemoteStorageRequest,
    ) -> CoreResult<CreateRemoteStorageResponse>;

    async fn get_by_id(&self, user_id: Uuid, id: Uuid) -> CoreResult<RemoteStorageEntity>;

    async fn list_by_user(&self, user_id: Uuid) -> CoreResult<Vec<RemoteStorageSummary>>;

    /// Update the status of a remote storage (e.g. mark as AuthTokenExpired).
    async fn update_status(
        &self,
        user_id: Uuid,
        id: Uuid,
        status: RemoteStorageStatus,
    ) -> CoreResult<()>;

    /// Update the encrypted config and reset status (used during reauth).
    /// Identity verification is done client-side; this just stores the new blob.
    async fn update_config(
        &self,
        user_id: Uuid,
        id: Uuid,
        config: Base64EncryptedData,
        status: RemoteStorageStatus,
    ) -> CoreResult<()>;
}

#[async_trait]
pub trait BackupConfigRepo {
    async fn create(
        &self,
        user_id: Uuid,
        config: CreateBackupConfigRequest,
    ) -> CoreResult<CreateBackupConfigResponse>;
    async fn get_by_id(&self, user_id: Uuid, id: Uuid) -> CoreResult<BackupConfig>;
    async fn list_config_with_storage(
        &self,
        user_id: Uuid,
        physical_device_id: &str,
    ) -> CoreResult<Vec<BackupConfigWithRemoteStorage>>;

    async fn list_all_for_user(
        &self,
        user_id: Uuid,
    ) -> CoreResult<Vec<BackupConfigWithRemoteStorage>>;

    async fn set_active(
        &self,
        user_id: Uuid,
        id: Uuid,
        is_active: bool,
    ) -> CoreResult<ToggleBackupConfigResponse>;

    async fn rename(
        &self,
        user_id: Uuid,
        id: Uuid,
        display_name: String,
    ) -> CoreResult<RenameBackupConfigResponse>;

    async fn update_cleanup_type(
        &self,
        user_id: Uuid,
        id: Uuid,
        cleanup_type: CleanupType,
    ) -> CoreResult<UpdateCleanupTypeResponse>;

    async fn update_exclusion_config(
        &self,
        user_id: Uuid,
        id: Uuid,
        encrypted_exclusion_config: Vec<u8>,
        exclusion_config_nonce: Vec<u8>,
    ) -> CoreResult<UpdateExclusionConfigResponse>;

    /// Count total backup configs for a user.
    async fn count_for_user(&self, user_id: Uuid) -> CoreResult<u32>;
}

#[async_trait]
pub trait AuthRepo {
    async fn find_by_email(&self, email: &str) -> CoreResult<Option<User>>;
}

#[async_trait]
pub trait RefreshTokenRepo {
    async fn insert(&self, token: RefreshToken) -> CoreResult<()>;
    async fn find_valid(&self, token_hash: &str) -> CoreResult<Option<RefreshToken>>;
    async fn revoke(&self, id: Uuid) -> CoreResult<()>;
}

#[async_trait]
pub trait RemoteFileVersionRepo {
    async fn create(
        &self,
        user_id: Uuid,
        request: CreateFileVersionRequest,
    ) -> CoreResult<CreateFileVersionResponse>;

    async fn list_backed_up_files(
        &self,
        user_id: Uuid,
        request: ListBackedUpFilesRequest,
    ) -> CoreResult<ListBackedUpFilesResponse>;

    /// `retention_cutoff` — oldest visible version timestamp; `None` = no filter.
    async fn list_all_versions(
        &self,
        user_id: Uuid,
        request: ListAllVersionsRequest,
        retention_cutoff: Option<DateTime<Utc>>,
    ) -> CoreResult<ListAllVersionsResponse>;

    async fn update_status(
        &self,
        request: api_types::remote_file_version::UpdateFileVersionStatusRequest,
    ) -> CoreResult<()>;

    /// Move a single file version to bin (soft-delete).
    async fn move_to_bin(&self, user_id: Uuid, version_id: Uuid) -> CoreResult<()>;

    /// Move all versions of a file (by blind index within a backup config) to bin.
    async fn move_all_to_bin(
        &self,
        user_id: Uuid,
        request: MoveAllVersionsToBinRequest,
    ) -> CoreResult<()>;

    /// Restore a single file version from bin.
    async fn restore_from_bin(&self, user_id: Uuid, version_id: Uuid) -> CoreResult<()>;

    /// List file versions currently in bin for a backup config.
    async fn list_bin_versions(
        &self,
        user_id: Uuid,
        request: ListBinVersionsRequest,
    ) -> CoreResult<ListBinVersionsResponse>;
}

#[async_trait]
pub trait LocalDeviceRepo {
    async fn create(
        &self,
        user_id: Uuid,
        local_device: CreateLocalDeviceRequest,
    ) -> CoreResult<CreateLocalDeviceResponse>;

    async fn get_by_physical_id(
        &self,
        user_id: Uuid,
        physical_device_id: &str,
    ) -> CoreResult<GetLocalDeviceByPhysicalIdResponse>;

    async fn list_by_platform(
        &self,
        user_id: Uuid,
        platform: &str,
    ) -> CoreResult<ListDevicesByPlatformResponse>;

    async fn list_all(&self, user_id: Uuid) -> CoreResult<ListAllDevicesResponse>;

    /// Count total registered devices for a user.
    async fn count_for_user(&self, user_id: Uuid) -> CoreResult<u32>;

    /// Finds an existing device matching this identity. Desktop devices (non-empty
    /// physical_device_id) match exactly on that stable id. Mobile devices (empty
    /// physical_device_id) match on (user_id, platform, display_name) using SQL `=`
    /// semantics, so NULL display_name never matches another NULL — two nameless
    /// devices are always treated as different, never silently merged.
    async fn find_matching_device(
        &self,
        user_id: Uuid,
        physical_device_id: &str,
        platform: &str,
        display_name: Option<&str>,
    ) -> CoreResult<Option<GetLocalDeviceByPhysicalIdResponse>>;
}

#[async_trait]
pub trait EncryptedDekRepo {
    async fn store(
        &self,
        user_id: Uuid,
        request: StoreEncryptedDekRequest,
    ) -> CoreResult<StoreEncryptedDekResponse>;

    async fn get(&self, user_id: Uuid, key_type: DekKeyType)
    -> CoreResult<GetEncryptedDekResponse>;
}

#[async_trait]
pub trait BlogPostRepo {
    async fn create(
        &self,
        user_id: Uuid,
        req: CreateBlogPostRequest,
    ) -> CoreResult<BlogPostResponse>;

    async fn update(
        &self,
        user_id: Uuid,
        req: UpdateBlogPostRequest,
    ) -> CoreResult<BlogPostResponse>;

    async fn delete(&self, user_id: Uuid, id: Uuid) -> CoreResult<()>;

    async fn get_by_slug(&self, slug: &str) -> CoreResult<Option<BlogPostResponse>>;

    /// Fetches a post by slug regardless of published status. Used by admin edit routes.
    async fn get_by_slug_any(&self, slug: &str) -> CoreResult<Option<BlogPostResponse>>;

    async fn list_published(&self) -> CoreResult<Vec<BlogPostSummary>>;

    /// Returns all posts including unpublished drafts. Used by admin listing.
    async fn list_all(&self) -> CoreResult<Vec<BlogPostSummary>>;

    // async fn list_all_by_user(&self, user_id: Uuid) -> CoreResult<Vec<BlogPostSummary>>;
}

#[async_trait]
pub trait EmailTemplateRepo {
    async fn create(&self, req: CreateEmailTemplateRequest) -> CoreResult<EmailTemplateResponse>;
    async fn update(&self, req: UpdateEmailTemplateRequest) -> CoreResult<EmailTemplateResponse>;
    async fn delete(&self, id: Uuid) -> CoreResult<()>;
    async fn get_by_id(&self, id: Uuid) -> CoreResult<Option<EmailTemplateResponse>>;
    async fn list_all(&self) -> CoreResult<Vec<EmailTemplateSummary>>;
    async fn get_latest_by_type(
        &self,
        template_type: EmailTemplateType,
    ) -> CoreResult<Option<EmailTemplateResponse>>;
}

/// Record for a password reset token stored in the database.
pub struct PasswordResetTokenRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait PasswordResetTokenRepo {
    async fn insert(
        &self,
        user_id: Uuid,
        token_hash: String,
        expires_at: DateTime<Utc>,
    ) -> CoreResult<()>;
    async fn find_valid(&self, token_hash: &str) -> CoreResult<Option<PasswordResetTokenRecord>>;
    async fn mark_used(&self, id: Uuid) -> CoreResult<()>;
}

#[async_trait]
pub trait EmailPort: Send + Sync {
    async fn send_email(&self, to: &str, subject: &str, body_html: &str) -> CoreResult<()>;
}

#[async_trait]
pub trait PolicyRepo {
    // Admin - policies
    async fn create_policy(&self, req: CreatePolicyRequest) -> CoreResult<CreatePolicyResponse>;
    async fn update_policy(&self, req: UpdatePolicyRequest) -> CoreResult<PolicyResponse>;
    async fn list_policies(&self) -> CoreResult<ListPoliciesResponse>;

    // Admin - versions
    async fn create_version(
        &self,
        req: CreatePolicyVersionRequest,
    ) -> CoreResult<CreatePolicyVersionResponse>;
    async fn update_version(
        &self,
        req: UpdatePolicyVersionRequest,
    ) -> CoreResult<PolicyVersionResponse>;
    async fn publish_version(
        &self,
        req: PublishPolicyVersionRequest,
    ) -> CoreResult<PolicyVersionResponse>;
    async fn list_versions(&self, policy_id: Uuid) -> CoreResult<ListPolicyVersionsResponse>;
    async fn get_version(&self, id: Uuid) -> CoreResult<PolicyVersionResponse>;

    // User-facing
    async fn get_pending_policies(&self, user_id: Uuid) -> CoreResult<GetPendingPoliciesResponse>;
    async fn accept_policy(
        &self,
        user_id: Uuid,
        req: AcceptPolicyRequest,
    ) -> CoreResult<AcceptPolicyResponse>;
    async fn skip_policy(
        &self,
        user_id: Uuid,
        req: SkipPolicyRequest,
    ) -> CoreResult<SkipPolicyResponse>;

    // Public (no auth) — latest published version for a given policy type
    async fn get_latest_published(
        &self,
        policy_type: PolicyType,
    ) -> CoreResult<Option<PublicPolicyResponse>>;

    /// Returns the pending Refund & Cancellation policy for the user, or None if already accepted.
    async fn get_checkout_policy(&self, user_id: Uuid) -> CoreResult<GetCheckoutPolicyResponse>;
}

#[async_trait]
pub trait RestoreJobRepo {
    async fn create_job(
        &self,
        user_id: Uuid,
        req: api_types::restore_job::CreateRestoreJobRequest,
    ) -> CoreResult<api_types::restore_job::CreateRestoreJobResponse>;

    async fn create_job_file(
        &self,
        user_id: Uuid,
        req: api_types::restore_job::CreateRestoreJobFileRequest,
    ) -> CoreResult<api_types::restore_job::CreateRestoreJobFileResponse>;

    async fn complete_job_file(
        &self,
        user_id: Uuid,
        req: api_types::restore_job::CompleteRestoreJobFileRequest,
    ) -> CoreResult<()>;

    async fn complete_job(
        &self,
        user_id: Uuid,
        req: api_types::restore_job::CompleteRestoreJobRequest,
    ) -> CoreResult<()>;

    async fn list_jobs(
        &self,
        user_id: Uuid,
        req: api_types::restore_job::ListRestoreJobsRequest,
    ) -> CoreResult<api_types::restore_job::ListRestoreJobsResponse>;

    async fn get_job_detail(
        &self,
        user_id: Uuid,
        job_id: Uuid,
    ) -> CoreResult<api_types::restore_job::GetRestoreJobDetailResponse>;

    async fn get_latest_job(
        &self,
        user_id: Uuid,
    ) -> CoreResult<api_types::restore_job::GetLatestRestoreJobResponse>;

    /// Returns a running restore job with its incomplete files, if any exists.
    /// Used to resume an interrupted restore.
    async fn get_resumable_job(
        &self,
        user_id: Uuid,
    ) -> CoreResult<api_types::restore_job::GetResumableRestoreJobResponse>;
}

#[async_trait]
pub trait SecurityEventRepo {
    async fn store(
        &self,
        user_id: Uuid,
        request: StoreSecurityEventRequest,
    ) -> CoreResult<StoreSecurityEventResponse>;

    async fn list(&self, user_id: Uuid) -> CoreResult<ListSecurityEventsResponse>;
}

#[async_trait]
pub trait GcRepo {
    /// Phase A: mark expired MovedToBin versions as Deleted, record audit trail, return orphaned chunks.
    async fn collect_expired_versions(&self, user_id: Uuid) -> CoreResult<GcCollectResponse>;

    /// Phase C: delete confirmed orphan chunks from DB, finalize gc_run record.
    async fn confirm_chunk_deletions(
        &self,
        user_id: Uuid,
        request: ConfirmChunkDeletionsRequest,
    ) -> CoreResult<ConfirmChunkDeletionsResponse>;

    /// List recent GC runs for a user (for history UI).
    async fn list_gc_runs(&self, user_id: Uuid, limit: i32) -> CoreResult<ListGcRunsResponse>;

    /// Get detailed info for a specific GC run.
    async fn get_gc_run_detail(
        &self,
        user_id: Uuid,
        gc_run_id: Uuid,
    ) -> CoreResult<GcRunDetailResponse>;

    /// Get the user's bin retention settings.
    async fn get_retention_settings(
        &self,
        user_id: Uuid,
    ) -> CoreResult<GetRetentionSettingsResponse>;

    /// Update the user's bin retention period.
    async fn update_retention_settings(
        &self,
        user_id: Uuid,
        bin_retention_days: i32,
    ) -> CoreResult<()>;
}

/// Record for an email verification code stored in the database.
pub struct VerificationCodeRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub code_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait VerificationCodeRepo {
    /// Store a hashed verification code with an expiration time.
    async fn insert(
        &self,
        user_id: Uuid,
        code_hash: String,
        expires_at: DateTime<Utc>,
    ) -> CoreResult<()>;

    /// Find a valid (unexpired, unused) verification code for a user.
    async fn find_valid(
        &self,
        user_id: Uuid,
        code_hash: &str,
    ) -> CoreResult<Option<VerificationCodeRecord>>;

    /// Mark a verification code as used.
    async fn mark_used(&self, id: Uuid) -> CoreResult<()>;

    /// Invalidate all unused codes for a user (when resending).
    async fn invalidate_all(&self, user_id: Uuid) -> CoreResult<()>;
}

// ---------------------------------------------------------------------------
// Internal structs for provider-specific subscription fields
// ---------------------------------------------------------------------------

/// Provider-specific fields on a subscription row. Not exposed to clients.
pub struct SubscriptionProviderFields {
    pub subscription_id: Uuid,
    pub provider: String,
    pub provider_subscription_id: Option<String>,
    pub provider_customer_id: Option<String>,
    pub provider_status: Option<String>,
    pub provider_product_id: Option<String>,
    pub last_webhook_at: Option<DateTime<Utc>>,
    pub last_synced_at: Option<DateTime<Utc>>,
}

/// Parameters for recording an incoming webhook event row.
pub struct RecordWebhookEventParams<'a> {
    pub subscription_id: Option<Uuid>,
    pub webhook_id: &'a str,
    pub event_type: &'a str,
    pub provider: &'a str,
    pub raw_headers: serde_json::Value,
    pub payload: serde_json::Value,
    pub verification_status: &'a str,
}

/// Parameters for inserting a checkout session row.
pub struct CreateCheckoutSessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub tier: SubscriptionTier,
    pub provider: String,
    pub provider_checkout_session_id: Option<String>,
    pub provider_product_id: String,
    pub checkout_url: String,
    pub return_url: String,
    pub cancel_url: String,
    pub metadata: serde_json::Value,
}

// ---------------------------------------------------------------------------
// SubscriptionRepo trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait SubscriptionRepo {
    /// Get a user's subscription, if one exists.
    async fn get_by_user_id(&self, user_id: Uuid) -> CoreResult<Option<Subscription>>;

    /// Get a subscription by its provider subscription ID.
    async fn get_by_provider_subscription_id(
        &self,
        provider_subscription_id: &str,
    ) -> CoreResult<Option<Subscription>>;

    /// Create a new subscription for a user.
    async fn create(
        &self,
        user_id: Uuid,
        tier: SubscriptionTier,
        status: SubscriptionStatus,
    ) -> CoreResult<Subscription>;

    /// Update a subscription's tier, status, period, and provider metadata.
    async fn update_tier_and_status(
        &self,
        id: Uuid,
        tier: SubscriptionTier,
        status: SubscriptionStatus,
        period_start: Option<DateTime<Utc>>,
        period_end: Option<DateTime<Utc>>,
        cancel_at_period_end: bool,
    ) -> CoreResult<Subscription>;

    /// Set or update the payment provider IDs on a subscription.
    /// Pass `None` for `provider_subscription_id` on one-time purchases (Lifetime)
    /// where there is no recurring subscription ID; the column is preserved via COALESCE.
    async fn set_provider_ids(
        &self,
        subscription_id: Uuid,
        provider_subscription_id: Option<&str>,
        provider_customer_id: &str,
    ) -> CoreResult<()>;

    /// Update provider-specific metadata fields (status string, product, billing dates).
    async fn update_provider_fields(
        &self,
        subscription_id: Uuid,
        provider_status: &str,
        provider_product_id: Option<&str>,
        next_billing_date: Option<DateTime<Utc>>,
        expires_at: Option<DateTime<Utc>>,
        last_webhook_at: Option<DateTime<Utc>>,
    ) -> CoreResult<()>;

    /// Fetch provider-specific fields for a subscription. Not part of the public DTO.
    async fn get_provider_fields(
        &self,
        subscription_id: Uuid,
    ) -> CoreResult<Option<SubscriptionProviderFields>>;

    /// Record a webhook event and return `false` if the webhook_id already exists
    /// (idempotency: caller should skip processing on false).
    async fn record_webhook_event(&self, params: RecordWebhookEventParams<'_>) -> CoreResult<bool>;

    /// Mark a previously-recorded webhook event as processed (or failed).
    async fn mark_event_processed(&self, webhook_id: &str, error: Option<&str>) -> CoreResult<()>;

    /// Insert a new checkout session tracking row.
    async fn create_checkout_session_record(
        &self,
        record: CreateCheckoutSessionRecord,
    ) -> CoreResult<()>;

    /// Mark a checkout session as completed.
    async fn complete_checkout_session(&self, provider_checkout_session_id: &str)
    -> CoreResult<()>;

    /// Mark a checkout session as completed using the internal cloudless checkout ID.
    /// Called from webhook handlers where `cloudless_checkout_id` is available in metadata.
    /// No-op if the session was already completed or the ID is not found.
    async fn complete_checkout_session_by_id(&self, checkout_id: Uuid) -> CoreResult<()>;

    /// Mark a checkout session as failed using the internal cloudless checkout ID.
    /// Called from payment.failed webhook handlers so failed purchases are
    /// distinguishable from still-processing ones in ops/support tooling.
    async fn fail_checkout_session_by_id(&self, checkout_id: Uuid) -> CoreResult<()>;

    /// Return the status string of a checkout session ("pending", "completed", "failed").
    /// Returns None when the checkout_id is not found.
    async fn get_checkout_session_status(
        &self,
        checkout_id: Uuid,
        user_id: Uuid,
    ) -> CoreResult<Option<String>>;

    /// Record a subscription history entry for audit trail.
    async fn record_history(
        &self,
        subscription_id: Uuid,
        user_id: Uuid,
        previous_tier: Option<SubscriptionTier>,
        new_tier: SubscriptionTier,
        previous_status: Option<SubscriptionStatus>,
        new_status: SubscriptionStatus,
        reason: TierChangeReason,
        webhook_id: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> CoreResult<()>;

    /// Get the full subscription history for a user, ordered by most recent first.
    async fn get_history(&self, user_id: Uuid) -> CoreResult<Vec<SubscriptionHistoryEntry>>;

    /// Get the configurable limits for a subscription tier from the database.
    async fn get_tier_limits(&self, tier: SubscriptionTier) -> CoreResult<TierLimits>;

    /// Return webhook events stuck in `pending` state for longer than `min_age_secs`.
    async fn list_pending_webhook_events(
        &self,
        min_age_secs: i64,
    ) -> CoreResult<Vec<PendingWebhookEvent>>;

    /// Apply a complete subscription state change atomically in one DB transaction.
    /// Replaces the three-call sequence of update_tier_and_status + update_provider_fields
    /// + record_history, eliminating partial-state windows on process failure.
    async fn apply_subscription_change_atomic(
        &self,
        params: SubscriptionChangeParams<'_>,
    ) -> CoreResult<()>;
}

/// All parameters needed to atomically update subscription tier, status, provider
/// fields, and history in a single DB transaction.
pub struct SubscriptionChangeParams<'a> {
    pub subscription_id: Uuid,
    pub user_id: Uuid,
    pub new_tier: SubscriptionTier,
    pub new_status: SubscriptionStatus,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub provider_status: &'a str,
    pub provider_product_id: Option<&'a str>,
    pub next_billing_date: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub previous_tier: Option<SubscriptionTier>,
    pub previous_status: Option<SubscriptionStatus>,
    pub reason: TierChangeReason,
    pub webhook_id: Option<&'a str>,
    pub metadata: Option<serde_json::Value>,
    /// Set for webhook-triggered changes; preserved via COALESCE when None.
    pub last_webhook_at: Option<DateTime<Utc>>,
    /// Set for reconciliation-triggered changes; preserved via COALESCE when None.
    pub last_synced_at: Option<DateTime<Utc>>,
}

/// A webhook event row that was persisted but not yet fully processed.
pub struct PendingWebhookEvent {
    pub webhook_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
}

pub mod payment_provider;
pub mod payment_webhook;
