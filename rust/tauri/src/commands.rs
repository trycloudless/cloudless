use crate::error::{TauriError, TauriResult};
use crate::models::*;
use api_types::{
    auth::{LoginRequest, LoginResponse},
    backup_config::{
        BackupExclusionConfig, CreateBackupConfigResponse, CreateBackupConfigUiRequest,
        DecryptedBackupConfigWithRemoteStorage, PreviewBackupExclusionsRequest,
        PreviewBackupExclusionsResponse, RenameBackupConfigRequest, RenameBackupConfigResponse,
        ToggleBackupConfigRequest, ToggleBackupConfigResponse, UpdateCleanupTypeRequest,
        UpdateCleanupTypeResponse, UpdateExclusionConfigResponse,
    },
    backup_job::{
        AbandonStaleJobsResponse, DecryptedBackupJobDetailResponse, GetBackupJobDetailRequest,
        GetLatestBackupJobResponse, GetResumableBackupJobRequest, GetResumableBackupJobResponse,
        ListBackupJobsRequest, ListBackupJobsResponse,
    },
    dashboard::GetDashboardStatsResponse,
    email_verification::{
        ResendVerificationRequest, ResendVerificationResponse, VerifyEmailRequest,
        VerifyEmailResponse,
    },
    encrypted_dek::DekKeyType,
    local_device::{
        CreateLocalDeviceRequest, CreateLocalDeviceResponse, GetLocalDeviceByPhysicalIdResponse,
        GetOrCreateLocalDeviceRequest, GetOrCreateLocalDeviceResponse, ListAllDevicesResponse,
        ListDevicesByPlatformRequest,
    },
    policy::{
        AcceptPolicyRequest, AcceptPolicyResponse, GetCheckoutPolicyResponse,
        GetPendingPoliciesResponse, SkipPolicyRequest, SkipPolicyResponse,
    },
    remote_file_version::{
        ListBackedUpFilesResponse, ListBinVersionsResponse, MoveAllVersionsToBinRequest,
        MoveVersionToBinRequest, RestoreVersionFromBinRequest,
    },
    remote_storage::{
        CreateRemoteStorageRequest, CreateRemoteStorageResponse, GetRemoteStorageRequest,
        GoogleDriveCredentials, ListRemoteStoragesResponse, LocalFilesystemConfig,
        OneDriveCredentials, ReauthRemoteStorageRequest, ReauthRemoteStorageResponse,
        RemoteStorageConfig, RemoteStorageType, S3Credentials, SftpAuthConfig, SftpCredentials,
        SftpHostKeyPolicy,
    },
    restore_job::{
        DecryptedRestoreJobDetailResponse, GetLatestRestoreJobResponse, GetRestoreJobDetailRequest,
        ListRestoreJobsRequest, ListRestoreJobsResponse,
    },
    subscription::SubscriptionResponse,
    user::{UserCreateRequest, UserCreateResponse},
};
use cloudless_core::{
    adapters::aes_gcm_encryptor::AesGcmEncryptor,
    adapters::google_drive_storage_adaptor::GoogleDriveStorageAdaptor,
    adapters::onedrive_storage_adaptor::OneDriveStorageAdaptor,
    adapters::s3_storage_adaptor::S3StorageAdaptor,
    adapters::sftp_storage_adaptor::SftpStorageAdaptor,
    applications::backup::backup_config::{
        decrypt_backup_config_with_storage, decrypt_backup_job_detail, decrypt_restore_job_detail,
        encrypt_create_config_request,
    },
    applications::backup::backup_job::start_backup,
    applications::backup::env::BackupEnv,
    applications::backup::scheduler::{self, SchedulerConfig, SchedulerEvent},
    applications::restore::env::RestoreEnv,
    applications::restore::restore_job::{start_restore, RestoreFileSelection},
    domain::{dek::Dek, derived_keys::DerivedKeys},
    model::file::ObjectKey,
    ports::api::backup_job_api_port::BackupJobApiPort,
    ports::api::policy_api_port::PolicyApiPort,
    ports::api::remote_file_version_api_port::RemoteFileVersionApiPort,
    ports::api::remote_storage_api_port::RemoteStorageApiPort,
    ports::api::restore_job_api_port::RestoreJobApiPort,
    ports::storage::StoragePort,
    ports::Encryptor,
};
use tauri::{AppHandle, Emitter, State};

// ── Mobile device ID persistence (store-based) ────────────────────

#[cfg(any(target_os = "android", target_os = "ios"))]
const DEVICE_STORE_PATH: &str = "device.json";
#[cfg(any(target_os = "android", target_os = "ios"))]
const DEVICE_STORE_KEY: &str = "physical_device_id";

/// Reads the physical_device_id from the local store, if present.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn read_device_id_from_store(app: &AppHandle) -> Option<String> {
    use tauri_plugin_store::StoreExt;
    let store = app.store(DEVICE_STORE_PATH).ok()?;
    let value = store.get(DEVICE_STORE_KEY)?;
    value
        .as_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// Persists the physical_device_id to the local store so it survives app restarts.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn save_device_id_to_store(app: &AppHandle, device_id: &str) -> TauriResult<()> {
    use tauri_plugin_store::StoreExt;
    let store = app
        .store(DEVICE_STORE_PATH)
        .map_err(|e| TauriError::from(format!("Failed to open device store: {}", e)))?;
    store.set(DEVICE_STORE_KEY.to_string(), serde_json::json!(device_id));
    store
        .save()
        .map_err(|e| TauriError::from(format!("Failed to save device store: {}", e)))?;
    tracing::info!("Saved device ID to store: {}", device_id);
    Ok(())
}

// ── Auth commands ──────────────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state, request), fields(email = %request.email))]
pub async fn create_user(
    state: State<'_, AppState>,
    request: UserCreateRequest,
) -> TauriResult<UserCreateResponse> {
    tracing::info!("Received create_user command for email: {}", request.email);
    Ok(cloudless_core::applications::user::application::create_user(&*state.env, request).await?)
}

#[tauri::command]
#[tracing::instrument(skip(state, request), fields(email = %request.email))]
pub async fn signup_and_login(
    state: State<'_, AppState>,
    request: SignupAndLoginRequest,
) -> TauriResult<SignupAndLoginResponse> {
    tracing::info!("Signup+login for email: {}", request.email);

    let create_req = UserCreateRequest {
        name: request.name,
        email: request.email.clone(),
        password: request.password.clone(),
    };
    let user_resp =
        cloudless_core::applications::user::application::create_user(&*state.env, create_req)
            .await?;

    // Only attempt login immediately if the account is already verified.
    // When email verification is required, the frontend navigates to the
    // verification screen; login happens after the code is confirmed.
    if user_resp.email_verified {
        let login_req = LoginRequest {
            email: request.email,
            password: request.password,
        };
        cloudless_core::applications::user::application::login(&*state.env, login_req).await?;
    }

    Ok(SignupAndLoginResponse {
        user_id: user_resp.id,
        email_verified: user_resp.email_verified,
    })
}

#[tauri::command]
#[tracing::instrument(skip(state, request), fields(email = %request.email))]
pub async fn login(
    state: State<'_, AppState>,
    request: LoginRequest,
) -> TauriResult<LoginResponse> {
    tracing::info!("Login attempt for email: {}", request.email);
    *state.dek_cache.write().await = None;
    Ok(cloudless_core::applications::user::application::login(&*state.env, request).await?)
}

// ── Email verification commands ───────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn verify_email(
    state: State<'_, AppState>,
    request: VerifyEmailRequest,
) -> TauriResult<VerifyEmailResponse> {
    use cloudless_core::applications::user::env::UserEnv;
    use cloudless_core::ports::api::user_api_port::UserApiPort;
    tracing::info!("Verifying email: {}", request.email);
    Ok(UserEnv::user_api(&*state.env).verify_email(request).await?)
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn resend_verification(
    state: State<'_, AppState>,
    request: ResendVerificationRequest,
) -> TauriResult<ResendVerificationResponse> {
    use cloudless_core::applications::user::env::UserEnv;
    use cloudless_core::ports::api::user_api_port::UserApiPort;
    tracing::info!("Resending verification to: {}", request.email);
    Ok(UserEnv::user_api(&*state.env)
        .resend_verification(request)
        .await?)
}

// ── DEK management commands ────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state, request))]
pub async fn setup_encryption(
    state: State<'_, AppState>,
    request: SetupEncryptionRequest,
) -> TauriResult<()> {
    tracing::info!("Setting up encryption (generating DEK)");

    let dek = Dek::generate()?;

    cloudless_core::applications::user::application::update_dek(
        &*state.env,
        dek.clone(),
        &request.password,
        DekKeyType::Password,
    )
    .await?;

    let derived_keys = DerivedKeys::derive(&dek)?;
    *state.dek_cache.write().await = Some(dek);
    *state.derived_keys_cache.write().await = Some(derived_keys);
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state, app_handle, request))]
pub async fn unlock_encryption(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    request: UnlockEncryptionRequest,
) -> TauriResult<()> {
    tracing::info!("Unlocking encryption (decrypting DEK)");

    let dek = cloudless_core::applications::user::application::unlock_dek(
        &*state.env,
        &request.password,
        DekKeyType::Password,
    )
    .await?;

    // Derive purpose-specific keys from the DEK
    let derived_keys = DerivedKeys::derive(&dek)?;
    *state.dek_cache.write().await = Some(dek.clone());
    *state.derived_keys_cache.write().await = Some(derived_keys);

    // Mark any jobs that were running when the app was last killed as interrupted
    if let Err(e) = state.env.backup_job_api().abandon_stale_jobs().await {
        tracing::warn!("Failed to abandon stale backup jobs on unlock: {}", e);
    }

    // Auto-start scheduler only if auto-backup is both locally enabled and allowed
    // by the current subscription tier. A user who enabled auto-backup while paid
    // and then downgraded must not silently keep the scheduler running.
    if let Some(settings) = load_auto_backup_settings(&app_handle) {
        if settings.enabled {
            use cloudless_core::ports::api::subscription_api_port::SubscriptionApiPort;
            let allowed = state
                .env
                .subscription_api()
                .get_subscription()
                .await
                .map(|s| s.limits.auto_backup_enabled)
                .unwrap_or(false);

            if allowed {
                if let Err(e) = start_scheduler_internal(&state, &app_handle, dek, &settings).await
                {
                    tracing::warn!("Failed to auto-start backup scheduler: {}", e);
                }
            } else {
                tracing::info!(
                    "Auto-backup not permitted by subscription tier — disabling saved setting"
                );
                let disabled = AutoBackupSettings {
                    enabled: false,
                    interval_minutes: settings.interval_minutes,
                };
                let _ = save_auto_backup_settings(&app_handle, &disabled);
            }
        }
    }

    Ok(())
}

