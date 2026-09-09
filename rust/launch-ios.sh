#!/bin/bash
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# Usage: sh launch-ios.sh [--device]
# Default: runs on iOS simulator
# --device: runs on a connected physical device (requires code signing)
USE_DEVICE=false
if [[ "$1" == "--device" ]]; then
    USE_DEVICE=true
fi

echo "Starting build process for CloudLess (iOS)..."

# Check prerequisites
if [[ "$(uname)" != "Darwin" ]]; then
    echo "Error: iOS development is only supported on macOS."
    exit 1
fi

if ! xcode-select -p &> /dev/null; then
    echo "Error: Xcode is not installed. Please install Xcode from the App Store."
    exit 1
fi

if ! command -v pod &> /dev/null; then
    echo "Error: CocoaPods is not installed. Please run 'brew install cocoapods' first."
    exit 1
fi

# Write dev config
if [ "$USE_DEVICE" = true ]; then
    # Real device can't reach 127.0.0.1 — use the Mac's local network IP
    LOCAL_IP=$(ipconfig getifaddr en0 2>/dev/null || echo "")
    if [ -z "$LOCAL_IP" ]; then
        echo "Error: Could not determine local network IP. Make sure Wi-Fi is connected."
        exit 1
    fi
    echo "Using Mac IP for device: $LOCAL_IP"
    echo "API_BASE_URL=http://${LOCAL_IP}:9000/" > "$DIR/tauri/.env.dev"
else
    echo 'API_BASE_URL=http://127.0.0.1:9000/' > "$DIR/tauri/.env.dev"
fi

# Track the cargo tauri PID so we can clean up on exit/signal
TAURI_PID=""
cleanup() {
    echo "Cleaning up launch-ios..."
    rm -f "$DIR/tauri/.env.dev"
    if [ -n "$TAURI_PID" ] && kill -0 "$TAURI_PID" 2>/dev/null; then
        echo "Stopping cargo tauri (PID $TAURI_PID) and children..."
        # Kill the entire process group spawned by cargo tauri
        kill -- -"$TAURI_PID" 2>/dev/null || kill "$TAURI_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

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

# 2. Run Tauri iOS dev
echo "Launching Tauri iOS dev..."
cd "$DIR/tauri"

if [ "$USE_DEVICE" = true ]; then
    echo "Targeting physical device (requires code signing)..."
    cargo tauri ios dev &
    TAURI_PID=$!
    wait "$TAURI_PID"
else
    echo "Targeting iOS simulator..."
    # Boot the simulator first (simctl install fails if it's in Shutdown state)
    SIMULATOR_NAME="iPhone 16 Pro"
    SIMULATOR_UDID=$(xcrun simctl list devices available -j | python3 -c "
import json, sys
data = json.load(sys.stdin)
for runtime, devices in data['devices'].items():
    for d in devices:
        if d['name'] == '$SIMULATOR_NAME' and d['isAvailable']:
            print(d['udid']); sys.exit(0)
" 2>/dev/null || true)

    if [ -n "$SIMULATOR_UDID" ]; then
        echo "Booting simulator $SIMULATOR_NAME ($SIMULATOR_UDID)..."
        xcrun simctl boot "$SIMULATOR_UDID" 2>/dev/null || true
    fi

    cargo tauri ios dev "$SIMULATOR_NAME" &
    TAURI_PID=$!
    wait "$TAURI_PID"
fi
