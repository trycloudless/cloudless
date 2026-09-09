# Functional Test Cases

> Last updated: 2026-05-16 (added get_checkout_policy unit tests for checkout policy gate; updated coverage matrix for policy consent and checkout rows)
> All test suites listed by layer and file. Update this file whenever tests are added or removed.

---

## Supported Feature Coverage Matrix

This section maps supported product features to the tests that currently protect them. Use it as the first check when deciding whether a feature is ready, whether a refactor is covered, or where new regression tests should be added.

| Supported feature | Coverage status | Primary coverage |
|---|---:|---|
| Account signup and login | Covered | `login.e2e.ts`, `signup.e2e.ts`, `signup-wizard.e2e.ts`, `rust/core/src/applications/user/application.rs` |
| Email verification flow | Partial | Tauri commands and API application exist; signup E2E relies on test auto-verification |
| Policy consent and policy skipping | Covered | `rust/api_server/src/core/policy/application.rs` (create, update, list, version CRUD, pending, accept, skip, checkout gate), `rust/api_server/src/infra/psql/pg_policy_repo.rs`; no UI E2E |
| Encryption password setup and unlock | Covered | `signup.e2e.ts`, `signup-wizard.e2e.ts`, `navigation.e2e.ts`, user application tests |
| Recovery key export and recovery unlock | Partial | Settings E2E opens export modal; core recovery key and recovery unlock tests cover cryptography and app flow |
| Encryption password change | Partial | Settings E2E opens dialog; user application tests cover password rotation behavior |
| Device registration and device limits | Covered | signup wizard E2E, local storage wizard E2E, `e2e_tier_gate_enforcement`, local device unit tests |
| Mobile device resolution and confirmation | Not covered | Tauri commands exist; no automated tests listed |
| Remote storage registration | Covered | signup wizard E2E, config application tests |
| AWS S3 storage | Covered | full Rust backup and restore integration flow uses S3 when configured; S3 connection is exposed through Tauri |
| Local filesystem storage | Covered | `local-storage-wizard.e2e.ts`, local filesystem adapter tests |
| Google Drive storage and reauth | Not covered | Tauri commands and adapter exist; OAuth flow is not automated |
| Backup config creation | Covered | signup wizard E2E, config application tests, API backup config limit tests |
| Backup config rename | Covered | `backup-config-management.e2e.ts` |
| Backup config enable and disable | Covered | `backup-config-management.e2e.ts`, config application tests |
| Backup cleanup policy editing | Covered | `backup-config-management.e2e.ts`, config application tests |
| Manual backup from dashboard | Covered | `backup.e2e.ts`, local storage wizard E2E |
| Backup page run action | Covered | `backup.e2e.ts` |
| Incremental backup and deduplication | Covered | `e2e_full_backup_and_restore_flow`, `test_backup_resumption`, file backup unit tests |
| Backup resumption and resumable job lookup | Partial | `test_backup_resumption` verifies changed-file behavior; resumable job APIs exist |
| Backup reports and job details | Covered | `backup.e2e.ts`, backup job API application functions |
| Display-friendly backup status labels | Covered | `backup.e2e.ts`, `rust/api_types/src/backup_job.rs` |
| System file filtering | Covered | backup E2E, files E2E, restore report E2E, `rust/api_types/src/file_filter.rs` |
| File browser and backed-up file list | Covered | `files.e2e.ts`, backup browse unit tests |
| File search | Covered | `files.e2e.ts` |
| File version expansion | Covered | `files.e2e.ts`, `restore.e2e.ts` |
| Bulk file selection | Covered | `files.e2e.ts` |
| Move backed-up versions to bin | Covered | `files.e2e.ts`; API/core functions exist for single and bulk move |
| Restore version from bin | Partial | API/core functions exist; no UI E2E listed |
| Bin listing | Partial | API/core functions exist; UI E2E only validates Bin tab presence in file view |
| Single file restore | Covered | `restore.e2e.ts`, `e2e_full_backup_and_restore_flow`, restore file unit tests |
| Folder restore | Covered | `e2e_full_backup_and_restore_flow` |
| Restore to original path | Covered | restore job unit tests, Rust integration flow |
| Restore to downloads folder | Covered | restore modal E2E validates option; restore job unit tests validate path resolution |
| Restore to custom path | Covered | restore modal E2E validates option; restore job unit tests validate path resolution |
| Restore reports and restore job detail | Covered | `restore.e2e.ts`, backup restore-report E2E, restore job API functions |
| Local index rebuild from server | Covered | `e2e_full_backup_and_restore_flow`, backup recovery unit tests |
| Dashboard stats | Covered | `dashboard.e2e.ts`, Rust integration stats validation, dashboard application tests |
| Storage usage and non-zero metrics | Covered | `settings.e2e.ts`, dashboard application tests |
| Auto-backup settings UI | Partial | Settings E2E validates presence; Tauri commands exist for enable, disable, and read |
| Backup scheduler events | Partial | Scheduler unit tests cover defaults and event serialization |
| Retention settings | Covered | `settings.e2e.ts`, GC application functions |
| Garbage collection | Covered | `settings.e2e.ts`, Rust integration GC phase, cleanup unit tests |
| Subscription tier display | Covered | `settings.e2e.ts`, subscription API type tests |
| Subscription tier limits | Covered | `e2e_tier_gate_enforcement`, API local device and backup config unit tests, subscription type tests |
| Checkout, billing portal, and webhooks | Partial | DoDo webhook unit tests and payment application tests; checkout policy gate covered by `get_checkout_policy` unit tests; no Stripe/Dodo checkout or portal E2E |
| Subscription reconciliation | Covered | payment application reconciliation unit tests |
| Theme preferences | Partial | Settings E2E validates UI presence |
| Lock, unlock, and sign out navigation | Covered | `navigation.e2e.ts` |
| Folder picker and open file/path commands | Not covered | Tauri commands exist; no automated tests listed |
| Biometric unlock | Not covered | Tauri commands exist; no automated tests listed |
| Desktop update check and install | Not covered | Tauri commands exist; no automated tests listed |
| Public website home, pricing, download, and auth pages | Partial | Website pages exist; no automated website route E2E listed in this document |
| Blog publishing and blog rendering | Partial | Website blog pages and blog API exist; markdown renderer has unit tests |
| Legal policy pages and admin policy management | Partial | Policy API tests exist; no public website E2E listed |
| Email template admin | Not covered | API and website admin pages exist; no automated tests listed |
| Media upload for blog images | Not covered | API and website upload handler exist; no automated tests listed |
| Markdown sanitization | Covered | `rust/website/src/markdown.rs` |