// ── Config commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn register_local_device(
    state: State<'_, AppState>,
    request: CreateLocalDeviceRequest,
) -> TauriResult<CreateLocalDeviceResponse> {
    let response = cloudless_core::applications::config::application::register_local_device(
        &*state.env,
        request,
    )
    .await?;

    *state.device_id_cache.write().await = Some(response.id);

    Ok(response)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_local_device(
    state: State<'_, AppState>,
    physical_device_id: String,
) -> TauriResult<Option<GetLocalDeviceByPhysicalIdResponse>> {
    match cloudless_core::applications::config::application::get_local_device(
        &*state.env,
        physical_device_id,
    )
    .await
    {
        Ok(resp) => Ok(Some(resp)),
        Err(_) => Ok(None),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_or_create_device(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    display_name: String,
) -> TauriResult<GetOrCreateLocalDeviceResponse> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let physical_device_id = {
        let _ = &app;
        machine_uid::get().unwrap_or_default()
    };
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let physical_device_id =
        read_device_id_from_store(&app).unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    let request = GetOrCreateLocalDeviceRequest {
        physical_device_id: physical_device_id.clone(),
        display_name: Some(display_name),
        platform: std::env::consts::OS.to_string(),
    };

    let response = cloudless_core::applications::config::application::get_or_create_local_device(
        &*state.env,
        request,
    )
    .await?;

    *state.device_id_cache.write().await = Some(response.id);

    // On mobile, persist the physical_device_id to store for future sessions
    #[cfg(any(target_os = "android", target_os = "ios"))]
    save_device_id_to_store(&app, &response.physical_device_id)?;

    Ok(response)
}

#[tauri::command]
pub async fn register_remote_storage(
    state: State<'_, AppState>,
    request: CreateRemoteStorageRequest,
) -> TauriResult<CreateRemoteStorageResponse> {
    Ok(
        cloudless_core::applications::config::application::register_remote_storage(
            &*state.env,
            request,
        )
        .await?,
    )
}

/// Expand a leading `~` to the current user's home directory.
/// Paths that don't start with `~` are returned unchanged.
fn expand_tilde(path: &str) -> String {
    if path == "~" || path.starts_with("~/") || path.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            let rest = &path[1..];
            return format!("{}{}", home.display(), rest);
        }
    }
    path.to_string()
}

#[tauri::command]
pub async fn create_backup_config(
    state: State<'_, AppState>,
    request: CreateBackupConfigUiRequest,
) -> TauriResult<CreateBackupConfigResponse> {
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    let source_directory = expand_tilde(&request.source_directory);
    let encrypted_request = encrypt_create_config_request(
        &source_directory,
        request.storage_id,
        request.display_name,
        request.local_device_id,
        request.cleanup_type,
        &request.exclusion_config,
        &derived_keys,
    )?;

    Ok(
        cloudless_core::applications::config::application::create_backup_config(
            &*state.env,
            encrypted_request,
        )
        .await?,
    )
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_backup_configs(
    state: State<'_, AppState>,
    physical_device_id: String,
) -> TauriResult<Vec<DecryptedBackupConfigWithRemoteStorage>> {
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    let response = cloudless_core::applications::config::application::list_backup_configs(
        &*state.env,
        physical_device_id,
    )
    .await?;

    let decrypted = response
        .list
        .into_iter()
        .map(|c| decrypt_backup_config_with_storage(c, &derived_keys.metadata_key))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(decrypted)
}

#[tauri::command]
pub async fn list_all_backup_configs(
    state: State<'_, AppState>,
) -> TauriResult<Vec<DecryptedBackupConfigWithRemoteStorage>> {
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    let response =
        cloudless_core::applications::config::application::list_all_backup_configs(&*state.env)
            .await?;

    let decrypted = response
        .list
        .into_iter()
        .map(|c| decrypt_backup_config_with_storage(c, &derived_keys.metadata_key))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(decrypted)
}

#[tauri::command]
pub async fn toggle_backup_config(
    state: State<'_, AppState>,
    request: ToggleBackupConfigRequest,
) -> TauriResult<ToggleBackupConfigResponse> {
    Ok(
        cloudless_core::applications::config::application::toggle_backup_config(
            &*state.env,
            request,
        )
        .await?,
    )
}

#[tauri::command]
pub async fn rename_backup_config(
    state: State<'_, AppState>,
    request: RenameBackupConfigRequest,
) -> TauriResult<RenameBackupConfigResponse> {
    Ok(
        cloudless_core::applications::config::application::rename_backup_config(
            &*state.env,
            request,
        )
        .await?,
    )
}

#[tauri::command]
pub async fn update_cleanup_type(
    state: State<'_, AppState>,
    request: UpdateCleanupTypeRequest,
) -> TauriResult<UpdateCleanupTypeResponse> {
    Ok(
        cloudless_core::applications::config::application::update_cleanup_type(
            &*state.env,
            request,
        )
        .await?,
    )
}

#[tauri::command]
pub async fn preview_backup_exclusions(
    request: PreviewBackupExclusionsRequest,
) -> TauriResult<PreviewBackupExclusionsResponse> {
    Ok(
        cloudless_core::applications::backup::preview_exclusions::preview_backup_exclusions(
            request,
        )
        .await?,
    )
}

#[tauri::command]
pub async fn update_backup_exclusions(
    state: State<'_, AppState>,
    id: uuid::Uuid,
    exclusion_config: BackupExclusionConfig,
) -> TauriResult<UpdateExclusionConfigResponse> {
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    let encrypted_request =
        cloudless_core::applications::backup::backup_config::encrypt_update_exclusion_config_request(
            id,
            &exclusion_config,
            &derived_keys,
        )?;

    Ok(
        cloudless_core::applications::config::application::update_exclusion_config(
            &*state.env,
            encrypted_request,
        )
        .await?,
    )
}

#[tauri::command]
pub async fn list_all_devices(state: State<'_, AppState>) -> TauriResult<ListAllDevicesResponse> {
    Ok(cloudless_core::applications::config::application::list_all_devices(&*state.env).await?)
}

// ── Mobile device resolution ───────────────────────────────────────

/// Resolves the current mobile device by:
/// 1. Checking the local store for a saved physical_device_id
/// 2. If found, looking it up on the server
/// 3. If not found (or server lookup fails), querying devices by platform
/// Returns a `DeviceResolutionResult` for the UI to act on.
#[tauri::command]
pub async fn resolve_mobile_device(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> TauriResult<DeviceResolutionResult> {
    let platform = std::env::consts::OS.to_string();

    // 1. Try the local store first
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if let Some(stored_id) = read_device_id_from_store(&app) {
        tracing::debug!(physical_device_id = %stored_id, "Found device ID in store, looking up on server");
        let lookup = cloudless_core::applications::config::application::get_local_device(
            &*state.env,
            stored_id,
        )
        .await;

        if let Ok(device) = lookup {
            *state.device_id_cache.write().await = Some(device.id);
            return Ok(DeviceResolutionResult::Resolved {
                device: api_types::local_device::DeviceSummary {
                    id: device.id,
                    physical_device_id: device.physical_device_id,
                    display_name: device.display_name,
                    platform: device.platform,
                    created_at: device.created_at,
                },
            });
        }
        tracing::debug!("Stored device ID not found on server, falling through to platform query");
    }

    // Suppress unused variable warning on desktop
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = &app;

    // 2. Query server for all devices on this platform
    let response = cloudless_core::applications::config::application::list_devices_by_platform(
        &*state.env,
        ListDevicesByPlatformRequest {
            platform: platform.clone(),
        },
    )
    .await?;

    match response.devices.len() {
        0 => Ok(DeviceResolutionResult::NoneFound),
        1 => {
            let device = response
                .devices
                .into_iter()
                .next()
                .expect("checked len == 1");

            *state.device_id_cache.write().await = Some(device.id);

            // Auto-select and save to store
            #[cfg(any(target_os = "android", target_os = "ios"))]
            save_device_id_to_store(&app, &device.physical_device_id)?;

            Ok(DeviceResolutionResult::Resolved { device })
        }
        _ => Ok(DeviceResolutionResult::MultipleFound {
            devices: response.devices,
        }),
    }
}

/// Called after the user picks an existing device from the dropdown.
/// Saves the physical_device_id to the local store for future sessions.
#[tauri::command(rename_all = "snake_case")]
pub async fn confirm_mobile_device(
    app: tauri::AppHandle,
    physical_device_id: String,
) -> TauriResult<()> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    save_device_id_to_store(&app, &physical_device_id)?;

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = (app, physical_device_id);

    Ok(())
}

// ── Host info commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_hostname() -> TauriResult<String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        Ok(hostname::get()?
            .into_string()
            .map_err(|_| TauriError::from("hostname contains invalid UTF-8"))?)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok("mobile-device".to_string())
    }
}

#[tauri::command]
pub async fn get_machine_id(app: tauri::AppHandle) -> TauriResult<String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = app;
        Ok(machine_uid::get().map_err(|e| TauriError::from(e.to_string()))?)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(read_device_id_from_store(&app).unwrap_or_default())
    }
}

#[tauri::command]
pub async fn get_platform() -> TauriResult<String> {
    Ok(std::env::consts::OS.to_string())
}

#[tauri::command]
pub async fn list_remote_storages(
    state: State<'_, AppState>,
) -> TauriResult<ListRemoteStoragesResponse> {
    Ok(
        cloudless_core::applications::config::application::list_remote_storages(&*state.env)
            .await?,
    )
}

