#!/bin/bash
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

PLATFORM="${1:-android}"

if [[ "$PLATFORM" != "android" && "$PLATFORM" != "ios" ]]; then
    echo "Usage: $0 [android|ios]"
    exit 1
fi

echo "Starting build process for CloudLess ($PLATFORM)..."

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

# 2. Run Tauri mobile dev
echo "Launching Tauri $PLATFORM dev..."
cd "$DIR/bindings/tauri"
cargo tauri "$PLATFORM" dev
