/// Self-hosted binary entrypoint.
///
/// Runs the API server with billing disabled — all users receive Pro-equivalent
/// limits at no cost. The private `cloudless_hosting_app` binary mirrors this
/// file but wires `BillingAdapter::Dodo` for hosted Dodo billing.
use api_server::{
    MIGRATOR,
    app_env::{AppEnv, BillingProductIds},
    config::AppServerConfig,
    core::auth::jwt_service::JwtService,
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
    web::{self, RouterOptions},
};
use std::sync::Arc;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppServerConfig::load().expect("Failed to load configuration");

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.rust_log.clone().into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(false),
        )
        .init();

    tracing::info!("Starting CloudLess self-hosted API...");
    tracing::info!("Connecting to database at {}", config.database_url);

    let pg_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(30)
        .connect(&config.database_url)
        .await?;

    MIGRATOR
        .run(&pg_pool)
        .await
        .expect("Failed to run migrations");

    let email_adapter = EmailAdapter::from_config(&config).await;

    // Media storage (S3) for blog image uploads.
    let media_storage: Arc<Box<dyn cloudless_core::ports::storage::StoragePort>> = {
        let mut aws_cfg = aws_config::defaults(aws_config::BehaviorVersion::v2025_08_07())
            .region(aws_config::Region::new(config.aws_region.clone()));

        if let (Some(key_id), Some(secret)) =
            (&config.aws_access_key_id, &config.aws_secret_access_key)
        {
            let credentials = aws_sdk_s3::config::Credentials::new(
                key_id,
                secret,
                None,
                None,
                "cloudless-api-server",
            );
            aws_cfg = aws_cfg.credentials_provider(credentials);
        }

        let aws_cfg = aws_cfg.load().await;
        let s3_client = aws_sdk_s3::Client::new(&aws_cfg);
        let adaptor = cloudless_core::adapters::s3_storage_adaptor::S3StorageAdaptor::from_client(
            s3_client,
            config.media_s3_bucket.clone(),
        );
        Arc::new(Box::new(adaptor))
    };

    // Self-hosted: billing is disabled by default.
    // Set BILLING_MODE=enforce to enable tier-limit enforcement without a payment provider
    // (useful for integration tests and CI). The private cloudless_hosting binary uses
    // BillingAdapter::Dodo instead.
    let billing_enforcement_enabled = config.billing_mode == "enforce";
    let app_env = AppEnv {
        user_repo: PgUserRepo::new(pg_pool.clone()),
        auth_repo: PgAuthRepo::new(pg_pool.clone()),
        refresh_token_repo: PgRefreshTokenRepo::new(pg_pool.clone()),
        remote_storage_repo: PgRemoteStorageRepo::new(pg_pool.clone()),
        remote_file_version_repo: PgRemoteFileVersionRepo::new(pg_pool.clone()),
        backup_config_repo: PgBackupConfigRepo::new(pg_pool.clone()),
        local_device_repo: PgLocalDeviceRepo::new(pg_pool.clone()),
        chunk_repo: PgChunkRepo::new(pg_pool.clone()),
        dashboard_repo: PgDashboardRepo::new(pg_pool.clone()),
        backup_job_repo: PgBackupJobRepo::new(pg_pool.clone()),
        encrypted_dek_repo: PgEncryptedDekRepo::new(pg_pool.clone()),
        gc_repo: PgGcRepo::new(pg_pool.clone()),
        security_event_repo: PgSecurityEventRepo::new(pg_pool.clone()),
        blog_repo: PgBlogPostRepo::new(pg_pool.clone()),
        email_template_repo: PgEmailTemplateRepo::new(pg_pool.clone()),
        password_reset_token_repo: PgPasswordResetTokenRepo::new(pg_pool.clone()),
        policy_repo: PgPolicyRepo::new(pg_pool.clone()),
        restore_job_repo: PgRestoreJobRepo::new(pg_pool.clone()),
        subscription_repo: PgSubscriptionRepo::new(pg_pool.clone()),
        verification_code_repo: PgVerificationCodeRepo::new(pg_pool.clone()),
        email_adapter,
        frontend_base_url: config.frontend_base_url,
        media_storage,
        jwt_service: JwtService::new(config.jwt_secret, config.jwt_access_ttl_secs),
        email_sending_enabled: config.email_provider != "log",
        billing: BillingAdapter::Disabled(DisabledPaymentProvider),
        billing_enforcement_enabled,
        billing_product_ids: BillingProductIds::default(),
        payment_environment: String::new(),
        return_url_base: String::new(),
        cancel_url_base: String::new(),
    };

    // Parse CORS_ORIGINS env var (comma-separated) into the router options.
    let extra_cors_origins = config
        .cors_origins
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    // Billing routes and webhooks are hosted-only — not mounted in self-hosted mode.
    let router_opts = RouterOptions {
        enable_billing_webhooks: false,
        enable_checkout_routes: false,
        extra_cors_origins,
    };

    let app = web::create_router(app_env, router_opts);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