#[tauri::command]
#[tracing::instrument(skip(state, request), fields(storage_name = %request.storage_name))]
pub async fn add_remote_storage(
    state: State<'_, AppState>,
    request: AddRemoteStorageRequest,
) -> TauriResult<CreateRemoteStorageResponse> {
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let s3_creds = S3Credentials {
        access_key: request.s3_access_key,
        secret: request.s3_secret,
        region: request.s3_region,
        bucket: request.s3_bucket,
    };
    let config = RemoteStorageConfig::Aws(s3_creds);
    let config_json = serde_json::to_vec(&config)?;

    let encryptor = AesGcmEncryptor::new(&dek)?;
    let encrypted_config = encryptor.encrypt(&config_json)?;

    Ok(
        cloudless_core::applications::config::application::register_remote_storage(
            &*state.env,
            CreateRemoteStorageRequest {
                name: request.storage_name,
                storage_type: RemoteStorageType::Aws,
                config: encrypted_config.into(),
            },
        )
        .await?,
    )
}

// ── Storage setup (uses cached DEK) ───────────────────────────────
// todo: Break this into 2 separate steps, First Create a remote storage and then create backup config.
#[tauri::command]
#[tracing::instrument(skip(state, request), fields(storage_name = %request.storage_name))]
pub async fn complete_storage_setup(
    state: State<'_, AppState>,
    request: CompleteStorageSetupRequest,
) -> TauriResult<CompleteStorageSetupResponse> {
    tracing::info!("Starting storage setup: {}", request.storage_name);

    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let s3_creds = S3Credentials {
        access_key: request.s3_access_key,
        secret: request.s3_secret,
        region: request.s3_region,
        bucket: request.s3_bucket,
    };
    let config = RemoteStorageConfig::Aws(s3_creds);
    let config_json = serde_json::to_vec(&config)?;

    let encryptor = AesGcmEncryptor::new(&dek)?;
    let encrypted_config = encryptor.encrypt(&config_json)?;

    let storage_response =
        cloudless_core::applications::config::application::register_remote_storage(
            &*state.env,
            CreateRemoteStorageRequest {
                name: request.storage_name,
                storage_type: RemoteStorageType::Aws,
                config: encrypted_config.into(),
            },
        )
        .await?;

    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    let source_directory = expand_tilde(&request.source_directory);
    let encrypted_config_request = encrypt_create_config_request(
        &source_directory,
        storage_response.id,
        "Default Backup".to_string(),
        request.local_device_id,
        Default::default(),
        &BackupExclusionConfig::default(),
        &derived_keys,
    )?;

    let config_response = cloudless_core::applications::config::application::create_backup_config(
        &*state.env,
        encrypted_config_request,
    )
    .await?;

    Ok(CompleteStorageSetupResponse {
        storage_id: storage_response.id,
        config_id: config_response.id,
    })
}

// ── S3 connection test ─────────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(request), fields(bucket = %request.s3_bucket, region = %request.s3_region))]
pub async fn test_s3_connection(request: TestS3ConnectionRequest) -> TauriResult<bool> {
    tracing::info!("Testing S3 connection");
    let creds = S3Credentials {
        access_key: request.s3_access_key,
        secret: request.s3_secret,
        region: request.s3_region,
        bucket: request.s3_bucket,
    };

    let storage = S3StorageAdaptor::new(creds).await?;
    storage
        .exists(&ObjectKey::new(".mv-storage-connection-test".to_string()))
        .await?;

    Ok(true)
}

// ── Local filesystem storage ──────────────────────────────────────

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(skip(state, request), fields(storage_name = %request.storage_name, root_path = %request.root_path))]
pub async fn add_local_storage(
    state: State<'_, AppState>,
    request: AddLocalStorageRequest,
) -> TauriResult<CreateRemoteStorageResponse> {
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    // Validate the path exists and is a directory
    let path = std::path::Path::new(&request.root_path);
    if !path.exists() {
        return Err(TauriError::from(format!(
            "Directory does not exist: {}",
            request.root_path
        )));
    }
    if !path.is_dir() {
        return Err(TauriError::from(format!(
            "Path is not a directory: {}",
            request.root_path
        )));
    }

    let config = RemoteStorageConfig::LocalFilesystem(LocalFilesystemConfig {
        root_path: request.root_path,
    });
    let config_json = serde_json::to_vec(&config)?;

    let encryptor = AesGcmEncryptor::new(&dek)?;
    let encrypted_config = encryptor.encrypt(&config_json)?;

    Ok(
        cloudless_core::applications::config::application::register_remote_storage(
            &*state.env,
            CreateRemoteStorageRequest {
                name: request.storage_name,
                storage_type: RemoteStorageType::LocalFilesystem,
                config: encrypted_config.into(),
            },
        )
        .await?,
    )
}

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(skip(request), fields(root_path = %request.root_path))]
pub async fn test_local_connection(request: TestLocalConnectionRequest) -> TauriResult<bool> {
    tracing::info!("Testing local filesystem connection");

    let path = std::path::Path::new(&request.root_path);
    if !path.exists() {
        return Err(TauriError::from(format!(
            "Directory does not exist: {}",
            request.root_path
        )));
    }
    if !path.is_dir() {
        return Err(TauriError::from(format!(
            "Path is not a directory: {}",
            request.root_path
        )));
    }

    // Verify we can write to the directory by creating and removing a temp file
    let test_file = path.join(".cloudless-connection-test");
    tokio::fs::write(&test_file, b"test").await.map_err(|e| {
        TauriError::from(format!(
            "Directory is not writable: {} ({})",
            request.root_path, e
        ))
    })?;
    tokio::fs::remove_file(&test_file).await.ok();

    Ok(true)
}

// ── SFTP storage ─────────────────────────────────────────────────

fn build_sftp_auth_config(
    auth_method: &str,
    password: Option<String>,
    private_key_pem: Option<String>,
    private_key_passphrase: Option<String>,
) -> TauriResult<SftpAuthConfig> {
    match auth_method {
        "password" => {
            let password = password.filter(|p| !p.is_empty()).ok_or_else(|| {
                TauriError::from("Password is required for SFTP password authentication.")
            })?;
            Ok(SftpAuthConfig::Password { password })
        }
        "private_key" | "private-key" | "key" => {
            let private_key_pem = private_key_pem
                .filter(|k| !k.trim().is_empty())
                .ok_or_else(|| {
                    TauriError::from(
                        "SSH private key is required for SFTP private key authentication.",
                    )
                })?;
            Ok(SftpAuthConfig::PrivateKey {
                private_key_pem,
                passphrase: private_key_passphrase.filter(|p| !p.is_empty()),
            })
        }
        _ => Err(TauriError::from(
            "Unsupported SFTP authentication method. Use password or private_key.",
        )),
    }
}

fn build_sftp_credentials(
    host: String,
    port: u16,
    username: String,
    auth_method: String,
    password: Option<String>,
    private_key_pem: Option<String>,
    private_key_passphrase: Option<String>,
    remote_root_path: String,
    known_host_key: Option<String>,
) -> TauriResult<SftpCredentials> {
    Ok(SftpCredentials {
        host,
        port,
        username,
        auth: build_sftp_auth_config(
            &auth_method,
            password,
            private_key_pem,
            private_key_passphrase,
        )?,
        remote_root_path,
        host_key_policy: SftpHostKeyPolicy::Strict,
        known_host_key,
    })
}

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(skip(request), fields(host = %request.host, port = request.port, username = %request.username))]
pub async fn test_sftp_connection(
    request: TestSftpConnectionRequest,
) -> TauriResult<TestSftpConnectionResponse> {
    tracing::info!("Testing SFTP connection");

    let creds = build_sftp_credentials(
        request.host,
        request.port,
        request.username,
        request.auth_method,
        request.password,
        request.private_key_pem,
        request.private_key_passphrase,
        request.remote_root_path,
        request.known_host_key,
    )?;
    let result = SftpStorageAdaptor::test_connection(creds).await?;

    Ok(TestSftpConnectionResponse {
        host_key_fingerprint: result.host_key_fingerprint,
        root_exists: result.root_exists,
        root_writable: result.root_writable,
    })
}

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(skip(state, request), fields(storage_name = %request.storage_name, host = %request.host, port = request.port, username = %request.username))]
pub async fn add_sftp_storage(
    state: State<'_, AppState>,
    request: AddSftpStorageRequest,
) -> TauriResult<CreateRemoteStorageResponse> {
    let known_host_key = request
        .known_host_key
        .clone()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| {
            TauriError::from("Confirm the server fingerprint before saving this SFTP storage.")
        })?;

    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let config = RemoteStorageConfig::Sftp(build_sftp_credentials(
        request.host,
        request.port,
        request.username,
        request.auth_method,
        request.password,
        request.private_key_pem,
        request.private_key_passphrase,
        request.remote_root_path,
        Some(known_host_key),
    )?);
    let config_json = serde_json::to_vec(&config)?;

    let encryptor = AesGcmEncryptor::new(&dek)?;
    let encrypted_config = encryptor.encrypt(&config_json)?;

    Ok(
        cloudless_core::applications::config::application::register_remote_storage(
            &*state.env,
            CreateRemoteStorageRequest {
                name: request.storage_name,
                storage_type: RemoteStorageType::Sftp,
                config: encrypted_config.into(),
            },
        )
        .await?,
    )
}

// ── Google Drive storage ───────────────────────────────────────────

