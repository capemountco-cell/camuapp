#!/usr/bin/env bash
#
# Prove a .dmg is usable rather than assuming it: the image checksums, mounts
# read-only, contains the .app and an Applications link, and the .app inside
# has a valid signature. Run by build-dmg.sh; run it on any .dmg yourself:
#
#   npm run verify-dmg -- "release/My App-0.1.0-arm64.dmg"
#
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

DMG="${1:-}"
if [[ -z "$DMG" ]]; then
  DMG="$(ls -t release/*.dmg 2>/dev/null | head -1 || true)"
fi
[[ -n "$DMG" && -f "$DMG" ]] || fail "no .dmg to verify." "Pass a path, or run npm run dmg first."

say "Verifying $DMG"
hdiutil verify -quiet "$DMG" || fail "the image's checksum does not verify; it is damaged."

MOUNT="$(mktemp -d /tmp/verify-dmg.XXXXXX)"
cleanup() { hdiutil detach "$MOUNT" -force -quiet 2>/dev/null || true; rmdir "$MOUNT" 2>/dev/null || true; }
trap cleanup EXIT
hdiutil attach "$DMG" -readonly -nobrowse -noautoopen -mountpoint "$MOUNT" -quiet

APP="$MOUNT/$PRODUCT_NAME.app"
[[ -d "$APP" ]] || fail "the image does not contain $PRODUCT_NAME.app." "It contains: $(ls "$MOUNT")"
[[ -L "$MOUNT/Applications" ]] || fail "the image has no Applications link to drag the app onto."
[[ -x "$APP/Contents/MacOS/$(defaults read "$APP/Contents/Info" CFBundleExecutable)" ]] \
  || fail "the app's executable is missing or not executable."

codesign --verify --deep --strict "$APP" 2>/dev/null \
  || fail "the app's signature does not verify." \
  "macOS reports an app with a broken signature as \"damaged\" once downloaded." \
  "$(codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | head -5)"

SIGNATURE="$(codesign -dv "$APP" 2>&1 | grep -E '^(Signature|Authority)=' | head -1 || true)"
echo "  image checksum   ok"
echo "  contents         $PRODUCT_NAME.app, Applications link"
echo "  signature        ok (${SIGNATURE:-unknown})"
echo "  architectures    $(lipo -archs "$APP/Contents/MacOS/"* 2>/dev/null | head -1)"