---

## E2E Tests — WebDriver/Mocha (`e2e/specs/`)

### Login Flow (`login.e2e.ts`)
- **should display the login form by default** — verifies email, password, submit fields render; name field absent
- **should show error for invalid credentials** — verifies error message appears on bad login
- **should successfully login with valid credentials** — signs up, logs in, lands on Unlock Encryption screen
- **should toggle from login to signup** — toggle button switches heading and reveals name field

### Signup Flow (`signup.e2e.ts`)
- **should display the signup form when toggled** — all signup fields render after toggle
- **should toggle from signup to login** — reverse toggle hides name field
- **should successfully sign up and reach encryption setup** — full signup routes to Set Encryption Password

### Signup Wizard (`signup-wizard.e2e.ts`)
- **should complete the entire signup wizard end to end** — 5-step wizard: account creation → encryption setup → device registration → storage configuration → backup setup; lands on Dashboard

### Local Filesystem Storage Wizard (`local-storage-wizard.e2e.ts`)
- **should complete the full signup wizard with local storage** — same 5-step wizard using Local Filesystem storage instead of S3
- **should show the local storage in the dashboard** — verifies dashboard loads Quick Actions and Start Backup after local storage onboarding
- **should start and complete a backup to local storage** — triggers the quick backup button and waits for Running/Completed/uploaded state
- **should show local filesystem storage type in Settings** — navigates to Settings > Storage and verifies E2E Local Storage / Local Filesystem appears

### Dashboard (`dashboard.e2e.ts`)
- **should display the dashboard subtitle** — subtitle text "Your backup status at a glance" present
- **should show dashboard stat cards** — Files Protected, Storage Used, Active Configs cards present
- **should show the quick actions section** — Quick Actions and Start Backup present
- **should show the local device widget** — "This Device" widget present

### Backup (`backup.e2e.ts`)

#### Start Backup from Dashboard
- **should start a backup from the dashboard quick action** — triggers backup, confirms Running/Starting/Completed appears
- **should complete the backup with all test files** — waits for Completed status within 60s

#### Backup Page
- **should show the backup config card** — Default Backup config + storage type visible
- **should show the Run Now button on the backup config card** — `data-testid="backup-run-now"` present
- **should show Reports tab with the completed backup job** — completed job appears in Reports
- **should show correct file count in the backup report** — HTML contains expected TEST_FILE_COUNT
- **should display job status as display-friendly label, not a raw backend token** — "Completed" shown; "in_progress" / "completed_with_errors" absent
- **should label backup sizes with dedup context, not a bare arrow** — dedup-context label (stored/saved/uploaded) present
- **should not show raw dedup jargon in reports list** — "Dedup ratio" / "after dedup" absent from visible text
- **should open a backup job detail and hide system files by default** — .DS_Store / Thumbs.db absent; "System files" toggle present
- **should show correct column headers in the backup job file table** — "Original size", "Space saved", "Chunks reused" visible
- **should show status filter pills above the file table** — All, Completed, Failed pills present
- **should show correct default KPIs in job detail (not raw dedup jargon)** — Total files, Files completed, Uploaded visible; "Dedup ratio" absent
- **should show progress label with file count caption during active backup** — "of N files" caption present when backup running