/// Runs OAuth2 PKCE flow, encrypts credentials, and registers Google Drive storage.
#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(skip(state, app), fields(storage_name = %storage_name))]
pub async fn add_google_drive_storage(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    storage_name: String,
) -> TauriResult<CreateRemoteStorageResponse> {
    tracing::info!("Starting Google Drive storage setup: {}", storage_name);

    // No client secret needed — Google Desktop app type uses PKCE only.
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .or_else(|_| {
            option_env!("GOOGLE_CLIENT_ID")
                .map(String::from)
                .ok_or(std::env::VarError::NotPresent)
        })
        .map_err(|_| {
            TauriError::from("GOOGLE_CLIENT_ID not configured. Set it as an environment variable or bake it in at build time.")
        })?;

    // 1. Run OAuth flow (opens browser, waits for callback)
    let creds =
        crate::google_auth::google_drive_oauth_flow(&app, &client_id).await?;

    // 2. Get DEK for encryption
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    // 3. Encrypt credentials (same pattern as S3)
    let config = RemoteStorageConfig::GoogleDrive(creds);
    let config_json = serde_json::to_vec(&config)?;
    let encryptor = AesGcmEncryptor::new(&dek)?;
    let encrypted_config = encryptor.encrypt(&config_json)?;

    // 4. Register via API
    Ok(
        cloudless_core::applications::config::application::register_remote_storage(
            &*state.env,
            CreateRemoteStorageRequest {
                name: storage_name,
                storage_type: RemoteStorageType::GoogleDrive,
                config: encrypted_config.into(),
            },
        )
        .await?,
    )
}

/// Reauthorizes a Google Drive remote storage whose token has expired.
///
/// 1. Fetches the existing storage config and decrypts it client-side.
/// 2. Runs a fresh OAuth2 PKCE flow (opens browser).
/// 3. Fetches Google userinfo to get the new account's identity.
/// 4. Compares `google_user_id` with the stored value (identity verification).
/// 5. On match: updates credentials (preserving `root_folder_id`), encrypts, and
///    calls server reauth endpoint to reset status to Active.
/// 6. On mismatch: returns an error — never sends to server.
#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(skip(state, app), fields(storage_id = %storage_id))]
pub async fn reauth_google_drive_storage(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    storage_id: uuid::Uuid,
) -> TauriResult<ReauthRemoteStorageResponse> {
    tracing::info!("Starting Google Drive reauth for storage: {}", storage_id);

    // 1. Fetch existing storage and decrypt config
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let storage_response = RestoreEnv::remote_storage_api(&*state.env)
        .get_by_id(GetRemoteStorageRequest { id: storage_id })
        .await
        .map_err(|e| TauriError::from(format!("Failed to fetch storage: {}", e)))?;

    let encrypted_data: cloudless_core::model::base::EncryptedData = storage_response
        .storage
        .config
        .try_into()
        .map_err(|e| TauriError::from(format!("Failed to parse encrypted config: {}", e)))?;
    let encryptor = AesGcmEncryptor::new(&dek)?;
    let decrypted_config = encryptor.decrypt(&encrypted_data)?;
    let existing_config: RemoteStorageConfig = serde_json::from_slice(&decrypted_config)?;

    let existing_gdrive = match existing_config {
        RemoteStorageConfig::GoogleDrive(creds) => creds,
        _ => {
            return Err(TauriError::from(
                "Storage is not Google Drive — cannot reauth",
            ))
        }
    };

    // 2. Run OAuth flow (PKCE only — no client secret required for Desktop app type)
    let client_id = existing_gdrive.client_id.clone();
    let new_creds =
        crate::google_auth::google_drive_oauth_flow(&app, &client_id).await?;

    // 3. Client-side identity verification
    // Compare the new google_user_id with the stored one.
    // For storages created before identity capture, skip verification (first reauth
    // establishes the identity).
    if let Some(ref stored_user_id) = existing_gdrive.google_user_id {
        if let Some(ref new_user_id) = new_creds.google_user_id {
            if stored_user_id != new_user_id {
                let stored_email = existing_gdrive
                    .google_user_email
                    .as_deref()
                    .unwrap_or("unknown");
                let new_email = new_creds.google_user_email.as_deref().unwrap_or("unknown");
                return Err(TauriError::from(format!(
                    "Different Google account detected. Expected account: {} but got: {}. \
                     Please sign in with the same Google account to prevent data loss.",
                    stored_email, new_email
                )));
            }
        }
    }

    // 4. Build updated credentials — preserve root_folder_id from original config
    let updated_creds = GoogleDriveCredentials {
        client_id,
        client_secret: None,
        refresh_token: new_creds.refresh_token,
        root_folder_id: existing_gdrive.root_folder_id,
        google_user_id: new_creds.google_user_id.or(existing_gdrive.google_user_id),
        google_user_email: new_creds
            .google_user_email
            .or(existing_gdrive.google_user_email),
    };

    // 5. Encrypt and send to server
    let updated_config = RemoteStorageConfig::GoogleDrive(updated_creds);
    let config_json = serde_json::to_vec(&updated_config)?;
    let encrypted_config = encryptor.encrypt(&config_json)?;

    let response = RestoreEnv::remote_storage_api(&*state.env)
        .reauth(ReauthRemoteStorageRequest {
            id: storage_id,
            config: encrypted_config.into(),
        })
        .await
        .map_err(|e| TauriError::from(format!("Failed to reauth storage: {}", e)))?;

    tracing::info!("Google Drive storage reauth successful");
    Ok(response)
}

// ── Microsoft OneDrive storage ─────────────────────────────────────

