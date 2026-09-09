#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# CloudLess Test Runner
#
# Run unit, integration, and functional (E2E) tests separately or together.
#
# Usage:
#   ./run-functional-test.sh                  # Run all test suites
#   ./run-functional-test.sh unit             # Unit tests only
#   ./run-functional-test.sh integration      # Integration tests (needs Postgres)
#   ./run-functional-test.sh functional       # Functional E2E tests (needs Postgres + Tauri)
#   ./run-functional-test.sh unit integration # Multiple suites
#
# E2E services run on isolated ports to avoid clashing with dev:
#   Postgres:   54322  (dev uses 54321)
#   API server: 9099   (dev uses 9000)
#   WebDriver:  4445   (embedded in Tauri app)
#
# All logs are written to e2e/_logs/ for post-mortem debugging.
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$SCRIPT_DIR/e2e"
RUST_DIR="$SCRIPT_DIR/rust"
LOG_DIR="$E2E_DIR/_logs"

# Isolated e2e ports
E2E_PG_PORT=54322
E2E_API_PORT=9099
E2E_PG_CONTAINER="cloudless-e2e-postgres"
E2E_PG_DB="cloudless_e2e"
E2E_PG_USER="postgres"
E2E_PG_PASS="postgres"
E2E_DATABASE_URL="postgres://${E2E_PG_USER}:${E2E_PG_PASS}@localhost:${E2E_PG_PORT}/${E2E_PG_DB}"
E2E_API_BASE_URL="http://localhost:${E2E_API_PORT}"
E2E_SFTP_PORT="${E2E_SFTP_PORT:-2223}"
E2E_SFTP_CONTAINER="${E2E_SFTP_CONTAINER:-cloudless-e2e-sftp}"
E2E_SFTP_USER="${E2E_SFTP_USER:-cloudless}"
E2E_SFTP_PASS="${E2E_SFTP_PASS:-password}"
E2E_SFTP_DIR="${E2E_SFTP_DIR:-/tmp/cloudless-e2e-sftp}"
E2E_SFTP_REMOTE_ROOT="${E2E_SFTP_REMOTE_ROOT:-/upload/cloudless}"

API_PID=""
POSTGRES_STARTED=false
SFTP_STARTED=false
EXIT_CODE=0

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

log()     { echo -e "${CYAN}[test]${NC} $*"; }
warn()    { echo -e "${YELLOW}[test]${NC} $*"; }
err()     { echo -e "${RED}[test]${NC} $*" >&2; }
ok()      { echo -e "${GREEN}[test]${NC} $*"; }
section() { echo -e "\n${BOLD}${CYAN}── $* ──${NC}\n"; }

# ── Usage ────────────────────────────────────────────────────────────────────
usage() {
    echo "Usage: $0 [unit] [integration] [functional]"
    echo ""
    echo "Suites:"
    echo "  unit          Rust unit tests (no external dependencies)"
    echo "  integration   Integration tests (spins up Postgres + API server)"
    echo "  functional    WebdriverIO E2E tests (spins up Postgres + API + Tauri app)"
    echo ""
    echo "No arguments runs all three suites."
    echo ""
    echo "Examples:"
    echo "  $0                        # Run everything"
    echo "  $0 unit                   # Unit tests only"
    echo "  $0 integration functional # Integration + functional"
    echo ""
    echo "Logs (on failure): $LOG_DIR/"
    exit 1
}

# ── Cleanup ──────────────────────────────────────────────────────────────────
cleanup() {
    log "Cleaning up..."

    if [ -n "$API_PID" ] && kill -0 "$API_PID" 2>/dev/null; then
        log "Stopping API server (PID $API_PID)"
        kill "$API_PID"
        wait "$API_PID" 2>/dev/null || true
    fi

    if [ "$POSTGRES_STARTED" = true ]; then
        if docker ps -q --filter "name=$E2E_PG_CONTAINER" | grep -q .; then
            log "Stopping Postgres container"
            docker rm -f "$E2E_PG_CONTAINER" >/dev/null 2>&1 || true
        fi
    fi

    if [ "$SFTP_STARTED" = true ]; then
        if docker ps -q --filter "name=$E2E_SFTP_CONTAINER" | grep -q .; then
            log "Stopping SFTP container"
            docker rm -f "$E2E_SFTP_CONTAINER" >/dev/null 2>&1 || true
        fi
    fi

    if [ $EXIT_CODE -ne 0 ] && [ -d "$LOG_DIR" ]; then
        warn "Logs saved in: $LOG_DIR/"
        warn "  postgres:    $LOG_DIR/postgres.log"
        warn "  sftp:        $LOG_DIR/sftp.log"
        warn "  api server:  $LOG_DIR/api-server.log"
        warn "  wdio tests:  $LOG_DIR/wdio.log"
        warn "  screenshots: $E2E_DIR/screenshots/"
        warn "  videos:      $E2E_DIR/_results_/"
    fi
}
trap cleanup EXIT

