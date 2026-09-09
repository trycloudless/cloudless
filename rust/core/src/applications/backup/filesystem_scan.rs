/// Filesystem scanner that syncs the local file system state into the SQLite index.
///
/// Walks the source directory, compares each file's mtime/size against the local
/// index, and updates the index accordingly. Does not communicate with the server.
///
/// Directories and files matching the compiled exclusion glob-set are skipped via
/// `filter_entry`, so excluded directories are never traversed.
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use globset::{GlobSet, GlobSetBuilder};
use tracing::{info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

use api_types::backup_config::{
    BackupExclusionConfig, BackupExclusionEntry, BackupExclusionPreset, BackupExclusionRuleKind,
};

use crate::{
    applications::backup::env::BackupEnv,
    domain::{derived_keys::DerivedKeys, metadata_crypto::encrypt_file_path},
    model::{app_error::AppError, base::AppResult, local_index::LocalIndexEntry},
    ports::local_index::LocalIndexPort,
};

// ── Errors and result types ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScanError {
    pub file_name: String,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct FilesystemScanResult {
    pub new_files: usize,
    pub modified_files: usize,
    pub unchanged_files: usize,
    pub removed_files: u64,
    pub excluded_files: usize,
    pub excluded_dirs: usize,
    pub errors: Vec<ScanError>,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub follow_symlinks: bool,
    pub exclusions: BackupExclusionConfig,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            follow_symlinks: true,
            exclusions: BackupExclusionConfig::default(),
        }
    }
}

// ── Exclusion helpers ─────────────────────────────────────────────────────────

/// Returns the static glob patterns for a preset.
fn preset_globs(preset: &BackupExclusionPreset) -> &'static [&'static str] {
    match preset {
        BackupExclusionPreset::TemporaryAndCache => &[
            "**/*.tmp",
            "**/*.temp",
            "**/*.cache",
            "**/.cache/**",
            "**/tmp/**",
            "**/temp/**",
        ],
        BackupExclusionPreset::InstallersAndArchives => &[
            "**/*.dmg",
            "**/*.pkg",
            "**/*.msi",
            "**/*.exe",
            "**/*.deb",
            "**/*.rpm",
            "**/*.AppImage",
            "**/*.zip",
            "**/*.tar",
            "**/*.tar.gz",
            "**/*.tgz",
            "**/*.7z",
            "**/*.rar",
            "**/*.iso",
        ],
        BackupExclusionPreset::DeveloperDependencies => &[
            "**/node_modules/**",
            "**/target/**",
            "**/dist/**",
            "**/build/**",
            "**/.venv/**",
            "**/venv/**",
            "**/.gradle/**",
        ],
        BackupExclusionPreset::OsMetadata => &[
            ".DS_Store",
            "**/.DS_Store",
            "**/Thumbs.db",
            "**/desktop.ini",
        ],
    }
}

/// Converts a single `BackupExclusionEntry` into its glob pattern(s).
fn entry_to_globs(entry: &BackupExclusionEntry) -> Vec<String> {
    match entry {
        BackupExclusionEntry::Preset(p) => preset_globs(p).iter().map(|s| s.to_string()).collect(),
        BackupExclusionEntry::Custom(r) => {
            if !r.enabled || r.value.trim().is_empty() {
                return vec![];
            }
            let v = r.value.trim();
            let glob = match r.kind {
                BackupExclusionRuleKind::FileExtension => {
                    let ext = v.trim_start_matches('.');
                    format!("**/*.{ext}")
                }
                BackupExclusionRuleKind::FolderName => format!("**/{v}/**"),
                BackupExclusionRuleKind::FileOrFolderName => format!("**/{v}"),
                BackupExclusionRuleKind::PathContains => format!("**/*{v}*/**"),
            };
            vec![glob]
        }
        BackupExclusionEntry::Glob(g) => vec![g.clone()],
    }
}

/// Builds the full list of glob strings from a `BackupExclusionConfig`.
pub fn build_exclusion_patterns(config: &BackupExclusionConfig) -> Vec<String> {
    config.entries.iter().flat_map(entry_to_globs).collect()
}

/// A compiled glob-set together with the source patterns for matched-rule reporting.
pub struct CompiledExclusions {
    matcher: GlobSet,
    /// Parallel to the globs compiled into `matcher`.
    patterns: Vec<String>,
}