/// Runs Microsoft OAuth2 PKCE, encrypts credentials, and registers OneDrive storage.
#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(skip(state, app), fields(storage_name = %storage_name))]
pub async fn add_onedrive_storage(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    storage_name: String,
) -> TauriResult<CreateRemoteStorageResponse> {
    tracing::info!(
        "Starting Microsoft OneDrive storage setup: {}",
        storage_name
    );

    let client_id = std::env::var("ONEDRIVE_CLIENT_ID")
        .or_else(|_| {
            option_env!("ONEDRIVE_CLIENT_ID")
                .map(String::from)
                .ok_or(std::env::VarError::NotPresent)
        })
        .map_err(|_| {
            TauriError::from(
                "ONEDRIVE_CLIENT_ID not configured. Set it as an environment variable.",
            )
        })?;
    let tenant = std::env::var("ONEDRIVE_TENANT")
        .or_else(|_| {
            option_env!("ONEDRIVE_TENANT")
                .map(String::from)
                .ok_or(std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "common".to_string());
    let redirect_uri = std::env::var("ONEDRIVE_REDIRECT_URI")
        .or_else(|_| {
            option_env!("ONEDRIVE_REDIRECT_URI")
                .map(String::from)
                .ok_or(std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "http://localhost:3000/api/storage/onedrive/callback".to_string());

    let creds =
        crate::onedrive_auth::onedrive_oauth_flow(&app, &client_id, &tenant, &redirect_uri).await?;

    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let config = RemoteStorageConfig::OneDrive(creds);
    let config_json = serde_json::to_vec(&config)?;
    let encryptor = AesGcmEncryptor::new(&dek)?;
    let encrypted_config = encryptor.encrypt(&config_json)?;

    Ok(
        cloudless_core::applications::config::application::register_remote_storage(
            &*state.env,
            CreateRemoteStorageRequest {
                name: storage_name,
                storage_type: RemoteStorageType::OneDrive,
                config: encrypted_config.into(),
            },
        )
        .await?,
    )
}

/// Reauthorizes OneDrive storage and rejects login with a different account.
#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(skip(state, app), fields(storage_id = %storage_id))]
pub async fn reauth_onedrive_storage(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    storage_id: uuid::Uuid,
) -> TauriResult<ReauthRemoteStorageResponse> {
    tracing::info!(
        "Starting Microsoft OneDrive reauth for storage: {}",
        storage_id
    );

    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let storage_response = RestoreEnv::remote_storage_api(&*state.env)
        .get_by_id(GetRemoteStorageRequest { id: storage_id })
        .await
        .map_err(|e| TauriError::from(format!("Failed to fetch storage: {}", e)))?;

    let encrypted_data: cloudless_core::model::base::EncryptedData = storage_response
        .storage
        .config
        .try_into()
        .map_err(|e| TauriError::from(format!("Failed to parse encrypted config: {}", e)))?;
    let encryptor = AesGcmEncryptor::new(&dek)?;
    let decrypted_config = encryptor.decrypt(&encrypted_data)?;
    let existing_config: RemoteStorageConfig = serde_json::from_slice(&decrypted_config)?;

    let existing_onedrive = match existing_config {
        RemoteStorageConfig::OneDrive(creds) => creds,
        _ => {
            return Err(TauriError::from(
                "Storage is not Microsoft OneDrive — cannot reauth",
            ))
        }
    };

    let redirect_uri = std::env::var("ONEDRIVE_REDIRECT_URI")
        .or_else(|_| {
            option_env!("ONEDRIVE_REDIRECT_URI")
                .map(String::from)
                .ok_or(std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "http://localhost:3000/api/storage/onedrive/callback".to_string());
    let new_creds = crate::onedrive_auth::onedrive_oauth_flow(
        &app,
        &existing_onedrive.client_id,
        &existing_onedrive.tenant,
        &redirect_uri,
    )
    .await?;

    if let Some(ref stored_user_id) = existing_onedrive.microsoft_user_id {
        if let Some(ref new_user_id) = new_creds.microsoft_user_id {
            if stored_user_id != new_user_id {
                let stored_email = existing_onedrive
                    .microsoft_user_email
                    .as_deref()
                    .unwrap_or("unknown");
                let new_email = new_creds
                    .microsoft_user_email
                    .as_deref()
                    .unwrap_or("unknown");
                return Err(TauriError::from(format!(
                    "Different Microsoft account detected. Expected account: {} but got: {}. \
                     Please sign in with the same OneDrive account to prevent restore failures.",
                    stored_email, new_email
                )));
            }
        }
    }

    let updated_creds = OneDriveCredentials {
        client_id: existing_onedrive.client_id,
        tenant: existing_onedrive.tenant,
        refresh_token: new_creds.refresh_token,
        microsoft_user_id: new_creds
            .microsoft_user_id
            .or(existing_onedrive.microsoft_user_id),
        microsoft_user_email: new_creds
            .microsoft_user_email
            .or(existing_onedrive.microsoft_user_email),
    };

    let updated_config = RemoteStorageConfig::OneDrive(updated_creds);
    let config_json = serde_json::to_vec(&updated_config)?;
    let encrypted_config = encryptor.encrypt(&config_json)?;

    let response = RestoreEnv::remote_storage_api(&*state.env)
        .reauth(ReauthRemoteStorageRequest {
            id: storage_id,
            config: encrypted_config.into(),
        })
        .await
        .map_err(|e| TauriError::from(format!("Failed to reauth storage: {}", e)))?;

    tracing::info!("Microsoft OneDrive storage reauth successful");
    Ok(response)
}

// ── File browsing ──────────────────────────────────────────────────

#[tauri::command(rename_all = "snake_case")]
pub async fn list_backed_up_files(
    state: State<'_, AppState>,
    backup_config_id: uuid::Uuid,
    cursor: Option<Vec<u8>>,
    limit: Option<i64>,
) -> TauriResult<ListBackedUpFilesResponse> {
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or_else(|| TauriError::from("Encryption not unlocked"))?;

    let page_size = limit.unwrap_or(api_types::remote_file_version::DEFAULT_PAGE_SIZE);

    Ok(
        cloudless_core::applications::backup::browse::list_backed_up_files(
            &*state.env,
            backup_config_id,
            cursor,
            page_size,
            &derived_keys,
        )
        .await?,
    )
}

// ── Bin operations ─────────────────────────────────────────────────

#[tauri::command(rename_all = "snake_case")]
pub async fn move_version_to_bin(
    state: State<'_, AppState>,
    version_id: uuid::Uuid,
) -> TauriResult<()> {
    BackupEnv::remote_file_version_api(&*state.env)
        .move_to_bin(MoveVersionToBinRequest { version_id })
        .await?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn move_all_versions_to_bin(
    state: State<'_, AppState>,
    backup_config_id: uuid::Uuid,
    name_blind_index: Vec<u8>,
) -> TauriResult<()> {
    BackupEnv::remote_file_version_api(&*state.env)
        .move_all_to_bin(MoveAllVersionsToBinRequest {
            backup_config_id,
            name_blind_index,
        })
        .await?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn restore_version_from_bin(
    state: State<'_, AppState>,
    version_id: uuid::Uuid,
) -> TauriResult<()> {
    BackupEnv::remote_file_version_api(&*state.env)
        .restore_from_bin(RestoreVersionFromBinRequest { version_id })
        .await?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_bin_versions(
    state: State<'_, AppState>,
    backup_config_id: uuid::Uuid,
) -> TauriResult<ListBinVersionsResponse> {
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or_else(|| TauriError::from("Encryption not unlocked"))?;

    Ok(
        cloudless_core::applications::backup::browse::list_bin_versions(
            &*state.env,
            backup_config_id,
            &derived_keys,
        )
        .await?,
    )
}

// ── Recovery ───────────────────────────────────────────────────────

#[tauri::command(rename_all = "snake_case")]
pub async fn rebuild_index(state: State<'_, AppState>, config_id: uuid::Uuid) -> TauriResult<u64> {
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or_else(|| TauriError::from("Encryption not unlocked"))?;

    let result = cloudless_core::applications::backup::recovery::rebuild_local_index(
        &*state.env,
        config_id,
        &derived_keys,
    )
    .await?;

    Ok(result.files_recovered as u64)
}

// ── Backup (uses cached DEK) ───────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state, app_handle, request), fields(config_id = %request.config_id))]
pub async fn start_backup_command(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    request: StartBackupRequest,
) -> TauriResult<()> {
    tracing::info!("Starting backup for config: {}", request.config_id);

    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Derived keys not available. Please unlock encryption first.",
        ))?;

    let device_id = state.device_id_cache.read().await.ok_or(TauriError::from(
        "Device not registered. Please complete setup first.",
    ))?;

    let env_clone = (*state.env).clone();
    let mut rx = start_backup(env_clone, dek, derived_keys, device_id, request.config_id);

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Err(e) = app_handle.emit("backup-progress", &event) {
                tracing::error!("Failed to emit backup event: {}", e);
                break;
            }
        }
    });

    Ok(())
}

// ── Backup job queries ─────────────────────────────────────────────

#[tauri::command]
pub async fn list_backup_jobs(
    state: State<'_, AppState>,
    request: ListBackupJobsRequest,
) -> TauriResult<ListBackupJobsResponse> {
    Ok(state.env.backup_job_api().list_jobs(request).await?)
}

#[tauri::command]
pub async fn get_backup_job_detail(
    state: State<'_, AppState>,
    request: GetBackupJobDetailRequest,
) -> TauriResult<DecryptedBackupJobDetailResponse> {
    let encrypted_response = state.env.backup_job_api().get_job_detail(request).await?;
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;
    Ok(decrypt_backup_job_detail(
        encrypted_response,
        &derived_keys.metadata_key,
    )?)
}

#[tauri::command]
pub async fn get_latest_backup_job(
    state: State<'_, AppState>,
) -> TauriResult<GetLatestBackupJobResponse> {
    Ok(state.env.backup_job_api().get_latest_job().await?)
}

#[tauri::command]
pub async fn abandon_stale_backup_jobs(
    state: State<'_, AppState>,
) -> TauriResult<AbandonStaleJobsResponse> {
    let response = state.env.backup_job_api().abandon_stale_jobs().await?;
    tracing::info!("Abandoned {} stale backup job(s)", response.count);
    Ok(response)
}

#[tauri::command]
pub async fn get_resumable_job_for_config(
    state: State<'_, AppState>,
    request: GetResumableBackupJobRequest,
) -> TauriResult<GetResumableBackupJobResponse> {
    Ok(state
        .env
        .backup_job_api()
        .get_resumable_job(request)
        .await?)
}

// ── Dashboard stats ────────────────────────────────────────────────

#[tauri::command]
pub async fn get_dashboard_stats(
    state: State<'_, AppState>,
) -> TauriResult<GetDashboardStatsResponse> {
    Ok(
        cloudless_core::applications::dashboard::application::get_dashboard_stats(&*state.env)
            .await?,
    )
}

// ── Subscription ───────────────────────────────────────────────────

#[tauri::command]
pub async fn get_subscription(state: State<'_, AppState>) -> TauriResult<SubscriptionResponse> {
    use cloudless_core::ports::api::subscription_api_port::SubscriptionApiPort;
    Ok(state.env.subscription_api().get_subscription().await?)
}

// ── Restore (uses cached DEK) ──────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state, app_handle, request), fields(config_id = %request.config_id))]
pub async fn start_restore_command(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    request: StartRestoreRequest,
) -> TauriResult<()> {
    tracing::info!("Starting restore for config: {}", request.config_id);

    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let file_selections: Vec<RestoreFileSelection> = request
        .file_selections
        .into_iter()
        .map(|s| RestoreFileSelection {
            version_id: s.version_id,
            file_path: s.file_path,
            size: s.size,
            version: s.version,
        })
        .collect();

    let env_clone = (*state.env).clone();
    let mut rx = start_restore(
        env_clone,
        dek,
        request.config_id,
        file_selections,
        request.destination,
        request.overwrite_behavior,
    );

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match &event {
                cloudless_core::applications::restore::restore_job::RestoreResultType::Failed {
                    reason,
                    file_path,
                } => {
                    tracing::error!(
                        ?file_path,
                        reason,
                        "Restore failed"
                    );
                }
                cloudless_core::applications::restore::restore_job::RestoreResultType::Completed {
                    total_files,
                    total_bytes,
                } => {
                    tracing::info!(total_files, total_bytes, "Restore completed");
                }
                _ => {}
            }
            if let Err(e) = app_handle.emit("restore-progress", &event) {
                tracing::error!("Failed to emit restore event: {}", e);
                break;
            }
        }
    });

    Ok(())
}

// ── Restore job queries ────────────────────────────────────────────

#[tauri::command]
pub async fn list_restore_jobs(
    state: State<'_, AppState>,
    request: ListRestoreJobsRequest,
) -> TauriResult<ListRestoreJobsResponse> {
    Ok(state.env.restore_job_api().list_jobs(request).await?)
}

#[tauri::command]
pub async fn get_restore_job_detail(
    state: State<'_, AppState>,
    request: GetRestoreJobDetailRequest,
) -> TauriResult<DecryptedRestoreJobDetailResponse> {
    let encrypted_response = state.env.restore_job_api().get_job_detail(request).await?;
    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;
    Ok(decrypt_restore_job_detail(
        encrypted_response,
        &derived_keys.metadata_key,
    )?)
}

#[tauri::command]
pub async fn get_latest_restore_job(
    state: State<'_, AppState>,
) -> TauriResult<GetLatestRestoreJobResponse> {
    Ok(state.env.restore_job_api().get_latest_job().await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn pick_directory(app: tauri::AppHandle) -> TauriResult<Option<String>> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_dialog::DialogExt;
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog().file().pick_folder(move |folder| {
            let _ = tx.send(folder.map(|f| f.to_string()));
        });
        let result = rx.await.map_err(|_| "Dialog cancelled")?;
        Ok(result)
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        use tauri::Manager;
        let picker = app.state::<tauri_plugin_folder_picker::FolderPicker<tauri::Wry>>();
        let picker = picker.inner().clone();
        let result = tokio::task::spawn_blocking(move || picker.pick_folder())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(result)
    }
}

/// Resolve a well-known directory label ("Home", "Desktop", "Documents", "Pictures",
/// "Downloads") to its absolute path without opening any OS file picker dialog.
#[tauri::command(rename_all = "snake_case")]
pub async fn resolve_standard_dir(app: tauri::AppHandle, dir: String) -> TauriResult<String> {
    use tauri::Manager;
    let path_resolver = app.path();
    let path = match dir.as_str() {
        "Home" => path_resolver.home_dir(),
        // desktop_dir() doesn't exist on Android's PathResolver — guard it out.
        // On iOS it compiles but returns Err at runtime, which the UI handles gracefully.
        #[cfg(not(target_os = "android"))]
        "Desktop" => path_resolver.desktop_dir(),
        #[cfg(target_os = "android")]
        "Desktop" => {
            return Err(TauriError::from(
                "Desktop directory is not available on Android".to_string(),
            ))
        }
        "Documents" => path_resolver.document_dir(),
        "Pictures" => path_resolver.picture_dir(),
        "Downloads" => path_resolver.download_dir(),
        other => return Err(format!("Unknown directory: {other}").into()),
    }
    .map_err(|e| TauriError::from(e.to_string()))?;
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| TauriError::from("Path contains invalid UTF-8".to_string()))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_path_in_explorer(app: tauri::AppHandle, path: String) -> TauriResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = std::path::Path::new(&path);
    let dir = if p.is_file() {
        p.parent().unwrap_or(p).to_string_lossy().to_string()
    } else {
        path
    };
    app.opener()
        .reveal_item_in_dir(&dir)
        .map_err(|e| TauriError::from(format!("Cannot open folder: {}", e)))?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_file(app: tauri::AppHandle, path: String) -> TauriResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| TauriError::from(format!("Cannot open file: {}", e)))?;
    Ok(())
}

