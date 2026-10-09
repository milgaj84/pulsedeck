#!/usr/bin/env bash
# Package a built PulseDeck binary for a GitHub Release.
#
# Usage: scripts/package-release.sh <tag> <target> <binary-path> <out-dir>
#
# Produces <out-dir>/pulsedeck-<tag>-<target>.{tar.gz,zip} (zip for Windows
# targets) and a matching .sha256 file. The archive holds one directory with
# the binary, LICENSE and README.md.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <tag> <target> <binary-path> <out-dir>" >&2
  exit 2
fi

tag="$1"
target="$2"
binary="$3"
out_dir="$4"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
name="pulsedeck-${tag}-${target}"

if [ ! -f "$binary" ]; then
  echo "error: binary not found: $binary" >&2
  exit 1
fi

mkdir -p "$out_dir"
out_dir="$(cd "$out_dir" && pwd)"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

mkdir "$stage/$name"
cp "$binary" "$stage/$name/"
cp "$root/LICENSE" "$root/README.md" "$stage/$name/"

case "$target" in
  *windows*)
    archive="$name.zip"
    if command -v 7z >/dev/null 2>&1; then
      (cd "$stage" && 7z a -tzip -bso0 "$out_dir/$archive" "$name")
    else
      (cd "$stage" && zip -qr "$out_dir/$archive" "$name")
    fi
    ;;
  *)
    archive="$name.tar.gz"
    tar -C "$stage" -czf "$out_dir/$archive" "$name"
    ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$out_dir" && sha256sum "$archive" > "$archive.sha256")
else
  (cd "$out_dir" && shasum -a 256 "$archive" > "$archive.sha256")
fi

echo "$out_dir/$archive"
