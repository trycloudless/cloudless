use std::sync::Arc;

use api_types::subscription::SubscriptionTier;

use crate::{
    core::{
        auth::{env::AuthEnv, jwt_service::JwtService},
        backup_config::env::BackupConfigEnv,
        backup_job::env::BackupJobEnv,
        blog::env::BlogEnv,
        chunks::env::ChunkEnv,
        dashboard::env::DashboardEnv,
        email_template::env::EmailTemplateEnv,
        email_verification::env::EmailVerificationEnv,
        encrypted_dek::env::EncryptedDekEnv,
        gc::env::GcEnv,
        local_device::env::LocalDeviceEnv,
        media::env::MediaEnv,
        password_reset::env::PasswordResetEnv,
        policy::env::PolicyEnv,
        remote_file_version::env::RemoteFileVersionEnv,
        remote_storage::env::RemoteStorageEnv,
        restore_job::env::RestoreJobEnv,
        security_event::env::SecurityEventEnv,
        subscription::env::{PaymentEnv, SubscriptionEnv},
        user::env::UserEnv,
    },
    infra::{
        billing::{BillingAdapter, disabled::DisabledPaymentProvider},
        psql::{
            pg_auth_repo::PgAuthRepo, pg_backup_config_repo::PgBackupConfigRepo,
            pg_backup_job_repo::PgBackupJobRepo, pg_blog_repo::PgBlogPostRepo,
            pg_chunk_repo::PgChunkRepo, pg_dashboard_repo::PgDashboardRepo,
            pg_email_template_repo::PgEmailTemplateRepo, pg_encrypted_dek_repo::PgEncryptedDekRepo,
            pg_gc_repo::PgGcRepo, pg_local_device_repo::PgLocalDeviceRepo,
            pg_password_reset_token_repo::PgPasswordResetTokenRepo, pg_policy_repo::PgPolicyRepo,
            pg_refresh_token_repo::PgRefreshTokenRepo,
            pg_remote_file_version::PgRemoteFileVersionRepo,
            pg_remote_storage_repo::PgRemoteStorageRepo, pg_restore_job_repo::PgRestoreJobRepo,
            pg_security_event_repo::PgSecurityEventRepo, pg_subscription_repo::PgSubscriptionRepo,
            pg_user_repo::PgUserRepo, pg_verification_code_repo::PgVerificationCodeRepo,
        },
        ses::EmailAdapter,
    },
};

/// Product IDs for each paid subscription tier.
///
/// Grouped into a struct so `AppEnv` carries one field instead of three.
/// Values are `None` when the corresponding env var is not set.
#[derive(Clone, Debug, Default)]
pub struct BillingProductIds {
    pub starter: Option<String>,
    pub pro: Option<String>,
    pub lifetime: Option<String>,
}

#[derive(Clone)]
pub struct AppEnv {
    pub user_repo: PgUserRepo,
    pub auth_repo: PgAuthRepo,
    pub refresh_token_repo: PgRefreshTokenRepo,
    pub jwt_service: JwtService,
    pub remote_storage_repo: PgRemoteStorageRepo,
    pub remote_file_version_repo: PgRemoteFileVersionRepo,
    pub backup_config_repo: PgBackupConfigRepo,
    pub local_device_repo: PgLocalDeviceRepo,
    pub chunk_repo: PgChunkRepo,
    pub dashboard_repo: PgDashboardRepo,
    pub backup_job_repo: PgBackupJobRepo,
    pub encrypted_dek_repo: PgEncryptedDekRepo,
    pub gc_repo: PgGcRepo,
    pub security_event_repo: PgSecurityEventRepo,
    pub blog_repo: PgBlogPostRepo,
    pub email_template_repo: PgEmailTemplateRepo,
    pub password_reset_token_repo: PgPasswordResetTokenRepo,
    pub policy_repo: PgPolicyRepo,
    pub restore_job_repo: PgRestoreJobRepo,
    pub subscription_repo: PgSubscriptionRepo,
    pub verification_code_repo: PgVerificationCodeRepo,
    pub email_adapter: EmailAdapter,
    pub frontend_base_url: String,
    pub media_storage: Arc<Box<dyn cloudless_core::ports::storage::StoragePort>>,
    /// True when a real email provider (ses/resend) is active; false in "log" dev mode.
    /// Controls whether signup triggers email verification or auto-verifies the user.
    pub email_sending_enabled: bool,
    /// Active billing adapter — `BillingAdapter::Dodo` for hosted, `Disabled` for self-hosted.
    pub billing: BillingAdapter,
    /// When true, subscription tier limits are enforced even if `billing` is `Disabled`.
    ///
    /// Set this via `BILLING_MODE=enforce` to run the server with tier gating active
    /// without a real payment provider — useful for integration tests and CI.
    /// When false (default for self-hosted), all users receive Pro-equivalent limits.
    pub billing_enforcement_enabled: bool,
    /// Product IDs for each paid tier (read from env vars, `None` when unset).
    pub billing_product_ids: BillingProductIds,
    /// Environment label used as webhook metadata (e.g. "test_mode", "live_mode").
    pub payment_environment: String,
    /// Base URL for the billing return page (e.g. "https://trycloudless.io/billing/return").
    pub return_url_base: String,
    /// Base URL for the billing cancel page.
    pub cancel_url_base: String,
}