#### Restore Reports (within backup.e2e.ts)
- **should hide system files by default in restore job detail** — .DS_Store / Thumbs.db absent; System files toggle present

### Files (`files.e2e.ts`)

#### Files
- **should display the Files page subtitle** — "Browse and restore protected files" present
- **should show the backup config for selection** — "Default Backup" config card present
- **should show file list after selecting a config** — file list loads after clicking config card
- **should have view mode toggle buttons** — list view toggle button present
- **should show backed up test files** — "test-file-1" present in file list after re-navigation
- **should expand a file row to show version details** — clicking file row reveals version (v1 / Verified)
- **should show Restore button on version row** — Restore button exists on version row

#### Files — Bulk Selection
- **should show bulk action bar after checking a file** — bar appears with "1 selected" after checkbox click
- **should update count when a second file is checked** — bar updates to "2 selected"
- **should open the restore modal with selected files when Restore is clicked** — Restore Files modal appears
- **should retain selection after closing the restore modal without confirming** — bar still visible after cancel
- **should clear selection when Clear is clicked** — bar disappears after Clear
- **should select all loaded files when the header checkbox is clicked** — all visible checkboxes become checked
- **should not show system artifacts (.DS_Store, Thumbs.db) in the file list** — artifact filenames absent from body
- **should move selected files to bin when Move to Bin is clicked** — bulk action bar disappears after move-to-bin

#### Files — Search
- **should filter file list by search query** — typing in "Search files..." input narrows list to matching file; non-matching files absent
- **should clear search and restore the full file list** — clicking clear button resets query; original file set returns

### Restore (`restore.e2e.ts`)

#### Restore a file from Files page
- **should select config and show backed up files** — "test-file-1" visible after selecting config
- **should expand a file to show version details** — version (v1 / Verified) shown
- **should click Restore on a file version and open the modal** — Restore Files modal opens
- **should show destination options in the restore modal** — Original location / Downloads folder / Custom path options present
- **should proceed to review and confirm restore** — navigates to Confirm Restore; modal closes on success

#### Restore Reports
- **should show restore job in Reports > Restore tab** — restore job (Completed or empty state) renders in Reports

### Settings (`settings.e2e.ts`)

#### Backups Tab
- **should display auto-backup settings** — "automatic" / "Auto" text present
- **should display backup config section** — "Default Backup" / "cloudless-e2e-backup" present

#### Storage Tab
- **should display the current device** — "This Device" or "device" text present
- **should display the storage we configured** — E2E Test Storage / Local Filesystem / AWS present

#### Security Tab
- **should display encryption info section** — "AES-256-GCM" / "Encryption" text present
- **should show non-zero byte metrics when files have been backed up** — Original data and Storage used are non-zero
- **should mask device IDs (not show full UUIDs in primary view)** — masked format (…) used for device IDs
- **should display the Save Recovery Key button** — export recovery key button rendered
- **should display the Change Password button** — change password button rendered
- **should open and close export recovery key modal** — modal shows recovery key, Copy/Download buttons, disabled Continue until checkbox; closes cleanly
- **should open and close change password dialog** — dialog shows Current/New Password fields; closes cleanly

#### Retention Tab
- **should display retention settings** — "Retention" / "days" text present
- **should display garbage collection section** — "Garbage Collection" / "Run Garbage Collection" present
- **should run garbage collection** — GC completes showing deleted/freed/completed/No expired
- **should not show plain 'Loading...' text in GC History after load** — "Loading..." absent after async load; Completed/Failed/empty-state present

#### Security Tab (continued)
- **should display the Account section** — "Account" heading present in Security tab
- **should show the subscription tier** — "Free" / "Starter" / "Pro" / "Lifetime" label loads asynchronously from server

#### Preferences Tab
- **should display theme options** — "Theme" / "Dark" / "Light" text present

### Backup Config Management (`backup-config-management.e2e.ts`)
- **should rename a backup config** — clicks config name, enters new name, saves, verifies new name appears
- **should toggle a backup config off then back on** — two-step confirm flow to disable then re-enable; verifies "Disabled" badge
- **should update the cleanup policy on a backup config** — opens Edit inline form, checks "Delete after" checkbox, enters 7 days, saves, verifies persisted

