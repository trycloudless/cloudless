# Contributing to CloudLess

Thanks for your interest in contributing. CloudLess is a client-side encrypted backup engine: correctness and data safety take priority over everything else, including developer convenience. Please read this guide before opening a PR.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). Be respectful and constructive in issues, PRs, and discussions.

## Before you start

- For small fixes (typos, obvious bugs), open a PR directly.
- For anything larger (new features, storage backends, schema changes), open an issue first to discuss the approach. This project has strict architectural invariants (see below) and it's easier to align before you've written the code than after.
- Security vulnerabilities should **never** be reported via a public issue: see [`SECURITY.md`](SECURITY.md).

## Development setup

See the [Local Development](README.md#local-development) section of the README for installing prerequisites, running migrations, and starting the API server, UI, and desktop app.

## Required reading

This repository follows **Ports & Adapters (Hexagonal) architecture** with Env-based dependency injection, and that's non-negotiable. Before writing code, read:

- [`docs/architecture.md`](docs/architecture.md): crate structure, encryption model, backup/restore pipelines, where code belongs
- [`docs/contributing.md`](docs/contributing.md): layer boundaries, naming conventions, testing patterns, DRY rules
- [`docs/threat-model.md`](docs/threat-model.md): trust boundaries and security invariants that must never be weakened

If a change conflicts with what's documented there, treat the conflict as a signal to ask first, not to route around it.

## Layer boundaries

```
applications/ → ports/ (traits only) ← adapters/
```

- `applications/` contains use-case logic and depends only on port traits, accessed through the `Env` trait.
- `ports/` defines interfaces (traits): no implementation details, no adapter imports.
- `adapters/` implements ports against concrete external systems (HTTP, S3, SQLite, encryption, compression).
- `applications/` must **never** import `adapters/` directly. Only `app_env.rs` and entry points (Tauri commands, route handlers) wire adapters to ports.

## Adding a new feature

Client-side (`core/` crate):
1. Define request/response types in `api_types/`.
2. Add a port trait in `core/src/ports/api/`.
3. Implement the HTTP adapter in `core/src/adapters/api/`.
4. Add the use-case function in `core/src/applications/*/`, taking `env: &E where E: SomeEnv`.
5. Wire the adapter into `core/src/app_env.rs`.

Server-side (`api_server/` crate):
1. Define request/response types in `api_types/`.
2. Add a repo trait in `api_server/src/core/ports/`.
3. Implement it with SQL in `api_server/src/infra/psql/`.
4. Write the application function in `api_server/src/core/*/application.rs`.
5. Add the route handler in `api_server/src/web/routes/`.

## Coding conventions

| Item | Convention | Example |
|------|-----------|---------|
| Port traits | `*Port` suffix | `StoragePort`, `ChunkApiPort` |
| HTTP adapters | `Http*` prefix | `HttpChunkApi`, `HttpUserApi` |
| Request types | `*Request` suffix | `CreateChunkRequest` |
| Response types | `*Response` suffix | `CreateChunkResponse` |
| Environment traits | `*Env` suffix | `BackupEnv`, `UserEnv` |
| Application functions | `snake_case` verbs | `backup_file`, `start_backup` |
| Server repo traits | `*Repo` suffix | `ChunkRepo`, `UserRepo` |

Other rules:
- Rust stable edition. No `unwrap()` in production code: propagate errors with `?`.
- `sqlx` only for database access; no ORMs (Diesel, SeaORM). Prefer explicit SQL.
- Typed errors everywhere: `thiserror`, no `anyhow` in library crates, no `String` errors.
- `tracing` for all logging: never `println!`.
- Shared UI components go in `shared_ui`; shared types go in `api_types`; shared style constants go in `shared_ui::styles`. If you see logic duplicated across crates, extract it before adding more.
- Function-level `///` doc comments are required for public APIs: explain *why*, not just what.

## Testing

- Unit tests are required for all new application logic, and for any existing function you modify.
- Tests mock `Env` traits with hand-written test doubles: no real database or network calls in unit tests.
- Run tests before opening a PR:
  ```bash
  cargo test --workspace
  cargo clippy --workspace
  ```
- If you changed a migration, regenerate the sqlx query cache:
  ```bash
  cargo sqlx prepare --workspace
  ```

## Pull requests

- One concern per PR: don't bundle unrelated changes.
- Explain *why* the change is safe, not just what it does.
- List any invariants your change preserves or touches (device isolation, idempotency, encryption ordering; see [`docs/threat-model.md`](docs/threat-model.md)).
- If the change touches the database schema, include the migration and a rollback plan.
- `cargo test --workspace` and `cargo clippy --workspace` must pass with no warnings.
- Include or update test coverage for the logic you changed.

## AI-generated code

AI-assisted contributions are welcome but must follow the same rules as everything else in this repo: the architecture, testing, and PR requirements above apply regardless of how the code was written. Review AI-generated code for correctness yourself before submitting; don't merge blindly.

## License

By contributing, you agree that your contributions are licensed under the terms in [`LICENSE`](LICENSE).