impl AppEnv {
    /// Creates an `AppEnv` with billing disabled — for self-hosted deployments.
    ///
    /// All billing-related operations return errors when this constructor is used.
    /// The `billing` and URL fields are set to sensible defaults.
    pub fn with_disabled_billing(
        billing_product_ids: BillingProductIds,
        payment_environment: String,
        return_url_base: String,
        cancel_url_base: String,
    ) -> BillingFields {
        BillingFields {
            billing: BillingAdapter::Disabled(DisabledPaymentProvider),
            billing_enforcement_enabled: false,
            billing_product_ids,
            payment_environment,
            return_url_base,
            cancel_url_base,
        }
    }
}

/// Helper returned by `AppEnv::with_disabled_billing` so callers can spread the
/// fields into the struct literal without repeating them.
pub struct BillingFields {
    pub billing: BillingAdapter,
    pub billing_enforcement_enabled: bool,
    pub billing_product_ids: BillingProductIds,
    pub payment_environment: String,
    pub return_url_base: String,
    pub cancel_url_base: String,
}

impl UserEnv for AppEnv {
    type Repo = PgUserRepo;
    fn user_repo(&self) -> &Self::Repo {
        &self.user_repo
    }
}

impl AuthEnv for AppEnv {
    type AuthRepo = PgAuthRepo;

    type RefreshTokenRepo = PgRefreshTokenRepo;

    fn auth_repo(&self) -> &Self::AuthRepo {
        &self.auth_repo
    }

    fn refresh_token_repo(&self) -> &Self::RefreshTokenRepo {
        &self.refresh_token_repo
    }

    fn jwt_service(&self) -> &JwtService {
        &self.jwt_service
    }
}

impl RemoteStorageEnv for AppEnv {
    type Repo = PgRemoteStorageRepo;

    fn remote_storage_repo(&self) -> &Self::Repo {
        &self.remote_storage_repo
    }
}

impl RemoteFileVersionEnv for AppEnv {
    type Repo = PgRemoteFileVersionRepo;

    fn remote_file_version_repo(&self) -> &Self::Repo {
        &self.remote_file_version_repo
    }
}

impl BackupConfigEnv for AppEnv {
    type Repo = PgBackupConfigRepo;

    fn backup_config_repo(&self) -> &Self::Repo {
        &self.backup_config_repo
    }
}

impl LocalDeviceEnv for AppEnv {
    type LocalDeviceRepo = PgLocalDeviceRepo;

    fn local_device_repo(&self) -> &Self::LocalDeviceRepo {
        &self.local_device_repo
    }
}

impl ChunkEnv for AppEnv {
    type Repo = PgChunkRepo;

    fn chunk_repo(&self) -> &Self::Repo {
        &self.chunk_repo
    }
}

impl DashboardEnv for AppEnv {
    type Repo = PgDashboardRepo;

    fn dashboard_repo(&self) -> &Self::Repo {
        &self.dashboard_repo
    }
}

impl BackupJobEnv for AppEnv {
    type Repo = PgBackupJobRepo;

    fn backup_job_repo(&self) -> &Self::Repo {
        &self.backup_job_repo
    }
}

impl EncryptedDekEnv for AppEnv {
    type Repo = PgEncryptedDekRepo;

    fn encrypted_dek_repo(&self) -> &Self::Repo {
        &self.encrypted_dek_repo
    }
}