# ── Preflight ────────────────────────────────────────────────────────────────
check_prereqs() {
    local missing=0
    for cmd in cargo; do
        if ! command -v "$cmd" &>/dev/null; then
            err "Required: $cmd"
            missing=1
        fi
    done

    if [ "$NEED_DOCKER" = true ]; then
        if ! command -v docker &>/dev/null; then
            err "Required: docker (for integration/functional tests)"
            missing=1
        elif ! docker info >/dev/null 2>&1; then
            err "Docker daemon is not running."
            missing=1
        fi
    fi

    if [ "$RUN_INTEGRATION" = true ]; then
        if ! command -v ssh-keygen &>/dev/null; then
            err "Required: ssh-keygen (for SFTP integration tests)"
            missing=1
        fi
        if [ -z "${CLOUDLESS_S3_KEY:-}" ] || [ -z "${CLOUDLESS_S3_SECRET:-}" ]; then
            err "Required for integration tests: CLOUDLESS_S3_KEY and CLOUDLESS_S3_SECRET"
            err "Optional: CLOUDLESS_S3_REGION, CLOUDLESS_S3_BUCKET"
            missing=1
        fi
    fi

    if [ "$RUN_FUNCTIONAL" = true ]; then
        for cmd in node npm; do
            if ! command -v "$cmd" &>/dev/null; then
                err "Required: $cmd (for functional tests)"
                missing=1
            fi
        done
    fi

    [ $missing -eq 0 ] || exit 1
}

# ── Postgres ─────────────────────────────────────────────────────────────────
start_postgres() {
    if [ "$POSTGRES_STARTED" = true ]; then
        return
    fi

    section "Starting Postgres (port $E2E_PG_PORT)"

    docker rm -f "$E2E_PG_CONTAINER" >/dev/null 2>&1 || true

    docker run -d \
        --name "$E2E_PG_CONTAINER" \
        -p "$E2E_PG_PORT:5432" \
        -e POSTGRES_USER="$E2E_PG_USER" \
        -e POSTGRES_PASSWORD="$E2E_PG_PASS" \
        -e POSTGRES_DB="$E2E_PG_DB" \
        --tmpfs /var/lib/postgresql/data \
        --health-cmd "pg_isready -U $E2E_PG_USER" \
        --health-interval 3s \
        --health-timeout 2s \
        --health-retries 10 \
        postgres:17 \
        > "$LOG_DIR/postgres.log" 2>&1

    log "Waiting for Postgres..."
    local retries=0
    while [ $retries -lt 30 ]; do
        local status
        status=$(docker inspect --format='{{.State.Health.Status}}' "$E2E_PG_CONTAINER" 2>/dev/null || echo "missing")
        if [ "$status" = "healthy" ]; then
            ok "Postgres is ready."
            POSTGRES_STARTED=true
            return
        fi
        retries=$((retries + 1))
        sleep 1
    done

    err "Postgres failed to become healthy within 30s."
    docker logs "$E2E_PG_CONTAINER" >> "$LOG_DIR/postgres.log" 2>&1
    err "Last 10 lines of postgres.log:"
    tail -10 "$LOG_DIR/postgres.log" >&2
    exit 1
}

