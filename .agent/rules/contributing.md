---
trigger: always_on
---

# Contributing Guidelines

This repository is **correctness-sensitive**.

---

## Architecture: Ports & Adapters (Mandatory)

This project strictly follows the **Ports & Adapters (Hexagonal) architecture** with Env-based dependency injection. This is non-negotiable.

### Core Rules

1. **Business logic lives in `applications/`** — it depends only on port traits, never on concrete adapters or frameworks.
2. **Ports are traits** defined in `ports/` — they describe what the application needs (e.g. `StoragePort`, `BackupConfigApiPort`). Ports never import adapter code.
3. **Adapters implement ports** in `adapters/` — they bridge external systems (HTTP APIs, S3, filesystem, encryption) to the port interfaces.
4. **Dependency injection via Env traits** — each application domain defines an `Env` trait (e.g. `BackupEnv`) that groups the port types it needs. The concrete `AppEnv` (in `core/src/app_env.rs`) wires all adapters. Functions accept `E: SomeEnv` as a generic parameter.
5. **Never bypass the Env pattern** — do not construct adapters directly inside business logic. Always access dependencies through `env.some_api()` or `env.some_repo()`.
6. **Server-side follows the same pattern** — the `api_server` crate has its own `core/ports/` traits (repo traits), `infra/psql/` adapters (Postgres implementations), and `core/*/env.rs` Env traits. Route handlers delegate to `core/*/application.rs` functions.
7. **Dependency flow** — `applications/` → imports `ports/` traits only. `adapters/` → imports `ports/` traits and implements them. `app_env.rs` → imports both `ports/` and `adapters/` to wire them together. No other module should import `adapters/` directly.

### Adding New Functionality (Client-Side)

When adding a new capability on the client (`core/` crate):
1. Define the request/response types in `api_types/`
2. Add a port trait (or method on existing trait) in `core/src/ports/api/`
3. Implement the HTTP adapter in `core/src/adapters/api/` (e.g. `HttpSomeApi`)
4. Add an associated type + accessor method to the relevant `Env` trait in `core/src/applications/*/env.rs`
5. Wire the adapter in `core/src/app_env.rs`
6. Write the application logic in `core/src/applications/*/` using only `env.some_api()` calls

### Adding New Functionality (Server-Side)

When adding a new capability on the server (`api_server/` crate):
1. Define the request/response types in `api_types/`
2. Add a repo trait in `api_server/src/core/ports/`
3. Implement the repo trait with SQL in `api_server/src/infra/psql/`
4. Write the application function in `api_server/src/core/*/application.rs`
5. Add the route handler in `api_server/src/web/routes/`
6. Wire the repo through the server's Env trait

### Testing

- Unit tests mock ports by implementing the trait with test doubles
- Stub ports use `unimplemented!()` for methods not under test
- Mock ports track calls via `Arc<Mutex<Vec<...>>>` for assertions
- Tests must never depend on real external services

### Port Naming Conventions

| Item | Convention | Example |
|------|-----------|---------|
| Port traits | `*Port` suffix | `StoragePort`, `ChunkApiPort` |
| HTTP adapters | `Http*` prefix | `HttpChunkApi`, `HttpUserApi` |
| Request types | `*Request` suffix | `CreateChunkRequest` |
| Response types | `*Response` suffix | `CreateChunkResponse` |
| Environment traits | `*Env` suffix | `BackupEnv`, `UserEnv` |
| Application functions | snake_case verbs | `backup_file`, `start_backup` |
| Server repo traits | `*Repo` suffix | `ChunkRepo`, `UserRepo` |

### Layer Boundaries

```
┌─────────────────────────────────────────────┐
│  Tauri Commands / Route Handlers            │  ← entry points
├─────────────────────────────────────────────┤
│  applications/ (business logic)             │  ← uses Env traits only
│    └── env.rs (Env trait definition)        │
├─────────────────────────────────────────────┤
│  ports/ (trait definitions)                 │  ← pure interfaces
├─────────────────────────────────────────────┤
│  adapters/ (concrete implementations)       │  ← implements port traits
├─────────────────────────────────────────────┤
│  app_env.rs (wiring)                        │  ← connects adapters to ports
└─────────────────────────────────────────────┘
```

