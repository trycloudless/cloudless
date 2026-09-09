#!/bin/bash
# Re-signs the development binary with Keychain entitlements.
# Run this after each `cargo build` / `cargo tauri dev` rebuild
# to enable biometric unlock on macOS during development.
#
# Usage: ./sign-dev.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY="$SCRIPT_DIR/../target/debug/cloudless"
ENTITLEMENTS="$SCRIPT_DIR/Entitlements.plist"
DEV_ENTITLEMENTS="$SCRIPT_DIR/Entitlements.dev.plist"
SIGNING_IDENTITY="84A9681EC2526E92387073C850F58FBC9AA0138C"

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY"
    echo "Run 'cargo build' first."
    exit 1
fi

TEAM_ID=$(security find-identity -v -p codesigning \
    | sed -n "/$SIGNING_IDENTITY/s/.*(\([A-Z0-9]*\)).*/\1/p" \
    | head -1)

if [ -z "$TEAM_ID" ]; then
    TEAM_ID="TDC274ZHS4"
fi

sed "s/K8HNPF47LC\\.com\\.cloudless\\.app/${TEAM_ID}.com.cloudless.app/" \
    "$ENTITLEMENTS" > "$DEV_ENTITLEMENTS"

codesign --force --sign "$SIGNING_IDENTITY" --entitlements "$DEV_ENTITLEMENTS" "$BINARY"

if [ $? -eq 0 ]; then
    echo "Signed successfully. Biometric unlock will work."
else
    echo "Signing failed. Check your certificate with: security find-identity -v -p codesigning"
    exit 1
fi