# ── SFTP ─────────────────────────────────────────────────────────────────────
start_sftp() {
    if [ "$SFTP_STARTED" = true ]; then
        return
    fi

    section "Starting SFTP (port $E2E_SFTP_PORT)"

    docker rm -f "$E2E_SFTP_CONTAINER" >/dev/null 2>&1 || true
    rm -rf "$E2E_SFTP_DIR"
    mkdir -p "$E2E_SFTP_DIR/upload"

    ssh-keygen -t ed25519 -N "" -f "$E2E_SFTP_DIR/id_ed25519" \
        > "$LOG_DIR/sftp.log" 2>&1

    docker run -d \
        --name "$E2E_SFTP_CONTAINER" \
        -p "127.0.0.1:${E2E_SFTP_PORT}:22" \
        -v "$E2E_SFTP_DIR/id_ed25519.pub:/home/${E2E_SFTP_USER}/.ssh/keys/id_ed25519.pub:ro" \
        -v "$E2E_SFTP_DIR/upload:/home/${E2E_SFTP_USER}/upload" \
        atmoz/sftp:latest \
        "${E2E_SFTP_USER}:${E2E_SFTP_PASS}:::upload" \
        >> "$LOG_DIR/sftp.log" 2>&1

    log "Waiting for SFTP..."
    local retries=0
    while [ $retries -lt 30 ]; do
        if docker logs "$E2E_SFTP_CONTAINER" 2>&1 | grep -q "Server listening"; then
            ok "SFTP is ready at 127.0.0.1:$E2E_SFTP_PORT"
            SFTP_STARTED=true
            return
        fi
        if ! docker ps -q --filter "name=$E2E_SFTP_CONTAINER" | grep -q .; then
            err "SFTP container exited unexpectedly."
            docker logs "$E2E_SFTP_CONTAINER" >> "$LOG_DIR/sftp.log" 2>&1 || true
            err "Last 20 lines of sftp.log:"
            tail -20 "$LOG_DIR/sftp.log" >&2
            exit 1
        fi
        retries=$((retries + 1))
        sleep 1
    done

    err "SFTP failed to start within 30s."
    docker logs "$E2E_SFTP_CONTAINER" >> "$LOG_DIR/sftp.log" 2>&1 || true
    err "Last 20 lines of sftp.log:"
    tail -20 "$LOG_DIR/sftp.log" >&2
    exit 1
}

# ── API Server ───────────────────────────────────────────────────────────────
start_api_server() {
    if [ -n "$API_PID" ] && kill -0 "$API_PID" 2>/dev/null; then
        return
    fi

    section "Starting API server (port $E2E_API_PORT)"

    # Kill any stale api_server from previous runs so the port is free.
    # Without this, cargo run exits immediately with "Address already in use"
    # but the health check succeeds against the OLD server (wrong binary/DB).
    pkill -f "target/debug/api_server" 2>/dev/null || true
    sleep 1

    SQLX_OFFLINE=true \
    DATABASE_URL="$E2E_DATABASE_URL" \
    JWT_SECRET="e2e-test-secret" \
    JWT_ACCESS_TTL_SECS=3600 \
    HOST="0.0.0.0" \
    PORT="$E2E_API_PORT" \
    FRONTEND_BASE_URL="$E2E_API_BASE_URL" \
    EMAIL_PROVIDER="log" \
    SES_ENABLED="false" \
    BILLING_MODE="enforce" \
    RUST_LOG="api_server=debug,tower_http=info" \
    cargo run -p api_server --manifest-path "$RUST_DIR/Cargo.toml" \
        > "$LOG_DIR/api-server.log" 2>&1 &
    API_PID=$!

    log "Waiting for API server (PID $API_PID)..."
    local retries=0
    while [ $retries -lt 60 ]; do
        if curl -sf "$E2E_API_BASE_URL/health" >/dev/null 2>&1; then
            ok "API server is ready at $E2E_API_BASE_URL"
            return
        fi
        if ! kill -0 "$API_PID" 2>/dev/null; then
            err "API server exited unexpectedly."
            err "Last 20 lines of api-server.log:"
            tail -20 "$LOG_DIR/api-server.log" >&2
            exit 1
        fi
        retries=$((retries + 1))
        sleep 1
    done

    err "API server failed to start within 60s."
    err "Last 20 lines of api-server.log:"
    tail -20 "$LOG_DIR/api-server.log" >&2
    exit 1
}