impl CompiledExclusions {
    pub fn is_empty(&self) -> bool {
        self.matcher.is_empty()
    }

    /// Returns `true` if the relative path matches any exclusion pattern.
    /// Directories are matched both with and without a trailing `/`.
    pub fn is_match(&self, rel_path: &str, is_dir: bool) -> bool {
        if self.matcher.is_match(rel_path) {
            return true;
        }
        if is_dir {
            self.matcher.is_match(format!("{rel_path}/"))
        } else {
            false
        }
    }

    /// Returns the first matching glob pattern for the given relative path.
    pub fn matched_rule(&self, rel_path: &str) -> Option<&str> {
        let hits = self.matcher.matches(rel_path);
        if let Some(&idx) = hits.first() {
            return Some(&self.patterns[idx]);
        }
        None
    }
}

/// Compiles a `BackupExclusionConfig` into a `GlobSet`.
/// Returns an error if any pattern is syntactically invalid.
pub fn compile_exclusions(config: &BackupExclusionConfig) -> AppResult<CompiledExclusions> {
    let patterns = build_exclusion_patterns(config);
    if patterns.is_empty() {
        return Ok(CompiledExclusions {
            matcher: GlobSet::empty(),
            patterns: vec![],
        });
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in &patterns {
        let glob = globset::Glob::new(pattern).map_err(|e| AppError::Internal {
            message: format!("Invalid exclusion glob pattern `{pattern}`: {e}"),
            source: None,
        })?;
        builder.add(glob);
    }
    let matcher = builder.build().map_err(|e| AppError::Internal {
        message: format!("Failed to build exclusion GlobSet: {e}"),
        source: None,
    })?;

    Ok(CompiledExclusions { matcher, patterns })
}

/// Returns the path of `entry` relative to `source`, with `/` separators.
pub fn normalize_relative_path(source: &Path, entry: &Path) -> String {
    entry
        .strip_prefix(source)
        .unwrap_or(entry)
        .to_string_lossy()
        .replace('\\', "/")
}

// ── Main scan function ────────────────────────────────────────────────────────

/// Walks the filesystem and updates the local SQLite index.
///
/// For each file found:
/// - If not in the index → new file, encrypt name + insert
/// - If in the index but mtime or size changed → modified, re-encrypt name + update
/// - If unchanged → skip
///
/// Files and directories matching the compiled exclusion patterns are skipped via
/// `filter_entry`, so excluded directories are never descended into.
///
/// After scanning, removes entries from the index for files that no longer exist
/// on disk (including files that became excluded — they are removed from future
/// backup candidates).
#[tracing::instrument(skip(env, derived_keys, options), fields(source = %source_directory))]
pub async fn sync_filesystem_to_index<E: BackupEnv>(
    env: &E,
    source_directory: &str,
    derived_keys: &DerivedKeys,
    options: &ScanOptions,
    config_id: Uuid,
) -> AppResult<FilesystemScanResult> {
    let local_index = env.local_index();
    let compiled = compile_exclusions(&options.exclusions)?;
    let source_path = Path::new(source_directory).to_path_buf();

    let mut errors: Vec<ScanError> = Vec::new();
    let mut existing_paths: Vec<String> = Vec::new();
    let mut new_files = 0usize;
    let mut modified_files = 0usize;
    let mut unchanged_files = 0usize;

    // AtomicUsize so the counter references are Send (required by the async future).
    let excluded_files_counter = AtomicUsize::new(0);
    let excluded_dirs_counter = AtomicUsize::new(0);

    let iter = WalkDir::new(source_directory)
        .follow_links(options.follow_symlinks)
        .into_iter()
        .filter_entry(|entry| {
            // Never exclude the root itself.
            if entry.depth() == 0 {
                return true;
            }
            if compiled.is_empty() {
                return true;
            }
            let rel = normalize_relative_path(&source_path, entry.path());
            let is_dir = entry.file_type().is_dir();
            if compiled.is_match(&rel, is_dir) {
                if is_dir {
                    excluded_dirs_counter.fetch_add(1, Ordering::Relaxed);
                } else {
                    excluded_files_counter.fetch_add(1, Ordering::Relaxed);
                }
                return false;
            }
            true
        });

    for result in iter {
        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                let path = e
                    .path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                errors.push(ScanError {
                    file_name: path,
                    error: format!("WalkDir error: {e}"),
                });
                continue;
            }
        };

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                errors.push(ScanError {
                    file_name: entry.path().to_string_lossy().to_string(),
                    error: format!("Error reading metadata: {e}"),
                });
                continue;
            }
        };

        if !metadata.is_file() {
            continue;
        }

        let path = entry.path().to_string_lossy().to_string();
        let size = metadata.len() as i64;
        let mtime = metadata
            .modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| {
                warn!("File has no modified time, falling back to current time");
                Utc::now()
            });

        existing_paths.push(path.clone());

        let existing = local_index.get_by_path(config_id, &path).await?;

        match existing {
            Some(ref existing_entry)
                if existing_entry.size == size && existing_entry.mtime == mtime =>
            {
                unchanged_files += 1;
            }
            Some(_) => {
                let encrypted =
                    encrypt_file_path(&path, &derived_keys.metadata_key, &derived_keys.index_key)?;
                let updated = LocalIndexEntry {
                    backup_config_id: config_id,
                    path: path.clone(),
                    size,
                    mtime,
                    content_hash: None,
                    encrypted_name: encrypted.encrypted_name,
                    encrypted_name_nonce: encrypted.nonce,
                    blind_index: encrypted.blind_index,
                    remote_file_id: None,
                    last_backed_up_version: None,
                    synced_at: None,
                };
                local_index.upsert_file(&updated).await?;
                modified_files += 1;
            }
            None => {
                let encrypted =
                    encrypt_file_path(&path, &derived_keys.metadata_key, &derived_keys.index_key)?;
                let new_entry = LocalIndexEntry {
                    backup_config_id: config_id,
                    path: path.clone(),
                    size,
                    mtime,
                    content_hash: None,
                    encrypted_name: encrypted.encrypted_name,
                    encrypted_name_nonce: encrypted.nonce,
                    blind_index: encrypted.blind_index,
                    remote_file_id: None,
                    last_backed_up_version: None,
                    synced_at: None,
                };
                local_index.upsert_file(&new_entry).await?;
                new_files += 1;
            }
        }
    }

    let excluded_files = excluded_files_counter.load(Ordering::Relaxed);
    let excluded_dirs = excluded_dirs_counter.load(Ordering::Relaxed);

    // Remove entries for files that no longer exist on disk, including files
    // that became excluded — they should disappear from future backup candidates.
    let removed_files = local_index
        .remove_missing(config_id, source_directory, &existing_paths)
        .await?;
    if removed_files > 0 {
        info!(removed = removed_files, "Removed missing files from index");
    }

    let result = FilesystemScanResult {
        new_files,
        modified_files,
        unchanged_files,
        removed_files,
        excluded_files,
        excluded_dirs,
        errors,
    };

    info!(
        new = result.new_files,
        modified = result.modified_files,
        unchanged = result.unchanged_files,
        removed = result.removed_files,
        excluded_files = result.excluded_files,
        excluded_dirs = result.excluded_dirs,
        errors = result.errors.len(),
        "Filesystem scan completed"
    );

    Ok(result)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_local_index::SqliteLocalIndex;
    use crate::domain::dek::Dek;
    use crate::ports::api::{
        ApiResult, backup_config_api_port::BackupConfigApiPort,
        backup_job_api_port::BackupJobApiPort, chunk_api_port::ChunkApiPort,
        local_device_api_port::LocalDeviceApiPort,
        remote_file_version_api_port::RemoteFileVersionApiPort,
        remote_storage_api_port::RemoteStorageApiPort, user_api_port::UserApiPort,
    };
    use api_types::{
        auth::*,
        backup_config::*,
        backup_job::*,
        chunk::*,
        email_verification::{
            ResendVerificationRequest, ResendVerificationResponse, VerifyEmailRequest,
            VerifyEmailResponse,
        },
        local_device::*,
        remote_file_version::*,
        remote_storage::*,
        restore_file_info::*,
        user::*,
    };
    use async_trait::async_trait;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;
    use zeroize::Zeroizing;

    fn test_keys() -> DerivedKeys {
        DerivedKeys::derive(&Dek {
            key: Zeroizing::new([42u8; 32]),
        })
        .unwrap()
    }

    fn test_config_id() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn exclusion_config_with_entries(entries: Vec<BackupExclusionEntry>) -> BackupExclusionConfig {
        BackupExclusionConfig { entries }
    }

    // ── Minimal stubs ─────────────────────────────────────────────────────────

    #[derive(Clone)]
    struct StubUserApi;
    #[async_trait]
    impl UserApiPort for StubUserApi {
        async fn create_user(&self, _: UserCreateRequest) -> ApiResult<UserCreateResponse> {
            unimplemented!()
        }
        async fn login(&self, _: LoginRequest) -> ApiResult<LoginResponse> {
            unimplemented!()
        }
        async fn get_me(&self, _: &str) -> ApiResult<UserInfo> {
            unimplemented!()
        }
        async fn list_all_users(
            &self,
            _: &str,
            _: u32,
            _: u32,
        ) -> ApiResult<AdminUserListResponse> {
            unimplemented!()
        }
        async fn verify_email(&self, _: VerifyEmailRequest) -> ApiResult<VerifyEmailResponse> {
            unimplemented!()
        }
        async fn resend_verification(
            &self,
            _: ResendVerificationRequest,
        ) -> ApiResult<ResendVerificationResponse> {
            unimplemented!()
        }

        async fn refresh(&self, _: &str) -> ApiResult<Tokens> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct StubBackupConfigApi;
    #[async_trait]
    impl BackupConfigApiPort for StubBackupConfigApi {
        async fn create(
            &self,
            _: CreateBackupConfigRequest,
        ) -> ApiResult<CreateBackupConfigResponse> {
            unimplemented!()
        }
        async fn list_with_remote_storage(
            &self,
            _: ListBackupConfigWithRemoteStorageRequest,
        ) -> ApiResult<ListBackupConfigWithRemoteStorageResponse> {
            unimplemented!()
        }
        async fn get_by_id(&self, _: GetBackupConfigRequest) -> ApiResult<GetBackupConfigResponse> {
            unimplemented!()
        }
        async fn list_all(&self) -> ApiResult<ListAllBackupConfigsResponse> {
            unimplemented!()
        }
        async fn toggle_active(
            &self,
            _: ToggleBackupConfigRequest,
        ) -> ApiResult<ToggleBackupConfigResponse> {
            unimplemented!()
        }
        async fn rename(
            &self,
            _: RenameBackupConfigRequest,
        ) -> ApiResult<RenameBackupConfigResponse> {
            unimplemented!()
        }
        async fn update_cleanup_type(
            &self,
            _: UpdateCleanupTypeRequest,
        ) -> ApiResult<UpdateCleanupTypeResponse> {
            unimplemented!()
        }
        async fn update_exclusion_config(
            &self,
            _: UpdateExclusionConfigRequest,
        ) -> ApiResult<UpdateExclusionConfigResponse> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct StubLocalDeviceApi;
    #[async_trait]
    impl LocalDeviceApiPort for StubLocalDeviceApi {
        async fn create(
            &self,
            _: CreateLocalDeviceRequest,
        ) -> ApiResult<CreateLocalDeviceResponse> {
            unimplemented!()
        }
        async fn get_by_physical_id(
            &self,
            _: GetLocalDeviceByPhysicalIdRequest,
        ) -> ApiResult<GetLocalDeviceByPhysicalIdResponse> {
            unimplemented!()
        }
        async fn get_or_create(
            &self,
            _: GetOrCreateLocalDeviceRequest,
        ) -> ApiResult<GetOrCreateLocalDeviceResponse> {
            unimplemented!()
        }
        async fn list_by_platform(
            &self,
            _: ListDevicesByPlatformRequest,
        ) -> ApiResult<ListDevicesByPlatformResponse> {
            unimplemented!()
        }
        async fn list_all(&self) -> ApiResult<ListAllDevicesResponse> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct StubRemoteStorageApi;
    #[async_trait]
    impl RemoteStorageApiPort for StubRemoteStorageApi {
        async fn create(
            &self,
            _: CreateRemoteStorageRequest,
        ) -> ApiResult<CreateRemoteStorageResponse> {
            unimplemented!()
        }
        async fn get_by_id(
            &self,
            _: GetRemoteStorageRequest,
        ) -> ApiResult<GetRemoteStorageResponse> {
            unimplemented!()
        }
        async fn list(&self) -> ApiResult<ListRemoteStoragesResponse> {
            unimplemented!()
        }
        async fn update_status(
            &self,
            _: UpdateRemoteStorageStatusRequest,
        ) -> ApiResult<UpdateRemoteStorageStatusResponse> {
            unimplemented!()
        }
        async fn reauth(
            &self,
            _: ReauthRemoteStorageRequest,
        ) -> ApiResult<ReauthRemoteStorageResponse> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct StubRemoteFileVersionApi;
    #[async_trait]
    impl RemoteFileVersionApiPort for StubRemoteFileVersionApi {
        async fn create(
            &self,
            _: CreateFileVersionRequest,
        ) -> ApiResult<CreateFileVersionResponse> {
            unimplemented!()
        }
        async fn update_status(&self, _: UpdateFileVersionStatusRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn list_backed_up_files(
            &self,
            _: ListBackedUpFilesRequest,
        ) -> ApiResult<ListBackedUpFilesResponse> {
            unimplemented!()
        }
        async fn list_all_versions(
            &self,
            _: ListAllVersionsRequest,
        ) -> ApiResult<ListAllVersionsResponse> {
            unimplemented!()
        }
        async fn move_to_bin(&self, _: MoveVersionToBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn move_all_to_bin(&self, _: MoveAllVersionsToBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn restore_from_bin(&self, _: RestoreVersionFromBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn list_bin_versions(
            &self,
            _: ListBinVersionsRequest,
        ) -> ApiResult<ListBinVersionsResponse> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct StubChunkApi;
    #[async_trait]
    impl ChunkApiPort for StubChunkApi {
        async fn create(&self, _: CreateChunkRequest) -> ApiResult<CreateChunkResponse> {
            unimplemented!()
        }
        async fn get_chunks_for_version(
            &self,
            _: GetFileVersionChunksRequest,
        ) -> ApiResult<GetFileVersionChunksResponse> {
            unimplemented!()
        }
        async fn update_storage_meta(&self, _: UpdateChunkStorageMetaRequest) -> ApiResult<()> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct StubBackupJobApi;
    #[async_trait]
    impl BackupJobApiPort for StubBackupJobApi {
        async fn create_job(
            &self,
            _: CreateBackupJobRequest,
        ) -> ApiResult<CreateBackupJobResponse> {
            unimplemented!()
        }
        async fn create_job_file(
            &self,
            _: CreateBackupJobFileRequest,
        ) -> ApiResult<CreateBackupJobFileResponse> {
            unimplemented!()
        }
        async fn complete_job_file(&self, _: CompleteBackupJobFileRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn complete_job(&self, _: CompleteBackupJobRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn list_jobs(&self, _: ListBackupJobsRequest) -> ApiResult<ListBackupJobsResponse> {
            unimplemented!()
        }
        async fn get_job_detail(
            &self,
            _: GetBackupJobDetailRequest,
        ) -> ApiResult<GetBackupJobDetailResponse> {
            unimplemented!()
        }
        async fn get_latest_job(&self) -> ApiResult<GetLatestBackupJobResponse> {
            unimplemented!()
        }
        async fn get_resumable_job(
            &self,
            _: GetResumableBackupJobRequest,
        ) -> ApiResult<GetResumableBackupJobResponse> {
            unimplemented!()
        }
        async fn log_cleanup_files(&self, _: LogCleanupFilesRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn abandon_stale_jobs(&self) -> ApiResult<AbandonStaleJobsResponse> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct TestEnv {
        local_index: SqliteLocalIndex,
    }

    impl BackupEnv for TestEnv {
        type UserApi = StubUserApi;
        type BackupConfigApi = StubBackupConfigApi;
        type LocalIndex = SqliteLocalIndex;
        type LocalDeviceApi = StubLocalDeviceApi;
        type RemoteStorageApi = StubRemoteStorageApi;
        type RemoteFileVersionApi = StubRemoteFileVersionApi;
        type ChunkApi = StubChunkApi;
        type BackupJobApi = StubBackupJobApi;

        fn clone_env(&self) -> Self {
            self.clone()
        }
        fn user_api(&self) -> &Self::UserApi {
            &StubUserApi
        }
        fn backup_config_api(&self) -> &Self::BackupConfigApi {
            &StubBackupConfigApi
        }
        fn local_index(&self) -> &Self::LocalIndex {
            &self.local_index
        }
        fn local_device_api(&self) -> &Self::LocalDeviceApi {
            &StubLocalDeviceApi
        }
        fn remote_storage_api(&self) -> &Self::RemoteStorageApi {
            &StubRemoteStorageApi
        }
        fn remote_file_version_api(&self) -> &Self::RemoteFileVersionApi {
            &StubRemoteFileVersionApi
        }
        fn chunk_api(&self) -> &Self::ChunkApi {
            &StubChunkApi
        }
        fn backup_job_api(&self) -> &Self::BackupJobApi {
            &StubBackupJobApi
        }
    }

    fn make_env() -> TestEnv {
        TestEnv {
            local_index: futures::executor::block_on(SqliteLocalIndex::open_in_memory()).unwrap(),
        }
    }

    // ── Existing tests (unchanged behaviour) ──────────────────────────────────

    #[tokio::test]
    async fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &ScanOptions::default(),
            cid,
        )
        .await
        .unwrap();

        assert_eq!(result.new_files, 0);
        assert_eq!(result.modified_files, 0);
        assert_eq!(result.unchanged_files, 0);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_scan_new_files() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("a.txt"))
            .unwrap()
            .write_all(b"hello")
            .unwrap();
        File::create(temp_dir.path().join("b.txt"))
            .unwrap()
            .write_all(b"world")
            .unwrap();

        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &ScanOptions::default(),
            cid,
        )
        .await
        .unwrap();

        assert_eq!(result.new_files, 2);
        assert_eq!(result.modified_files, 0);

        let all = env.local_index().list_all(cid).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_scan_unchanged_files() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("a.txt"))
            .unwrap()
            .write_all(b"hello")
            .unwrap();

        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &ScanOptions::default(),
            cid,
        )
        .await
        .unwrap();

        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &ScanOptions::default(),
            cid,
        )
        .await
        .unwrap();

        assert_eq!(result.new_files, 0);
        assert_eq!(result.unchanged_files, 1);
        assert_eq!(result.modified_files, 0);
    }

    #[tokio::test]
    async fn test_scan_removes_deleted_files() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("to_delete.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"temp")
            .unwrap();

        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &ScanOptions::default(),
            cid,
        )
        .await
        .unwrap();

        assert_eq!(env.local_index().list_all(cid).await.unwrap().len(), 1);

        std::fs::remove_file(&file_path).unwrap();

        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &ScanOptions::default(),
            cid,
        )
        .await
        .unwrap();

        assert_eq!(result.removed_files, 1);
        assert!(env.local_index().list_all(cid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_scan_nonexistent_directory() {
        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        let result = sync_filesystem_to_index(
            &env,
            "/nonexistent/path",
            &keys,
            &ScanOptions::default(),
            cid,
        )
        .await
        .unwrap();

        assert_eq!(result.new_files, 0);
        assert!(!result.errors.is_empty());
    }

    // ── Exclusion tests ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_scan_excludes_by_file_extension() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("a.txt"))
            .unwrap()
            .write_all(b"keep")
            .unwrap();
        File::create(temp_dir.path().join("b.dmg"))
            .unwrap()
            .write_all(b"exclude")
            .unwrap();

        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        let options = ScanOptions {
            follow_symlinks: true,
            exclusions: exclusion_config_with_entries(vec![BackupExclusionEntry::Custom(
                BackupExclusionRule {
                    id: "r1".to_string(),
                    enabled: true,
                    kind: BackupExclusionRuleKind::FileExtension,
                    value: "dmg".to_string(),
                },
            )]),
        };

        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &options,
            cid,
        )
        .await
        .unwrap();

        assert_eq!(result.new_files, 1, "only a.txt should be indexed");
        assert_eq!(
            result.excluded_files, 1,
            "b.dmg should be counted as excluded"
        );

        let all = env.local_index().list_all(cid).await.unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].path.ends_with("a.txt"));
    }

    #[tokio::test]
    async fn test_scan_excludes_folder() {
        let temp_dir = TempDir::new().unwrap();
        let nm = temp_dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        File::create(temp_dir.path().join("main.js"))
            .unwrap()
            .write_all(b"app")
            .unwrap();
        File::create(nm.join("lib.js"))
            .unwrap()
            .write_all(b"dep")
            .unwrap();

        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        let options = ScanOptions {
            follow_symlinks: true,
            exclusions: exclusion_config_with_entries(vec![BackupExclusionEntry::Custom(
                BackupExclusionRule {
                    id: "r1".to_string(),
                    enabled: true,
                    kind: BackupExclusionRuleKind::FolderName,
                    value: "node_modules".to_string(),
                },
            )]),
        };

        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &options,
            cid,
        )
        .await
        .unwrap();

        assert_eq!(result.new_files, 1, "only main.js should be indexed");
        assert_eq!(
            result.excluded_dirs, 1,
            "node_modules dir should be excluded"
        );

        let all = env.local_index().list_all(cid).await.unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].path.ends_with("main.js"));
    }

    #[tokio::test]
    async fn test_scan_excluded_file_pruned_from_index() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("archive.zip"))
            .unwrap()
            .write_all(b"data")
            .unwrap();
        File::create(temp_dir.path().join("notes.txt"))
            .unwrap()
            .write_all(b"text")
            .unwrap();

        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        // First scan — no exclusions, both files indexed.
        sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &ScanOptions::default(),
            cid,
        )
        .await
        .unwrap();
        assert_eq!(env.local_index().list_all(cid).await.unwrap().len(), 2);

        // Second scan — exclude .zip; archive.zip removed from index via remove_missing.
        let options = ScanOptions {
            follow_symlinks: true,
            exclusions: exclusion_config_with_entries(vec![BackupExclusionEntry::Custom(
                BackupExclusionRule {
                    id: "r1".to_string(),
                    enabled: true,
                    kind: BackupExclusionRuleKind::FileExtension,
                    value: "zip".to_string(),
                },
            )]),
        };
        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &options,
            cid,
        )
        .await
        .unwrap();

        assert_eq!(
            result.removed_files, 1,
            "archive.zip should be removed from index"
        );
        let remaining = env.local_index().list_all(cid).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(remaining[0].path.ends_with("notes.txt"));
    }

    #[tokio::test]
    async fn test_compile_exclusions_invalid_glob() {
        let config =
            exclusion_config_with_entries(vec![BackupExclusionEntry::Glob("[invalid".to_string())]);
        let result = compile_exclusions(&config);
        assert!(result.is_err(), "invalid glob should return an error");
    }

    #[tokio::test]
    async fn test_scan_with_preset_os_metadata() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join(".DS_Store"))
            .unwrap()
            .write_all(b"mac")
            .unwrap();
        File::create(temp_dir.path().join("document.txt"))
            .unwrap()
            .write_all(b"text")
            .unwrap();

        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        let options = ScanOptions {
            follow_symlinks: true,
            exclusions: exclusion_config_with_entries(vec![BackupExclusionEntry::Preset(
                BackupExclusionPreset::OsMetadata,
            )]),
        };

        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &options,
            cid,
        )
        .await
        .unwrap();

        assert_eq!(result.new_files, 1, "only document.txt should be indexed");
        let all = env.local_index().list_all(cid).await.unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].path.ends_with("document.txt"));
    }

    #[tokio::test]
    async fn test_disabled_rule_does_not_exclude() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("file.dmg"))
            .unwrap()
            .write_all(b"data")
            .unwrap();

        let env = make_env();
        let keys = test_keys();
        let cid = test_config_id();

        let options = ScanOptions {
            follow_symlinks: true,
            exclusions: exclusion_config_with_entries(vec![BackupExclusionEntry::Custom(
                BackupExclusionRule {
                    id: "r1".to_string(),
                    enabled: false, // disabled
                    kind: BackupExclusionRuleKind::FileExtension,
                    value: "dmg".to_string(),
                },
            )]),
        };

        let result = sync_filesystem_to_index(
            &env,
            temp_dir.path().to_str().unwrap(),
            &keys,
            &options,
            cid,
        )
        .await
        .unwrap();

        assert_eq!(
            result.new_files, 1,
            "disabled rule should not exclude file.dmg"
        );
    }
}