### Navigation (`navigation.e2e.ts`)
- **should navigate to Files** — Files heading appears after sidebar click
- **should navigate to Backup** — Backup heading appears
- **should navigate to Settings** — Settings heading appears
- **should navigate back to Dashboard** — Dashboard heading appears
- **should lock the app and show unlock encryption screen** — Lock button triggers Unlock Encryption heading
- **should unlock with encryption password and return to main app** — enters password, waits for sidebar to appear (previously SKIPPED; fixed by fetching hostname before `get_or_create_device`)
- **should sign out and return to login page** — clicks Sign Out, waits for login heading

---

## Integration Tests — Rust (`rust/core/tests/`)

### Full Backup & Restore Flow (`e2e_backup_restore.rs`)
- **e2e_full_backup_and_restore_flow** — 14-phase end-to-end pipeline:
  - Phase 1: user signup, login, DEK setup
  - Phase 2: device & S3 storage registration
  - Phase 3: backup config creation with 5 test files
  - Phase 4: initial backup (small text, binary, large binary, empty files)
  - Phase 5: timestamp-only change (deduplication — no re-upload)
  - Phase 6: content modification backup (new chunks uploaded)
  - Phase 6b: mixed backup (new files + unchanged existing)
  - Phase 7: file deletion + replacement
  - Phase 8: single file restore (to original location)
  - Phase 9: folder restore (all files in config)
  - Phase 10: restore original version (before content modification)
  - Phase 10b: restore newly added files
  - Phase 11: dashboard stats validation (counts, bytes match expectations)
  - Phase 13: GC pipeline (set 0-day retention → move report.pdf to bin → run GC → assert ≥1 version + ≥1 chunk deleted + positive bytes freed)
  - Phase 14: index rebuild (clear SQLite index → rebuild from server → assert file count matches browse → run backup to confirm index is usable)
  - Phase 12: cleanup (test data removed)

### Backup Incremental / Resumption (`e2e_backup_restore.rs`)
- **test_backup_resumption** — standalone test with its own user/device/storage/config:
  - 3 small text files backed up (initial run, all 3 FileCompleted)
  - file-a and file-b modified; file-c unchanged
  - second backup run: verifies file-a and file-b in FileCompleted events; file-c chunks are deduplicated (ChunkUploaded deduplicated=true)
  - browse confirms file-a and file-b at version ≥2; file-c still at version 1

### Subscription Tier Enforcement (`e2e_backup_restore.rs`)
- **e2e_tier_gate_enforcement** — validates server-side Free tier limits end to end:
  - registering the first device succeeds
  - registering a second distinct device returns PermissionDenied / HTTP 403
  - re-registering the same physical device succeeds and bypasses the limit
  - creating the first backup config succeeds
  - creating a second backup config returns PermissionDenied / HTTP 403

---

## Unit Tests — Rust

### Adapters

#### SQLite Local Index (`rust/core/src/adapters/sqlite_local_index.rs`)
- **upsert_and_get_by_path** — insert then retrieve by path returns correct entry
- **get_by_path_not_found** — missing path returns None/error
- **upsert_updates_existing** — second upsert overwrites first entry
- **list_all_returns_sorted** — list returns entries sorted consistently
- **list_by_prefix_filters_correctly** — prefix filter excludes unrelated paths
- **remove_missing_deletes_absent_files** — missing-from-scan paths get removed
- **remove_missing_with_empty_existing_removes_all** — full removal when scan is empty
- **mark_backed_up_updates_fields** — synced_at and chunk info updated after backup
- **mark_backed_up_nonexistent_returns_error** — error returned for unknown entry
- **clear_all_removes_everything** — all entries gone after clear
- **upsert_preserves_backup_state_on_re_scan** — rescan does not reset synced_at
- **different_configs_are_isolated** — entries in config A not visible in config B
- **find_cleanup_candidates_returns_old_synced_files** — files synced > threshold returned
- **find_cleanup_candidates_excludes_not_yet_synced** — unsynced files excluded
- **find_cleanup_candidates_excludes_recently_synced** — recently synced files excluded
- **find_cleanup_candidates_scoped_to_config** — candidates scoped to given config_id

#### Local Filesystem Storage (`rust/core/src/adapters/local_fs_storage_adaptor.rs`)
- **test_put_and_get** — round-trip write/read
- **test_exists_true_false** — exists returns true/false correctly
- **test_delete** — delete removes file, exists returns false
- **test_get_nonexistent_returns_error** — error on missing key
- **test_put_creates_parent_dirs** — deeply nested key creates directories
- **test_list_with_prefix** — prefix filter on object listing
- **test_new_nonexistent_dir_returns_error** — error if root dir does not exist
- **test_large_data_roundtrip** — large buffer round-trips without corruption

#### Byte Stream Model (`rust/core/src/model/storage.rs`)
- **test_byte_stream_from_bytes** — ByteStream wraps &[u8]
- **test_byte_stream_from_vec** — ByteStream wraps Vec<u8>
- **test_byte_stream_from_string** — ByteStream wraps String

