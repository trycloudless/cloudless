---
trigger: always_on
---

# architecture.md
## CloudLess – System Architecture

This document describes the **business and technical architecture** of CloudLess.

It explains *why* decisions exist so they are not accidentally reversed.

---

## 1. SYSTEM OVERVIEW

CloudLess is an encrypted backup engine. It provides:
- Client-side encryption (zero-knowledge — server never sees plaintext)
- Chunk-based deduplication (content-addressable by SHA-256)
- Per-device file versioning with soft-delete bin
- Multi-device support (desktop, iOS, Android)
- Hybrid storage backends (AWS S3, Google Drive)
- File restoration with integrity verification
- Recovery key and biometric unlock support
- Garbage collection with configurable retention
- Policy management (ToS, privacy policy acceptance)
- Admin website for blog, email templates, and user management

It is explicitly **not** a synchronization system.

---

## 2. HIGH-LEVEL ARCHITECTURE DIAGRAM

```
                          CloudLess Architecture
    ================================================================

    CLIENTS (Tauri + Leptos WASM)
    ┌──────────────────────────────────────────────────────────────┐
    │                                                              │
    │  ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌───────────┐  │
    │  │  macOS   │  │  iOS     │  │  Android  │  │  Website  │  │
    │  │ Desktop  │  │  App     │  │   App     │  │   (SSR)   │  │
    │  └────┬─────┘  └────┬─────┘  └─────┬─────┘  └─────┬─────┘  │
    │       │              │              │              │         │
    │       └──────────┬───┴──────────────┘              │         │
    │                  │                                 │         │
    │    ┌─────────────▼──────────────┐    ┌─────────────▼──────┐  │
    │    │     Tauri Shell            │    │   Leptos SSR       │  │
    │    │  (46 commands, events)     │    │   (Axum server)    │  │
    │    └─────────────┬──────────────┘    └─────────────┬──────┘  │
    │                  │                                 │         │
    │    ┌─────────────▼──────────────┐                  │         │
    │    │     cloudless_core         │                  │         │
    │    │  (ports & adapters)        │                  │         │
    │    │                            │                  │         │
    │    │  ┌────────┐ ┌───────────┐  │                  │         │
    │    │  │Encrypt │ │ Compress  │  │                  │         │
    │    │  │AES-256 │ │  zstd     │  │                  │         │
    │    │  └────────┘ └───────────┘  │                  │         │
    │    │  ┌────────┐ ┌───────────┐  │                  │         │
    │    │  │Chunker │ │ SQLite    │  │                  │         │
    │    │  │ 4 MiB  │ │Local Index│  │                  │         │
    │    │  └────────┘ └───────────┘  │                  │         │
    │    └─────────────┬──────────────┘                  │         │
    └──────────────────┼─────────────────────────────────┼─────────┘
                       │                                 │
         ─ ─ ─ ─ ─ ─ ─│─ ─ ─ NETWORK ─ ─ ─ ─ ─ ─ ─ ─ ─│─ ─ ─ ─
                       │                                 │
    CONTROL PLANE      │              ┌──────────────────┘
    ┌──────────────────▼──────────────▼────────────────────────────┐
    │                                                              │
    │              API Server (Axum + REST + JSON)                 │
    │            JWT auth, ~70 endpoints, stateless                │
    │                                                              │
    │    ┌─────────────────────────────────────────────────────┐   │
    │    │                  PostgreSQL                         │   │
    │    │                                                     │   │
    │    │  users, devices, backup_configs, remote_storages    │   │
    │    │  remote_file_versions, chunks, file_version_chunks  │   │
    │    │  backup_jobs, restore_jobs, gc_runs                 │   │
    │    │  encrypted_deks, policies, security_events          │   │
    │    │  blog_posts, email_templates, refresh_tokens        │   │
    │    │                                                     │   │
    │    └─────────────────────────────────────────────────────┘   │
    └──────────────────────────────────────────────────────────────┘

    DATA PLANE (client uploads directly — never through API server)
    ┌──────────────────────────────────────────────────────────────┐
    │                                                              │
    │   ┌──────────────────┐        ┌──────────────────────────┐  │
    │   │     AWS S3       │        │     Google Drive          │  │
    │   │  (user-owned)    │        │  (user-owned, OAuth2)     │  │
    │   │                  │        │  drive.file scope only    │  │
    │   │  Encrypted       │        │                           │  │
    │   │  chunks only     │        │  Encrypted chunks only    │  │
    │   └──────────────────┘        └──────────────────────────┘  │
    │                                                              │
    └──────────────────────────────────────────────────────────────┘
```

