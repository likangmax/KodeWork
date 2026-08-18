#!/usr/bin/env bash
set -euo pipefail

# Native macOS/Linux release builder.  Run this on the target operating
# system; it intentionally refuses to pretend that a Windows cross-build is a
# valid desktop release.  Signing credentials are read from environment
# variables or an explicitly supplied private-key file and are never printed.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${TARGET_TRIPLE:-$(rustc -vV | awk '/host:/ {print $2}')}"

case "$target" in
  aarch64-apple-darwin|x86_64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)
    ;;
  *)
    echo "build-release.sh must run on a supported native macOS/Linux target; got $target" >&2
    exit 2
    ;;
esac

if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -n "${TAURI_SIGNING_PRIVATE_KEY_PATH:-}" ]]; then
  [[ -f "$TAURI_SIGNING_PRIVATE_KEY_PATH" ]] || {
    echo "TAURI_SIGNING_PRIVATE_KEY_PATH does not point to a file" >&2
    exit 2
  }
  export TAURI_SIGNING_PRIVATE_KEY="$(<"$TAURI_SIGNING_PRIVATE_KEY_PATH")"
fi
[[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] || {
  echo "TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH is required" >&2
  exit 2
}

pushd "$root" >/dev/null
bash scripts/prepare-tailscale.sh
npm ci
npm run lint
npm run test:frontend
npm run build
npm run tauri -- build --ci --target "$target"

bundle_dir="$root/target/$target/release/bundle"
[[ -d "$bundle_dir" ]] || { echo "bundle directory missing: $bundle_dir" >&2; exit 1; }

if command -v sha256sum >/dev/null 2>&1; then
  find "$bundle_dir" -type f -print0 | sort -z | xargs -0 sha256sum > "$bundle_dir/SHA256SUMS"
else
  find "$bundle_dir" -type f -print0 | sort -z | while IFS= read -r -d '' file; do
    shasum -a 256 "$file"
  done > "$bundle_dir/SHA256SUMS"
fi
popd >/dev/null

echo "Native release artifacts and checksums are in target/$target/release/bundle/"
