#!/usr/bin/env bash
#
# Build the app and its .dmg, check the image actually works, and put it at a
# predictable path:
#
#   release/<Product Name>-<version>-<arch>.dmg
#
#   npm run dmg                 this Mac's architecture
#   npm run dmg:universal       one image for Apple silicon and Intel
#   npm run dmg -- --plain      skip the Finder window styling (see below)
#
# Each step below exists because its absence is a known way for `tauri build`
# to fail at the very end, after minutes of compiling. See docs/dmg.md.
#
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

[[ "$(uname -s)" == "Darwin" ]] || fail "a .dmg can only be built on macOS (this is $(uname -s))." \
  "The core crate and the frontend still build and test here: npm run check."
xcode-select -p >/dev/null 2>&1 || fail "the Xcode command line tools are missing." "Run: xcode-select --install"

# macOS ships bash 3.2, where expanding an empty array under `set -u` is an
# "unbound variable" error. Every expansion below uses the ${a[@]+...} form.
TARGET_ARGS=()
ARCH="$(uname -m)"
for arg in "$@"; do
  case "$arg" in
    --universal)
      say "Adding both Rust targets for a universal build"
      rustup target add aarch64-apple-darwin x86_64-apple-darwin
      TARGET_ARGS=(--target universal-apple-darwin)
      ARCH="universal"
      ;;
    --plain)
      # Tauri's dmg script styles the window through AppleScript and Finder.
      # Without a logged-in GUI session (SSH, some CI, a locked screen) that
      # step hangs or fails. CI=true tells the bundler to skip it; the image
      # still works, it just opens with Finder's default layout.
      export CI=true
      ;;
    *) fail "unknown option $arg" "Options: --universal, --plain" ;;
  esac
done

# No GUI session means no Finder to style the window with.
if [[ -n "${SSH_CONNECTION:-}" && -z "${CI:-}" ]]; then
  echo "note: running over SSH, so skipping the Finder window styling."
  export CI=true
fi

say "Ejecting any mounted image named like $PRODUCT_NAME"
# A volume left mounted by an earlier build, or by opening an old .dmg, makes
# the next build fail with "resource busy" or produce a volume called
# "$PRODUCT_NAME 1" whose window layout no longer applies.
shopt -s nullglob
for volume in /Volumes/"$PRODUCT_NAME" /Volumes/"$PRODUCT_NAME "[0-9]*; do
  echo "  ejecting $volume"
  hdiutil detach "$volume" -force -quiet || true
done
shopt -u nullglob

say "Removing images from earlier builds"
# A failed run leaves a writable rw.*.dmg next to the output; the next run
# then trips over it.
find target -path '*/bundle/dmg/*' -name '*.dmg' -delete 2>/dev/null || true
find target -path '*/bundle/macos/rw.*.dmg' -delete 2>/dev/null || true

say "Installing npm packages"
if [[ -f package-lock.json ]]; then npm ci --no-audit --no-fund; else npm install --no-audit --no-fund; fi

if [[ ! -f src-tauri/icons/icon.icns ]]; then
  say "Generating icons from src-tauri/app-icon.png"
  npm run icons
fi

say "Checking before spending minutes on a release build"
npm run typecheck
npm run test
cargo test --workspace

say "Building $PRODUCT_NAME $VERSION ($ARCH)"
npm run tauri -- build --bundles app dmg ${TARGET_ARGS[@]+"${TARGET_ARGS[@]}"}

# Searched for, not assumed: in a Cargo workspace the bundle lands in the
# workspace target directory, and a --target build adds a directory level.
DMG="$(find target -type f -path '*/release/bundle/dmg/*.dmg' ! -name 'rw.*' -print -quit)"
[[ -n "$DMG" ]] || fail "the build finished but produced no .dmg." \
  "Looked for target/**/release/bundle/dmg/*.dmg. Contents:" \
  "$(find target -path '*/release/bundle*' -maxdepth 4 2>/dev/null | head -20)"

mkdir -p release
FINAL="release/${PRODUCT_NAME}-${VERSION}-${ARCH}.dmg"
cp -f "$DMG" "$FINAL"

bash scripts/verify-dmg.sh "$FINAL"

echo
echo "Built $FINAL"
echo "  size    $(du -h "$FINAL" | cut -f1)"
echo "  sha256  $(shasum -a 256 "$FINAL" | cut -d' ' -f1)"
if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo
  echo "Ad-hoc signed, not notarized. It opens normally on this Mac. Anyone who"
  echo "downloads it must approve it once: System Settings > Privacy & Security >"
  echo "Open Anyway. To remove that step, see docs/signing.md."
fi
