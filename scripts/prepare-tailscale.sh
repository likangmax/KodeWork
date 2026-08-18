#!/usr/bin/env bash
set -euo pipefail

# Builds the pinned Tailscale CLI and userspace daemon for a native Unix
# release runner.  Windows keeps using prepare-tailscale.ps1 because its
# embedded named-pipe patch is intentionally Win32-specific.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_dir="$root/references/tailscale"
output_dir="$root/src-tauri/binaries"
tag="v1.102.2"
commit="eb67e5dcbe145d63e1128b9b4b630f8a82da101f"
target="${TARGET_TRIPLE:-$(rustc -vV | awk '/host:/ {print $2}')}"

case "$target" in
  x86_64-unknown-linux-gnu)
    goos=linux
    goarch=amd64
    ;;
  aarch64-unknown-linux-gnu)
    goos=linux
    goarch=arm64
    ;;
  x86_64-apple-darwin)
    goos=darwin
    goarch=amd64
    ;;
  aarch64-apple-darwin)
    goos=darwin
    goarch=arm64
    ;;
  *)
    echo "unsupported Unix Tailscale target: $target" >&2
    exit 2
    ;;
esac

command -v go >/dev/null || { echo "Go is required to build Tailscale" >&2; exit 127; }

if [[ ! -d "$source_dir" ]]; then
  mkdir -p "$(dirname "$source_dir")"
  git clone --filter=blob:none --branch "$tag" --depth 1 \
    https://github.com/tailscale/tailscale.git "$source_dir"
fi

actual="$(git -C "$source_dir" rev-parse HEAD)"
if [[ "$actual" != "$commit" ]]; then
  echo "Tailscale source must be pinned to $tag ($commit), found $actual" >&2
  exit 1
fi

mkdir -p "$output_dir"
version_stamp='1.102.2-kodework.1'
ldflags="-s -w -X tailscale.com/version.longStamp=$version_stamp -X tailscale.com/version.shortStamp=1.102.2 -X tailscale.com/version.gitCommitStamp=$commit"

pushd "$source_dir" >/dev/null
CGO_ENABLED=0 GOOS="$goos" GOARCH="$goarch" \
  go build -trimpath -buildvcs=false -ldflags="$ldflags" \
  -o "$output_dir/tailscale-$target" ./cmd/tailscale
CGO_ENABLED=0 GOOS="$goos" GOARCH="$goarch" \
  go build -trimpath -buildvcs=false -ldflags="$ldflags" \
  -o "$output_dir/tailscaled-$target" ./cmd/tailscaled
cp LICENSE "$output_dir/TAILSCALE-LICENSE.txt"
"$output_dir/tailscale-$target" version
popd >/dev/null

echo "Pinned Tailscale sidecars prepared in $output_dir for $target"