### Application Layer

#### Config Application (`rust/core/src/applications/config/application.rs`)
- **test_register_local_device_success / failure** — device registration happy/sad path
- **test_register_remote_storage_success / failure** — storage registration happy/sad path
- **test_create_backup_config_success / failure** — config creation happy/sad path
- **test_get_local_device_success / failure** — device fetch happy/sad path
- **test_get_or_create_local_device_success / failure** — idempotent device creation
- **test_list_remote_storages_success / failure** — list storages happy/sad path
- **test_list_backup_configs_success / failure** — list configs for device
- **test_list_all_backup_configs_success / failure** — list all configs across devices
- **test_list_devices_by_platform_success / failure** — platform-filtered device list
- **test_list_all_devices_success / failure** — all devices for user
- **test_toggle_backup_config_success / failure** — enable/disable config

#### User Application (`rust/core/src/applications/user/application.rs`)
- **test_create_user_success / failure** — user creation happy/sad path
- **test_login_success / failure** — login happy/sad path
- **test_create_user_returns_valid_uuid** — returned ID is valid UUID
- **test_login_returns_tokens** — login response includes access + refresh tokens
- **test_login_returns_user_info** — login response includes user metadata
- **test_update_dek_success / failure** — DEK storage happy/sad path
- **test_generate_recovery_key_success / api_failure** — recovery key generation
- **test_unlock_with_recovery_key_success** — correct phrase decrypts DEK
- **test_unlock_with_recovery_key_invalid_format** — malformed phrase rejected
- **test_unlock_with_recovery_key_wrong_key** — wrong phrase fails decryption
- **test_change_password_success** — password change happy path
- **test_full_recovery_flow** — recover account using recovery key phrase
- **test_recovery_preserves_dek** — DEK unchanged after recovery
- **test_key_rotation_preserves_dek** — DEK unchanged after recovery key rotation
- **test_existing_data_accessible_after_password_change** — DEK usable after password change
- **test_existing_data_accessible_after_recovery_key_rotation** — DEK usable after key rotation
- **test_change_encryption_password_wrong_current** — wrong current password rejected
- **test_password_rotation_invalidates_old_password** — old password fails after rotation
- **test_recovery_then_password_change_invalidates_old_password** — password invalid after recovery change
- **test_password_and_recovery_key_rotation_together** — concurrent rotation succeeds

#### Dashboard Application (`rust/core/src/applications/dashboard/application.rs`)
- **test_get_dashboard_stats_success / failure** — stats fetch happy/sad path
- **test_get_dashboard_stats_returns_correct_byte_counts** — byte totals match backed-up data

### Backup Module

#### Backup Config Encryption (`rust/core/src/applications/backup/backup_config.rs`)
- **encrypt_create_config_request_round_trip** — encrypt then decrypt returns original request
- **decrypt_backup_config_round_trip** — decrypt encrypted blob returns plaintext config
- **decrypt_with_wrong_key_fails** — wrong metadata_key fails decryption

#### Backup Scheduler (`rust/core/src/applications/backup/scheduler.rs`)
- **scheduler_config_defaults** — default interval and enabled state are correct
- **scheduler_event_serialization** — SchedulerEvent serialises/deserialises correctly

#### Backup Cleanup (`rust/core/src/applications/backup/cleanup.rs`)
- **no_cleanup_returns_zero** — NoCleanup policy deletes nothing
- **empty_candidates_skips_log_call** — no log API call when no candidates
- **deletes_file_and_logs_to_server** — file deleted from disk; deletion logged
- **server_log_failure_does_not_abort_cleanup** — partial failure does not stop cleanup
- **nonexistent_file_counts_as_already_freed** — missing file treated as already deleted

#### Index Recovery (`rust/core/src/applications/backup/recovery.rs`)
- **test_recovery_empty_remote** — empty remote state produces empty index
- **test_recovery_populates_index** — remote versions are written to local index
- **test_recovery_keeps_latest_version** — only the newest version per file is kept

#### File Backup (`rust/core/src/applications/backup/backup_file.rs`)
- **test_backup_small_file_single_chunk** — file ≤ 4 MiB produces one chunk
- **test_backup_large_file_multiple_chunks** — file > 4 MiB is split into multiple chunks
- **test_backup_deduplicates_existing_chunks** — matching S3 hash skips upload
- **test_backup_reuploads_stale_s3_chunk** — S3 miss forces re-upload despite matching local hash
- **test_backup_empty_file** — empty file creates version with zero chunks
- **test_backup_nonexistent_file_returns_error** — missing file returns error
- **test_file_version_created_with_correct_metadata** — version record has correct size, mtime
- **test_chunk_entries_have_correct_metadata** — chunk records have correct hash and index

