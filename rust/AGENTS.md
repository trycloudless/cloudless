# Rust Workspace — AI Agent Rules

This file covers the rules AI agents most commonly violate in this codebase.
Full rules are in `/.agent/rules/agents.md` (also the repo-root `AGENTS.md`).

---

## Architecture: Ports & Adapters (Non-Negotiable)

```
┌─────────────────────────────────────────────────┐
│  Entry points: Tauri commands / Route handlers  │
├─────────────────────────────────────────────────┤
│  applications/  (business logic)                │  ← imports ports/ ONLY
│    └── env.rs   (Env trait)                     │
├─────────────────────────────────────────────────┤
│  ports/         (trait definitions)             │  ← pure interfaces
├─────────────────────────────────────────────────┤
│  adapters/      (concrete implementations)      │  ← implements port traits
├─────────────────────────────────────────────────┤
│  app_env.rs     (wiring)                        │  ← only place that imports adapters
└─────────────────────────────────────────────────┘
```

**`applications/` MUST NOT import `adapters/` directly. Ever.**
All dependencies flow through Env trait accessors (`env.some_api()`).

---

## How to Add New Functionality

### Client-side (`core/` crate)
1. Define request/response types in `api_types/`
2. Add a port trait in `core/src/ports/api/some_api_port.rs`
3. Implement the HTTP adapter in `core/src/adapters/api/http_some_api.rs`
4. Add associated type + accessor to the relevant `Env` trait in `core/src/applications/*/env.rs`
5. Wire the adapter in `core/src/app_env.rs`
6. Write business logic in `core/src/applications/*/` using only `env.some_api()` calls

### Server-side (`api_server/` crate)
1. Define request/response types in `api_types/`
2. Add a repo trait in `api_server/src/core/ports/`
3. Implement with SQL in `api_server/src/infra/psql/`
4. Write the application function in `api_server/src/core/*/application.rs`
5. Add the route handler in `api_server/src/web/routes/`
6. Wire the repo through the server's Env trait

---

## Naming Conventions

| Item | Convention | Example |
|------|-----------|---------|
| Client port traits | `*Port` suffix | `ChunkApiPort`, `StoragePort` |
| Server repo traits | `*Repo` suffix | `ChunkRepo`, `UserRepo` |
| HTTP client adapters | `Http*` prefix | `HttpChunkApi` |
| Request types | `*Request` suffix | `CreateChunkRequest` |
| Response types | `*Response` suffix | `CreateChunkResponse` |
| Env traits | `*Env` suffix | `BackupEnv`, `UserEnv` |
| Application functions | `snake_case` verbs | `backup_file`, `start_backup` |

---

## Error Handling

```rust
// Client-side (core crate) — model/app_error.rs
pub enum AppError { NotFound, PermissionDenied, Network, Validation, Internal, AuthTokenExpired }
pub type AppResult<T> = Result<T, AppError>;

// API port layer (ports/api/mod.rs)
pub enum ApiClientError { Transport(reqwest::Error), InvalidUrl(String), Api(ApiError) }
pub type ApiResult<T> = Result<T, ApiClientError>;

// Server-side (api_server/src/core/mod.rs)
pub enum CoreError { NotFound, Validation, Authentication, Forbidden, Conflict, Database, ExternalService, Internal }
```

Rules:
- Use `thiserror` for all error enums. No `anyhow` in library crates.
- `ApiResult` for port traits in `ports/api/`. `AppResult` for application logic.
- `From<ApiClientError> for AppError` is implemented — use `?` to propagate across the boundary.
- Map `sqlx::Error` to `CoreError` via `map_sqlx_error` on the server side.

---

## Hard Rules

- **No `unwrap()`** in production code (allowed in tests)
- **No `println!`** — use `tracing::{info, warn, error, debug}` everywhere
- **`sqlx` only** for database access — no ORMs
- **No blocking I/O on async paths** — use `tokio::fs`, not `std::fs`
- **`Uuid::now_v7()`** not `Uuid::new_v4()` (v4 feature is not enabled)
- **`#[async_trait]`** on all async trait impls
- **Add doc comments** (`///`) on every public function explaining what and why
- **Add unit tests** for all new functions — mock the Env trait, never hit real services

---

## Key Invariants (Never Violate)

- Encryption order: `read → hash → compress → encrypt → upload`
- Chunk hash = SHA-256 of **plaintext** (before compression/encryption)
- File identity = `(backup_config_id, blind_index)` — same path on two devices is NOT the same file
- PostgreSQL is the only source of truth — never infer state from object storage
- Idempotency keys required for file version creation and chunk completion
- Server never stores plaintext file data or paths
