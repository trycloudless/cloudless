---
trigger: always_on
---

# agents.md

## AI Agent Operating Rules – Backup Engine

This file defines **non-negotiable rules** for AI agents operating in this repository.

AI agents MUST treat this file as authoritative.

If a suggestion conflicts with this document, the suggestion is WRONG.

> **Status note:** the rules below are the target invariants this system is built toward. If you're touching code in an area covered by a rule below, bring it into compliance rather than treating existing behavior as precedent.

---

## 1. CORE PHILOSOPHY

Priority order (highest → lowest):

1. Correctness
2. Idempotency
3. Recoverability
4. Security
5. Performance
6. Developer convenience

This is a **backup system**, not a sync engine.

Silent failure is strictly forbidden.

---

## 2. HARD CONSTRAINTS (DO NOT VIOLATE)

### 2.1 Device isolation

- Files are **device-scoped**
- File identity = `(backup_config_id, name_blind_index)`
- Same path on two devices MUST NOT be merged

### 2.2 Database authority

- PostgreSQL is the **only source of truth**
- Object storage (S3 / Google Drive) is dumb and untrusted
- Never infer metadata from storage listings

### 2.3 Deduplication

- Deduplication is **by user_id, device_id and chunk hash**
- Chunk hash = SHA-256 of plaintext
- Encryption happens **after hashing**

### 2.4 Encryption order (MANDATORY)

- read → hash → compress → encrypt → upload

---

## 3. STORAGE RULES

### Google Drive and AWS S3

- Uploads happen directly from device
- Uses user OAuth (`drive.file` scope only)
- Backend may list/verify files ONLY with user consent
- No service-account-only access
- Google Drive and AWS S3 credentials should be stored in encrypted format in the database.

### Universal rule

- Backend must NEVER assume a chunk exists
- Verification happens on restore, GC, or integrity scan

---

## 4. VERSIONING & CONFLICTS

### Versioning

- Per file (per device)
- Linear integer versions: `1, 2, 3, …`
- Assigned by server only

### Conflicts

- Only possible within the **same device**
- Detected using `base_version`
- On conflict:
  - Return HTTP `409`
  - Never overwrite existing versions
  - Never drop user data

---

## 5. IDEMPOTENCY (CRITICAL)

Idempotency is REQUIRED for:

- File version creation
- Chunk completion

Rules:

- Enforced at PostgreSQL level
- Same idempotency key + different payload = `409 Conflict`
- Completed requests must replay stored responses

---

## 6. GARBAGE COLLECTION RULES

- GC is content-based, not device-based
- Live chunks = any chunk referenced by a manifest
- Two-phase deletion is mandatory
- Grace period required (7–30 days)
- GC must hold a database lock
- Missing chunks must be detected and surfaced

---

## 7. RUST IMPLEMENTATION RULES

### Allowed

- Rust stable
- `tokio` async runtime
- `sqlx` for database access
- Explicit SQL
- Streams or channels for async pipelines
- `tracing` for structured logging

### Forbidden

- Heavy ORMs (Diesel, SeaORM)
- Blocking I/O on async paths
- `unwrap()` in production code
- Silent error handling
- Local databases on device
- `println!` for logging (use `tracing` instead)

---

## 8. API DESIGN RULES

- REST (HTTP + JSON) for device communication
- Retry-safe and idempotent endpoints
- Ownership of every device-scoped resource (storage, config, file version, chunk) enforced via composite database foreign keys at the point of use, not via URL structure or headers
- Required header: Authorization: Bearer <JWT>

AI agents MUST NOT:

- Suggest gRPC for device communication
- Remove device scoping
- Remove idempotency keys

---

## 9. SECURITY RULES

- Server never sees plaintext file data
- OAuth refresh tokens encrypted at rest
- Access tokens short-lived
- Revocation must hard-stop Drive access
- Fail loudly on integrity issues

---

## 10. WHAT AI AGENTS MUST NEVER DO

❌ Merge device namespaces  
❌ Treat backup as sync  
❌ Assume storage consistency  
❌ Ignore restore verification  
❌ Introduce silent corruption paths  
❌ Store plaintext data server-side

---

## 11. EXPECTED AI BEHAVIOR

When generating code or suggestions:

- Respect all invariants
- Prefer explicit, boring solutions
- Ask before changing schemas
- Reject optimizations that risk correctness

---