#### Backup Candidates (`rust/core/src/applications/backup/backup_candidates.rs`)
- **test_no_candidates_when_all_backed_up** — no files returned when all hashes match S3
- **test_new_files_are_candidates** — new files (no S3 record) are backup candidates
- **test_modified_files_are_candidates** — changed hash makes file a candidate
- **test_prefix_filtering** — prefix filter limits candidates to matching paths

#### File Browser (`rust/core/src/applications/backup/browse.rs`)
- **test_list_backed_up_files_success** — returns non-empty list for config with backed-up files
- **test_list_backed_up_files_failure** — API error propagated correctly
- **test_list_backed_up_files_contains_versions** — each file entry includes at least one version

#### Filesystem Scan (`rust/core/src/applications/backup/filesystem_scan.rs`)
- **test_scan_empty_directory** — empty dir produces empty scan result
- **test_scan_new_files** — new files appear in scan result
- **test_scan_unchanged_files** — unchanged files (same mtime/size) not in new_files
- **test_scan_removes_deleted_files** — files deleted from disk are removed from index
- **test_scan_nonexistent_directory** — error returned for missing source dir

### Restore Module

#### Restore Job (`rust/core/src/applications/restore/restore_job.rs`)
- **test_resolve_base_destination_original_path** — OriginalPath destination returns source dir
- **test_resolve_base_destination_download_folder** — DownloadFolder destination returns ~/Downloads
- **test_resolve_base_destination_custom_path** — CustomPath returns provided path
- **test_resolve_file_destination_original_path** — file restored to original absolute path
- **test_resolve_file_destination_custom_path_preserves_relative** — relative path maintained under custom root
- **test_resolve_file_destination_download_folder_relative** — file placed under ~/Downloads preserving relative path
- **test_resolve_file_destination_overwrite_returns_same_path** — Overwrite mode returns destination unchanged
- **test_resolve_file_destination_skip_if_exists_returns_same_path** — Skip mode returns destination unchanged
- **test_resolve_file_destination_keep_both_no_conflict** — no conflict → path unchanged
- **test_resolve_file_destination_file_outside_source** — file path outside source dir handled correctly
- **test_restore_file_selection_construction** — RestoreFileSelection built with correct fields

#### Restore File (`rust/core/src/applications/restore/restore_file.rs`)
- **test_restore_file_single_chunk_success** — single-chunk file restored correctly
- **test_restore_file_multi_chunk_success** — multi-chunk file assembled in order
- **test_restore_file_storage_error** — storage download error propagated
- **test_restore_file_hash_mismatch** — chunk integrity failure detected and reported
- **test_restore_chunk_meta_construction** — chunk metadata struct built correctly

### Domain — Cryptography

#### Metadata Crypto (`rust/core/src/domain/metadata_crypto.rs`)
- **round_trip_encrypt_decrypt** — encrypt then decrypt returns original plaintext
- **blind_index_is_deterministic** — same path + key always produces same index
- **different_paths_produce_different_blind_indexes** — distinct paths produce distinct indexes
- **different_keys_produce_different_blind_indexes** — different keys produce distinct indexes
- **nonce_uniqueness_across_encryptions** — repeated encryption uses unique nonces
- **blind_index_is_32_bytes** — output is exactly 32 bytes
- **decrypt_with_wrong_key_fails** — wrong key fails decryption

#### Derived Keys (`rust/core/src/domain/derived_keys.rs`)
- **derive_is_deterministic** — same DEK always produces same derived keys
- **derived_keys_are_independent** — metadata_key ≠ file_path_key ≠ file_content_key
- **derived_keys_are_32_bytes** — each derived key is exactly 32 bytes
- **different_deks_produce_different_keys** — distinct DEKs produce distinct derived keys

#### DEK (`rust/core/src/domain/dek.rs`)
- **test_encrypt_decrypt_dek_with_raw_key_round_trip** — AES-GCM round-trip with raw key
- **test_wrong_raw_key_fails_decrypt** — wrong key fails DEK decryption
- **test_raw_key_encrypt_produces_different_ciphertext** — repeated encryption differs (random nonce)

#### Recovery Key (`rust/core/src/domain/recovery_key.rs`)
- **generate_produces_32_bytes** — generated key is 32 random bytes
- **display_string_round_trip** — encode then decode returns original key
- **display_string_has_clrk_prefix** — human-readable form starts with "clrk"
- **invalid_checksum_rejected** — tampered phrase rejected
- **invalid_prefix_rejected** — wrong prefix rejected
- **invalid_base32_rejected** — non-base32 characters rejected
- **too_short_rejected** — truncated phrase rejected
- **derive_is_deterministic** — same input always produces same derived material
- **different_rs_different_rk** — different recovery secret → different recovery key
- **encrypt_decrypt_dek_round_trip** — DEK encrypted with recovery key decrypts correctly
- **wrong_rk_fails_decrypt** — wrong recovery key fails DEK decryption
- **recovery_key_is_32_bytes** — generated key is 32 bytes