**Import rules**: `applications/` imports only `ports/`. `adapters/` imports `ports/` to implement them. Only `app_env.rs` and entry points (Tauri commands, route handlers) import `adapters/`.

---

## Functional Design Patterns

1. **Pure functions over stateful methods** — prefer free functions that take `&env` over methods on structs carrying state. Application functions are standalone `async fn` that receive `env` as a parameter.
2. **Compose small functions** — break complex operations into focused, single-responsibility helper functions (e.g. `create_file_version`, `upload_or_deduplicate_chunk`, `register_chunk`). Each function should do one thing clearly.
3. **Data flows through function parameters and return values** — avoid hidden side effects. Make inputs and outputs explicit in function signatures.
4. **Use `Result<T, E>` for all fallible operations** — propagate errors with `?`. Never panic in application logic.
5. **Callbacks and closures for extensibility** — shared components (e.g. `LoginFormView`) use callbacks (`Callback<T>`) or optional props to adapt behavior without inheritance.

---

## Rust Idiomatic Patterns

1. **Trait-based polymorphism** — use traits and generics for abstraction, not dynamic dispatch (unless trait objects are required, e.g. `Box<dyn StoragePort>`).
2. **Ownership and borrowing** — pass `&self` and `&T` by default. Only clone when necessary. Use `Arc` for shared ownership across async tasks.
3. **Type-driven design** — encode invariants in the type system. Use newtype wrappers (e.g. `ObjectKey`, `Dek`) rather than raw primitives.
4. **Error types** — use `thiserror` for defining error enums. Domain errors (`AppError`) are separate from API errors (`ApiClientError`) and server errors (`CoreError`).
5. **`async_trait`** — use `#[async_trait]` for async trait methods until native async traits are stable.
6. **Derive when possible** — use `#[derive(Debug, Clone, Serialize, Deserialize)]` on data types. Implement traits manually only when custom behavior is needed.
7. **Module organization** — one concern per file. Use `mod.rs` only for re-exports. Keep files focused and under 300 lines where practical.

---

## DRY Principle (Strict)

1. **Shared presentational components** go in `shared_ui` — any UI component used by both `leptos_ui` (CSR/Tauri) and `website` (SSR) must be extracted to `shared_ui`. No duplication across the two.
2. **Shared types** go in `api_types` — request/response structs, enums, and DTOs are defined once and shared across all crates.
3. **Shared style constants** go in `shared_ui::styles` — Tailwind class strings used across components are constants, not inline strings.
4. **Utility functions** are shared — functions like `format_bytes`, `format_date` live in `shared_ui::utils` or `cloudless_core` as appropriate.
5. **Test helpers** — when three or more test modules need the same stub implementation, extract it to a shared test utilities module. Do not copy-paste stubs across files.
6. **If you see duplication, fix it** — when touching code that duplicates logic found elsewhere, extract the common part before proceeding.

---

## Rules for Contributors

1. Read `agents.md` before coding
2. Do not change schema without migration
3. Do not weaken idempotency
4. Do not introduce local DBs on device
5. Prefer explicit SQL over ORM abstractions

---

## Code Style

- Rust stable edition
- `sqlx` only for DB access
- No `unwrap()` in production code (allowed in tests)
- Typed errors everywhere — no `String` errors, no `anyhow` in library code
- Explicit transactions for multi-statement DB operations
- Document public functions with `///` doc comments explaining what and why

---

## PR Requirements

- Explain why change is safe
- List invariants preserved
- Include rollback plan if schema changes
- Demonstrate that ports & adapters boundaries are respected

---

## AI-Generated Code

AI-generated code must:
- Follow `agents.md` and this contributing guide
- Respect ports & adapters architecture — no shortcuts
- Be reviewed for correctness
- Never be merged blindly