# ── Test suites ──────────────────────────────────────────────────────────────
run_unit_tests() {
    section "Unit Tests"
    cd "$RUST_DIR"
    SQLX_OFFLINE=true cargo test -p cloudless_core --lib 2>&1
    SQLX_OFFLINE=true cargo test -p api_types --lib 2>&1
    ok "Unit tests passed."
}

run_integration_tests() {
    section "Integration Tests"

    start_postgres
    start_sftp
    start_api_server

    cd "$RUST_DIR"
    API_BASE_URL="$E2E_API_BASE_URL" \
    TEST_S3_ACCESS_KEY="${CLOUDLESS_S3_KEY:-}" \
    TEST_S3_SECRET="${CLOUDLESS_S3_SECRET:-}" \
    TEST_S3_REGION="${CLOUDLESS_S3_REGION:-us-east-1}" \
    TEST_S3_BUCKET="${CLOUDLESS_S3_BUCKET:-cloudless-e2e-tests-local}" \
    CLOUDLESS_SFTP_TEST_HOST="127.0.0.1" \
    CLOUDLESS_SFTP_TEST_PORT="$E2E_SFTP_PORT" \
    CLOUDLESS_SFTP_TEST_USERNAME="$E2E_SFTP_USER" \
    CLOUDLESS_SFTP_TEST_PASSWORD="$E2E_SFTP_PASS" \
    CLOUDLESS_SFTP_TEST_REMOTE_ROOT="$E2E_SFTP_REMOTE_ROOT" \
    CLOUDLESS_SFTP_TEST_PRIVATE_KEY="$(cat "$E2E_SFTP_DIR/id_ed25519")" \
    RUST_LOG=error \
    cargo test -p cloudless_core --test e2e_backup_restore -- --nocapture 2>&1
    local test_exit=$?

    if [ $test_exit -ne 0 ]; then
        err "Integration tests failed (exit code $test_exit)"
        return $test_exit
    fi
    ok "Integration tests passed."
}

run_functional_tests() {
    section "Functional Tests (WebdriverIO E2E)"

    start_postgres
    start_api_server

    log "Building Leptos frontend (WASM)..."
    cd "$RUST_DIR/leptos_ui"
    npx tailwindcss -i ./input.css -o ./output.css --minify 2>&1 | tail -3
    NO_COLOR=true trunk build 2>&1 | tail -5
    ok "Frontend built."

    log "Building Tauri app with e2e feature..."
    cd "$RUST_DIR"
    cargo build -p cloudless_tauri --features e2e 2>&1 | tail -5
    ok "Tauri app built."

    log "Clearing previous screenshots..."
    rm -rf "$E2E_DIR/screenshots"
    mkdir -p "$E2E_DIR/screenshots"

    log "Installing E2E npm dependencies..."
    cd "$E2E_DIR"
    npm install --silent

    # Create temp directories for e2e tests
    mkdir -p /tmp/cloudless-e2e-backup
    mkdir -p /tmp/cloudless-e2e-local-storage
    echo "Hello from e2e test - file 1" > /tmp/cloudless-e2e-backup/test-file-1.txt
    echo "Hello from e2e test - file 2" > /tmp/cloudless-e2e-backup/test-file-2.txt
    echo '{"key": "value", "nested": {"a": 1}}' > /tmp/cloudless-e2e-backup/test-data.json
    dd if=/dev/urandom bs=1024 count=64 of=/tmp/cloudless-e2e-backup/test-binary.bin 2>/dev/null
    mkdir -p /tmp/cloudless-e2e-backup/subdir
    echo "Nested file in subdirectory" > /tmp/cloudless-e2e-backup/subdir/nested-file.txt
    # System artifacts — these should be backed up but hidden in the Files view.
    touch /tmp/cloudless-e2e-backup/.DS_Store
    touch /tmp/cloudless-e2e-backup/Thumbs.db

    # Kill any stale cloudless processes from previous runs before starting wdio.
    # A leftover process holding port 4445 causes the embedded WebDriver poll to
    # time out on the first worker, failing the login and signup test groups.
    pkill -f "target/debug/cloudless" 2>/dev/null || true
    sleep 1

    log "Running WebdriverIO tests..."
    NODE_OPTIONS="--max-old-space-size=8192" \
    E2E_API_BASE_URL="$E2E_API_BASE_URL" \
    CLOUDLESS_S3_KEY="${CLOUDLESS_S3_KEY:-}" \
    CLOUDLESS_S3_SECRET="${CLOUDLESS_S3_SECRET:-}" \
    CLOUDLESS_S3_REGION="${CLOUDLESS_S3_REGION:-us-east-1}" \
    CLOUDLESS_S3_BUCKET="${CLOUDLESS_S3_BUCKET:-cloudless-e2e-tests-local}" \
    npx wdio run wdio.conf.ts 2>&1 | tee "$LOG_DIR/wdio.log"
    local test_exit=${PIPESTATUS[0]}

    if [ $test_exit -ne 0 ]; then
        err "Functional tests failed (exit code $test_exit)"
        return $test_exit
    fi
    ok "Functional tests passed."
}