### Services

#### File Service (`rust/core/src/services/file_service.rs`)
- **test_process_chunks_logic** — chunk ordering and ID assignment are correct

### API Server

#### DoDo Webhook (`rust/api_server/src/infra/dodo/webhook.rs`)
- **test_valid_signature** — correctly signed payload accepted
- **test_wrong_signature_rejected** — tampered payload rejected
- **test_stale_timestamp_rejected** — old timestamp rejected (replay protection)
- **test_multiple_signatures_one_valid** — accepts payload with one valid signature among multiple

#### Subscription Payment Application (`rust/api_server/src/core/subscription/payment_application.rs`)
- **test_extract_product_id_uses_top_level_when_present** — checkout product_id is read from the top-level payload when present
- **test_extract_product_id_falls_back_to_cart** — checkout product_id falls back to cart items when top-level value is absent
- **test_extract_product_id_first_cart_item_only** — first cart item is used for product mapping
- **test_extract_product_id_none_when_both_absent / cart_empty** — missing product IDs are handled without false matches
- **test_status_mapping_known_values / unknown_returns_none** — provider subscription statuses map only when supported
- **test_tier_from_product_id_matches_configured_products / unknown_returns_none** — configured product IDs map to CloudLess tiers
- **test_subscription_active_known_product_triggers_entitlement_change** — active provider subscription applies the expected tier
- **test_subscription_active_unknown_product_returns_err** — unknown active product fails explicitly
- **test_payment_succeeded_lifetime_via_product_id / product_cart** — lifetime purchase is applied from direct or cart product ID
- **test_payment_succeeded_recurring_is_skipped** — recurring payment success does not duplicate subscription updates
- **test_payment_succeeded_unrecognized_product_skipped / no_product_skipped** — unknown or absent product IDs are ignored
- **test_payment_failed_marks_checkout_failed** — failed payment updates checkout state
- **test_reconciliation_cancelled_with_future_period_end_sets_cancel_at_period_end_true** — canceled subscription with future period end is retained until period end
- **test_reconciliation_cancelled_without_future_period_end_sets_cancel_at_period_end_false** — canceled subscription without future period end is treated as ended

#### Backup Config API Application (`rust/api_server/src/core/backup_config/application.rs`)
- **test_create_config_under_limit** — user can create a config below tier limit
- **test_create_config_at_limit_is_rejected** — tier config limit is enforced
- **test_create_config_pro_tier_no_limit** — Pro tier can create additional configs

#### Local Device API Application (`rust/api_server/src/core/local_device/application.rs`)
- **test_create_device_under_limit** — user can create a device below tier limit
- **test_create_device_at_limit_is_rejected** — tier device limit is enforced
- **test_create_device_pro_tier_no_limit** — Pro tier can register additional devices
- **test_get_or_create_existing_device_bypasses_limit** — idempotent device lookup does not consume another device slot
- **test_get_or_create_new_device_under_limit** — get-or-create can create a new device below limit
- **test_get_or_create_new_device_at_limit_is_rejected** — get-or-create rejects new devices at tier limit
- **test_get_or_create_pro_tier_no_limit** — Pro tier get-or-create allows additional devices

#### Policy Repository (`rust/api_server/src/infra/psql/pg_policy_repo.rs`)
- **skip_blocked_when_max_skips_zero** — cannot skip when max_skips is 0
- **skip_blocked_when_exhausted** — skip blocked after exhausting skip budget
- **skip_allowed_when_remaining** — skip succeeds while budget remains

#### Policy Application (`rust/api_server/src/core/policy/application.rs`)
- **test_create_policy_success / failure** — policy creation happy/sad path
- **test_update_policy_success / failure** — policy update happy/sad path
- **test_list_policies_success / failure** — list all policies
- **test_create_version_success / failure** — policy version creation
- **test_update_version_success / failure** — policy version update
- **test_publish_version_success / failure** — publish makes version active
- **test_list_versions_success / failure** — list policy versions
- **test_get_version_success / failure** — fetch specific version
- **test_get_pending_policies_success / failure** — get user's pending policy list
- **test_accept_policy_success / failure** — record user acceptance
- **test_skip_policy_success / failure** — record skip with budget check
- **test_get_checkout_policy_success / failure** — fetch Refund & Cancellation policy gate; happy/sad path
- **test_get_checkout_policy_already_accepted** — returns `policy: None` when user has already accepted the latest version