## 12. ONE-SENTENCE SUMMARY

> Devices are independent worlds, PostgreSQL is truth, object storage is dumb, and correctness is sacred.

---

## 13. CODEBASE ARCHITECTURE (Hexagonal / Ports & Adapters)

The project follows **Hexagonal Architecture** (Ports & Adapters) with clear separation of concerns.

### 13.1 Project Structure

```
rust/
├── api_types/          # Shared request/response DTOs (used by both server and client)
├── api_server/         # HTTP API server (Axum-based)
│   ├── core/           # Server-side application logic
│   │   ├── ports/      # Repository traits (database abstractions)
│   │   └── */application.rs  # Use-case implementations
│   │   # Modules: auth, backup_config, backup_job, blog, chunks, dashboard,
│   │   # email_template, email_verification, encrypted_dek, gc, local_device,
│   │   # media, password_reset, policy, remote_file_version, remote_storage,
│   │   # restore_job, security_event, subscription, user
│   ├── infra/psql/     # PostgreSQL repository implementations
│   └── web/routes/     # HTTP route handlers
├── core/               # Client-side core library (cloudless_core)
│   ├── ports/          # Abstract interfaces (traits)
│   │   ├── api/        # API port traits: one file per domain entity:
│   │   │               # backup_config, backup_job, blog, chunk, dashboard,
│   │   │               # email_template, encrypted_dek, gc, local_device, policy,
│   │   │               # remote_file_version, remote_storage, restore_job,
│   │   │               # security_event, subscription, user
│   │   ├── storage.rs  # Storage port trait
│   │   └── local_index.rs # Local index port trait
│   ├── adapters/       # Concrete implementations (HTTP clients, S3, encryption, compression)
│   │   ├── api/        # HTTP API client adapters (one per port trait)
│   │   ├── s3_storage_adaptor.rs
│   │   ├── google_drive_storage_adaptor.rs
│   │   ├── local_fs_storage_adaptor.rs  # Local filesystem storage (dev/test)
│   │   ├── sftp_storage_adaptor.rs      # SFTP storage backend
│   │   ├── sqlite_local_index.rs
│   │   ├── aes_gcm_encryptor.rs
│   │   ├── zstd_compressor.rs
│   │   └── fixed_size_chunker.rs
│   ├── applications/   # Use-case implementations (business logic)
│   │   │               # Modules: backup, config, dashboard, gc, restore, user
│   │   └── */env.rs    # Environment trait for dependency injection
│   ├── domain/         # Core domain types (DEK, KEK)
│   ├── model/          # Value objects, AppError, AppResult
│   ├── entities/       # Domain entity types
│   ├── services/       # Domain services
│   └── app_env.rs      # Concrete AppEnv wiring adapters to ports
├── tauri/              # Tauri desktop app commands and models
├── leptos_ui/          # CSR/WASM Leptos UI (runs inside Tauri WebView)
├── website/            # SSR Leptos website (trycloudless.io)
├── shared_ui/          # Shared presentational components (no csr/ssr feature flag)
├── bindings/           # Shared type bindings
└── cli/                # CLI utility
```

### 13.2 Key Architectural Principles

#### Ports (Interfaces)

- Define **traits** in `ports/` directory
- Traits are async (`#[async_trait]`)
- Traits define the contract, not the implementation
- Example: `RemoteFileVersionApiPort`, `StoragePort`, `ChunkApiPort`

#### Adapters (Implementations)

- Implement port traits in `adapters/` directory
- HTTP client adapters: `HttpRemoteFileVersionApi`, `HttpChunkApi`
- Storage adapters: `S3StorageAdaptor`
- Encryption: `AesGcmEncryptor`
- Compression: `ZstdCompressor`

#### Applications (Use Cases)

- Business logic lives in `applications/` directory
- Each application module has an `env.rs` defining its environment trait
- Application functions take `&E` where `E: SomeEnv`
- Never depend on concrete implementations directly

### 13.3 Environment Pattern (Dependency Injection)

Application functions use an **Environment trait** for dependency injection:

```rust
// Define environment trait with associated types
pub trait BackupEnv {
    type UserApi: UserApiPort;
    type BackupConfigApi: BackupConfigApiPort;
    type ChunkApi: ChunkApiPort;
    // ... other API ports

    fn user_api(&self) -> &Self::UserApi;
    fn backup_config_api(&self) -> &Self::BackupConfigApi;
    fn chunk_api(&self) -> &Self::ChunkApi;

    // Required for spawning tasks with owned environment
    fn clone_env(&self) -> Self where Self: Sized;
}

// Application function uses the environment
pub async fn some_use_case<E: BackupEnv>(env: &E, ...) -> AppResult<()> {
    let response = env.backup_config_api().get_by_id(request).await?;
    // ...
}
```

### 13.4 API Types Convention

Request/Response types live in `api_types/` crate:

```rust
// api_types/src/some_entity.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSomeEntityRequest {
    pub field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSomeEntityResponse {
    pub id: Uuid,
}
```

### 13.5 Port Trait Convention

```rust
// ports/api/some_api_port.rs
use api_types::some_entity::{CreateRequest, CreateResponse};
use async_trait::async_trait;
use crate::ports::api::ApiResult;

#[async_trait]
pub trait SomeApiPort: Send + Sync {
    async fn create(&self, request: CreateRequest) -> ApiResult<CreateResponse>;
    async fn get_by_id(&self, request: GetRequest) -> ApiResult<GetResponse>;
}
```

### 13.6 HTTP Adapter Convention

```rust
// adapters/api/http_some_api.rs
#[derive(Clone)]
pub struct HttpSomeApi {
    api: HttpApi,  // Shared HTTP client with auth
}

impl HttpSomeApi {
    pub fn new(api: HttpApi) -> Self {
        Self { api }
    }
}

#[async_trait]
impl SomeApiPort for HttpSomeApi {
    async fn create(&self, request: CreateRequest) -> ApiResult<CreateResponse> {
        let url = self.api.get_url("api/some_entity/create")?;
        let req = self.api.client.post(url).json(&request);
        self.api.send_with_auth(req).await
    }
}
```

### 13.7 Async Streaming Pattern

For operations that process data in chunks, use `tokio::sync::mpsc` channels:

```rust
pub fn backup_file<E, S>(
    env: E,
    storage: S,
    candidate: BackupCandidate,
    // ...
) -> mpsc::Receiver<AppResult<ChunkMeta>>
where
    E: BackupEnv + Send + Sync + 'static,
    S: StoragePort + 'static,
{
    let (tx, rx) = mpsc::channel(16);

    tokio::spawn(async move {
        // Process chunks and send results through channel
        while let Some(chunk) = chunk_rx.recv().await {
            let result = process_chunk(&chunk).await;
            if tx.send(result).await.is_err() {
                break; // Receiver dropped
            }
        }
    });

    rx
}
```

### 13.8 Error Handling

Use `thiserror` for all error types. Never use `anyhow` in library crates.

```rust
// core/src/model/app_error.rs: client-side errors
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Not found: {message}")]
    NotFound { message: String, source: Option<Box<dyn Error + Send + Sync>> },
    #[error("Permission denied: {message}")]
    PermissionDenied { message: String, source: Option<Box<dyn Error + Send + Sync>> },
    #[error("Network error: {message}")]
    Network { message: String, source: Option<Box<dyn Error + Send + Sync>> },
    #[error("Validation error: {message}")]
    Validation { message: String, source: Option<Box<dyn Error + Send + Sync>> },
    #[error("Internal error: {message}")]
    Internal { message: String, source: Option<Box<dyn Error + Send + Sync>> },
    /// OAuth refresh token expired/revoked: surface a reauth prompt to the user.
    #[error("Auth token expired for storage {storage_id}")]
    AuthTokenExpired { storage_id: Uuid },
}

pub type AppResult<T> = Result<T, AppError>;

// core/src/ports/api/mod.rs: API layer errors
#[derive(Debug, thiserror::Error)]
pub enum ApiClientError {
    #[error("transport error")]
    Transport(#[from] reqwest::Error),
    #[error("Invalid Url {0}")]
    InvalidUrl(String),
    #[error("{0}")]
    Api(ApiError),  // ApiError is from api_types::error
}

pub type ApiResult<T> = Result<T, ApiClientError>;
```

Rules:
- `AppResult` / `AppError` for application-layer code (`applications/`, Tauri commands)
- `ApiResult` / `ApiClientError` for port traits in `ports/api/`
- Implement `From<ApiClientError> for AppError` so `?` propagates across the boundary
- Implement `From` traits for all foreign error types (io::Error, serde_json::Error, etc.)
- Map `sqlx::Error` to domain errors via `map_sqlx_error` (server side)