### Key Principles
- **Control Plane** (PostgreSQL): Single source of truth for all metadata
- **Data Plane** (S3/Google Drive): Dumb, untrusted blob storage — encrypted chunks only
- **Clients**: All encryption/decryption happens here — server never sees plaintext
- **Direct upload**: Chunks flow directly from client to storage, never through the API server

---

## 3. CRATE STRUCTURE

```
rust/
├── api_types/        Shared DTOs (request/response types, enums)
├── api_server/       Axum HTTP server (control plane)
│   ├── core/ports/   Repository traits (18 traits)
│   ├── infra/psql/   PostgreSQL implementations
│   ├── core/*/       Application services (per domain)
│   └── web/routes/   HTTP route handlers + JWT middleware
├── core/             Client core library (cloudless_core)
│   ├── ports/        Port traits (storage, API, local index, crypto)
│   │   ├── api/      API port traits (remote service abstractions)
│   │   ├── storage.rs Storage port trait
│   │   └── local_index.rs Local index port trait
│   ├── adapters/     Concrete implementations
│   │   ├── api/      HTTP API adapters (17 port implementations)
│   │   ├── s3_storage_adaptor.rs           S3StorageAdaptor
│   │   ├── google_drive_storage_adaptor.rs GoogleDriveStorageAdaptor
│   │   ├── local_fs_storage_adaptor.rs     LocalFsStorageAdaptor (dev/test)
│   │   ├── sftp_storage_adaptor.rs         SftpStorageAdaptor
│   │   ├── sqlite_local_index.rs           SqliteLocalIndex
│   │   ├── aes_gcm_encryptor.rs            AES-256-GCM encryption
│   │   ├── zstd_compressor.rs              Zstd compression
│   │   └── fixed_size_chunker.rs           4 MiB fixed-size chunker
│   ├── applications/ Business logic orchestrators (Env-based DI per domain)
│   │   ├── backup/   Backup pipeline (scan, chunk, compress, encrypt, upload)
│   │   ├── restore/  Restore pipeline (download, decrypt, decompress, verify, write)
│   │   ├── user/     Auth, DEK management, recovery key, password rotation
│   │   ├── gc/       Client-side garbage collection (storage deletion)
│   │   ├── config/   Backup config management
│   │   └── dashboard/ Stats retrieval
│   ├── domain/       Cryptographic primitives (DEK, KEK, HKDF, recovery key)
│   ├── model/        Core data types (Chunk, EncryptedData, ObjectKey, etc.)
│   ├── entities/     Domain entity types (User)
│   ├── services/     Domain services (file_service)
│   └── app_env.rs    Concrete AppEnv wiring all adapters to port traits
├── bindings/         TypeScript bindings for Tauri commands
├── tauri/            Tauri desktop+mobile shell (46 commands, event bridge)
├── tauri-plugin-folder-picker/ Custom Tauri plugin for folder selection
├── leptos_ui/        CSR Leptos UI for Tauri (WASM, phase-based state machine)
├── shared_ui/        Shared presentational components (no CSR/SSR coupling)
├── website/          SSR Leptos website (admin portal, blog, policies)
├── cli/              CLI utility (placeholder)
├── scripts/          Build and deployment scripts
└── playground/       Development sandbox
```

---

## 4. DEVICE MODEL

- Each device is an independent namespace
- File identity: `(backup_config_id, blind_index)` — path encrypted, blind index for equality search
- Same path on different devices = different files
- Deduplication happens only by content hash (cross-device, cross-config)
- Desktop: device matched by machine UID
- Mobile: device resolved by platform + display name (no stable machine UID)

This avoids false conflicts and unintended merges.

---

## 5. FILE VERSIONING MODEL

- Each file has a linear version history (monotonically increasing integers)
- New versions reference a base version
- Conflicts only occur when base version is stale (same device, same file)
- Soft-delete: versions can be moved to bin, restored from bin
- Bin retention: configurable per-user (default 30 days), enforced by GC

There are no snapshots or DAGs.

---

## 6. ENCRYPTION MODEL

### Key Hierarchy