impl GcEnv for AppEnv {
    type GcRepo = PgGcRepo;

    fn gc_repo(&self) -> &Self::GcRepo {
        &self.gc_repo
    }
}

impl SecurityEventEnv for AppEnv {
    type Repo = PgSecurityEventRepo;

    fn security_event_repo(&self) -> &Self::Repo {
        &self.security_event_repo
    }
}

impl BlogEnv for AppEnv {
    type Repo = PgBlogPostRepo;

    fn blog_repo(&self) -> &Self::Repo {
        &self.blog_repo
    }
}

impl MediaEnv for AppEnv {
    type Storage = Box<dyn cloudless_core::ports::storage::StoragePort>;

    fn media_storage(&self) -> &Self::Storage {
        &self.media_storage
    }
}

impl EmailTemplateEnv for AppEnv {
    type Repo = PgEmailTemplateRepo;

    fn email_template_repo(&self) -> &Self::Repo {
        &self.email_template_repo
    }
}

impl PasswordResetEnv for AppEnv {
    type PasswordResetTokenRepo = PgPasswordResetTokenRepo;
    type AuthRepo = PgAuthRepo;
    type UserRepo = PgUserRepo;
    type EmailTemplateRepo = PgEmailTemplateRepo;
    type EmailAdapter = EmailAdapter;

    fn password_reset_token_repo(&self) -> &Self::PasswordResetTokenRepo {
        &self.password_reset_token_repo
    }

    fn auth_repo(&self) -> &Self::AuthRepo {
        &self.auth_repo
    }

    fn user_repo(&self) -> &Self::UserRepo {
        &self.user_repo
    }

    fn email_template_repo(&self) -> &Self::EmailTemplateRepo {
        &self.email_template_repo
    }

    fn email_adapter(&self) -> &Self::EmailAdapter {
        &self.email_adapter
    }

    fn frontend_base_url(&self) -> &str {
        &self.frontend_base_url
    }
}

impl PolicyEnv for AppEnv {
    type Repo = PgPolicyRepo;

    fn policy_repo(&self) -> &Self::Repo {
        &self.policy_repo
    }
}

impl RestoreJobEnv for AppEnv {
    type Repo = PgRestoreJobRepo;

    fn restore_job_repo(&self) -> &Self::Repo {
        &self.restore_job_repo
    }
}

impl EmailVerificationEnv for AppEnv {
    type VerificationCodeRepo = PgVerificationCodeRepo;
    type AuthRepo = PgAuthRepo;
    type UserRepo = PgUserRepo;
    type EmailAdapter = EmailAdapter;

    fn verification_code_repo(&self) -> &Self::VerificationCodeRepo {
        &self.verification_code_repo
    }

    fn auth_repo(&self) -> &Self::AuthRepo {
        &self.auth_repo
    }

    fn user_repo(&self) -> &Self::UserRepo {
        &self.user_repo
    }

    fn email_adapter(&self) -> &Self::EmailAdapter {
        &self.email_adapter
    }
}

impl SubscriptionEnv for AppEnv {
    type Repo = PgSubscriptionRepo;

    fn subscription_repo(&self) -> &Self::Repo {
        &self.subscription_repo
    }
}

impl PaymentEnv for AppEnv {
    type Provider = BillingAdapter;
    type Verifier = BillingAdapter;

    fn payment_provider(&self) -> &Self::Provider {
        &self.billing
    }

    fn payment_webhook_verifier(&self) -> &Self::Verifier {
        &self.billing
    }

    fn billing_enabled(&self) -> bool {
        // Dodo active = full hosted billing; enforce flag = tier limits with disabled provider.
        matches!(self.billing, BillingAdapter::Dodo(_)) || self.billing_enforcement_enabled
    }

    fn product_id_for_tier(&self, tier: SubscriptionTier) -> Option<String> {
        match tier {
            SubscriptionTier::Starter => self.billing_product_ids.starter.clone(),
            SubscriptionTier::Pro => self.billing_product_ids.pro.clone(),
            SubscriptionTier::Lifetime => self.billing_product_ids.lifetime.clone(),
            SubscriptionTier::Free => None,
        }
    }

    fn payment_environment(&self) -> &str {
        &self.payment_environment
    }

    fn return_url_base(&self) -> &str {
        &self.return_url_base
    }

    fn cancel_url_base(&self) -> &str {
        &self.cancel_url_base
    }
}