/// Opens a restored file from the actual restore destination recorded in the
/// restore report, resolving configured-storage object keys to provider URLs.
#[tauri::command(rename_all = "snake_case")]
pub async fn open_restored_file(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    destination_type: String,
    destination_storage_id: Option<uuid::Uuid>,
    destination_path: String,
) -> TauriResult<()> {
    use tauri_plugin_opener::OpenerExt;

    if destination_type != "configured_storage" {
        app.opener()
            .open_path(&destination_path, None::<&str>)
            .map_err(|e| TauriError::from(format!("Cannot open restored file: {}", e)))?;
        return Ok(());
    }

    let storage_id = destination_storage_id.ok_or_else(|| {
        TauriError::from("Restore report is missing the destination storage id".to_string())
    })?;
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;
    let storage_response = RestoreEnv::remote_storage_api(&*state.env)
        .get_by_id(GetRemoteStorageRequest { id: storage_id })
        .await
        .map_err(|e| TauriError::from(format!("Failed to fetch storage: {}", e)))?;
    let encrypted_data: cloudless_core::model::base::EncryptedData = storage_response
        .storage
        .config
        .try_into()
        .map_err(|e| TauriError::from(format!("Failed to parse encrypted config: {}", e)))?;
    let encryptor = AesGcmEncryptor::new(&dek)?;
    let decrypted_config = encryptor.decrypt(&encrypted_data)?;
    let storage_config: RemoteStorageConfig = serde_json::from_slice(&decrypted_config)?;
    let key = ObjectKey::new(destination_path.clone());

    let open_url = match storage_config {
        RemoteStorageConfig::GoogleDrive(creds) => {
            let storage =
                GoogleDriveStorageAdaptor::new_for_restored_files(creds, storage_id).await?;
            Some(storage.web_url_for_restored_file(&key).await?)
        }
        RemoteStorageConfig::OneDrive(creds) => {
            let storage = OneDriveStorageAdaptor::new_for_restored_files(creds, storage_id).await?;
            Some(storage.web_url_for_restored_file(&key).await?)
        }
        RemoteStorageConfig::LocalFilesystem(config) => {
            let local_path = std::path::Path::new(&config.root_path).join(destination_path);
            app.opener()
                .open_path(local_path.to_string_lossy().as_ref(), None::<&str>)
                .map_err(|e| TauriError::from(format!("Cannot open restored file: {}", e)))?;
            None
        }
        RemoteStorageConfig::Aws(_) | RemoteStorageConfig::Sftp(_) => {
            return Err(TauriError::from(
                "Opening restored files is not supported for this storage provider yet."
                    .to_string(),
            ));
        }
    };

    if let Some(url) = open_url {
        app.opener()
            .open_url(&url, None::<&str>)
            .map_err(|e| TauriError::from(format!("Cannot open restored file: {}", e)))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_billing_portal(state: State<'_, AppState>, app: AppHandle) -> TauriResult<()> {
    use cloudless_core::ports::api::subscription_api_port::SubscriptionApiPort;
    use tauri_plugin_opener::OpenerExt;
    let portal_url = state.env.subscription_api().create_portal_session().await?;
    app.opener()
        .open_url(&portal_url, None::<&str>)
        .map_err(|e| TauriError::from(format!("Cannot open billing portal: {}", e)))?;
    Ok(())
}

#[tauri::command]
pub async fn open_checkout(
    state: State<'_, AppState>,
    app: AppHandle,
    tier: api_types::subscription::SubscriptionTier,
) -> TauriResult<String> {
    use cloudless_core::ports::api::subscription_api_port::SubscriptionApiPort;
    use tauri_plugin_opener::OpenerExt;
    let (checkout_url, checkout_id) = state
        .env
        .subscription_api()
        .create_checkout_session(tier)
        .await?;
    app.opener()
        .open_url(&checkout_url, None::<&str>)
        .map_err(|e| TauriError::from(format!("Cannot open checkout: {}", e)))?;
    Ok(checkout_id.to_string())
}

#[tauri::command]
pub async fn get_checkout_status(
    state: State<'_, AppState>,
    checkout_id: String,
) -> TauriResult<String> {
    use cloudless_core::ports::api::subscription_api_port::SubscriptionApiPort;
    let id = uuid::Uuid::parse_str(&checkout_id)
        .map_err(|e| TauriError::from(format!("Invalid checkout ID: {}", e)))?;
    Ok(state.env.subscription_api().get_checkout_status(id).await?)
}

// ── Auto-backup scheduler ─────────────────────────────────────────

const AUTO_BACKUP_STORE: &str = "auto-backup.json";
const AUTO_BACKUP_KEY: &str = "settings";

fn load_auto_backup_settings(app: &AppHandle) -> Option<AutoBackupSettings> {
    use tauri_plugin_store::StoreExt;
    let store = app.store(AUTO_BACKUP_STORE).ok()?;
    let val = store.get(AUTO_BACKUP_KEY)?;
    serde_json::from_value(val).ok()
}

fn save_auto_backup_settings(app: &AppHandle, settings: &AutoBackupSettings) -> TauriResult<()> {
    use tauri_plugin_store::StoreExt;
    let store = app
        .store(AUTO_BACKUP_STORE)
        .map_err(|e| TauriError::from(format!("Failed to open auto-backup store: {}", e)))?;
    store.set(
        AUTO_BACKUP_KEY.to_string(),
        serde_json::to_value(settings)
            .map_err(|e| TauriError::from(format!("Failed to serialize settings: {}", e)))?,
    );
    store
        .save()
        .map_err(|e| TauriError::from(format!("Failed to save auto-backup store: {}", e)))?;
    Ok(())
}

/// Resolves the physical device ID for the current platform.
async fn resolve_physical_device_id(app: &AppHandle) -> TauriResult<String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = app;
        machine_uid::get().map_err(|e| TauriError::from(e.to_string()))
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        read_device_id_from_store(app).ok_or_else(|| {
            TauriError::from("Device ID not found. Please register your device first.")
        })
    }
}

/// Internal helper to start the scheduler. Used by both `enable_auto_backup`
/// and the auto-start logic in `unlock_encryption`.
pub(crate) async fn start_scheduler_internal(
    state: &AppState,
    app_handle: &AppHandle,
    dek: Dek,
    settings: &AutoBackupSettings,
) -> TauriResult<()> {
    // Stop any existing scheduler first
    stop_scheduler_internal(state).await;

    let physical_device_id = resolve_physical_device_id(app_handle).await?;

    let interval = std::time::Duration::from_secs(settings.interval_minutes * 60);
    let config = SchedulerConfig {
        interval,
        enabled: true,
    };

    let (config_tx, config_rx) = tokio::sync::watch::channel(config);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<SchedulerEvent>(32);
    let cancel = tokio_util::sync::CancellationToken::new();

    let env_clone = (*state.env).clone();
    let cancel_clone = cancel.clone();

    let derived_keys = state
        .derived_keys_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Derived keys not available for scheduler.",
        ))?;

    let device_id = state
        .device_id_cache
        .read()
        .await
        .ok_or(TauriError::from("Device not registered for scheduler."))?;

    let join_handle = tokio::spawn(async move {
        scheduler::run_scheduler_loop(
            env_clone,
            dek,
            derived_keys,
            device_id,
            physical_device_id,
            config_rx,
            event_tx,
            cancel_clone,
        )
        .await;
    });

    // Forward scheduler events to the frontend
    let app_clone = app_handle.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Err(e) = app_clone.emit("scheduler-event", &event) {
                tracing::error!("Failed to emit scheduler event: {}", e);
                break;
            }
        }
    });

    *state.scheduler_handle.write().await = Some(SchedulerHandle {
        config_tx,
        cancel,
        join_handle,
    });

    tracing::info!(
        "Auto-backup scheduler started (interval: {} min)",
        settings.interval_minutes
    );
    Ok(())
}

