#!/bin/bash
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# Usage: sh launch-android.sh [--device]
# Default: runs on Android emulator
# --device: runs on a connected physical device
USE_DEVICE=false
if [[ "$1" == "--device" ]]; then
    USE_DEVICE=true
fi

echo "Starting build process for CloudLess (Android)..."

# Check prerequisites
if [ -z "$ANDROID_HOME" ]; then
    if [ -d "$HOME/Library/Android/sdk" ]; then
        export ANDROID_HOME="$HOME/Library/Android/sdk"
    else
        echo "Error: ANDROID_HOME is not set and Android SDK not found at ~/Library/Android/sdk"
        exit 1
    fi
fi

if [ -z "$NDK_HOME" ]; then
    NDK_DIR=$(ls -d "$ANDROID_HOME/ndk/"* 2>/dev/null | sort -V | tail -1)
    if [ -n "$NDK_DIR" ]; then
        export NDK_HOME="$NDK_DIR"
    else
        echo "Error: NDK_HOME is not set and no NDK found in $ANDROID_HOME/ndk/"
        exit 1
    fi
fi

if ! command -v cmake &> /dev/null; then
    echo "Error: 'cmake' is not installed. Please run 'brew install cmake' first."
    exit 1
fi

# aws-lc-sys needs ANDROID_NDK_ROOT for CMake Android toolchain
export ANDROID_NDK_ROOT="$NDK_HOME"

echo "Using ANDROID_HOME=$ANDROID_HOME"
echo "Using NDK_HOME=$NDK_HOME"
echo "Using ANDROID_NDK_ROOT=$ANDROID_NDK_ROOT"

# Write dev config
if [ "$USE_DEVICE" = true ]; then
    # Real device can't reach 10.0.2.2 or 127.0.0.1 — use the Mac's local network IP
    LOCAL_IP=$(ipconfig getifaddr en0 2>/dev/null || echo "")
    if [ -z "$LOCAL_IP" ]; then
        echo "Error: Could not determine local network IP. Make sure Wi-Fi is connected."
        exit 1
    fi
    echo "Using Mac IP for device: $LOCAL_IP"
    echo "API_BASE_URL=http://${LOCAL_IP}:9000/" > "$DIR/tauri/.env.dev"
else
    # 10.0.2.2 is Android emulator's alias for host localhost
    echo 'API_BASE_URL=http://10.0.2.2:9000/' > "$DIR/tauri/.env.dev"
fi
cleanup() { rm -f "$DIR/tauri/.env.dev"; }
trap cleanup EXIT

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

# 2. Target device or emulator
ADB="$ANDROID_HOME/platform-tools/adb"

if [ "$USE_DEVICE" = true ]; then
    # Find connected physical device (non-emulator)
    DEVICE_SERIAL=$("$ADB" devices | grep -v "emulator-" | grep "device$" | awk '{print $1}' | head -1)
    if [ -z "$DEVICE_SERIAL" ]; then
        echo "Error: No physical Android device found. Connect via USB and enable USB debugging."
        exit 1
    fi
    echo "Using physical device: $DEVICE_SERIAL"

    # 3. Run Tauri Android dev, targeting the physical device
    echo "Launching Tauri Android dev..."
    cd "$DIR/tauri"
    ANDROID_SERIAL="$DEVICE_SERIAL" cargo tauri android dev
else
    # Ensure the correct emulator is running
    AVD_NAME="Pixel_7a"

    # Check if Pixel_7a is already running by matching its AVD name
    PIXEL7A_SERIAL=""
    for serial in $("$ADB" devices | grep "emulator-" | awk '{print $1}'); do
        avd=$("$ADB" -s "$serial" emu avd name 2>/dev/null | head -1 | tr -d '\r')
        if [ "$avd" = "$AVD_NAME" ]; then
            PIXEL7A_SERIAL="$serial"
            break
        fi
    done

    if [ -z "$PIXEL7A_SERIAL" ]; then
        echo "Booting emulator $AVD_NAME..."
        "$ANDROID_HOME/emulator/emulator" -avd "$AVD_NAME" &
        "$ADB" wait-for-device
        # Wait for the device to fully boot
        while [ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" != "1" ]; do
            sleep 1
        done
        PIXEL7A_SERIAL=$("$ADB" devices | grep "emulator-" | awk '{print $1}' | tail -1)
        echo "Emulator $AVD_NAME booted ($PIXEL7A_SERIAL)."
    else
        echo "Emulator $AVD_NAME already running ($PIXEL7A_SERIAL)."
    fi

    # 3. Run Tauri Android dev, targeting the correct emulator
    echo "Launching Tauri Android dev..."
    cd "$DIR/tauri"
    ANDROID_SERIAL="$PIXEL7A_SERIAL" cargo tauri android dev
fi
