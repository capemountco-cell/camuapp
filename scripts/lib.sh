# Shared helpers for the build scripts. Sourced, not run.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Read a value from a JSON file with Node, which every machine building this
# app already has. Avoids depending on jq.
json_value() {
  node -e 'const v=require(require("path").resolve(process.argv[1]))[process.argv[2]];if(v===undefined)process.exit(1);process.stdout.write(String(v))' "$1" "$2"
}

PRODUCT_NAME="$(json_value src-tauri/tauri.conf.json productName)"
VERSION="$(json_value package.json version)"

say() { printf '\n==> %s\n' "$*"; }
fail() {
  printf '\nerror: %s\n' "$1" >&2
  shift
  for line in "$@"; do printf '       %s\n' "$line" >&2; done
  exit 1
}