async fn stop_scheduler_internal(state: &AppState) {
    let handle = state.scheduler_handle.write().await.take();
    if let Some(h) = handle {
        h.cancel.cancel();
        let _ = h.join_handle.await;
        tracing::info!("Auto-backup scheduler stopped");
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn enable_auto_backup(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    interval_minutes: u64,
) -> TauriResult<()> {
    use cloudless_core::ports::api::subscription_api_port::SubscriptionApiPort;
    let sub = state.env.subscription_api().get_subscription().await?;
    if !sub.limits.auto_backup_enabled {
        return Err(TauriError::from(
            "Scheduled backups require a paid subscription. Upgrade to Starter or higher.",
        ));
    }

    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from(
            "Encryption not unlocked. Please enter your encryption password first.",
        ))?;

    let settings = AutoBackupSettings {
        enabled: true,
        interval_minutes,
    };

    save_auto_backup_settings(&app_handle, &settings)?;
    start_scheduler_internal(&state, &app_handle, dek, &settings).await?;
    Ok(())
}

#[tauri::command]
pub async fn disable_auto_backup(
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> TauriResult<()> {
    stop_scheduler_internal(&state).await;

    let settings = AutoBackupSettings {
        enabled: false,
        interval_minutes: 60, // preserve a default
    };
    save_auto_backup_settings(&app_handle, &settings)?;
    Ok(())
}

#[tauri::command]
pub async fn get_auto_backup_settings(app_handle: AppHandle) -> TauriResult<AutoBackupSettings> {
    Ok(
        load_auto_backup_settings(&app_handle).unwrap_or(AutoBackupSettings {
            enabled: false,
            interval_minutes: 60,
        }),
    )
}

// ── Policy acceptance ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_checkout_policy(
    state: State<'_, AppState>,
) -> TauriResult<GetCheckoutPolicyResponse> {
    let response = state.env.policy_api().get_checkout_policy().await?;
    Ok(response)
}

#[tauri::command]
pub async fn get_pending_policies(
    state: State<'_, AppState>,
) -> TauriResult<GetPendingPoliciesResponse> {
    let response = state.env.policy_api().get_pending_policies().await?;
    Ok(response)
}

#[tauri::command]
pub async fn accept_policy(
    state: State<'_, AppState>,
    request: AcceptPolicyRequest,
) -> TauriResult<AcceptPolicyResponse> {
    let response = state.env.policy_api().accept_policy(request).await?;
    Ok(response)
}

#[tauri::command]
pub async fn skip_policy(
    state: State<'_, AppState>,
    request: SkipPolicyRequest,
) -> TauriResult<SkipPolicyResponse> {
    let response = state.env.policy_api().skip_policy(request).await?;
    Ok(response)
}

// ── Recovery key commands ─────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn export_recovery_key(
    state: State<'_, AppState>,
) -> TauriResult<ExportRecoveryKeyResponse> {
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    let display_string =
        cloudless_core::applications::user::application::generate_recovery_key(&*state.env, &dek)
            .await?;

    tracing::info!("Recovery key exported");
    Ok(ExportRecoveryKeyResponse {
        recovery_key: display_string,
    })
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn rotate_recovery_key(
    state: State<'_, AppState>,
) -> TauriResult<ExportRecoveryKeyResponse> {
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    let display_string =
        cloudless_core::applications::user::application::generate_recovery_key(&*state.env, &dek)
            .await?;

    tracing::info!("Recovery key rotated");
    Ok(ExportRecoveryKeyResponse {
        recovery_key: display_string,
    })
}

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn has_recovery_key(state: State<'_, AppState>) -> TauriResult<bool> {
    use cloudless_core::ports::api::encrypted_dek_api_port::EncryptedDekApiPort;

    let result = state
        .env
        .encrypted_dek_api()
        .get(api_types::encrypted_dek::GetEncryptedDekRequest {
            key_type: DekKeyType::Recovery,
        })
        .await;

    match result {
        Ok(_) => Ok(true),
        Err(cloudless_core::ports::api::ApiClientError::Api(
            api_types::error::ApiError::NotFound { .. },
        )) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[tauri::command]
#[tracing::instrument(skip(state, request))]
pub async fn unlock_with_recovery_key(
    state: State<'_, AppState>,
    request: UnlockWithRecoveryKeyRequest,
) -> TauriResult<()> {
    tracing::info!("Attempting DEK unlock via recovery key");

    let dek = cloudless_core::applications::user::application::unlock_with_recovery_key(
        &*state.env,
        &request.recovery_key,
    )
    .await?;

    let derived_keys = DerivedKeys::derive(&dek)?;
    *state.dek_cache.write().await = Some(dek);
    *state.derived_keys_cache.write().await = Some(derived_keys);

    tracing::info!("DEK unlocked via recovery key");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state, request))]
pub async fn change_password_after_recovery(
    state: State<'_, AppState>,
    request: ChangePasswordAfterRecoveryRequest,
) -> TauriResult<()> {
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    cloudless_core::applications::user::application::change_password(
        &*state.env,
        &dek,
        &request.new_password,
    )
    .await?;

    tracing::info!("Password changed after recovery");
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip(state, request))]
pub async fn change_encryption_password(
    state: State<'_, AppState>,
    request: ChangeEncryptionPasswordRequest,
) -> TauriResult<()> {
    // Verify current password by attempting to unlock
    let dek = cloudless_core::applications::user::application::unlock_dek(
        &*state.env,
        &request.current_password,
        DekKeyType::Password,
    )
    .await
    .map_err(|_| TauriError::from("Current password is incorrect."))?;

    // Re-encrypt DEK with new password
    cloudless_core::applications::user::application::change_password(
        &*state.env,
        &dek,
        &request.new_password,
    )
    .await?;

    tracing::info!("Encryption password changed");
    Ok(())
}

// ── Biometric unlock commands ─────────────────────────────────────
//
// Biometric unlock stores a random 256-bit "Biometric Key" (BK) in the
// OS keychain (protected by biometrics via tauri-plugin-biometry), and an
// AES-256-GCM-encrypted DEK blob in the local tauri-plugin-store.
// On biometric unlock, the BK is retrieved (triggering OS biometric prompt),
// then used to decrypt the DEK blob — bypassing Argon2id password derivation.

const BIOMETRIC_STORE_PATH: &str = "biometric.json";
const BIOMETRIC_DEK_KEY: &str = "biometric_dek";
const BIOMETRIC_ENABLED_KEY: &str = "biometric_enabled";
const BIOMETRIC_KEYCHAIN_DOMAIN: &str = "app.cloudless.biometric";
const BIOMETRIC_KEYCHAIN_NAME: &str = "biometric_key";

/// Check whether biometric hardware is available and enrolled on this device.
#[tauri::command]
#[tracing::instrument(skip(app_handle))]
pub async fn is_biometric_available(app_handle: AppHandle) -> TauriResult<BiometricStatusResponse> {
    use tauri_plugin_biometry::BiometryExt;

    let status = app_handle
        .biometry()
        .status()
        .map_err(|e| TauriError::from(format!("Biometry status check failed: {}", e)))?;

    Ok(BiometricStatusResponse {
        is_available: status.is_available,
        biometry_type: format!("{:?}", status.biometry_type),
    })
}

/// Check whether biometric unlock has been enabled (local store flag).
#[tauri::command]
#[tracing::instrument(skip(app_handle))]
pub async fn is_biometric_enabled(app_handle: AppHandle) -> TauriResult<bool> {
    use tauri_plugin_store::StoreExt;

    let store = match app_handle.store(BIOMETRIC_STORE_PATH) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };

    let enabled = store
        .get(BIOMETRIC_ENABLED_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(enabled)
}

/// Generate a biometric key, encrypt the cached DEK with it, and store both:
/// - BK goes into the OS keychain (biometric-protected)
/// - Encrypted DEK blob goes into the local store
///
/// Requires DEK to be in cache (user must have unlocked with password first).
#[tauri::command]
#[tracing::instrument(skip(state, app_handle))]
pub async fn enable_biometric_unlock(
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> TauriResult<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use tauri_plugin_biometry::{BiometryExt, SetDataOptions};
    use tauri_plugin_store::StoreExt;

    // Require DEK to be unlocked
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    // Generate a random 256-bit biometric key (BK)
    let mut bk = [0u8; 32];
    rand::TryRngCore::try_fill_bytes(&mut rand::rngs::OsRng, &mut bk)
        .map_err(|e| TauriError::from(format!("Failed to generate biometric key: {}", e)))?;

    // Encrypt DEK with BK
    let encrypted_blob = dek.encrypt_dek_with_raw_key(&bk).map_err(|e| {
        TauriError::from(format!("Failed to encrypt DEK with biometric key: {}", e))
    })?;

    // Store BK in OS keychain (biometric-protected)
    let bk_base64 = STANDARD.encode(&bk);
    app_handle
        .biometry()
        .set_data(SetDataOptions {
            domain: BIOMETRIC_KEYCHAIN_DOMAIN.to_string(),
            name: BIOMETRIC_KEYCHAIN_NAME.to_string(),
            data: bk_base64,
        })
        .map_err(|e| {
            TauriError::from(format!("Failed to store biometric key in keychain: {}", e))
        })?;

    // Store encrypted DEK blob in local store
    let blob_json = serde_json::to_string(&encrypted_blob)
        .map_err(|e| TauriError::from(format!("Failed to serialize encrypted DEK: {}", e)))?;

    let store = app_handle
        .store(BIOMETRIC_STORE_PATH)
        .map_err(|e| TauriError::from(format!("Failed to open biometric store: {}", e)))?;
    store.set(BIOMETRIC_DEK_KEY.to_string(), serde_json::json!(blob_json));
    store.set(BIOMETRIC_ENABLED_KEY.to_string(), serde_json::json!(true));
    store
        .save()
        .map_err(|e| TauriError::from(format!("Failed to save biometric store: {}", e)))?;

    // Zero the BK from memory
    zeroize::Zeroize::zeroize(&mut bk);

    tracing::info!("Biometric unlock enabled");
    Ok(())
}

/// Unlock DEK using the biometric key from the OS keychain.
/// Triggers the OS biometric prompt (Face ID / Touch ID / fingerprint / Windows Hello)
/// to retrieve the BK, then decrypts the locally-stored encrypted DEK blob.
#[tauri::command]
#[tracing::instrument(skip(state, app_handle))]
pub async fn biometric_unlock(
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> TauriResult<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use tauri_plugin_biometry::{BiometryExt, GetDataOptions};
    use tauri_plugin_store::StoreExt;

    // Retrieve BK from OS keychain (triggers biometric prompt)
    let data_response = app_handle
        .biometry()
        .get_data(GetDataOptions {
            domain: BIOMETRIC_KEYCHAIN_DOMAIN.to_string(),
            name: BIOMETRIC_KEYCHAIN_NAME.to_string(),
            reason: "Unlock your Cloudless encryption".to_string(),
            cancel_title: None,
        })
        .map_err(|e| TauriError::from(format!("Biometric authentication failed: {}", e)))?;

    let bk_bytes = STANDARD
        .decode(&data_response.data)
        .map_err(|e| TauriError::from(format!("Invalid biometric key encoding: {}", e)))?;

    if bk_bytes.len() != 32 {
        return Err(TauriError::from("Invalid biometric key length."));
    }
    let mut bk = [0u8; 32];
    bk.copy_from_slice(&bk_bytes);

    // Load encrypted DEK blob from local store
    let store = app_handle
        .store(BIOMETRIC_STORE_PATH)
        .map_err(|e| TauriError::from(format!("Failed to open biometric store: {}", e)))?;

    let blob_json = store
        .get(BIOMETRIC_DEK_KEY)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or(TauriError::from(
            "No biometric DEK blob found in local store.",
        ))?;

    let encrypted_blob: cloudless_core::model::base::EncryptedData =
        serde_json::from_str(&blob_json)
            .map_err(|e| TauriError::from(format!("Failed to parse encrypted DEK blob: {}", e)))?;

    // Decrypt DEK with BK
    let dek = cloudless_core::domain::dek::Dek::decrypt_dek_with_raw_key(&bk, &encrypted_blob)
        .map_err(|e| {
            TauriError::from(format!("Failed to decrypt DEK with biometric key: {}", e))
        })?;

    // Zero BK from memory
    zeroize::Zeroize::zeroize(&mut bk);

    // Cache DEK and derived keys (same as password unlock)
    let derived_keys = DerivedKeys::derive(&dek)?;
    *state.dek_cache.write().await = Some(dek);
    *state.derived_keys_cache.write().await = Some(derived_keys);

    tracing::info!("DEK unlocked via biometric authentication");
    Ok(())
}