```
  User Password
       │
       ▼ Argon2id(password, salt)
  ┌─────────┐
  │   KEK   │  Key Encryption Key (256-bit, password-derived)
  └────┬────┘
       │ AES-256-GCM
       ▼
  ┌─────────┐
  │   DEK   │  Data Encryption Key (256-bit, random, stored encrypted on server)
  └────┬────┘
       │ HKDF-SHA256 (domain separation)
       ├──────────────────┬──────────────────┐
       ▼                  ▼                  ▼
  ┌──────────┐     ┌──────────────┐    ┌──────────────┐
  │content_key│    │metadata_key  │    │  index_key   │
  │AES-256-GCM│    │AES-256-GCM  │    │HMAC-SHA256   │
  │chunk data │    │file paths    │    │blind indexes │
  └───────────┘    └──────────────┘    └──────────────┘
```

### Unlock Methods
- **Password**: `Argon2id(password, salt) -> KEK -> decrypt DEK`
- **Recovery Key**: `HKDF-SHA256(recovery_secret) -> recovery_key -> decrypt DEK`
  - Display format: `CLRK-XXXX-XXXX-...` (Base32 with 2-byte checksum)
- **Biometric**: OS keychain stores raw key -> `decrypt DEK`
  - Backed by Face ID / Touch ID / fingerprint

### File Path Encryption
- Encrypted with `metadata_key` (AES-256-GCM, random nonce per encryption)
- Blind index computed via `HMAC-SHA256(path, index_key)` for server-side equality search
- Server stores: `encrypted_name`, `nonce`, `blind_index` — never sees plaintext path

### All sensitive keys zeroized on drop (`Zeroizing<T>`)

---

## 7. BACKUP PIPELINE

```
  Local File
       │
       ▼
  ┌──────────────┐
  │  Filesystem   │  Walk source dir, compare mtime/size against SQLite local index
  │    Scan       │  Encrypt new file paths, compute blind indexes
  └──────┬───────┘
         │ candidates (new + modified files)
         ▼
  ┌──────────────┐
  │  Fixed-Size   │  4 MiB chunks, SHA-256 hash per chunk (plaintext)
  │   Chunker     │  Streams via mpsc channel for progress
  └──────┬───────┘
         │ chunks
         ▼
  ┌──────────────┐
  │   Compress    │  zstd (level 4 default, adaptive for mobile)
  └──────┬───────┘
         │ compressed
         ▼
  ┌──────────────┐
  │   Encrypt     │  AES-256-GCM with content_key (random nonce per chunk)
  └──────┬───────┘
         │ encrypted ciphertext
         ▼
  ┌──────────────┐
  │  Dedup Check  │  Check if SHA-256 hash exists in storage
  │               │  If yes: skip upload (deduplicated)
  │               │  If no: PUT to S3/Google Drive
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │  Register     │  POST chunk metadata to API server
  │  Chunk        │  (hash, index, storage_meta, nonce, algorithm)
  └──────────────┘
```

### Job Tracking
- Server creates `backup_job` + `backup_job_files` per run
- Resumable: interrupted jobs can be resumed from last incomplete file
- Progress events emitted via Tauri events: `ConfigLoaded -> IndexUpdated -> CandidatesFound -> FileProgress -> Completed`

---

## 8. RESTORE PIPELINE

```
  Selected File Versions
       │
       ▼
  ┌──────────────┐
  │  Integrity    │  For each chunk: verify exists in storage (HEAD request)
  │    Check      │  Abort if any chunk missing
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │  Download     │  GET encrypted ciphertext from S3/Google Drive
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │   Decrypt     │  AES-256-GCM with content_key (nonce from server metadata)
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │  Decompress   │  zstd
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │  Verify Hash  │  SHA-256 of plaintext chunk must match original
  │               │  Abort on mismatch (integrity failure)
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │  Atomic Write │  Reassemble chunks in order
  │               │  Write to temp file -> rename (atomic)
  └──────────────┘
```

---

## 9. CHUNKING & DEDUPLICATION

### Chunk Properties
- Fixed-size: 4 MiB
- Hash: SHA-256 of plaintext (before compression/encryption)
- Content-addressable: hash used as storage key (ObjectKey)
- Each chunk independently compressed then encrypted

### Deduplication
- Per user, across all remote storages and backup configs
- Hash-addressed: same content = same key in storage
- One chunk stored once per storage backend
- Stale chunks (exist in storage but not in DB) are re-encrypted and re-registered

---

## 10. STORAGE BACKENDS

### Google Drive
- User-owned storage
- OAuth2 with `drive.file` scope only (app can only access files it creates)
- REST API v3 with automatic token refresh on 401
- Client-side prefix filtering (Drive substring matching is insufficient)
- Multipart upload for new files, PATCH for updates

### AWS S3
- User-owned storage (user provides access key + secret + bucket + region)
- AWS SDK v2 with custom TLS (webpki-roots for Android compatibility)
- Avoids EC2 IMDS probing (hangs on mobile)