#### API Server Config (`rust/api_server/src/config.rs`)
- **test_defaults_are_applied** — default config values correct when env vars absent

#### Remote File Version API Application (`rust/api_server/src/core/remote_file_version/application.rs`)
- **test_list_all_versions_free_tier_passes_cutoff_timestamp** — Free tier version list applies the retention cutoff
- **test_list_all_versions_paid_tier_passes_none** — paid tier version list does not apply the Free tier cutoff
- **test_list_all_versions_starter_tier_passes_none** — Starter tier version list keeps full version history

### API Types

#### File Filter (`rust/api_types/src/file_filter.rs`)
- **hides_macos_artifacts** — .DS_Store, .localized, etc. filtered
- **hides_windows_artifacts** — Thumbs.db, desktop.ini, etc. filtered
- **hides_vcs_directories** — .git, .svn, .hg filtered
- **hides_build_and_tool_directories** — node_modules, target, .cache, etc. filtered
- **hides_compiled_bytecode_extensions** — .pyc, .class, etc. filtered
- **allows_user_files** — user documents pass the filter
- **case_insensitive** — filter is case-insensitive for extensions
- **handles_backslash_paths** — Windows-style paths handled
- **file_type_group_documents** — documents group returns correct extensions
- **file_type_group_images** — images group correct
- **file_type_group_videos** — videos group correct
- **file_type_group_audio** — audio group correct
- **file_type_group_archives** — archives group correct
- **file_type_group_unknown_returns_all** — unknown group returns all files
- **file_type_group_uses_last_extension** — double extensions resolved correctly

#### Subscription Tiers (`rust/api_types/src/subscription.rs`)
- **test_free/starter/pro/lifetime_tier_limits** — each tier has correct storage/config limits
- **test_effective_tier_active_paid** — active subscription uses paid tier
- **test_effective_tier_past_due_keeps_tier** — past-due subscription retains tier
- **test_effective_tier_on_hold_keeps_tier** — on-hold subscription retains tier
- **test_effective_tier_canceled/expired_falls_to_free** — canceled/expired falls back to Free
- **test_effective_tier_lifetime_never_downgrades** — Lifetime tier never falls to Free
- **test_is_paid** — is_paid() correct for each tier
- **test_tier/status_from_str** — parse from string
- **test_tier/status_display_roundtrip** — Display and FromStr are inverse
- **test_serde_tier/status_roundtrip** — JSON serialization round-trip
- **test_tier_rank_ordering** — tiers ordered Free < Starter < Pro < Lifetime
- **test_change_reason_upgrade/downgrade/renewal/lifetime_purchase** — change reason correct
- **test_tier_change_reason_roundtrip / from_str_invalid** — parse + display roundtrip
- **test_tier_usage_under/at/over_limit / unlimited** — usage limit enforcement

#### Backup Job Status (`rust/api_types/src/backup_job.rs`)
- **job_status_display_labels** — raw backend tokens map to display-friendly labels
- **file_status_display_labels** — raw file status tokens map to display-friendly labels

### Website

#### Markdown Renderer (`rust/website/src/markdown.rs`)
- **test_basic_markdown** — headings, bold, italic render correctly
- **test_image_preserved** — `<img>` tags pass through unchanged
- **test_inline_html_img** — inline HTML images preserved
- **test_script_stripped** — `<script>` tags stripped for security
- **test_table_support** — markdown tables rendered as HTML

---

## Remaining Coverage Gaps

The following flows have no automated test coverage. Candidates for a future sprint:

- **Multi-device dedup** — two devices sharing same S3 bucket, verify shared chunk references across devices
- **Restore job resumption** — interrupt mid-restore, resume, verify files restored correctly
- **Google Drive OAuth flow** — requires real OAuth credentials; not testable in CI
- **Storage reauth** — Google Drive token refresh + identity verification flow
- **Password reset via email** — requires email delivery; server auto-verifies in test environment
- **Subscription checkout/portal** — requires Dodo payment sandbox credentials
- **Policy acceptance UI** — API policy consent and checkout gate (`get_checkout_policy`) are covered by unit tests; the desktop `CheckoutPolicyModal` and signup `PolicyAcceptance` screen are not covered by UI E2E
- **Email verification UI** — API commands exist, but test signup uses auto-verification
- **Biometric unlock** — Tauri commands exist, but no automated biometric availability, enable, unlock, or disable tests are listed
- **Desktop updater** — update check and install commands are not covered
- **Folder picker and open path commands** — desktop shell commands are not covered
- **Google Drive storage adapter** — adapter and Tauri commands exist, but no automated OAuth-backed storage test is listed
- **Blog, media, email template, and website admin flows** — public rendering and admin CRUD pages are not covered by E2E
- **Legal page public rendering** — policy repository and application are covered, but public legal routes are not covered by E2E