# ── Main ─────────────────────────────────────────────────────────────────────
main() {
    # Parse arguments
    RUN_UNIT=false
    RUN_INTEGRATION=false
    RUN_FUNCTIONAL=false

    if [ $# -eq 0 ]; then
        # No args: run all
        RUN_UNIT=true
        RUN_INTEGRATION=true
        RUN_FUNCTIONAL=true
    else
        for arg in "$@"; do
            case "$arg" in
                unit)         RUN_UNIT=true ;;
                integration)  RUN_INTEGRATION=true ;;
                functional)   RUN_FUNCTIONAL=true ;;
                -h|--help)    usage ;;
                *)            err "Unknown suite: $arg"; usage ;;
            esac
        done
    fi

    NEED_DOCKER=false
    if [ "$RUN_INTEGRATION" = true ] || [ "$RUN_FUNCTIONAL" = true ]; then
        NEED_DOCKER=true
    fi

    mkdir -p "$LOG_DIR"

    echo ""
    log "========================================="
    log "  CloudLess Test Runner"
    log "========================================="
    log "  Suites:    $([ "$RUN_UNIT" = true ] && echo 'unit ')$([ "$RUN_INTEGRATION" = true ] && echo 'integration ')$([ "$RUN_FUNCTIONAL" = true ] && echo 'functional')"
    if [ "$NEED_DOCKER" = true ]; then
        log "  Postgres:  localhost:$E2E_PG_PORT/$E2E_PG_DB"
        log "  API:       $E2E_API_BASE_URL"
    fi
    if [ "$RUN_INTEGRATION" = true ]; then
        log "  SFTP:      localhost:$E2E_SFTP_PORT ($E2E_SFTP_CONTAINER)"
    fi
    if [ "$RUN_FUNCTIONAL" = true ]; then
        log "  WebDriver: localhost:4445 (embedded)"
    fi
    log "  Logs:      $LOG_DIR/"
    log "========================================="
    echo ""

    check_prereqs

    local failed_suites=()

    if [ "$RUN_UNIT" = true ]; then
        if ! run_unit_tests; then
            failed_suites+=("unit")
        fi
    fi

    if [ "$RUN_INTEGRATION" = true ]; then
        if ! run_integration_tests; then
            failed_suites+=("integration")
        fi
    fi

    if [ "$RUN_FUNCTIONAL" = true ]; then
        if ! run_functional_tests; then
            failed_suites+=("functional")
        fi
    fi

    # Summary
    echo ""
    if [ ${#failed_suites[@]} -eq 0 ]; then
        ok "========================================="
        ok "  All test suites passed!"
        ok "========================================="
    else
        EXIT_CODE=1
        err "========================================="
        err "  Failed suites: ${failed_suites[*]}"
        err "========================================="
        err ""
        err "Debug:"
        err "  Postgres logs:    cat $LOG_DIR/postgres.log"
        err "  API server logs:  cat $LOG_DIR/api-server.log"
        err "  WDIO test logs:   cat $LOG_DIR/wdio.log"
        err "  Screenshots:      ls $E2E_DIR/screenshots/"
        err "  Videos:           ls $E2E_DIR/_results_/"
    fi

    exit $EXIT_CODE
}

main "$@"