### Common Properties
- Storage never owns metadata — PostgreSQL is truth
- Client uploads directly to storage (never through API server)
- Storage adapter implements `StoragePort` trait: `put`, `get`, `delete`, `list`, `exists`

---

## 11. GARBAGE COLLECTION

### Three-Phase Design (client-driven)

**Phase A — Server Collect** (`POST /api/gc/collect`):
1. Find `MovedToBin` versions past retention period (`bin_retention_days`, default 30)
2. Mark expired versions as `Deleted`
3. Remove chunk-version junction entries
4. Identify orphaned chunks (no remaining version references)
5. Record audit trail in `gc_run_versions` and `gc_run_chunks`
6. Return orphaned chunk list to client

**Phase B — Client Delete** (client-side):
1. Group orphaned chunks by storage_id
2. Build storage adapter per storage (S3 or Google Drive)
3. Delete chunks from storage
4. Handle partial failures gracefully

**Phase C — Server Confirm** (`POST /api/gc/confirm_chunk_deletions`):
1. Re-verify chunks still orphaned (race condition protection)
2. Delete chunk rows from database
3. Mark gc_run as completed with stats

### Safety Measures
- Two-phase verification (server re-checks before DB delete)
- Audit trail preserved in `gc_runs`, `gc_run_versions`, `gc_run_chunks`
- Configurable retention per user

---

## 12. MANIFEST DESIGN

- One manifest per file version (ordered list of chunk hashes in `remote_file_version_chunks`)
- Stored only in PostgreSQL (junction table with chunk index ordering)
- Immutable once file version is `VerifiedOnRemoteStorage`
- Manifests define the restore truth

---

## 13. REST API MODEL

- Axum server, REST + JSON, ~70 endpoints
- JWT authentication (access + refresh token pair)
- Stateless — client drives all workflows
- Device-scoped operations
- Designed for mobile reliability (resumable jobs, retry-safe)
- Chunk data never flows through API server

### Route Groups
| Group | Endpoints | Purpose |
|-------|-----------|---------|
| `/auth/*` | 4 | Login, signup, refresh, password reset |
| `/api/backup_config/*` | 5 | Config CRUD, toggle active |
| `/api/backup_job/*` | 8 | Job lifecycle, file tracking, resume |
| `/api/restore_job/*` | 8 | Mirror of backup jobs for restore |
| `/api/remote_file_version/*` | 8 | File versions, bin operations |
| `/api/chunk/*` | 3 | Chunk registration, retrieval |
| `/api/remote_storage/*` | 3 | Storage config management |
| `/api/local_device/*` | 5 | Device registration and lookup |
| `/api/encrypted_dek/*` | 2 | DEK storage (password, biometric, recovery) |
| `/api/gc/*` | 6 | Garbage collection lifecycle |
| `/api/security_event/*` | 2 | Audit trail |
| `/api/dashboard/*` | 1 | Stats |
| `/api/policy/*` | 9 | Policy management and acceptance |
| `/api/subscription/*` | 5 | Subscription management, checkout, portal, history |
| `/api/blog/*` | 3 | Blog CRUD (admin) |
| `/api/email_template/*` | 5 | Email template CRUD |
| `/blog/*` | 2 | Public blog (no auth) |
| `/webhook/dodo` | 1 | Dodo Payments webhook (HMAC signature-verified, unauthenticated) |

---

## 14. CLIENT APPLICATION (Tauri + Leptos)

### Phase-Based State Machine
```
Auth -> Setup -> PolicyAcceptance -> Main
              (EncryptionPassword -> Device -> RemoteStorage -> BackupConfig)
```

### Main Views
- **Dashboard**: Stats cards, backup/restore summaries, quick actions
- **Files**: File browser (list/folder/bin views), version history, restore modal
- **Backup**: Manual trigger, job history, progress tracking
- **Settings**: System (auto-backup, configs), Infrastructure (storages, devices), Security (password, recovery key, biometric), Data (GC, retention), Preferences

### Event Bridge
Tauri emits real-time events that the Leptos UI listens to:
- `backup-progress` -> `BackupResult` (ConfigLoaded, IndexUpdated, CandidatesFound, FileProgress, Completed, Failed)
- `restore-progress` -> `RestoreResult` (ConfigLoaded, IntegrityCheckPassed, FileProgress, Completed, Failed)
- `scheduler-event` -> `SchedulerEvent` (NextBackupAt, BackupStarted, BackupCompleted, BackupFailed)

