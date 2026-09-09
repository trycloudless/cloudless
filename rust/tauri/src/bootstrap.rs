use api_types::{
    auth::LoginRequest,
    backup_config::BackupExclusionConfig,
    encrypted_dek::DekKeyType,
    local_device::GetOrCreateLocalDeviceRequest,
    remote_storage::{
        CreateRemoteStorageRequest, RemoteStorageConfig, RemoteStorageType, S3Credentials,
    },
    user::UserCreateRequest,
};
use cloudless_core::{
    adapters::aes_gcm_encryptor::AesGcmEncryptor,
    app_env::AppEnv,
    applications::backup::backup_config::encrypt_create_config_request,
    applications::{config::application, user::application as user_app},
    domain::{dek::Dek, derived_keys::DerivedKeys},
    ports::Encryptor,
};

/// Populates the database with test data for development.
/// Errors are intentionally ignored — if data already exists, this is a no-op.
pub async fn bootstrap_test_data(env: &AppEnv) {
    tracing::info!("Bootstrapping test data for dev mode...");

    // 1. Create user
    let user = match user_app::create_user(
        env,
        UserCreateRequest {
            name: "Rohit".to_string(),
            email: "dev@example.com".to_string(),
            password: "devpassword".to_string(),
        },
    )
    .await
    {
        Ok(u) => {
            tracing::info!("Created test user: {}", u.email);
            u
        }
        Err(e) => {
            tracing::debug!("User creation skipped (may already exist): {e}");
            return;
        }
    };

    // 2. Login to get auth token set
    if let Err(e) = user_app::login(
        env,
        LoginRequest {
            email: "dev@example.com".to_string(),
            password: "devpassword".to_string(),
        },
    )
    .await
    {
        tracing::warn!("Bootstrap login failed: {e}");
        return;
    }
    tracing::info!("Logged in as test user");

    // 3. Setup encryption with password "aaaaaaaa"
    let dek = match Dek::generate() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("Bootstrap DEK generation failed: {e}");
            return;
        }
    };

    if let Err(e) = user_app::update_dek(env, dek.clone(), "aaaaaaaa", DekKeyType::Password).await {
        tracing::warn!("Bootstrap DEK storage failed: {e}");
        return;
    }
    tracing::info!("Encryption set up with password");

    // 4. Get or create local device
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let physical_device_id = machine_uid::get().unwrap_or_default();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let physical_device_id = String::new();

    let device = match application::get_or_create_local_device(
        env,
        GetOrCreateLocalDeviceRequest {
            physical_device_id,
            display_name: Some("Dev MacBook".to_string()),
            platform: std::env::consts::OS.to_string(),
        },
    )
    .await
    {
        Ok(d) => {
            tracing::info!(
                "Device {}: {} (was_created={})",
                if d.was_created { "created" } else { "found" },
                d.id,
                d.was_created
            );
            d
        }
        Err(e) => {
            tracing::warn!("Bootstrap device registration failed: {e}");
            return;
        }
    };

    // 5. Create remote storage with encrypted S3 config
    // Set BOOTSTRAP_AWS_* env vars before running; placeholders are used otherwise.
    let s3_creds = S3Credentials {
        access_key: std::env::var("BOOTSTRAP_AWS_ACCESS_KEY").unwrap_or_else(|_| "YOUR_AWS_ACCESS_KEY".to_string()),
        secret: std::env::var("BOOTSTRAP_AWS_SECRET_KEY").unwrap_or_else(|_| "YOUR_AWS_SECRET_KEY".to_string()),
        region: std::env::var("BOOTSTRAP_AWS_REGION").unwrap_or_else(|_| "ap-south-1".to_string()),
        bucket: std::env::var("BOOTSTRAP_S3_BUCKET").unwrap_or_else(|_| "your-dev-backup-bucket".to_string()),
    };
    let config = RemoteStorageConfig::Aws(s3_creds);
    let config_json = match serde_json::to_vec(&config) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("Bootstrap S3 config serialization failed: {e}");
            return;
        }
    };

    let encryptor = match AesGcmEncryptor::new(&dek) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Bootstrap encryptor creation failed: {e}");
            return;
        }
    };
    let encrypted_config = match encryptor.encrypt(&config_json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Bootstrap config encryption failed: {e}");
            return;
        }
    };

    let storage = match application::register_remote_storage(
        env,
        CreateRemoteStorageRequest {
            name: "Dev S3 Storage".to_string(),
            storage_type: RemoteStorageType::Aws,
            config: encrypted_config.into(),
        },
    )
    .await
    {
        Ok(s) => {
            tracing::info!("Created remote storage: {}", s.id);
            s
        }
        Err(e) => {
            tracing::warn!("Bootstrap remote storage creation failed: {e}");
            return;
        }
    };

    // 6. Create backup config (encrypt source directory)
    let derived_keys = match DerivedKeys::derive(&dek) {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!("Bootstrap derived keys generation failed: {e}");
            return;
        }
    };

    let encrypted_config_request = match encrypt_create_config_request(
        "/tmp/cloudless-dev-backup",
        storage.id,
        "Default Backup".to_string(),
        device.id,
        Default::default(),
        &BackupExclusionConfig::default(),
        &derived_keys,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Bootstrap source directory encryption failed: {e}");
            return;
        }
    };

    match application::create_backup_config(env, encrypted_config_request).await {
        Ok(c) => tracing::info!("Created backup config: {}", c.id),
        Err(e) => tracing::warn!("Bootstrap backup config creation failed: {e}"),
    }

    tracing::info!("Bootstrap complete — user: {}", user.email);
}
