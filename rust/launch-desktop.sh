#!/bin/bash
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

echo "Starting build process forCloudLess Desktop..."

# Write dev config (override API URL, inherit everything else from .env)
# cp "$DIR/tauri/.env" "$DIR/tauri/.env.dev"
# sed -i '' 's|^API_BASE_URL=.*|API_BASE_URL=http://127.0.0.1:9000/|' "$DIR/tauri/.env.dev"
# cleanup() { rm -f "$DIR/tauri/.env.dev"; }
# trap cleanup EXIT

# 1. Build Leptos Frontend
echo "Building Leptos frontend..."
cd "$DIR/leptos_ui"

if ! command -v trunk &> /dev/null; then
    echo "Error: 'trunk' is not installed. Please run 'cargo install --locked trunk' first."
    exit 1
fi

if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo "Installing missing wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
fi

npm install --prefer-offline
npx tailwindcss -i ./input.css -o ./output.css --minify
trunk build

# 2. Run Tauri App
echo "Launching Tauri application..."
cd "$DIR/tauri"

LOG_FILE="$HOME/Library/Logs/com.cloudless.app/CloudLess.log"
cargo run &
APP_PID=$!

# Wait briefly for the app to create the log file, then tail it
sleep 2
if [ -f "$LOG_FILE" ]; then
    tail -f "$LOG_FILE" &
    TAIL_PID=$!
    trap "kill $TAIL_PID 2>/dev/null" EXIT
fi

wait $APP_PID