### Auto-Backup Scheduler
- Configurable interval, enabled/disabled toggle
- Runs sequentially across all active configs for the device
- Graceful cancellation via `CancellationToken`
- Survives config changes without restart

---

## 15. DEPENDENCY INJECTION (Env Pattern)

All layers use **Env traits** that group port dependencies:

```rust
pub trait BackupEnv {
    type UserApi: UserApiPort;
    type BackupConfigApi: BackupConfigApiPort;
    type LocalIndex: LocalIndexPort;
    type ChunkApi: ChunkApiPort;
    // ... 8 associated types total
    fn user_api(&self) -> &Self::UserApi;
    // ... accessor methods
}

pub async fn start_backup<E: BackupEnv>(env: &E, req: StartBackupRequest) -> Result<()> {
    let config = env.backup_config_api().get_config(req.config_id).await?;
    // ...
}
```

### Env Traits by Domain
| Env Trait | Ports Grouped |
|-----------|--------------|
| `UserEnv` | UserApi, EncryptedDekApi |
| `ConfigEnv` | LocalDeviceApi, RemoteStorageApi, BackupConfigApi |
| `BackupEnv` | UserApi, BackupConfigApi, LocalIndex, LocalDeviceApi, RemoteStorageApi, RemoteFileVersionApi, ChunkApi, BackupJobApi |
| `RestoreEnv` | BackupConfigApi, RemoteStorageApi, RemoteFileVersionApi, ChunkApi, RestoreJobApi |
| `GcEnv` | GcApi, RemoteStorageApi |
| `DashboardEnv` | DashboardApi |
| `WebsiteEnv` | UserApi, BlogApi, EmailTemplateApi, PolicyApi |

---

## 16. MOBILE SUPPORT

### Platform Handling
- **Desktop**: Machine UID for device identification, `dirs` crate for paths
- **iOS/Android**: Tauri plugin store for device ID persistence, `app_data_dir()` for sandbox paths
- **Android-specific**: `rustls_platform_verifier` with JVM context for TLS, bundled Mozilla root certs (avoids EC2 IMDS probe)
- **iOS-specific**: Security-scoped file access restoration after app restart

### Device Resolution (Mobile)
No stable machine UID on mobile. Resolution flow:
1. Check stored device ID in local store
2. If not found: query server by platform + display name
3. Single match -> auto-resolve; Multiple -> picker UI; None -> create new

---

## 17. SECURITY MODEL

- Client-side encryption only (zero-knowledge server)
- DEK stored encrypted on server (3 key types: password, biometric, recovery)
- File paths encrypted in transit and at rest (blind index for server search)
- Per-chunk random nonces (IND-CPA security)
- All sensitive keys `Zeroizing<T>` (zeroed on drop)
- Security events logged: `export_recovery_key`, `change_password`, `recover_with_recovery_key`
- OAuth refresh tokens scoped to minimum permissions
- JWT access + refresh token pair with rotation

---

## 18. CONFLICT HANDLING

Conflicts occur only when:
- Same device, same file, based on stale version

Resolution: reject with HTTP `409`, preserve existing data, allow client retry.

Enforced server-side via a per-`(backup_config_id, name_blind_index)` advisory transaction lock plus a `base_version` compare-and-insert: the server serializes concurrent writers for the same file identity, checks the client's `base_version` against the current max version, and only assigns the next version if they match. File-version creation and chunk registration additionally carry a deterministic idempotency key (derived from the request's own identity, not randomly generated), so a retried request either replays the original stored response or gets a `409` if the payload diverged — never a silent duplicate or a second side effect.

Cross-device conflicts do not exist by design.

---

## 19. FAILURE MODES & RECOVERY

| Failure | Handling |
|---------|----------|
| Network interruption | Resumable backup/restore jobs |
| App crash mid-backup | Job stays `running`, client resumes from last incomplete file |
| Partial chunk upload | Chunk not registered in DB until upload verified |
| Storage chunk missing | Pre-restore integrity check aborts before any writes |
| Stale S3 chunks | Re-encrypted and re-registered during backup |
| Password forgotten | Recovery key unlocks DEK |
| Device lost | New device, same account — all metadata in PostgreSQL |

Detection happens on restore and GC. Recovery is explicit and user-visible.

---

## 20. DESIGN PRIORITIES

```
Correctness > Idempotency > Recoverability > Security > Performance
```

This system trades convenience for correctness.

---

## 21. ONE-SENTENCE SUMMARY

> PostgreSQL defines reality, devices are isolated, content is global, encryption is client-side, and storage is replaceable.
