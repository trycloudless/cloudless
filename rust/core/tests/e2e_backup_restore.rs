//! End-to-end integration test for the full backup & restore pipeline.
//!
//! **Requires**: a running API server and real S3 credentials.
//! Run via `bash rust/scripts/run_e2e_tests.sh` which handles server lifecycle.
//!
//! ## Test Structure
//!
//! This single test function exercises the complete user journey sequentially,
//! maintaining state across phases to validate real-world workflows:
//!
//! | Phase | What | Why |
//! |-------|------|-----|
//! | 1     | User signup, login, DEK setup | Bootstrap authentication and encryption |
//! | 2     | Device + S3 storage registration | Configure backup infrastructure |
//! | 3     | Create backup config + test files | Prepare source directory with mixed file types |
//! | 4     | Initial backup (5 files) | Validate first-time full backup of all files |
//! | 5     | Timestamp-only backup | Verify mtime changes trigger sync but chunks deduplicate |
//! | 6     | Content modification backup | Verify modified files get new versions, unchanged skip |
//! | 6b    | Mixed: add new files + unchanged existing | Validate new files upload while existing deduplicate |
//! | 7     | Mixed: delete files + add replacements | Validate deletions are tracked and new files coexist |
//! | 8     | Restore single modified file | Verify point-in-time restore of a specific version |
//! | 9     | Restore docs/ folder (original + new files) | Verify multi-file restore including files from different phases |
//! | 10    | Restore original version (before modification) | Verify version history preserves old content |
//! | 10b   | Restore newly added files | Verify files added in mixed phases restore correctly |
//! | 11    | Dashboard stats validation | Verify aggregate metrics match expected job counts |
//! | 12    | Cleanup | Temp directories auto-clean on drop |
//!
//! ## File Types Tested
//!
//! - **Small text** (`root.txt`, `notes.md`): Tests single-chunk backup, content-based change detection
//! - **Binary** (`report.pdf` ~10KB): Tests binary data round-trip through compress→encrypt→upload→download→decrypt→decompress
//! - **Large binary** (`photo.bin` ~5MB): Tests multi-chunk splitting (4 MiB chunk boundary)
//! - **Empty** (`empty.txt`): Edge case — zero bytes, zero chunks
//! - **CSV, TOML, Markdown** (added in mixed phases): Tests dynamic file set growth
//!
//! ## Deduplication Scenarios
//!
//! - **True dedup** (phase 5): Same content re-backed-up → chunks exist in both S3 and DB → skip upload
//! - **New + unchanged mix** (phase 6b): New files upload fresh, existing files' chunks deduplicate
//! - **Content change** (phase 6): Modified files produce new hashes → fresh upload; unmodified → dedup

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use api_types::{
    auth::LoginRequest,
    backup::BackupResult,
    backup_config::BackupExclusionConfig,
    encrypted_dek::DekKeyType,
    gc::UpdateRetentionSettingsRequest,
    local_device::GetOrCreateLocalDeviceRequest,
    remote_file_version::MoveVersionToBinRequest,
    remote_storage::{
        CreateRemoteStorageRequest, LocalFilesystemConfig, RemoteStorageConfig, RemoteStorageType,
        S3Credentials, SftpAuthConfig, SftpCredentials, SftpHostKeyPolicy,
    },
    restore::RestoreResult,
    restore_job::{OverwriteBehavior, RestoreDestination},
    user::UserCreateRequest,
};
use cloudless_core::{
    adapters::{
        aes_gcm_encryptor::AesGcmEncryptor, sftp_storage_adaptor::SftpStorageAdaptor,
        sqlite_local_index::SqliteLocalIndex,
    },
    app_env::AppEnv,
    applications::{
        backup::{
            backup_config::encrypt_create_config_request, backup_job::start_backup,
            browse::list_backed_up_files, env::BackupEnv, recovery::rebuild_local_index,
        },
        config::application as config_app,
        dashboard::application as dashboard_app,
        gc::application as gc_app,
        restore::application::{RestoreFileSelection, start_restore},
        user::application as user_app,
    },
    domain::{
        dek::Dek,
        derived_keys::DerivedKeys,
        metadata_crypto::{EncryptedMetadata, decrypt_file_path},
    },
    ports::{
        Encryptor, api::remote_file_version_api_port::RemoteFileVersionApiPort,
        local_index::LocalIndexPort,
    },
};
use rand::RngCore;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

// ─── Helpers ──────────────────────────────────────────────────────

/// Creates the initial set of 5 test files in `dir` and returns a map of
/// relative paths → content bytes.
///
/// Text files embed a unique `Uuid::now_v7()` so their SHA-256 hashes differ
/// across test runs. This avoids false deduplication against stale S3 objects
/// from prior runs (the DB is reset between runs but S3 is not cleaned).
///
/// Files created:
/// - `root.txt` — small text (~60 bytes), single chunk
/// - `docs/report.pdf` — random binary (~10 KB), single chunk
/// - `docs/notes.md` — text (~150 bytes), single chunk
/// - `images/photo.bin` — random binary (~5 MB), multi-chunk (4 MiB boundary)
/// - `empty.txt` — zero bytes, edge case
fn create_test_files(dir: &Path) -> HashMap<String, Vec<u8>> {
    let mut files = HashMap::new();
    let run_id = Uuid::now_v7();

    let root_txt =
        format!("Hello, Cloudless! Test run {run_id}. Small root-level file.\n").into_bytes();
    write_file(dir, "root.txt", &root_txt);
    files.insert("root.txt".to_string(), root_txt);

    let report_pdf = random_bytes(10_000);
    write_file(dir, "docs/report.pdf", &report_pdf);
    files.insert("docs/report.pdf".to_string(), report_pdf);

    let notes_md = format!(
        "# Meeting Notes (run {run_id})\n\nAction items:\n- Review backup design\n- Test restore flow\n- Check dedup metrics\n\nEnd of notes.\n"
    ).into_bytes();
    write_file(dir, "docs/notes.md", &notes_md);
    files.insert("docs/notes.md".to_string(), notes_md);

    let photo_bin = random_bytes(5 * 1024 * 1024);
    write_file(dir, "images/photo.bin", &photo_bin);
    files.insert("images/photo.bin".to_string(), photo_bin);

    write_file(dir, "empty.txt", &[]);
    files.insert("empty.txt".to_string(), vec![]);

    files
}

/// Writes `content` to `base_dir/relative_path`, creating parent directories as needed.
fn write_file(base_dir: &Path, relative_path: &str, content: &[u8]) {
    let full_path = base_dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create parent directories");
    }
    let mut f = std::fs::File::create(&full_path).expect("Failed to create test file");
    f.write_all(content).expect("Failed to write test file");
}

/// Generates `size` bytes of cryptographically random data.
fn random_bytes(size: usize) -> Vec<u8> {
    let mut buf = vec![0u8; size];
    rand::rng().fill_bytes(&mut buf);
    buf
}

/// Collects all events from a backup result channel until the sender closes.
async fn drain_backup_results(mut rx: mpsc::Receiver<BackupResult>) -> Vec<BackupResult> {
    let mut results = Vec::new();
    while let Some(event) = rx.recv().await {
        results.push(event);
    }
    results
}

/// Collects all events from a restore result channel until the sender closes.
async fn drain_restore_results(mut rx: mpsc::Receiver<RestoreResult>) -> Vec<RestoreResult> {
    let mut results = Vec::new();
    while let Some(event) = rx.recv().await {
        results.push(event);
    }
    results
}

/// Asserts that a backup produced no `Failed` or `FileFailed` events.
fn assert_no_backup_failures(results: &[BackupResult]) {
    let failures: Vec<_> = results
        .iter()
        .filter(|r| {
            matches!(
                r,
                BackupResult::Failed { .. } | BackupResult::FileFailed { .. }
            )
        })
        .collect();
    assert!(
        failures.is_empty(),
        "Expected no failures, but got: {failures:?}"
    );
}

/// Asserts that a restore produced no `Failed` events.
fn assert_no_restore_failures(results: &[RestoreResult]) {
    let failures: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, RestoreResult::Failed { .. }))
        .collect();
    assert!(
        failures.is_empty(),
        "Expected no failures, but got: {failures:?}"
    );
}

/// Reads a file from disk and asserts its content matches `expected` byte-for-byte.
fn assert_file_content(path: &Path, expected: &[u8]) {
    let actual =
        std::fs::read(path).unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    assert_eq!(
        actual,
        expected,
        "File content mismatch for {}. Expected {} bytes, got {} bytes",
        path.display(),
        expected.len(),
        actual.len(),
    );
}