### 13.9 Testing Pattern

Tests use mock implementations of environment traits:

```rust
#[cfg(test)]
mod tests {
    // Mock API implementations
    #[derive(Clone)]
    struct MockSomeApi { /* test state */ }

    #[async_trait]
    impl SomeApiPort for MockSomeApi { /* mock behavior */ }

    // Mock environment
    #[derive(Clone)]
    struct MockEnv {
        some_api: MockSomeApi,
        // ... other mocks
    }

    impl BackupEnv for MockEnv {
        type SomeApi = MockSomeApi;
        fn some_api(&self) -> &Self::SomeApi { &self.some_api }
        fn clone_env(&self) -> Self { self.clone() }
    }

    #[tokio::test]
    async fn test_use_case() {
        let env = MockEnv::new(/* ... */);
        let result = some_use_case(&env, /* ... */).await;
        assert!(result.is_ok());
    }
}
```

---

## 14. CODING CONVENTIONS

### 14.1 Naming Conventions

| Item                  | Convention         | Example                                |
| --------------------- | ------------------ | -------------------------------------- |
| Port traits           | `*Port` suffix     | `StoragePort`, `ChunkApiPort`          |
| HTTP adapters         | `Http*` prefix     | `HttpChunkApi`, `HttpUserApi`          |
| Request types         | `*Request` suffix  | `CreateChunkRequest`                   |
| Response types        | `*Response` suffix | `CreateChunkResponse`                  |
| Environment traits    | `*Env` suffix      | `BackupEnv`, `UserEnv`                 |
| Application functions | snake_case verbs   | `backup_file`, `sync_local_files_meta` |

### 14.2 Import Organization

```rust
// 1. Standard library
use std::path::PathBuf;

// 2. External crates
use api_types::...;
use chrono::Utc;
use tokio::sync::mpsc;
use uuid::Uuid;

// 3. Internal crate modules
use crate::{
    adapters::...,
    applications::...,
    model::...,
    ports::...,
};
```

### 14.3 Trait Bounds for Spawned Tasks

When spawning tokio tasks that capture environment:

- Add `Send + Sync + 'static` bounds to the environment type
- Implement `clone_env()` method on environment trait
- Clone owned values into the spawned task

### 14.4 Database Access (Server Side)

- Use `sqlx::query!` macro for compile-time checked SQL
- Repository traits in `core/ports/`
- PostgreSQL implementations in `infra/psql/`
- Map `sqlx::Error` to domain errors via `map_sqlx_error`

## 15. Code Comments
- Always add a brief comment on each function, Explain what it does and reasoning it.
- Optionally add comment inside functions for complicated conditions

## 16. Unit Testing
- Always add unit test for all AI Generated code.
- Add unit test cases while updating any existing function.
- Be very descriptive and add comments in unit test cases.
- Think of developer as a person with short memory, who don't remember why he/she wrote this code yesterday.

---

## 17. REQUIRED REFERENCE DOCUMENTS

AI agents MUST read and follow these companion documents before generating code:

| Document | Path | Purpose |
|----------|------|---------|
| **Architecture** | `.agent/rules/architecture.md` | Full system architecture: crate structure, encryption model, backup/restore pipelines, storage backends, Env pattern, mobile support. Understand the system before changing it. |
| **Contributing** | `.agent/rules/contributing.md` | Ports & adapters rules, layer boundaries, naming conventions, DRY principles, testing patterns, PR requirements. Follow these conventions exactly. |
| **Threat Model** | `.agent/rules/threat-model.md` | Trust boundaries, threat mitigations, security invariants. Never weaken these properties. Every code change must preserve the security invariants listed there. |

### Instructions for AI Models

1. **Before writing any code**, consult `architecture.md` to understand where the code belongs in the crate structure and how it fits the ports & adapters pattern.
2. **Before adding new functionality**, follow the step-by-step process in `contributing.md`: define types in `api_types`, add port traits, implement adapters, wire through Env.
3. **Before modifying security-related code**, review `threat-model.md` to ensure no security invariant is weakened. If unsure, ask.
4. **All three documents are authoritative**: if your suggestion conflicts with any of them, the suggestion is WRONG.
5. **Layer boundaries are non-negotiable**: `applications/` must never import `adapters/` directly. All dependencies flow through port traits and Env accessors.
