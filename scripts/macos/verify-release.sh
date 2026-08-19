#!/usr/bin/env bash
set -euo pipefail

APP_PATH="${1:-}"
DMG_PATH="${2:-}"
EXPECTED_VERSION="${EXPECTED_VERSION:-0.1.2}"
EXPECTED_BUNDLE_ID="com.billiards.matrix"

ok() { printf '[OK] %s\n' "$1"; }
warn() { printf '[PENDING] %s\n' "$1"; }
fail() { printf '[FAIL] %s\n' "$1"; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || fail "This script must run on macOS"

if [[ -z "$APP_PATH" ]]; then
  APP_PATH="$(find "$PWD/src-tauri/target/release/bundle/macos" -maxdepth 1 -name '*.app' -print -quit 2>/dev/null || true)"
fi
[[ -n "$APP_PATH" && -d "$APP_PATH" ]] || fail "macOS .app path is required"

bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP_PATH/Contents/Info.plist")"
version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_PATH/Contents/Info.plist")"
[[ "$bundle_id" == "$EXPECTED_BUNDLE_ID" ]] || fail "Bundle identifier: $bundle_id"
[[ "$version" == "$EXPECTED_VERSION" ]] || fail "Bundle version: $version"
ok "Bundle identifier $bundle_id"
ok "Bundle version $version"

binary="$APP_PATH/Contents/MacOS/billiards_matrix"
[[ -x "$binary" ]] || fail "Application binary missing: $binary"
arch="$(file -b "$binary")"
ok "Application binary: $arch"

support_dir="$HOME/Library/Application Support/$EXPECTED_BUNDLE_ID"
if [[ -f "$support_dir/billiards.sqlite3" ]]; then
  ok "SQLite database exists"
else
  warn "SQLite database not created yet; launch the app once"
fi

for account in cloud_access_token cloud_refresh_token; do
  if security find-generic-password -s "$EXPECTED_BUNDLE_ID" -a "$account" >/dev/null 2>&1; then
    ok "Keychain item exists: $account"
  else
    warn "Keychain item missing or user is not logged in: $account"
  fi
done

if [[ -n "$DMG_PATH" ]]; then
  [[ -f "$DMG_PATH" ]] || fail "DMG not found: $DMG_PATH"
  hdiutil verify "$DMG_PATH" >/dev/null
  ok "DMG integrity verified"
  shasum -a 256 "$DMG_PATH"
  ok "DMG SHA-256 calculated"
fi

warn "GUI: install the DMG, launch once, log in, test registration approval, sync, offline rendering, upgrade, and data retention"
warn "GUI: quit the app and confirm no billiards_matrix process remains"
printf 'Mac release checks completed; PENDING items require a real GUI session.\n'