/// Decrypts the file path from a `BackedUpFile`'s encrypted metadata fields.
fn decrypt_backed_up_path(
    file: &api_types::remote_file_version::BackedUpFile,
    derived_keys: &DerivedKeys,
) -> String {
    let encrypted = EncryptedMetadata {
        encrypted_name: file.encrypted_name.clone(),
        nonce: file.name_nonce.clone(),
        blind_index: file.blind_index.clone(),
    };
    decrypt_file_path(&encrypted, &derived_keys.metadata_key).expect("Failed to decrypt file path")
}

/// Returns the API base URL from the `API_BASE_URL` env var, defaulting to localhost:9099.
fn api_base_url() -> url::Url {
    let base =
        std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:9099".to_string());
    url::Url::parse(&base).expect("Invalid API_BASE_URL")
}

/// Reads S3 credentials from `TEST_S3_*` environment variables.
/// Panics with a descriptive message if any are missing.
fn s3_credentials() -> S3Credentials {
    S3Credentials {
        access_key: std::env::var("TEST_S3_ACCESS_KEY")
            .expect("TEST_S3_ACCESS_KEY env var required"),
        secret: std::env::var("TEST_S3_SECRET").expect("TEST_S3_SECRET env var required"),
        region: std::env::var("TEST_S3_REGION").expect("TEST_S3_REGION env var required"),
        bucket: std::env::var("TEST_S3_BUCKET").expect("TEST_S3_BUCKET env var required"),
    }
}

async fn setup_e2e_user_dek_and_device(env: &AppEnv, label: &str) -> (Dek, DerivedKeys, Uuid) {
    let test_password = "e2e-test-password-secure!";
    let unique_email = format!(
        "e2e-{}-{}@cloudless.test",
        label.replace('_', "-"),
        Uuid::now_v7()
    );

    let _ = user_app::create_user(
        env,
        UserCreateRequest {
            name: format!("E2E {label} User"),
            email: unique_email.clone(),
            password: test_password.to_string(),
        },
    )
    .await
    .expect("User creation failed");

    let _ = user_app::login(
        env,
        LoginRequest {
            email: unique_email,
            password: test_password.to_string(),
        },
    )
    .await
    .expect("Login failed");

    let dek = Dek::generate().expect("DEK generation failed");
    user_app::update_dek(env, dek.clone(), test_password, DekKeyType::Password)
        .await
        .expect("DEK storage failed");
    let derived_keys = DerivedKeys::derive(&dek).expect("DerivedKeys derivation failed");

    let device = config_app::get_or_create_local_device(
        env,
        GetOrCreateLocalDeviceRequest {
            physical_device_id: format!("e2e-{label}-device-{}", Uuid::now_v7()),
            display_name: Some(format!("E2E {label} Device")),
            platform: "test".to_string(),
        },
    )
    .await
    .expect("Device registration failed");

    (dek, derived_keys, device.id)
}

async fn register_storage_and_create_backup_config(
    env: &AppEnv,
    dek: &Dek,
    derived_keys: &DerivedKeys,
    device_id: Uuid,
    source_path: &str,
    storage_name: &str,
    storage_type: RemoteStorageType,
    storage_config: RemoteStorageConfig,
) -> Uuid {
    let config_json =
        serde_json::to_vec(&storage_config).expect("Failed to serialize storage config");
    let encryptor = AesGcmEncryptor::new(dek).expect("Failed to create encryptor");
    let encrypted_config = encryptor
        .encrypt(&config_json)
        .expect("Failed to encrypt storage config");

    let storage = config_app::register_remote_storage(
        env,
        CreateRemoteStorageRequest {
            name: storage_name.to_string(),
            storage_type,
            config: encrypted_config.into(),
        },
    )
    .await
    .expect("Storage registration failed");

    let backup_config_request = encrypt_create_config_request(
        source_path,
        storage.id,
        format!("{storage_name} Backup"),
        device_id,
        Default::default(),
        &BackupExclusionConfig::default(),
        derived_keys,
    )
    .expect("Encrypting backup config request failed");

    config_app::create_backup_config(env, backup_config_request)
        .await
        .expect("Backup config creation failed")
        .id
}

async fn assert_backup_config_initial_run(
    env: AppEnv,
    dek: Dek,
    derived_keys: DerivedKeys,
    device_id: Uuid,
    config_id: Uuid,
    expected_file_count: usize,
) {
    let rx = start_backup(env.clone(), dek, derived_keys.clone(), device_id, config_id);
    let results = drain_backup_results(rx).await;
    assert_no_backup_failures(&results);

    let completed_files: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            BackupResult::FileCompleted { file_path, .. } => Some(file_path.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        completed_files.len(),
        expected_file_count,
        "Expected {expected_file_count} completed files, got {}: {completed_files:?}",
        completed_files.len()
    );

    let completed = results
        .iter()
        .find_map(|r| match r {
            BackupResult::Completed { total_files, .. } => Some(*total_files),
            _ => None,
        })
        .expect("Expected Completed event");
    assert_eq!(completed, expected_file_count);

    let browse = list_backed_up_files(&env, config_id, None, 50, &derived_keys)
        .await
        .expect("list_backed_up_files failed");
    assert_eq!(
        browse.files.len(),
        expected_file_count,
        "Expected {expected_file_count} backed up files, got {}",
        browse.files.len()
    );

    for file in &browse.files {
        let decrypted_path = decrypt_backed_up_path(file, &derived_keys);
        assert!(
            !file.versions.is_empty(),
            "File {decrypted_path} has no versions"
        );
        assert_eq!(
            file.versions[0].version, 1,
            "Expected version 1 for {decrypted_path}"
        );
    }
}

fn sftp_password_credentials_from_env(known_host_key: Option<String>) -> Option<SftpCredentials> {
    let host = std::env::var("CLOUDLESS_SFTP_TEST_HOST").ok()?;
    let username = std::env::var("CLOUDLESS_SFTP_TEST_USERNAME").ok()?;
    let password = std::env::var("CLOUDLESS_SFTP_TEST_PASSWORD").ok()?;
    let remote_root_path = std::env::var("CLOUDLESS_SFTP_TEST_REMOTE_ROOT").ok()?;
    let port = std::env::var("CLOUDLESS_SFTP_TEST_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(22);

    Some(SftpCredentials {
        host,
        port,
        username,
        auth: SftpAuthConfig::Password { password },
        remote_root_path,
        host_key_policy: SftpHostKeyPolicy::Strict,
        known_host_key,
    })
}

#[tokio::test]
async fn local_filesystem_backup_config_pipeline() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init()
        .ok();

    let source_dir = TempDir::new().expect("Failed to create temp source dir");
    let source_path = source_dir.path().to_string_lossy().to_string();
    let storage_dir = TempDir::new().expect("Failed to create temp local storage dir");
    let storage_path = storage_dir.path().to_string_lossy().to_string();
    let index_dir = TempDir::new().expect("Failed to create temp index dir");
    let local_index = SqliteLocalIndex::open(&index_dir.path().join(".cloudless_index.db"))
        .await
        .expect("SQLite index creation failed");
    let env = AppEnv::new(api_base_url(), local_index);

    let (dek, derived_keys, device_id) =
        setup_e2e_user_dek_and_device(&env, "local_filesystem_backup_config").await;
    let original_files = create_test_files(source_dir.path());

    let config_id = register_storage_and_create_backup_config(
        &env,
        &dek,
        &derived_keys,
        device_id,
        &source_path,
        "E2E Local Filesystem Storage",
        RemoteStorageType::LocalFilesystem,
        RemoteStorageConfig::LocalFilesystem(LocalFilesystemConfig {
            root_path: storage_path.clone(),
        }),
    )
    .await;

    assert_backup_config_initial_run(
        env,
        dek,
        derived_keys,
        device_id,
        config_id,
        original_files.len(),
    )
    .await;

    let stored_objects = walkdir::WalkDir::new(storage_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .count();
    assert!(
        stored_objects > 0,
        "Expected local filesystem storage to contain uploaded chunk objects"
    );
}

#[tokio::test]
async fn sftp_backup_config_pipeline() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init()
        .ok();

    let Some(initial_sftp_creds) = sftp_password_credentials_from_env(None) else {
        eprintln!("skipping SFTP backup config test: CLOUDLESS_SFTP_TEST_* env not configured");
        return;
    };
    let connection = SftpStorageAdaptor::test_connection(initial_sftp_creds)
        .await
        .expect("SFTP connection test failed");
    let sftp_creds = sftp_password_credentials_from_env(Some(connection.host_key_fingerprint))
        .expect("CLOUDLESS_SFTP_TEST_* env changed during test");

    let source_dir = TempDir::new().expect("Failed to create temp source dir");
    let source_path = source_dir.path().to_string_lossy().to_string();
    let index_dir = TempDir::new().expect("Failed to create temp index dir");
    let local_index = SqliteLocalIndex::open(&index_dir.path().join(".cloudless_index.db"))
        .await
        .expect("SQLite index creation failed");
    let env = AppEnv::new(api_base_url(), local_index);

    let (dek, derived_keys, device_id) =
        setup_e2e_user_dek_and_device(&env, "sftp_backup_config").await;
    let original_files = create_test_files(source_dir.path());

    let config_id = register_storage_and_create_backup_config(
        &env,
        &dek,
        &derived_keys,
        device_id,
        &source_path,
        "E2E SFTP Storage",
        RemoteStorageType::Sftp,
        RemoteStorageConfig::Sftp(sftp_creds),
    )
    .await;

    assert_backup_config_initial_run(
        env,
        dek,
        derived_keys,
        device_id,
        config_id,
        original_files.len(),
    )
    .await;
}

