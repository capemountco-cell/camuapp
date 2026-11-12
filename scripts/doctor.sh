#!/usr/bin/env bash
#
# Check that this Mac can build the app and its .dmg, and say exactly what to
# do about anything missing. Run it first on any new machine.
#
#   npm run doctor
#
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

problems=0
ok() { printf '  ok    %s\n' "$1"; }
bad() { printf '  FIX   %s\n        %s\n' "$1" "$2"; problems=$((problems + 1)); }

echo "Checking what $PRODUCT_NAME needs to build."
echo

if [[ "$(uname -s)" == "Darwin" ]]; then
  ok "macOS $(sw_vers -productVersion) on $(uname -m)"
else
  bad "Not macOS ($(uname -s))" "You can develop and test the core crate here, but a .app and .dmg can only be built on a Mac."
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
  if xcode-select -p >/dev/null 2>&1; then
    ok "Xcode command line tools at $(xcode-select -p)"
  else
    bad "Xcode command line tools are missing" "Run: xcode-select --install"
  fi
  command -v hdiutil >/dev/null && ok "hdiutil (makes the disk image)" || bad "hdiutil not found" "It ships with macOS; check your PATH."
  command -v codesign >/dev/null && ok "codesign" || bad "codesign not found" "Install the Xcode command line tools."
fi

if command -v node >/dev/null; then
  major="$(node -p 'process.versions.node.split(".")[0]')"
  if (( major >= 20 )); then ok "Node $(node -v)"; else bad "Node $(node -v) is too old" "Install Node 20 or newer (https://nodejs.org or: brew install node)."; fi
else
  bad "Node is not installed" "Install Node 20 or newer (https://nodejs.org or: brew install node)."
fi

if command -v cargo >/dev/null; then
  ok "Rust $(rustc -V | cut -d' ' -f2)"
else
  bad "Rust is not installed" "Run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi

if command -v rustup >/dev/null && [[ "$(uname -s)" == "Darwin" ]]; then
  installed="$(rustup target list --installed)"
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    if grep -qx "$target" <<<"$installed"; then
      ok "Rust target $target"
    else
      printf '  note  Rust target %s is not installed (only needed for npm run dmg:universal)\n' "$target"
    fi
  done
fi

if [[ -d node_modules ]]; then ok "npm packages installed"; else bad "npm packages are not installed" "Run: npm install"; fi

if [[ -f src-tauri/icons/icon.icns ]]; then
  ok "App icon"
else
  bad "src-tauri/icons/icon.icns is missing" "Run: npm run icons (from src-tauri/app-icon.png, 1024x1024)"
fi

if grep -q '"identifier": "com.example.starter"' src-tauri/tauri.conf.json; then
  printf '  note  Still named "Starter" (com.example.starter). Run: npm run init -- --name "My App" --id com.yourname.myapp\n'
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
  stale=(/Volumes/"$PRODUCT_NAME"*)
  if [[ -e "${stale[0]}" ]]; then
    printf '  note  A disk image named like this app is still mounted (%s). npm run dmg ejects it first.\n' "${stale[*]}"
  fi
fi

echo
if (( problems == 0 )); then
  echo "Ready. npm run dev to run it, npm run dmg to package it."
else
  echo "$problems thing(s) to fix above."
  exit 1
fi
