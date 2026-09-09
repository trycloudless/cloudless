# CloudLess

**Client-side encrypted backup with chunk deduplication, multi-storage support, and self-hosting.**

CloudLess is a backup engine — not a sync tool. Files are chunked, hashed, compressed, and encrypted entirely on your device before leaving it. The server never sees plaintext data.

---

## Features

- **Zero-knowledge encryption** — AES-256-GCM with per-file data encryption keys (DEKs) wrapped by a user key-encryption key (KEK). The server stores only ciphertext.
- **Content-addressed deduplication** — chunks deduplicated by SHA-256 of plaintext; duplicate content uploaded once per user.
- **Multiple storage backends** — AWS S3, Google Drive (via user OAuth), SFTP, and local filesystem for dev/test.
- **Per-device file versioning** — linear integer versions per `(device_id, path)`. Each device is an independent namespace; files are never merged across devices.
- **Self-hosted mode** — run your own server with Docker Compose. Billing is disabled; all users get full backup features.
- **Desktop app** — Tauri + Leptos (WASM) desktop client for macOS, Linux, and Windows.

---

## Architecture

CloudLess follows **hexagonal architecture** (Ports & Adapters) with a strict priority order: correctness → idempotency → recoverability → security → performance.

```
Encryption pipeline (client-side):
  read → hash (SHA-256) → compress (zstd) → encrypt (AES-256-GCM) → upload

Crate layout:
  api_types/          Shared request/response DTOs
  api_server/         Axum HTTP server (PostgreSQL, JWT auth)
  cloudless_core/     Client library (backup, restore, encryption, storage adapters)
  cloudless_tauri/    Tauri desktop commands
  leptos_ui/          CSR/WASM Leptos UI (runs inside Tauri WebView)
  website/            SSR Leptos marketing + account site
  shared_ui/          Shared presentational components
  cli/                CLI utility
```

See [`docs/engineering/`](docs/engineering/) for detailed architecture documentation:
- [`architecture`](.agent/rules/architecture.md) — encryption model, backup/restore pipelines, storage backends, Env DI pattern
- [`known_issues.md`](docs/engineering/known_issues.md) — current known bugs and limitations

---

## Local Development

### Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Rust stable | ≥ 1.80 | All Rust crates |
| PostgreSQL | ≥ 16 | API server database |
| `sqlx-cli` | latest | Database migrations |
| `trunk` | 0.21.14 | Leptos/WASM frontend build |
| Node.js | ≥ 18 | Tailwind CSS compilation |

Install `sqlx-cli` and `trunk`:

```bash
cargo install sqlx-cli --no-default-features --features postgres
cargo install --locked trunk
rustup target add wasm32-unknown-unknown
```

### 1. Database setup

```bash
# Start a local PostgreSQL instance (or use an existing one)
createdb cloudless

# Set the connection URL (used by sqlx-cli and the server)
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/cloudless"
```

### 2. Run migrations

```bash
cd rust/api_server
sqlx migrate run
```

### 3. API server

Create `rust/api_server/.env`:

```dotenv
DATABASE_URL=postgres://postgres:postgres@localhost:5432/cloudless
JWT_SECRET=dev_secret_change_me
FRONTEND_BASE_URL=http://localhost:3000
BILLING_MODE=disabled
EMAIL_PROVIDER=log
HOST=127.0.0.1
PORT=9000
RUST_LOG=debug
```

Start the server:

```bash
cargo run -p api_server
# Server listens on http://localhost:9000
```

### 4. Leptos UI (desktop frontend)

The desktop UI is a WASM app served inside Tauri's WebView. For browser-based development:

```bash
cd rust/leptos_ui
npm install                # install tailwind
trunk serve                # hot-reload dev server at http://localhost:8080
```

### 5. Tauri desktop app

```bash
# macOS / Linux
cargo tauri dev

# The command builds both the Leptos WASM frontend and the Tauri native shell.
```

### 6. Tests

```bash
# Unit + integration tests (requires DATABASE_URL set)
cargo test -p api_server
cargo test -p cloudless_core

# All workspace tests
cargo test --workspace
```

SQLX queries are checked at compile time. If you change a migration, run:

```bash
cargo sqlx prepare --workspace
```

---

## Self-Hosting

CloudLess ships a Docker Compose configuration for self-hosted deployments. Billing is always disabled in self-hosted mode — all users receive Pro-equivalent limits.

### Prerequisites