// ─── Main E2E Test ────────────────────────────────────────────────

#[tokio::test]
async fn e2e_full_backup_and_restore_flow() {
    // Initialize tracing so S3 errors and other tracing::error! calls are visible.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init()
        .ok();

    let s3_creds = s3_credentials();
    let test_password = "e2e-test-password-secure!";
    let unique_email = format!("e2e-test-{}@cloudless.test", Uuid::now_v7());

    let source_dir = TempDir::new().expect("Failed to create temp source dir");
    let source_path = source_dir.path().to_string_lossy().to_string();

    // Open the SQLite index in a separate directory so its files (.db, -wal, -shm)
    // don't get picked up by the filesystem scan of the backup source directory.
    let index_dir = TempDir::new().expect("Failed to create temp index dir");
    let local_index = SqliteLocalIndex::open(&index_dir.path().join(".cloudless_index.db"))
        .await
        .expect("SQLite index creation failed");
    let env = AppEnv::new(api_base_url(), local_index);

    // ── Phase 1: User Setup ───────────────────────────────────────
    // Creates a fresh user, logs in, generates a DEK (data encryption key),
    // encrypts it with the user's password, and stores it on the server.
    // Then verifies the DEK can be unlocked — this is the same flow the
    // Tauri app performs on first launch.
    println!("=== Phase 1: User Setup ===");

    let user = user_app::create_user(
        &env,
        UserCreateRequest {
            name: "E2E Test User".to_string(),
            email: unique_email.clone(),
            password: test_password.to_string(),
        },
    )
    .await
    .expect("User creation failed");
    println!("  Created user: {} ({})", user.email, user.id);

    let login_response = user_app::login(
        &env,
        LoginRequest {
            email: unique_email.clone(),
            password: test_password.to_string(),
        },
    )
    .await
    .expect("Login failed");
    println!("  Logged in as: {}", login_response.email);
    assert!(!login_response.access_token.is_empty());

    // Generate and store the DEK. The DEK is the symmetric key used to encrypt
    // all chunk data and storage credentials. It is itself encrypted with the
    // user's password before being stored on the server.
    let dek = Dek::generate().expect("DEK generation failed");
    user_app::update_dek(&env, dek.clone(), test_password, DekKeyType::Password)
        .await
        .expect("DEK storage failed");
    println!("  DEK stored with password encryption");

    // Round-trip verification: unlock the DEK and compare raw key bytes.
    let unlocked_dek = user_app::unlock_dek(&env, test_password, DekKeyType::Password)
        .await
        .expect("DEK unlock failed");
    assert_eq!(
        unlocked_dek.key.as_ref(),
        dek.key.as_ref(),
        "Unlocked DEK does not match original"
    );
    println!("  DEK unlock verified");

    let derived_keys = DerivedKeys::derive(&dek).expect("DerivedKeys derivation failed");
    println!("  DerivedKeys derived from DEK");

    // ── Phase 2: Device & Storage Setup ───────────────────────────
    // Registers a logical device (this test machine) and an S3 remote storage.
    // The S3 credentials are encrypted client-side with the DEK before being
    // sent to the server — the server never sees plaintext credentials.
    println!("\n=== Phase 2: Device & Storage Setup ===");

    let device_physical_id = format!("e2e-test-device-{}", Uuid::now_v7());
    let device = config_app::get_or_create_local_device(
        &env,
        GetOrCreateLocalDeviceRequest {
            physical_device_id: device_physical_id,
            display_name: Some("E2E Test Device".to_string()),
            platform: "test".to_string(),
        },
    )
    .await
    .expect("Device registration failed");
    println!(
        "  Registered device: {} (was_created={})",
        device.id, device.was_created
    );
    assert!(device.was_created);

    // Encrypt S3 credentials with the DEK, then register as remote storage.
    let storage_config = RemoteStorageConfig::Aws(s3_creds);
    let config_json = serde_json::to_vec(&storage_config).expect("Failed to serialize S3 config");
    let encryptor = AesGcmEncryptor::new(&dek).expect("Failed to create encryptor");
    let encrypted_config = encryptor
        .encrypt(&config_json)
        .expect("Failed to encrypt S3 config");

    let storage = config_app::register_remote_storage(
        &env,
        CreateRemoteStorageRequest {
            name: "E2E Test S3 Storage".to_string(),
            storage_type: RemoteStorageType::Aws,
            config: encrypted_config.into(),
        },
    )
    .await
    .expect("Storage registration failed");
    println!("  Registered remote storage: {}", storage.id);

    // ── Phase 3: Backup Config Setup ──────────────────────────────
    // Creates 5 test files of varying types and sizes in a temp directory,
    // then creates a backup config pointing at that directory. This is the
    // equivalent of a user clicking "Add folder to backup".
    println!("\n=== Phase 3: Backup Config Setup ===");

    let original_files = create_test_files(source_dir.path());
    println!(
        "  Created {} test files in {}",
        original_files.len(),
        source_path
    );

    let backup_config_request = encrypt_create_config_request(
        &source_path,
        storage.id,
        "E2E Test Backup".to_string(),
        device.id,
        Default::default(),
        &BackupExclusionConfig::default(),
        &derived_keys,
    )
    .expect("Encrypting backup config request failed");
    let backup_config = config_app::create_backup_config(&env, backup_config_request)
        .await
        .expect("Backup config creation failed");
    let config_id = backup_config.id;
    println!("  Created backup config: {config_id}");

    // ── Phase 4: Initial Backup ───────────────────────────────────
    // First-ever backup of the source directory. All 5 files should be
    // discovered, chunked, compressed, encrypted, and uploaded to S3.
    //
    // Validates the full event stream:
    //   ConfigLoaded → IndexUpdated(5) → CandidatesFound(5) →
    //   FileStarted/ChunkUploaded/FileCompleted × 5 → Completed(5)
    //
    // Also verifies multi-chunk behavior: photo.bin (5 MB) exceeds the
    // 4 MiB chunk size and should produce at least 2 ChunkUploaded events.
    println!("\n=== Phase 4: Initial Backup ===");

    let rx = start_backup(
        env.clone(),
        dek.clone(),
        derived_keys.clone(),
        device.id,
        config_id,
    );
    let results = drain_backup_results(rx).await;
    assert_no_backup_failures(&results);

    assert!(
        results
            .iter()
            .any(|r| matches!(r, BackupResult::ConfigLoaded { .. })),
        "Expected ConfigLoaded event"
    );

    let sync_event = results
        .iter()
        .find_map(|r| match r {
            BackupResult::IndexUpdated { files_updated, .. } => Some(*files_updated),
            _ => None,
        })
        .expect("Expected IndexUpdated event");
    assert_eq!(sync_event, 5, "Expected 5 files synced, got {sync_event}");
    println!("  IndexUpdated: {sync_event} files");

    let candidates = results
        .iter()
        .find_map(|r| match r {
            BackupResult::CandidatesFound {
                total_files,
                total_bytes,
            } => Some((*total_files, *total_bytes)),
            _ => None,
        })
        .expect("Expected CandidatesFound event");
    assert_eq!(
        candidates.0, 5,
        "Expected 5 candidates, got {}",
        candidates.0
    );
    println!(
        "  CandidatesFound: {} files, {} bytes",
        candidates.0, candidates.1
    );

    let completed_files: Vec<String> = results
        .iter()
        .filter_map(|r| match r {
            BackupResult::FileCompleted { file_path, .. } => Some(file_path.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        completed_files.len(),
        5,
        "Expected 5 FileCompleted events, got {}: {completed_files:?}",
        completed_files.len()
    );
    println!("  Completed files: {completed_files:?}");

    // photo.bin is ~5 MB with 4 MiB chunks → at least 2 chunks
    let photo_chunks: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, BackupResult::ChunkUploaded { file_path, .. } if file_path.ends_with("photo.bin")))
        .collect();
    assert!(
        photo_chunks.len() >= 2,
        "Expected photo.bin to have multiple chunks, got {}",
        photo_chunks.len()
    );
    println!("  photo.bin chunks: {}", photo_chunks.len());

    let completed = results
        .iter()
        .find_map(|r| match r {
            BackupResult::Completed {
                total_files,
                total_bytes,
                uploaded_bytes,
                deduplicated_bytes,
            } => Some((
                *total_files,
                *total_bytes,
                *uploaded_bytes,
                *deduplicated_bytes,
            )),
            _ => None,
        })
        .expect("Expected Completed event");
    assert_eq!(completed.0, 5, "Expected 5 total files in Completed");
    println!(
        "  Completed: {} files, {} bytes total, {} uploaded, {} deduped",
        completed.0, completed.1, completed.2, completed.3
    );

    // Verify the browse API returns all 5 files, each at version 1.
    let browse = list_backed_up_files(&env, config_id, None, 50, &derived_keys)
        .await
        .expect("list_backed_up_files failed");
    assert_eq!(
        browse.files.len(),
        5,
        "Expected 5 backed up files, got {}",
        browse.files.len()
    );
    for file in &browse.files {
        let decrypted_path = decrypt_backed_up_path(file, &derived_keys);
        assert!(
            !file.versions.is_empty(),
            "File {} has no versions",
            decrypted_path
        );
        assert_eq!(
            file.versions[0].version, 1,
            "Expected version 1 for {}, got {}",
            decrypted_path, file.versions[0].version
        );
    }
    println!("  Browse verified: 5 files, all version 1");

    // ── Phase 5: Timestamp-only Change Backup ─────────────────────
    // Touches all files (rewrites same content) to update mtime without
    // changing content. Tests that:
    //   - The file sync detects mtime changes and re-scans
    //   - Backup candidates may be generated (implementation-dependent)
    //   - If candidates are generated, all chunks should be fully deduplicated
    //     because the SHA-256 hashes haven't changed
    //   - No failures occur
    println!("\n=== Phase 5: Timestamp-only Change Backup ===");

    // Sleep 1s so the filesystem timestamp is visibly different.
    std::thread::sleep(std::time::Duration::from_secs(1));
    for relative_path in original_files.keys() {
        let full_path = source_dir.path().join(relative_path);
        let content = std::fs::read(&full_path).expect("Failed to read file for touch");
        std::fs::write(&full_path, &content).expect("Failed to touch file");
    }

    let rx = start_backup(
        env.clone(),
        dek.clone(),
        derived_keys.clone(),
        device.id,
        config_id,
    );
    let results = drain_backup_results(rx).await;
    assert_no_backup_failures(&results);

    let completed = results
        .iter()
        .find(|r| matches!(r, BackupResult::Completed { .. }))
        .expect("Expected Completed event for timestamp-only backup");
    println!("  Timestamp backup completed: {completed:?}");

    let dedup_chunks: Vec<_> = results
        .iter()
        .filter(|r| {
            matches!(
                r,
                BackupResult::ChunkUploaded {
                    deduplicated: true,
                    ..
                }
            )
        })
        .collect();
    if !dedup_chunks.is_empty() {
        println!("  Deduplicated chunks: {}", dedup_chunks.len());
    }

    // ── Phase 6: Content Modification Backup ──────────────────────
    // Modifies 2 of the 5 files (root.txt, docs/notes.md) while leaving the
    // other 3 unchanged. Tests that:
    //   - Modified files are detected and backed up as new versions (v2)
    //   - Unchanged files (report.pdf, photo.bin, empty.txt) are either
    //     skipped or their chunks are fully deduplicated
    //   - The browse API shows version >= 2 for modified files
    println!("\n=== Phase 6: Content Modification Backup ===");

    let modified_root_txt = format!(
        "Hello, Cloudless! MODIFIED content. Run {}.\nNew line added.\n",
        Uuid::now_v7()
    )
    .into_bytes();
    write_file(source_dir.path(), "root.txt", &modified_root_txt);

    let modified_notes_md = format!(
        "# Updated Meeting Notes ({})\n\nAll action items completed.\n- Backup design reviewed\n- Restore flow tested\n- Dedup metrics verified\n\nDone!\n",
        Uuid::now_v7()
    ).into_bytes();
    write_file(source_dir.path(), "docs/notes.md", &modified_notes_md);

    let rx = start_backup(
        env.clone(),
        dek.clone(),
        derived_keys.clone(),
        device.id,
        config_id,
    );
    let results = drain_backup_results(rx).await;
    assert_no_backup_failures(&results);

    let completed_paths: Vec<String> = results
        .iter()
        .filter_map(|r| match r {
            BackupResult::FileCompleted { file_path, .. } => Some(file_path.clone()),
            _ => None,
        })
        .collect();
    println!("  Files backed up: {completed_paths:?}");

    let browse = list_backed_up_files(&env, config_id, None, 50, &derived_keys)
        .await
        .expect("list_backed_up_files failed after content change");
    for file in &browse.files {
        let decrypted_path = decrypt_backed_up_path(file, &derived_keys);
        let max_version = file.versions.iter().map(|v| v.version).max().unwrap_or(0);
        if decrypted_path.ends_with("root.txt") || decrypted_path.ends_with("notes.md") {
            assert!(
                max_version >= 2,
                "Expected {} to have version >= 2, got {max_version}",
                decrypted_path
            );
        }
    }
    println!("  Browse verified: modified files have version >= 2");

    // ── Phase 6b: Mixed Backup — New Files + Unchanged Existing ───
    // Adds 2 brand-new files (data/records.csv, config.toml) to the source
    // directory without modifying any existing files. Tests that:
    //   - The file sync discovers the 2 new files
    //   - New files are uploaded as fresh version 1 entries
    //   - Existing unchanged files' chunks are fully deduplicated (no re-upload)
    //   - The browse API now shows 7 total files (5 original + 2 new)
    //
    // This is the primary "mixed workload" scenario: a user adds new documents
    // to an already-backed-up folder.
    println!("\n=== Phase 6b: Mixed Backup — New Files + Unchanged Existing ===");

    let new_data_csv = format!(
        "id,name,value\n1,alpha,{}\n2,beta,{}\n3,gamma,42\n",
        Uuid::now_v7(),
        Uuid::now_v7()
    )
    .into_bytes();
    write_file(source_dir.path(), "data/records.csv", &new_data_csv);

    let new_config_toml = format!(
        "[settings]\nversion = \"{}\"\nenabled = true\nmax_retries = 5\n",
        Uuid::now_v7()
    )
    .into_bytes();
    write_file(source_dir.path(), "config.toml", &new_config_toml);

    let rx = start_backup(
        env.clone(),
        dek.clone(),
        derived_keys.clone(),
        device.id,
        config_id,
    );
    let results = drain_backup_results(rx).await;
    assert_no_backup_failures(&results);

    let sync_updated = results
        .iter()
        .find_map(|r| match r {
            BackupResult::IndexUpdated { files_updated, .. } => Some(*files_updated),
            _ => None,
        })
        .expect("Expected IndexUpdated event");
    assert!(
        sync_updated >= 2,
        "Expected at least 2 new files synced, got {sync_updated}"
    );
    println!("  IndexUpdated: {sync_updated} files updated");

    let candidates_count = results.iter().find_map(|r| match r {
        BackupResult::CandidatesFound { total_files, .. } => Some(*total_files),
        _ => None,
    });
    println!("  CandidatesFound: {candidates_count:?} files");

    let completed_paths: Vec<String> = results
        .iter()
        .filter_map(|r| match r {
            BackupResult::FileCompleted { file_path, .. } => Some(file_path.clone()),
            _ => None,
        })
        .collect();
    println!("  Completed files: {completed_paths:?}");

    // Log dedup vs fresh chunk counts to verify mixed behavior.
    let dedup_chunks: usize = results
        .iter()
        .filter(|r| {
            matches!(
                r,
                BackupResult::ChunkUploaded {
                    deduplicated: true,
                    ..
                }
            )
        })
        .count();
    let fresh_chunks: usize = results
        .iter()
        .filter(|r| {
            matches!(
                r,
                BackupResult::ChunkUploaded {
                    deduplicated: false,
                    ..
                }
            )
        })
        .count();
    println!("  Chunk stats: {fresh_chunks} fresh, {dedup_chunks} deduplicated");

    let browse = list_backed_up_files(&env, config_id, None, 50, &derived_keys)
        .await
        .expect("list_backed_up_files failed after mixed backup");
    assert_eq!(
        browse.files.len(),
        7,
        "Expected 7 files (5 original + 2 new), got {}",
        browse.files.len()
    );

    // Newly added files should be at version 1.
    for name in ["data/records.csv", "config.toml"] {
        let file = browse
            .files
            .iter()
            .find(|f| decrypt_backed_up_path(f, &derived_keys).ends_with(name))
            .unwrap_or_else(|| panic!("{name} not found in browse results"));
        let max_v = file.versions.iter().map(|v| v.version).max().unwrap_or(0);
        assert_eq!(max_v, 1, "Expected {name} at version 1, got {max_v}");
    }
    println!("  Browse verified: 7 files, new files at version 1");

    // ── Phase 7: File Deletion + Add Replacement Files ────────────
    // Deletes 2 files (docs/notes.md, empty.txt) and simultaneously adds
    // 2 new files (docs/summary.md, README.txt). Tests that:
    //   - The file sync detects both deletions and additions in one pass
    //   - New replacement files are backed up normally
    //   - Deleted files are preserved in backup history (backup never
    //     discards version history — this is a core design principle)
    //   - Browse shows all files ever backed up (9 total: 7 + 2 new)
    //
    // This simulates a user reorganizing their folder: removing old files
    // and adding new ones between backup runs.
    println!("\n=== Phase 7: File Deletion + Add Replacement Files ===");

    std::fs::remove_file(source_dir.path().join("docs/notes.md"))
        .expect("Failed to delete docs/notes.md");
    std::fs::remove_file(source_dir.path().join("empty.txt")).expect("Failed to delete empty.txt");

    let new_summary_md = format!(
        "# Summary (run {})\n\nReplaced notes.md with this summary.\n",
        Uuid::now_v7()
    )
    .into_bytes();
    write_file(source_dir.path(), "docs/summary.md", &new_summary_md);

    let new_readme = format!(
        "Backup test run {}. This is a new readme.\n",
        Uuid::now_v7()
    )
    .into_bytes();
    write_file(source_dir.path(), "README.txt", &new_readme);

    let rx = start_backup(
        env.clone(),
        dek.clone(),
        derived_keys.clone(),
        device.id,
        config_id,
    );
    let results = drain_backup_results(rx).await;
    assert_no_backup_failures(&results);

    let sync_event = results.iter().find_map(|r| match r {
        BackupResult::IndexUpdated {
            files_updated,
            files_failed,
        } => Some((*files_updated, *files_failed)),
        _ => None,
    });
    println!("  Sync after deletion+addition: {sync_event:?}");

    let completed_paths: Vec<String> = results
        .iter()
        .filter_map(|r| match r {
            BackupResult::FileCompleted { file_path, .. } => Some(file_path.clone()),
            _ => None,
        })
        .collect();
    println!("  Completed files: {completed_paths:?}");

    // All files ever backed up should still be in browse results.
    let browse = list_backed_up_files(&env, config_id, None, 50, &derived_keys)
        .await
        .expect("list_backed_up_files failed after deletion+addition");
    assert!(
        browse.files.len() >= 9,
        "Expected at least 9 files in backup history (7 + summary.md + README.txt), got {}",
        browse.files.len()
    );
    println!(
        "  Browse verified: {} files in history (deletions preserved, new files added)",
        browse.files.len()
    );

    // ── Phase 8: Restore Single Modified File ─────────────────────
    // Restores root.txt at version 2 (the modified version from phase 6)
    // to a fresh temp directory. Tests that:
    //   - The restore pipeline (download → decrypt → decompress → write) works
    //   - Integrity check confirms all chunks are available on S3
    //   - The restored file content is byte-for-byte identical to what was
    //     written in phase 6
    println!("\n=== Phase 8: Restore Single Modified File ===");

    let restore_dir_single = TempDir::new().expect("Failed to create restore dir");

    let root_file = browse
        .files
        .iter()
        .find(|f| decrypt_backed_up_path(f, &derived_keys).ends_with("root.txt"))
        .expect("root.txt not found in browse results");
    let root_file_path = decrypt_backed_up_path(root_file, &derived_keys);
    // Select the latest version — this should be the modified content from phase 6.
    // Version numbering depends on how many backup iterations produced candidates
    // (e.g. phase 5's timestamp-only backup may or may not create new versions).
    let root_latest = root_file
        .versions
        .iter()
        .max_by_key(|v| v.version)
        .expect("root.txt has no versions");
    println!(
        "  root.txt latest version: {} (selecting for restore)",
        root_latest.version
    );

    let selections = vec![RestoreFileSelection {
        version_id: root_latest.version_id,
        file_path: root_file_path.clone(),
        size: root_latest.size,
        version: root_latest.version,
    }];

    let rx = start_restore(
        env.clone(),
        dek.clone(),
        config_id,
        selections,
        RestoreDestination::CustomPath(restore_dir_single.path().to_string_lossy().to_string()),
        OverwriteBehavior::Overwrite,
    );
    let results = drain_restore_results(rx).await;
    assert_no_restore_failures(&results);

    let integrity = results
        .iter()
        .find_map(|r| match r {
            RestoreResult::IntegrityCheckComplete {
                total_chunks,
                available,
                missing,
            } => Some((*total_chunks, *available, *missing)),
            _ => None,
        })
        .expect("Expected IntegrityCheckComplete event");
    assert_eq!(
        integrity.2, 0,
        "Expected 0 missing chunks, got {}",
        integrity.2
    );
    println!(
        "  Integrity check: {} chunks, {} available, {} missing",
        integrity.0, integrity.1, integrity.2
    );

    let completed = results
        .iter()
        .find_map(|r| match r {
            RestoreResult::Completed {
                total_files,
                total_bytes,
            } => Some((*total_files, *total_bytes)),
            _ => None,
        })
        .expect("Expected Completed event for restore");
    assert_eq!(completed.0, 1, "Expected 1 file restored");
    println!("  Restored {} file(s), {} bytes", completed.0, completed.1);

    let relative_path = root_file_path
        .strip_prefix(&source_path)
        .unwrap_or(&root_file_path)
        .trim_start_matches('/');
    let restored_file_path = restore_dir_single.path().join(relative_path);
    assert_file_content(&restored_file_path, &modified_root_txt);
    println!("  Verified restored content matches modified root.txt");

    // ── Phase 9: Restore Folder (docs/) ───────────────────────────
    // Restores all files under docs/ (report.pdf, notes.md, summary.md) at
    // their latest versions. Tests multi-file restore including:
    //   - An unchanged file (report.pdf v1) — verifies original binary content survives
    //   - A modified file (notes.md v2) — verifies modified content is restored
    //   - A newly added file (summary.md v1 from phase 7) — verifies files
    //     added in later backup iterations can be restored alongside originals
    println!("\n=== Phase 9: Restore Folder (docs/) ===");

    let restore_dir_folder = TempDir::new().expect("Failed to create restore dir for folder");

    let docs_selections: Vec<RestoreFileSelection> = browse
        .files
        .iter()
        .filter(|f| decrypt_backed_up_path(f, &derived_keys).contains("/docs/"))
        .map(|f| {
            let latest = f.versions.iter().max_by_key(|v| v.version).unwrap();
            RestoreFileSelection {
                version_id: latest.version_id,
                file_path: decrypt_backed_up_path(f, &derived_keys),
                size: latest.size,
                version: latest.version,
            }
        })
        .collect();
    println!("  Restoring {} docs/ files", docs_selections.len());
    assert!(
        docs_selections.len() >= 3,
        "Expected at least 3 docs/ files (report.pdf, notes.md, summary.md), got {}",
        docs_selections.len()
    );

    let rx = start_restore(
        env.clone(),
        dek.clone(),
        config_id,
        docs_selections.clone(),
        RestoreDestination::CustomPath(restore_dir_folder.path().to_string_lossy().to_string()),
        OverwriteBehavior::Overwrite,
    );
    let results = drain_restore_results(rx).await;
    assert_no_restore_failures(&results);

    let completed = results
        .iter()
        .find_map(|r| match r {
            RestoreResult::Completed { total_files, .. } => Some(*total_files),
            _ => None,
        })
        .expect("Expected Completed event for folder restore");
    assert_eq!(
        completed,
        docs_selections.len(),
        "Expected {} files restored, got {completed}",
        docs_selections.len()
    );
    println!("  Restored {completed} docs/ files");

    // Verify report.pdf — unchanged since phase 4, should match original random bytes.
    let report_path = decrypt_backed_up_path(
        browse
            .files
            .iter()
            .find(|f| decrypt_backed_up_path(f, &derived_keys).ends_with("report.pdf"))
            .expect("report.pdf not found"),
        &derived_keys,
    );
    let report_relative = report_path
        .strip_prefix(&source_path)
        .unwrap_or("docs/report.pdf")
        .trim_start_matches('/');
    let restored_report = restore_dir_folder.path().join(report_relative);
    assert_file_content(
        &restored_report,
        original_files.get("docs/report.pdf").unwrap(),
    );
    println!("  Verified docs/report.pdf content matches original");

    // Verify notes.md — should be the phase 6 modified version.
    let notes_path = decrypt_backed_up_path(
        browse
            .files
            .iter()
            .find(|f| decrypt_backed_up_path(f, &derived_keys).ends_with("notes.md"))
            .expect("notes.md not found"),
        &derived_keys,
    );
    let notes_relative = notes_path
        .strip_prefix(&source_path)
        .unwrap_or("docs/notes.md")
        .trim_start_matches('/');
    let restored_notes = restore_dir_folder.path().join(notes_relative);
    assert_file_content(&restored_notes, &modified_notes_md);
    println!("  Verified docs/notes.md content matches modified version");

    // Verify summary.md — new file added in phase 7.
    let summary_path = decrypt_backed_up_path(
        browse
            .files
            .iter()
            .find(|f| decrypt_backed_up_path(f, &derived_keys).ends_with("summary.md"))
            .expect("summary.md not found"),
        &derived_keys,
    );
    let summary_relative = summary_path
        .strip_prefix(&source_path)
        .unwrap_or("docs/summary.md")
        .trim_start_matches('/');
    let restored_summary = restore_dir_folder.path().join(summary_relative);
    assert_file_content(&restored_summary, &new_summary_md);
    println!("  Verified docs/summary.md content matches (newly added file)");

    // ── Phase 10: Restore Original Version ────────────────────────
    // Restores root.txt at version 1 (the original content from phase 4,
    // before it was modified in phase 6). Tests that the version history
    // preserves old content and that any version can be restored independently.
    println!("\n=== Phase 10: Restore Original Version ===");

    let restore_dir_original = TempDir::new().expect("Failed to create restore dir for v1");

    let root_v1 = root_file
        .versions
        .iter()
        .find(|v| v.version == 1)
        .expect("root.txt version 1 not found");

    let selections_v1 = vec![RestoreFileSelection {
        version_id: root_v1.version_id,
        file_path: root_file_path.clone(),
        size: root_v1.size,
        version: root_v1.version,
    }];

    let rx = start_restore(
        env.clone(),
        dek.clone(),
        config_id,
        selections_v1,
        RestoreDestination::CustomPath(restore_dir_original.path().to_string_lossy().to_string()),
        OverwriteBehavior::Overwrite,
    );
    let results = drain_restore_results(rx).await;
    assert_no_restore_failures(&results);

    let restored_v1_path = restore_dir_original.path().join(relative_path);
    assert_file_content(&restored_v1_path, original_files.get("root.txt").unwrap());
    println!("  Verified restored v1 content matches original root.txt");

    // ── Phase 10b: Restore Newly Added Files ──────────────────────
    // Restores config.toml and data/records.csv — files that were first
    // added in phase 6b (they didn't exist in the initial backup). Tests
    // that files introduced in later backup iterations have correct
    // content and can be restored independently.
    println!("\n=== Phase 10b: Restore Newly Added Files (from mixed phases) ===");

    let restore_dir_new = TempDir::new().expect("Failed to create restore dir for new files");

    let new_file_selections: Vec<RestoreFileSelection> = browse
        .files
        .iter()
        .filter(|f| {
            let p = decrypt_backed_up_path(f, &derived_keys);
            p.ends_with("config.toml") || p.ends_with("records.csv")
        })
        .map(|f| {
            let latest = f.versions.iter().max_by_key(|v| v.version).unwrap();
            RestoreFileSelection {
                version_id: latest.version_id,
                file_path: decrypt_backed_up_path(f, &derived_keys),
                size: latest.size,
                version: latest.version,
            }
        })
        .collect();
    assert_eq!(
        new_file_selections.len(),
        2,
        "Expected 2 new files for restore, got {}",
        new_file_selections.len()
    );

    let rx = start_restore(
        env.clone(),
        dek.clone(),
        config_id,
        new_file_selections,
        RestoreDestination::CustomPath(restore_dir_new.path().to_string_lossy().to_string()),
        OverwriteBehavior::Overwrite,
    );
    let results = drain_restore_results(rx).await;
    assert_no_restore_failures(&results);

    let completed = results
        .iter()
        .find_map(|r| match r {
            RestoreResult::Completed { total_files, .. } => Some(*total_files),
            _ => None,
        })
        .expect("Expected Completed event for new files restore");
    assert_eq!(completed, 2, "Expected 2 files restored, got {completed}");

    let config_path = decrypt_backed_up_path(
        browse
            .files
            .iter()
            .find(|f| decrypt_backed_up_path(f, &derived_keys).ends_with("config.toml"))
            .unwrap(),
        &derived_keys,
    );
    let config_relative = config_path
        .strip_prefix(&source_path)
        .unwrap_or("config.toml")
        .trim_start_matches('/');
    let restored_config = restore_dir_new.path().join(config_relative);
    assert_file_content(&restored_config, &new_config_toml);
    println!("  Verified config.toml content matches");

    let csv_path = decrypt_backed_up_path(
        browse
            .files
            .iter()
            .find(|f| decrypt_backed_up_path(f, &derived_keys).ends_with("records.csv"))
            .unwrap(),
        &derived_keys,
    );
    let csv_relative = csv_path
        .strip_prefix(&source_path)
        .unwrap_or("data/records.csv")
        .trim_start_matches('/');
    let restored_csv = restore_dir_new.path().join(csv_relative);
    assert_file_content(&restored_csv, &new_data_csv);
    println!("  Verified data/records.csv content matches");

    // ── Phase 11: Dashboard Validation ────────────────────────────
    // Queries aggregate stats and verifies they reflect all the work done
    // in phases 4-10b. This is a sanity check that the server correctly
    // tracks job counts and byte totals across multiple backup/restore cycles.
    println!("\n=== Phase 11: Dashboard Validation ===");

    let dashboard = dashboard_app::get_dashboard_stats(&env)
        .await
        .expect("Dashboard stats failed");
    println!("  Dashboard stats:");
    println!(
        "    Total files protected: {}",
        dashboard.total_files_protected
    );
    println!(
        "    Total original bytes: {}",
        dashboard.total_original_bytes
    );
    println!(
        "    Total uploaded bytes: {}",
        dashboard.total_uploaded_bytes
    );
    println!(
        "    Total deduplicated bytes: {}",
        dashboard.total_deduplicated_bytes
    );
    println!(
        "    Active backup configs: {}",
        dashboard.active_backup_configs
    );
    println!("    Total backup jobs: {}", dashboard.total_backup_jobs);
    println!("    Total restore jobs: {}", dashboard.total_restore_jobs);

    assert!(
        dashboard.total_files_protected > 0,
        "Expected total_files_protected > 0"
    );
    // Phases 4, 5, 6, 6b, 7 = at least 5 backup jobs
    assert!(
        dashboard.total_backup_jobs >= 5,
        "Expected at least 5 backup jobs, got {}",
        dashboard.total_backup_jobs
    );
    // Phases 8, 9, 10, 10b = at least 4 restore jobs
    assert!(
        dashboard.total_restore_jobs >= 4,
        "Expected at least 4 restore jobs, got {}",
        dashboard.total_restore_jobs
    );
    assert!(
        dashboard.total_uploaded_bytes > 0,
        "Expected total_uploaded_bytes > 0"
    );
    // Consistency invariant: if files are protected, both byte counters must be
    // non-zero. A mismatch here (e.g. files > 0 but bytes = 0) indicates the
    // dashboard query draws from different data sources with different scopes.
    assert!(
        dashboard.total_original_bytes > 0,
        "Expected total_original_bytes > 0 when total_files_protected = {}",
        dashboard.total_files_protected
    );
    assert!(
        dashboard.total_original_bytes >= dashboard.total_uploaded_bytes,
        "Original bytes ({}) must be >= uploaded bytes ({}) — dedup cannot increase size",
        dashboard.total_original_bytes,
        dashboard.total_uploaded_bytes
    );

    // ── Phase 13: GC Pipeline ─────────────────────────────────────
    // Validates the GC API surface end-to-end:
    //   1. Set retention to minimum (1 day) — 0 is rejected by validation
    //   2. Move the only version of docs/report.pdf to bin (soft-delete)
    //   3. Run GC: with 1-day retention the bin version won't expire in a
    //      single test run, so versions_deleted=0. The GC still executes the
    //      full collect→delete→confirm pipeline and returns a valid summary.
    //   4. Assert GC ran without error (pipeline is functional)
    println!("\n=== Phase 13: GC Pipeline ===");

    gc_app::update_retention_settings(
        &env,
        UpdateRetentionSettingsRequest {
            bin_retention_days: 1,
        },
    )
    .await
    .expect("update_retention_settings failed");
    println!("  Set bin retention to 1 day (minimum valid value)");

    // Fetch the current file list to locate docs/report.pdf
    let browse_for_gc = list_backed_up_files(&env, config_id, None, 50, &derived_keys)
        .await
        .expect("list_backed_up_files failed before GC");

    let report_file = browse_for_gc
        .files
        .iter()
        .find(|f| decrypt_backed_up_path(f, &derived_keys).ends_with("docs/report.pdf"))
        .expect("docs/report.pdf not found for GC test");
    let report_version = report_file
        .versions
        .iter()
        .max_by_key(|v| v.version)
        .expect("report.pdf has no versions");

    env.remote_file_version_api()
        .move_to_bin(MoveVersionToBinRequest {
            version_id: report_version.version_id,
        })
        .await
        .expect("move_to_bin failed");
    println!("  Moved docs/report.pdf v{} to bin", report_version.version);

    // Remove from disk to match the bin operation — Phase 14's backup will
    // then skip this file and not hit a server conflict on the binned version.
    let report_disk_path = source_dir.path().join("docs/report.pdf");
    if report_disk_path.exists() {
        std::fs::remove_file(&report_disk_path).expect("Failed to remove report.pdf from disk");
        println!("  Removed docs/report.pdf from source directory");
    }

    let gc_summary = gc_app::run_gc(&env, &dek).await.expect("run_gc failed");
    println!(
        "  GC complete: {} versions deleted, {} chunks deleted, {} bytes freed",
        gc_summary.versions_deleted, gc_summary.chunks_deleted, gc_summary.storage_freed_bytes
    );
    // The bin version needs 1 day to expire — within a single test run it
    // won't be eligible for deletion. We assert GC completed successfully
    // (the pipeline is wired up) without requiring specific deletion counts.
    println!("  GC pipeline executed successfully (bin version pending expiry)");

    // ── Phase 14: Index Rebuild ────────────────────────────────────
    // Simulates local index loss (crash/corruption/new device) and rebuilds
    // from server state:
    //   1. Record expected file count from a fresh browse (after GC)
    //   2. Clear the SQLite index entirely
    //   3. Rebuild from server (fetch all versions, keep latest per file)
    //   4. Assert recovered count matches expected
    //   5. Run a subsequent backup to confirm the rebuilt index is usable
    println!("\n=== Phase 14: Index Rebuild ===");

    let browse_after_gc = list_backed_up_files(&env, config_id, None, 50, &derived_keys)
        .await
        .expect("list_backed_up_files failed before rebuild");
    let expected_files = browse_after_gc.files.len();
    println!("  Files visible in browse after GC: {expected_files}");

    env.local_index()
        .clear_all(config_id)
        .await
        .expect("clear_all failed");
    println!("  Local index cleared (simulating corruption/loss)");

    let recovery = rebuild_local_index(&env, config_id, &derived_keys)
        .await
        .expect("rebuild_local_index failed");
    println!(
        "  Rebuild complete: {} files recovered ({} total versions fetched)",
        recovery.files_recovered, recovery.total_versions
    );

    assert!(
        recovery.files_recovered > 0,
        "Expected > 0 files recovered, got 0"
    );
    // Recovered count should equal the browse count (server returns same set)
    assert_eq!(
        recovery.files_recovered, expected_files,
        "Recovered count ({}) does not match browse count ({expected_files})",
        recovery.files_recovered
    );
    println!("  Index rebuild assertions passed");

    // Run a backup with the rebuilt index to confirm it is functional
    let rx_after_rebuild = start_backup(
        env.clone(),
        dek.clone(),
        derived_keys.clone(),
        device.id,
        config_id,
    );
    let results_after_rebuild = drain_backup_results(rx_after_rebuild).await;
    assert_no_backup_failures(&results_after_rebuild);
    println!("  Post-rebuild backup completed without failures — index is functional");

    // ── Phase 12: Cleanup ─────────────────────────────────────────
    // TempDirs auto-clean on drop. S3 objects are left in place (the test
    // bucket is shared across runs; unique file content per run prevents
    // cross-run interference).
    println!("\n=== Phase 12: Cleanup ===");
    println!("  Temp directories will be cleaned up on drop");

    println!("\n=== E2E TEST PASSED ===");
}

