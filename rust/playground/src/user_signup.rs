mod util;

use api_types::{
    auth::LoginRequest,
    backup_config::BackupExclusionConfig,
    encrypted_dek::{DekKeyType, StoreEncryptedDekRequest},
    local_device::CreateLocalDeviceRequest,
    remote_storage::{
        CreateRemoteStorageRequest, RemoteStorageConfig, RemoteStorageType, S3Credentials,
    },
    user::UserCreateRequest,
};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use cloudless_core::{
    adapters::{aes_gcm_encryptor::AesGcmEncryptor, sqlite_local_index::SqliteLocalIndex},
    app_env::AppEnv,
    applications::{
        backup::{backup_config::encrypt_create_config_request, env::BackupEnv},
        user::application::{create_user, login},
    },
    domain::{dek::Dek, derived_keys::DerivedKeys, kek::Kek},
    model::base::EncryptedData,
    ports::{
        Encryptor,
        api::{
            backup_config_api_port::BackupConfigApiPort,
            encrypted_dek_api_port::EncryptedDekApiPort, local_device_api_port::LocalDeviceApiPort,
            remote_storage_api_port::RemoteStorageApiPort,
        },
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url: url::Url = util::BASE_URL.parse()?;
    println!("Connecting to server at {}", base_url);
    let local_index = SqliteLocalIndex::open_in_memory().await?;
    let env = AppEnv::new(base_url, local_index);

    // Step 1: Create a new user (ignore error if already exists)
    println!("Creating user...");
    let user_request = UserCreateRequest {
        name: "Test User".to_string(),
        email: util::USER_EMAIL.to_string(),
        password: util::USER_PASSWORD.to_string(),
    };
    let _ = create_user(&env, user_request).await;

    // Step 2: Login
    let login_request = LoginRequest {
        email: util::USER_EMAIL.to_string(),
        password: util::USER_PASSWORD.to_string(),
    };
    login(&env, login_request).await?;
    println!("Login successful");

    // Step 3: Create a local device
    let local_device_request = CreateLocalDeviceRequest {
        physical_device_id: util::PHYSICAL_DEVICE_ID.to_string(),
        display_name: Some("My MacBook".to_string()),
        platform: "macos".to_string(),
    };
    let local_device_response = env.local_device_api().create(local_device_request).await?;
    println!("Created local device: {:?}", local_device_response);

    // Step 4: Generate DEK and store encrypted copies on server
    let dek = Dek::generate()?;

    // Encrypt DEK with password-derived KEK
    let password_salt = SaltString::generate(&mut OsRng);
    let password_kek = Kek::derive(util::PASSWORD, &password_salt)?;
    let password_encrypted_dek: EncryptedData = dek.encrypt_dek(&password_kek)?;

    env.encrypted_dek_api()
        .store(StoreEncryptedDekRequest {
            key_type: DekKeyType::Password,
            encrypted_key: password_encrypted_dek.into(),
            salt: password_salt.to_string(),
        })
        .await?;
    println!("Stored password-encrypted DEK");

    // Encrypt DEK with recovery-key-derived KEK
    let recovery_salt = SaltString::generate(&mut OsRng);
    let recovery_kek = Kek::derive(util::_RECOVERY_KEY, &recovery_salt)?;
    let recovery_encrypted_dek: EncryptedData = dek.encrypt_dek(&recovery_kek)?;

    env.encrypted_dek_api()
        .store(StoreEncryptedDekRequest {
            key_type: DekKeyType::Recovery,
            encrypted_key: recovery_encrypted_dek.into(),
            salt: recovery_salt.to_string(),
        })
        .await?;
    println!("Stored recovery-encrypted DEK");

    // Step 5: Encrypt S3 config with the DEK
    let encryptor = AesGcmEncryptor::new(&dek)?;

    let s3_config = RemoteStorageConfig::Aws(S3Credentials {
        access_key: std::env::var("AWS_ACCESS_KEY_ID").expect("AWS_ACCESS_KEY_ID env var required"),
        secret: std::env::var("AWS_SECRET_ACCESS_KEY")
            .expect("AWS_SECRET_ACCESS_KEY env var required"),
        region: std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
        bucket: std::env::var("AWS_S3_BUCKET").expect("AWS_S3_BUCKET env var required"),
    });

    let config_json = serde_json::to_vec(&s3_config)?;
    let encrypted_config: EncryptedData = encryptor.encrypt(&config_json)?;

    let remote_storage_request = CreateRemoteStorageRequest {
        name: "My S3 Backup".to_string(),
        storage_type: RemoteStorageType::Aws,
        config: encrypted_config.into(),
    };

    let remote_storage_response = env
        .remote_storage_api()
        .create(remote_storage_request)
        .await?;
    println!("Created remote storage: {:?}", remote_storage_response);

    // Step 6: Create backup config with ~/Downloads as source directory
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let downloads_path = format!("{}/Downloads/PDF", home_dir);
    let derived_keys = DerivedKeys::derive(&dek)?;

    let backup_config_request = encrypt_create_config_request(
        &downloads_path,
        remote_storage_response.id,
        "Downloads Backup".to_string(),
        local_device_response.id,
        Default::default(),
        &BackupExclusionConfig::default(),
        &derived_keys,
    )?;

    let backup_config_response = env
        .backup_config_api()
        .create(backup_config_request)
        .await?;
    println!("Created backup config: {:?}", backup_config_response);

    println!("\nSetup complete! Run start_backup to begin backing up files.");

    Ok(())
}
