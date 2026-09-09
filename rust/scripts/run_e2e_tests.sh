#!/bin/bash
#
# End-to-end integration test runner for Cloudless.
#
# Prerequisites:
#   - PostgreSQL running in Docker (see api_server/.env for connection details)
#   - S3 credentials exported or in api_server/.env
#
# Usage:
#   bash rust/scripts/run_e2e_tests.sh
#
#   # Or with custom S3 creds:
#   TEST_S3_ACCESS_KEY=AKIA... TEST_S3_SECRET=xxx \
#   TEST_S3_REGION=us-east-1 TEST_S3_BUCKET=my-bucket \
#     bash rust/scripts/run_e2e_tests.sh

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$SCRIPT_DIR/.."
API_SERVER_DIR="$PROJECT_ROOT/api_server"
SERVER_PID=""
EXIT_CODE=0

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log()   { echo -e "${GREEN}[e2e]${NC} $*"; }
warn()  { echo -e "${YELLOW}[e2e]${NC} $*"; }
error() { echo -e "${RED}[e2e]${NC} $*"; }

cleanup() {
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        log "Stopping API server (PID $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# ── 1. Load environment from api_server/.env ──────────────────────
log "Loading environment from $API_SERVER_DIR/.env"
if [ ! -f "$API_SERVER_DIR/.env" ]; then
    error "api_server/.env not found. Create it with DATABASE_URL, JWT_SECRET, etc."
    exit 1
fi

set -a
source "$API_SERVER_DIR/.env"
set +a

# Override port for tests to avoid conflicts with running dev server
export PORT="${E2E_TEST_PORT:-9099}"
export HOST="0.0.0.0"
export API_BASE_URL="http://localhost:${PORT}"

# ── 2. Validate S3 credentials ───────────────────────────────────
if [ -z "${TEST_S3_ACCESS_KEY:-}" ] || [ -z "${TEST_S3_SECRET:-}" ] || \
   [ -z "${TEST_S3_REGION:-}" ] || [ -z "${TEST_S3_BUCKET:-}" ]; then
    error "S3 credentials required. Export these env vars:"
    error "  TEST_S3_ACCESS_KEY, TEST_S3_SECRET, TEST_S3_REGION, TEST_S3_BUCKET"
    exit 1
fi

log "S3 bucket: $TEST_S3_BUCKET ($TEST_S3_REGION)"

# ── 3. Reset the database ────────────────────────────────────────
log "Resetting database..."
bash "$API_SERVER_DIR/reset_db.sh"

# ── 4. Run database migrations ───────────────────────────────────
# sqlx compile-time query checking requires the schema to exist before building.
log "Running database migrations..."
if command -v sqlx &> /dev/null; then
    sqlx migrate run --source "$API_SERVER_DIR/migrations"
else
    # Fall back to cargo-sqlx if sqlx CLI is not on PATH
    cargo sqlx migrate run --source "$API_SERVER_DIR/migrations"
fi

# ── 5. Build the API server ──────────────────────────────────────
# Touch main.rs to force recompile — cargo may not detect changes in the
# migrations/ directory since sqlx::migrate!() reads them at compile time
# but cargo only tracks src/ files automatically.
log "Building API server..."
cd "$PROJECT_ROOT"
touch "$API_SERVER_DIR/src/main.rs"
cargo build -p api_server 2>&1 | tail -1

# ── 6. Start the API server in the background ────────────────────
# The server's MIGRATOR.run() is idempotent — migrations already applied
# by step 4 will be skipped.
log "Starting API server on port $PORT..."
cargo run -p api_server &
SERVER_PID=$!

# Wait for the server to be ready
log "Waiting for API server to be ready..."
MAX_WAIT=30
for i in $(seq 1 $MAX_WAIT); do
    if curl -sf "http://localhost:${PORT}/health" > /dev/null 2>&1; then
        log "API server ready (took ${i}s)"
        break
    fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        error "API server process died during startup"
        exit 1
    fi
    if [ "$i" -eq "$MAX_WAIT" ]; then
        error "API server failed to start within ${MAX_WAIT}s"
        exit 1
    fi
    sleep 1
done

# ── 7. Run the integration tests ─────────────────────────────────
log "Running E2E integration tests..."
cd "$PROJECT_ROOT"

# Pass env vars to the test binary
export API_BASE_URL
export TEST_S3_ACCESS_KEY
export TEST_S3_SECRET
export TEST_S3_REGION
export TEST_S3_BUCKET

if cargo test -p cloudless_core --test e2e_backup_restore -- --nocapture 2>&1; then
    log "All E2E tests passed!"
    EXIT_CODE=0
else
    error "E2E tests failed!"
    EXIT_CODE=1
fi

# ── 8. Cleanup (trap handles server shutdown) ─────────────────────
exit $EXIT_CODE