- Docker ≥ 24 with Compose v2
- A reverse proxy (Caddy, nginx, Traefik) to terminate TLS

### Setup

```bash
cd deploy/self-hosted

# Copy the example config and fill in the values
cp .env.example .env
```

Edit `.env`:

```dotenv
# Strong random password for PostgreSQL
DB_PASSWORD=change_me_strong_password

# Generate with: openssl rand -base64 32
JWT_SECRET=change_me_strong_secret

# The URL where users access the frontend (used for email links)
FRONTEND_BASE_URL=https://backup.yourdomain.com

BILLING_MODE=disabled

# "log" prints emails to stdout — no external service needed.
# Set to "resend" and add RESEND_API_KEY to send real emails.
EMAIL_PROVIDER=log
```

### Start

```bash
docker compose up -d
```

The API server is available at `http://localhost:9000` by default. Point your reverse proxy at port 9000 and configure TLS.

### Storage configuration

CloudLess stores backup chunks in object storage. Configure one of:

- **AWS S3** — add `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_REGION`, `S3_BUCKET` to `.env`
- **Google Drive** — users authenticate via OAuth from the desktop app (see below)
- **OneDrive** — users authenticate via OAuth from the desktop app (see below)
- **SFTP** — specify connection details in the desktop app

### Environment variable reference

See [`deploy/self-hosted/.env.example`](deploy/self-hosted/.env.example) for the complete environment variable reference.

### Updating

```bash
docker compose pull
docker compose up -d
```

Migrations run automatically on startup.

---

## Building from Source

### API server Docker image

```bash
docker build -f rust/api_server/Dockerfile -t cloudless-api:local .
```

### Tauri desktop app

The desktop app requires OAuth credentials for Google Drive and OneDrive. You must register your own OAuth apps — you cannot use the official CloudLess app credentials (see [Google's ToS](https://developers.google.com/terms) and [Microsoft's ToS](https://learn.microsoft.com/en-us/legal/microsoft-apis/terms-of-use)).

#### Google Drive

1. Go to [Google Cloud Console](https://console.cloud.google.com/) → APIs & Services → Credentials.
2. Create an OAuth 2.0 Client ID, application type **Desktop app**.
3. Enable the **Google Drive API** for the project.
4. Copy the Client ID (no client secret needed — desktop apps use PKCE).

#### Microsoft OneDrive

1. Go to [Azure Portal](https://portal.azure.com/) → App registrations → New registration.
2. Set the redirect URI to `http://localhost` (type: Public client/native).
3. Under Authentication, enable "Allow public client flows".
4. Copy the Application (client) ID.

#### Build with your credentials

Set the variables as environment variables before building. They are baked into the binary at compile time:

```bash
export GOOGLE_CLIENT_ID=your_google_client_id
export ONEDRIVE_CLIENT_ID=your_microsoft_application_id
export API_BASE_URL=https://your-api-server.example.com

cargo tauri build
# Output: rust/tauri/target/release/bundle/
```

For local development, put these in `rust/tauri/.env`:

```dotenv
GOOGLE_CLIENT_ID=your_google_client_id
ONEDRIVE_CLIENT_ID=your_microsoft_application_id
API_BASE_URL=http://localhost:9000
```

The desktop app embeds the Leptos WASM frontend. `trunk` is invoked automatically by the Tauri build process.

---

## Contributing

We follow the **Ports & Adapters** pattern strictly. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full guide: dev workflow, layer boundaries, coding conventions, testing requirements, and PR process.

---

## Security

CloudLess is designed so the server never sees plaintext data. The threat model and security invariants are documented in [`.agent/rules/threat-model.md`](.agent/rules/threat-model.md).

To report a vulnerability, see [`SECURITY.md`](SECURITY.md) — please do not open a public issue.

---

## Documentation

| Document | Contents |
|----------|----------|
| [`docs/engineering/known_issues.md`](docs/engineering/known_issues.md) | Current known bugs |
| [`.agent/rules/architecture.md`](.agent/rules/architecture.md) | Full system architecture reference |
| [`.agent/rules/contributing.md`](.agent/rules/contributing.md) | Contribution guide and patterns |
| [`.agent/rules/threat-model.md`](.agent/rules/threat-model.md) | Security threat model |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | How to contribute: setup, PR process, coding conventions |
| [`SECURITY.md`](SECURITY.md) | How to report a vulnerability |
| [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) | Community standards |

---

## License

[AGPL-3.0](LICENSE)
