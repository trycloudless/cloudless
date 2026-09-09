#!/usr/bin/env bash
# Usage: ./scripts/bump-version.sh <version>
# Example: ./scripts/bump-version.sh 0.2.0
#          ./scripts/bump-version.sh 0.3.0-beta.1
#
# Updates version in all four targets:
#   rust/tauri/tauri.conf.json                          (Tauri / desktop)
#   rust/tauri/Cargo.toml                               (Rust package)
#   rust/tauri/gen/android/app/tauri.properties         (Android versionName + versionCode)
#   rust/tauri/gen/apple/cloudless_tauri_iOS/Info.plist (iOS marketing version)
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "Usage: $0 <version>  (e.g. 0.2.0 or 0.3.0-beta.1)"
  exit 1
fi

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
  echo "Error: version must be semver — e.g. 0.2.0 or 0.3.0-beta.1"
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$ROOT/rust/tauri"

# Strip pre-release suffix for Android versionCode computation
BASE="${VERSION%%-*}"
MAJOR=$(echo "$BASE" | cut -d. -f1)
MINOR=$(echo "$BASE" | cut -d. -f2)
PATCH=$(echo "$BASE" | cut -d. -f3)
VERSION_CODE=$(( MAJOR * 10000 + MINOR * 100 + PATCH ))

echo "Bumping to $VERSION (Android versionCode: $VERSION_CODE)"
echo ""

# ── 1. tauri.conf.json ────────────────────────────────────────────────────────
CONF="$TAURI_DIR/tauri.conf.json"
jq --arg v "$VERSION" '.version = $v' "$CONF" > "$CONF.tmp" && mv "$CONF.tmp" "$CONF"
echo "  ✓ rust/tauri/tauri.conf.json"

# ── 2. Cargo.toml files ───────────────────────────────────────────────────────
# Only replaces the first occurrence (the [package] version, not dependency versions)
bump_cargo_version() {
  local FILE="$1"
  awk -v ver="$VERSION" '
    /^\[package\]/ { in_package=1 }
    /^\[/ && !/^\[package\]/ { in_package=0 }
    in_package && /^version = / { print "version = \"" ver "\""; next }
    { print }
  ' "$FILE" > "$FILE.tmp" && mv "$FILE.tmp" "$FILE"
}

bump_cargo_version "$TAURI_DIR/Cargo.toml"
echo "  ✓ rust/tauri/Cargo.toml"

# leptos_ui version must match so env!("CARGO_PKG_VERSION") in the UI is correct
bump_cargo_version "$ROOT/rust/leptos_ui/Cargo.toml"
echo "  ✓ rust/leptos_ui/Cargo.toml"

# ── 3. Android tauri.properties ───────────────────────────────────────────────
PROPS="$TAURI_DIR/gen/android/app/tauri.properties"
sed -i.bak \
  -e "s/^tauri\.android\.versionName=.*/tauri.android.versionName=$VERSION/" \
  -e "s/^tauri\.android\.versionCode=.*/tauri.android.versionCode=$VERSION_CODE/" \
  "$PROPS"
rm "$PROPS.bak"
echo "  ✓ rust/tauri/gen/android/app/tauri.properties"

# ── 4. iOS Info.plist ─────────────────────────────────────────────────────────
# CFBundleShortVersionString = marketing version (e.g. 0.2.0)
# CFBundleVersion = build number, set by CI at build time; we store the base
#                   version here as a fallback for local builds
PLIST="$TAURI_DIR/gen/apple/cloudless_tauri_iOS/Info.plist"
if command -v /usr/libexec/PlistBuddy &>/dev/null; then
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$PLIST"
  /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BASE" "$PLIST"
else
  # Fallback sed for non-macOS environments
  sed -i.bak \
    -e "/<key>CFBundleShortVersionString<\/key>/{n;s|<string>.*</string>|<string>$VERSION</string>|;}" \
    -e "/<key>CFBundleVersion<\/key>/{n;s|<string>.*</string>|<string>$BASE</string>|;}" \
    "$PLIST"
  rm "$PLIST.bak"
fi
echo "  ✓ rust/tauri/gen/apple/cloudless_tauri_iOS/Info.plist"

echo ""
echo "Done. All targets are now at $VERSION."
echo ""
echo "Next steps:"
echo "  git diff                              # review changes"
echo "  git commit -am \"Bump version to $VERSION\""
echo "  git tag v$VERSION"
echo "  git push origin HEAD v$VERSION        # tag triggers macOS CI build"
