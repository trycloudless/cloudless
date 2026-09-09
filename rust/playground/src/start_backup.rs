mod util;

use api_types::{
    auth::LoginRequest,
    backup_config::ListBackupConfigWithRemoteStorageRequest,
    encrypted_dek::{DekKeyType, GetEncryptedDekRequest},
};
use argon2::password_hash::SaltString;
use cloudless_core::{
    adapters::sqlite_local_index::SqliteLocalIndex,
    app_env::AppEnv,
    applications::{
        backup::{
            backup_config::decrypt_backup_config_with_storage,
            backup_job::{BackupResult, start_backup},
            env::BackupEnv,
        },
        user::application::login,
    },
    domain::{dek::Dek, derived_keys::DerivedKeys, kek::Kek},
    model::base::EncryptedData,
    ports::api::{
        backup_config_api_port::BackupConfigApiPort, encrypted_dek_api_port::EncryptedDekApiPort,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url: url::Url = util::BASE_URL.parse()?;
    println!("Connecting to server at {}", base_url);
    let index_path = std::path::PathBuf::from(".cloudless_playground_index.db");
    let local_index = SqliteLocalIndex::open(&index_path).await?;
    let env = AppEnv::new(base_url, local_index);

    // Step 1: Login
    let login_request = LoginRequest {
        email: util::USER_EMAIL.to_string(),
        password: util::USER_PASSWORD.to_string(),
    };
    login(&env, login_request).await?;
    println!("Login successful");

    // Step 2: Retrieve and decrypt DEK using password
    let dek_response = env
        .encrypted_dek_api()
        .get(GetEncryptedDekRequest {
            key_type: DekKeyType::Password,
        })
        .await?;

    let salt =
        SaltString::from_b64(&dek_response.salt).map_err(|e| format!("invalid salt: {e}"))?;
    let kek = Kek::derive(util::PASSWORD, &salt)?;
    let encrypted_data: EncryptedData = dek_response.encrypted_key.try_into()?;
    let dek = Dek::decrypt_dek(&kek, &encrypted_data)?;
    let derived_keys = DerivedKeys::derive(&dek)?;
    println!("Successfully decrypted DEK from server");

    println!(
        "Fetching backup configs for physical device: {}",
        util::PHYSICAL_DEVICE_ID
    );
    let configs = env
        .backup_config_api()
        .list_with_remote_storage(ListBackupConfigWithRemoteStorageRequest {
            physical_device_id: util::PHYSICAL_DEVICE_ID.to_string(),
        })
        .await?;

    if configs.list.is_empty() {
        println!("No backup configs found. Run user_signup first.");
        return Ok(());
    }

    println!("Found {} backup config(s):", configs.list.len());
    let decrypted_configs: Vec<_> = configs
        .list
        .into_iter()
        .map(|c| decrypt_backup_config_with_storage(c, &derived_keys.metadata_key))
        .collect::<Result<_, _>>()?;
    for config in &decrypted_configs {
        println!(
            "  - config={}, storage={}, dir={}",
            config.config_id, config.storage_id, config.source_directory
        );
    }

    // Step 4: Start backup for the first config
    let config = &decrypted_configs[0];
    println!(
        "\n--- Starting backup for {} ---\n",
        config.source_directory
    );

    let device_id = config
        .local_device_id
        .expect("backup config has no local_device_id");
    let mut rx = start_backup(env, dek, derived_keys, device_id, config.config_id);

    while let Some(event) = rx.recv().await {
        match event {
            BackupResult::ConfigLoaded {
                config_id,
                storage_id,
            } => {
                println!("[config] Loaded config={config_id}, storage={storage_id}");
            }
            BackupResult::IndexUpdated {
                files_updated,
                files_failed,
            } => {
                println!("[sync] {files_updated} files synced, {files_failed} failed");
            }
            BackupResult::CandidatesFound {
                total_files,
                total_bytes,
            } => {
                println!(
                    "[scan] Found {total_files} files to backup ({:.2} MB)",
                    total_bytes as f64 / (1024.0 * 1024.0)
                );
            }
            BackupResult::FileStarted {
                ref file_path,
                file_size,
                base_version,
            } => {
                let version_label = base_version.map_or("new".to_string(), |v| format!("v{v}"));
                println!(
                    "[file] Starting: {} ({version_label}, {:.2} MB)",
                    file_path,
                    file_size as f64 / (1024.0 * 1024.0)
                );
            }
            BackupResult::ChunkUploaded {
                ref file_path,
                chunk_index,
                bytes_uploaded,
                file_size,
                ..
            } => {
                let pct = if file_size > 0 {
                    (bytes_uploaded as f64 / file_size as f64) * 100.0
                } else {
                    100.0
                };
                let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
                println!("[upload] {file_name} chunk #{chunk_index} — {pct:.1}%");
            }
            BackupResult::FileCompleted {
                ref file_path,
                total_chunks,
                uploaded_bytes,
                deduplicated_bytes,
            } => {
                println!(
                    "[done] {} ({total_chunks} chunks, {:.2} MB uploaded, {:.2} MB deduplicated)",
                    file_path,
                    uploaded_bytes as f64 / (1024.0 * 1024.0),
                    deduplicated_bytes as f64 / (1024.0 * 1024.0)
                );
            }
            BackupResult::Completed {
                total_files,
                total_bytes,
                uploaded_bytes,
                deduplicated_bytes,
            } => {
                println!(
                    "\n[completed] Backup finished: {total_files} files, {:.2} MB total, {:.2} MB uploaded, {:.2} MB deduplicated",
                    total_bytes as f64 / (1024.0 * 1024.0),
                    uploaded_bytes as f64 / (1024.0 * 1024.0),
                    deduplicated_bytes as f64 / (1024.0 * 1024.0)
                );
            }
            BackupResult::FileFailed {
                ref file_path,
                ref reason,
            } => {
                eprintln!("[warn] File failed {file_path}: {reason}");
            }
            BackupResult::CleanupCompleted {
                files_deleted,
                bytes_freed,
            } => {
                println!(
                    "[cleanup] Deleted {files_deleted} local files, freed {:.2} MB",
                    bytes_freed as f64 / (1024.0 * 1024.0)
                );
            }
            BackupResult::Failed {
                ref reason,
                ref file_path,
            } => {
                if let Some(path) = file_path {
                    eprintln!("[error] Failed on {path}: {reason}");
                } else {
                    eprintln!("[error] Backup failed: {reason}");
                }
            }
        }
    }

    Ok(())
}