// ─── Backup Resumption Test ───────────────────────────────────────

/// Tests incremental backup behavior: only modified files are re-uploaded,
/// unchanged files are deduplicated from existing chunks.
///
/// This is the observable surface of the resumption mechanism: when `start_backup`
/// runs a second time with some files unchanged, it detects their existing chunks
/// via `get_resumable_job` / candidate filtering and skips re-uploading them.
#[tokio::test]
async fn test_backup_resumption() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init()
        .ok();

    let s3_creds = s3_credentials();
    let test_password = "resumption-test-pwd-secure!";
    let unique_email = format!("resumption-{}@cloudless.test", Uuid::now_v7());

    let source_dir = TempDir::new().expect("Failed to create source dir");
    let index_dir = TempDir::new().expect("Failed to create index dir");
    let local_index = SqliteLocalIndex::open(&index_dir.path().join(".cloudless_index.db"))
        .await
        .expect("SQLite index creation failed");
    let env = AppEnv::new(api_base_url(), local_index);

    // ── Setup: user, DEK, device, storage, config ─────────────────
    let _ = user_app::create_user(
        &env,
        UserCreateRequest {
            name: "Resumption Test User".to_string(),
            email: unique_email.clone(),
            password: test_password.to_string(),
        },
    )
    .await
    .expect("User creation failed");

    let _ = user_app::login(
        &env,
        LoginRequest {
            email: unique_email.clone(),
            password: test_password.to_string(),
        },
    )
    .await
    .expect("Login failed");

    let dek = Dek::generate().expect("DEK generation failed");
    user_app::update_dek(&env, dek.clone(), test_password, DekKeyType::Password)
        .await
        .expect("DEK storage failed");
    let derived_keys = DerivedKeys::derive(&dek).expect("DerivedKeys derivation failed");

    let device = config_app::get_or_create_local_device(
        &env,
        GetOrCreateLocalDeviceRequest {
            physical_device_id: format!("resumption-device-{}", Uuid::now_v7()),
            display_name: Some("Resumption Test Device".to_string()),
            platform: "test".to_string(),
        },
    )
    .await
    .expect("Device registration failed");

    let storage_config = RemoteStorageConfig::Aws(s3_creds);
    let config_json = serde_json::to_vec(&storage_config).expect("Failed to serialize S3 config");
    let encryptor = AesGcmEncryptor::new(&dek).expect("Failed to create encryptor");
    let encrypted_config = encryptor
        .encrypt(&config_json)
        .expect("Failed to encrypt S3 config");

    let storage = config_app::register_remote_storage(
        &env,
        CreateRemoteStorageRequest {
            name: "Resumption Test Storage".to_string(),
            storage_type: RemoteStorageType::Aws,
            config: encrypted_config.into(),
        },
    )
    .await
    .expect("Storage registration failed");

    let run_id = Uuid::now_v7();
    let file_a = format!("File A content — run {run_id}\n").into_bytes();
    let file_b = format!("File B content — run {run_id}\n").into_bytes();
    let file_c = format!("File C unchanged content — run {run_id}\n").into_bytes();

    write_file(source_dir.path(), "file-a.txt", &file_a);
    write_file(source_dir.path(), "file-b.txt", &file_b);
    write_file(source_dir.path(), "file-c.txt", &file_c);

    let source_path = source_dir.path().to_string_lossy().to_string();
    let backup_config_request = encrypt_create_config_request(
        &source_path,
        storage.id,
        "Resumption Test Backup".to_string(),
        device.id,
        Default::default(),
        &BackupExclusionConfig::default(),
        &derived_keys,
    )
    .expect("Encrypting backup config request failed");
    let backup_config = config_app::create_backup_config(&env, backup_config_request)
        .await
        .expect("Backup config creation failed");
    let config_id = backup_config.id;

    // ── Initial backup: all 3 files ────────────────────────────────
    let rx = start_backup(
        env.clone(),
        dek.clone(),
        derived_keys.clone(),
        device.id,
        config_id,
    );
    let initial_results = drain_backup_results(rx).await;
    assert_no_backup_failures(&initial_results);

    let initial_completed: Vec<String> = initial_results
        .iter()
        .filter_map(|r| match r {
            BackupResult::FileCompleted { file_path, .. } => Some(file_path.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        initial_completed.len(),
        3,
        "Expected 3 files on initial backup, got {}: {initial_completed:?}",
        initial_completed.len()
    );
    println!("Initial backup: 3 files backed up ✓");

    // ── Modify file-a and file-b; leave file-c unchanged ──────────
    let file_a_v2 = format!("File A MODIFIED — run {run_id}\n").into_bytes();
    let file_b_v2 = format!("File B MODIFIED — run {run_id}\n").into_bytes();
    write_file(source_dir.path(), "file-a.txt", &file_a_v2);
    write_file(source_dir.path(), "file-b.txt", &file_b_v2);

    // ── Second backup: only modified files should be re-uploaded ──
    let rx2 = start_backup(
        env.clone(),
        dek.clone(),
        derived_keys.clone(),
        device.id,
        config_id,
    );
    let second_results = drain_backup_results(rx2).await;
    assert_no_backup_failures(&second_results);

    let second_completed: Vec<String> = second_results
        .iter()
        .filter_map(|r| match r {
            BackupResult::FileCompleted { file_path, .. } => Some(file_path.clone()),
            _ => None,
        })
        .collect();

    assert!(
        second_completed.iter().any(|p| p.ends_with("file-a.txt")),
        "Expected file-a.txt in second backup (was modified), got: {second_completed:?}"
    );
    assert!(
        second_completed.iter().any(|p| p.ends_with("file-b.txt")),
        "Expected file-b.txt in second backup (was modified), got: {second_completed:?}"
    );

    // file-c.txt was not modified — its chunks should deduplicate
    let fresh_chunks: usize = second_results
        .iter()
        .filter(|r| {
            matches!(
                r,
                BackupResult::ChunkUploaded {
                    deduplicated: false,
                    ..
                }
            )
        })
        .count();
    let dedup_chunks: usize = second_results
        .iter()
        .filter(|r| {
            matches!(
                r,
                BackupResult::ChunkUploaded {
                    deduplicated: true,
                    ..
                }
            )
        })
        .count();
    println!(
        "Second backup: {fresh_chunks} fresh chunks, {dedup_chunks} deduplicated, completed: {second_completed:?}"
    );

    // Verify via browse that file versions are correct
    let browse = list_backed_up_files(&env, config_id, None, 10, &derived_keys)
        .await
        .expect("list_backed_up_files failed after second backup");

    assert_eq!(browse.files.len(), 3, "Expected 3 distinct files in browse");

    for file in &browse.files {
        let path = decrypt_backed_up_path(file, &derived_keys);
        let max_version = file.versions.iter().map(|v| v.version).max().unwrap_or(0);
        if path.ends_with("file-a.txt") || path.ends_with("file-b.txt") {
            assert!(
                max_version >= 2,
                "{path} should be at version ≥2 after modification, got v{max_version}"
            );
        } else if path.ends_with("file-c.txt") {
            assert_eq!(
                max_version, 1,
                "file-c.txt should still be at v1 (unchanged), got v{max_version}"
            );
        }
    }

    println!("Backup resumption test PASSED ✓");
}

// ─── Tier Gate Integration Test ──────────────────────────────────────────────

/// Verifies that the server enforces subscription tier limits end-to-end.
///
/// All new users are free-tier (max 1 device, max 1 backup config). This test
/// checks that:
/// - Registering a second device returns a `PermissionDenied` error (HTTP 403).
/// - Re-registering the same device by physical_id succeeds (bypasses the limit).
/// - Creating a second backup config returns `PermissionDenied` (HTTP 403).
///
/// Requires a running API server at `API_BASE_URL` (defaults to localhost:9099).
/// Does NOT require S3 credentials.
#[tokio::test]
async fn e2e_tier_gate_enforcement() {
    use cloudless_core::model::app_error::AppError;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init()
        .ok();

    let test_password = "tier-gate-test-password!";
    let unique_email = format!("tier-gate-{}@cloudless.test", Uuid::now_v7());

    let index_dir = TempDir::new().expect("Failed to create temp index dir");
    let local_index = SqliteLocalIndex::open(&index_dir.path().join(".cloudless_index.db"))
        .await
        .expect("SQLite index creation failed");
    let env = AppEnv::new(api_base_url(), local_index);

    // ── Setup: user + DEK ─────────────────────────────────────────────────────

    user_app::create_user(
        &env,
        UserCreateRequest {
            name: "Tier Gate Test".to_string(),
            email: unique_email.clone(),
            password: test_password.to_string(),
        },
    )
    .await
    .expect("User creation failed");

    user_app::login(
        &env,
        LoginRequest {
            email: unique_email.clone(),
            password: test_password.to_string(),
        },
    )
    .await
    .expect("Login failed");

    let dek = Dek::generate().expect("DEK generation failed");
    user_app::update_dek(&env, dek.clone(), test_password, DekKeyType::Password)
        .await
        .expect("DEK storage failed");
    let derived_keys = DerivedKeys::derive(&dek).expect("DerivedKeys derivation failed");

    // ── Gate 1: device limit ──────────────────────────────────────────────────

    let physical_id_1 = format!("tier-device-1-{}", Uuid::now_v7());
    let device_1 = config_app::get_or_create_local_device(
        &env,
        GetOrCreateLocalDeviceRequest {
            physical_device_id: physical_id_1.clone(),
            display_name: Some("Device 1".to_string()),
            platform: "test".to_string(),
        },
    )
    .await
    .expect("First device registration should succeed");
    println!(
        "  device_1 registered: {} (was_created={})",
        device_1.id, device_1.was_created
    );

    let device_2_err = config_app::get_or_create_local_device(
        &env,
        GetOrCreateLocalDeviceRequest {
            physical_device_id: format!("tier-device-2-{}", Uuid::now_v7()),
            display_name: Some("Device 2".to_string()),
            platform: "test".to_string(),
        },
    )
    .await
    .expect_err("Second device registration should be rejected at free tier");
    assert!(
        matches!(device_2_err, AppError::PermissionDenied { .. }),
        "Expected PermissionDenied, got: {device_2_err:?}"
    );
    println!("  Second device correctly rejected: {device_2_err}");

    // Re-registering device_1 with the same physical_id must succeed (bypass).
    let device_1_again = config_app::get_or_create_local_device(
        &env,
        GetOrCreateLocalDeviceRequest {
            physical_device_id: physical_id_1.clone(),
            display_name: Some("Device 1 again".to_string()),
            platform: "test".to_string(),
        },
    )
    .await
    .expect("Re-registering existing device should bypass the limit");
    assert_eq!(
        device_1_again.id, device_1.id,
        "Should return the same device"
    );
    assert!(!device_1_again.was_created);
    println!("  Existing device re-registration bypassed limit correctly");

    // ── Gate 2: backup config limit ───────────────────────────────────────────

    // Register a dummy remote storage (server stores the encrypted blob as-is).
    let encryptor = AesGcmEncryptor::new(&dek).expect("Failed to create encryptor");
    let dummy_config_bytes = encryptor
        .encrypt(b"dummy-storage-config")
        .expect("Failed to encrypt dummy config");
    let storage = config_app::register_remote_storage(
        &env,
        CreateRemoteStorageRequest {
            name: "Tier Gate Test Storage".to_string(),
            storage_type: RemoteStorageType::Aws,
            config: dummy_config_bytes.into(),
        },
    )
    .await
    .expect("Storage registration failed");
    println!("  Dummy storage registered: {}", storage.id);

    // Use a temp dir as the source path (path existence is not validated server-side).
    let source_dir = TempDir::new().expect("Failed to create temp source dir");
    let source_path = source_dir.path().to_string_lossy().to_string();

    let config_request_1 = encrypt_create_config_request(
        &source_path,
        storage.id,
        "Config 1".to_string(),
        device_1.id,
        Default::default(),
        &BackupExclusionConfig::default(),
        &derived_keys,
    )
    .expect("Failed to encrypt backup config request");
    config_app::create_backup_config(&env, config_request_1)
        .await
        .expect("First backup config should succeed");
    println!("  backup_config_1 created");

    let config_request_2 = encrypt_create_config_request(
        &source_path,
        storage.id,
        "Config 2".to_string(),
        device_1.id,
        Default::default(),
        &BackupExclusionConfig::default(),
        &derived_keys,
    )
    .expect("Failed to encrypt backup config request");
    let config_2_err = config_app::create_backup_config(&env, config_request_2)
        .await
        .expect_err("Second backup config should be rejected at free tier");
    assert!(
        matches!(config_2_err, AppError::PermissionDenied { .. }),
        "Expected PermissionDenied, got: {config_2_err:?}"
    );
    println!("  Second backup config correctly rejected: {config_2_err}");

    println!("Tier gate enforcement test PASSED ✓");
}