/// Disable biometric unlock: remove BK from OS keychain and clear local store.
#[tauri::command]
#[tracing::instrument(skip(app_handle))]
pub async fn disable_biometric_unlock(app_handle: AppHandle) -> TauriResult<()> {
    use tauri_plugin_biometry::{BiometryExt, DataOptions};
    use tauri_plugin_store::StoreExt;

    // Remove BK from OS keychain (best-effort, may already be gone)
    let _ = app_handle.biometry().remove_data(DataOptions {
        domain: BIOMETRIC_KEYCHAIN_DOMAIN.to_string(),
        name: BIOMETRIC_KEYCHAIN_NAME.to_string(),
    });

    // Clear local store
    if let Ok(store) = app_handle.store(BIOMETRIC_STORE_PATH) {
        store.delete(BIOMETRIC_DEK_KEY);
        store.set(BIOMETRIC_ENABLED_KEY.to_string(), serde_json::json!(false));
        let _ = store.save();
    }

    tracing::info!("Biometric unlock disabled");
    Ok(())
}

// ── Garbage Collection commands ────────────────────────────────────

#[tauri::command]
#[tracing::instrument(skip(state))]
pub async fn run_garbage_collection(
    state: State<'_, AppState>,
) -> TauriResult<cloudless_core::applications::gc::application::GcSummary> {
    let dek = state
        .dek_cache
        .read()
        .await
        .clone()
        .ok_or(TauriError::from("Encryption not unlocked."))?;

    Ok(cloudless_core::applications::gc::application::run_gc(&*state.env, &dek).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_gc_runs(
    state: State<'_, AppState>,
    limit: Option<i32>,
) -> TauriResult<api_types::gc::ListGcRunsResponse> {
    Ok(cloudless_core::applications::gc::application::list_gc_runs(
        &*state.env,
        api_types::gc::ListGcRunsRequest { limit },
    )
    .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_gc_run_detail(
    state: State<'_, AppState>,
    gc_run_id: uuid::Uuid,
) -> TauriResult<api_types::gc::GcRunDetailResponse> {
    Ok(
        cloudless_core::applications::gc::application::get_gc_run_detail(
            &*state.env,
            api_types::gc::GetGcRunDetailRequest { gc_run_id },
        )
        .await?,
    )
}

#[tauri::command]
pub async fn get_retention_settings(
    state: State<'_, AppState>,
) -> TauriResult<api_types::gc::GetRetentionSettingsResponse> {
    Ok(cloudless_core::applications::gc::application::get_retention_settings(&*state.env).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_retention_settings(
    state: State<'_, AppState>,
    bin_retention_days: i32,
) -> TauriResult<()> {
    Ok(
        cloudless_core::applications::gc::application::update_retention_settings(
            &*state.env,
            api_types::gc::UpdateRetentionSettingsRequest { bin_retention_days },
        )
        .await?,
    )
}

// ── Updater commands ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
}

/// Check whether a newer version is available from the updater endpoint.
/// Returns `Some(UpdateInfo)` if an update is available, `None` if already current.
/// No-op stub on mobile (updater is desktop-only).
#[tauri::command]
pub async fn check_for_updates(
    #[cfg(not(any(target_os = "android", target_os = "ios")))] app: AppHandle,
) -> TauriResult<Option<UpdateInfo>> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_updater::UpdaterExt;
        let updater = app.updater().map_err(|e| TauriError::Internal {
            message: e.to_string(),
        })?;
        match updater.check().await {
            Ok(Some(update)) => Ok(Some(UpdateInfo {
                version: update.version.clone(),
                current_version: update.current_version.clone(),
                body: update.body.clone(),
            })),
            Ok(None) => Ok(None),
            Err(e) => Err(TauriError::Internal {
                message: e.to_string(),
            }),
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(None)
    }
}

/// Download and install the latest update. The app will restart automatically
/// on Windows (passive mode) or prompt the user on macOS/Linux.
/// No-op stub on mobile.
#[tauri::command]
pub async fn install_update(
    #[cfg(not(any(target_os = "android", target_os = "ios")))] app: AppHandle,
) -> TauriResult<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_updater::UpdaterExt;
        let updater = app.updater().map_err(|e| TauriError::Internal {
            message: e.to_string(),
        })?;
        if let Some(update) = updater.check().await.map_err(|e| TauriError::Internal {
            message: e.to_string(),
        })? {
            update
                .download_and_install(|_, _| {}, || {})
                .await
                .map_err(|e| TauriError::Internal {
                    message: e.to_string(),
                })?;
        }
        Ok(())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(())
    }
}

// ── Tray config menu ──────────────────────────────────────────────

/// Builds the full tray menu.
///
/// Order:
///   [disabled status line + separator]  — only when `last_backup_label` is Some
///   [config items + separator]          — only when configs is non-empty
///   Backup Now  |  Open CloudLess
///   [separator]
///   [✓] Start at Login
///   [separator]
///   Quit
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn build_tray_menu(
    app: &AppHandle,
    configs: &[crate::models::TrayConfigItem],
    last_backup_label: Option<&str>,
    start_at_login: bool,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};

    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();

    // Status line (disabled, non-clickable)
    if let Some(label) = last_backup_label {
        items.push(Box::new(MenuItem::with_id(
            app,
            "status",
            label,
            false, // disabled
            None::<&str>,
        )?));
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
    }

    // Per-config navigation items
    for c in configs {
        items.push(Box::new(MenuItem::with_id(
            app,
            c.id.to_string(),
            &c.name,
            true,
            None::<&str>,
        )?));
    }
    if !configs.is_empty() {
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
    }

    items.push(Box::new(MenuItem::with_id(
        app,
        "backup-now",
        "Backup Now",
        true,
        None::<&str>,
    )?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "open",
        "Open CloudLess",
        true,
        None::<&str>,
    )?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(CheckMenuItem::with_id(
        app,
        "toggle-start-at-login",
        "Start at Login",
        true,
        start_at_login,
        None::<&str>,
    )?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "quit",
        "Quit",
        true,
        None::<&str>,
    )?));

    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|i| i.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// Reads all tray state (configs, last_backup_label, autostart) and rebuilds + applies the menu.
/// Called after any state change that affects the tray.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) async fn rebuild_tray_menu(app: &AppHandle) -> TauriResult<()> {
    use tauri::Manager;
    use tauri_plugin_autostart::ManagerExt;

    let tray_state = app.state::<crate::models::TrayState>();
    let configs = tray_state.configs.read().await.clone();
    let label = tray_state.last_backup_label.read().await.clone();
    let start_at_login = app.autolaunch().is_enabled().unwrap_or(false);

    let menu = build_tray_menu(app, &configs, label.as_deref(), start_at_login)
        .map_err(|e| TauriError::from(e.to_string()))?;

    let icon_guard = tray_state.icon.read().await;
    if let Some(icon) = icon_guard.as_ref() {
        icon.set_menu(Some(menu))
            .map_err(|e| TauriError::from(e.to_string()))?;
    }
    Ok(())
}

/// Updates the tray menu with the current backup config list.
/// Called by the frontend after unlock (and after config changes).
/// No-op on mobile.
#[tauri::command]
pub async fn set_tray_configs(
    app_handle: AppHandle,
    configs: Vec<crate::models::TrayConfigItem>,
) -> TauriResult<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri::Manager;
        let tray_state = app_handle.state::<crate::models::TrayState>();
        *tray_state.configs.write().await = configs;
        rebuild_tray_menu(&app_handle).await?;
    }
    let _ = app_handle;
    Ok(())
}

/// Updates the "Last backup" status line in the tray and rebuilds the menu.
/// Called by the frontend after a backup completes. No-op on mobile.
#[tauri::command]
pub async fn set_tray_status(app_handle: AppHandle, label: String) -> TauriResult<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri::Manager;
        let tray_state = app_handle.state::<crate::models::TrayState>();
        *tray_state.last_backup_label.write().await = Some(label);
        rebuild_tray_menu(&app_handle).await?;
    }
    // On mobile label is unused; on desktop it was moved into TrayState above.
    let _ = app_handle;
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let _ = label;
    Ok(())
}

// ── Start at login ────────────────────────────────────────────────

#[tauri::command]
pub async fn enable_start_at_login(app_handle: AppHandle) -> TauriResult<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_autostart::ManagerExt;
        app_handle
            .autolaunch()
            .enable()
            .map_err(|e| TauriError::from(format!("Failed to enable start at login: {e}")))?;
    }
    let _ = app_handle;
    Ok(())
}

#[tauri::command]
pub async fn disable_start_at_login(app_handle: AppHandle) -> TauriResult<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_autostart::ManagerExt;
        app_handle
            .autolaunch()
            .disable()
            .map_err(|e| TauriError::from(format!("Failed to disable start at login: {e}")))?;
    }
    let _ = app_handle;
    Ok(())
}

#[tauri::command]
pub async fn is_start_at_login_enabled(app_handle: AppHandle) -> TauriResult<bool> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_autostart::ManagerExt;
        return app_handle
            .autolaunch()
            .is_enabled()
            .map_err(|e| TauriError::from(format!("Failed to check start at login: {e}")));
    }
    #[allow(unreachable_code)]
    {
        let _ = app_handle;
        Ok(false)
    }
}
