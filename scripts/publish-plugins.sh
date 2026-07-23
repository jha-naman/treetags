#!/usr/bin/env bash
#
# Build, generate, and (optionally) release treetags plugins
#
# Pipeline:
#   1. Determine the current PLUGIN_ABI_VERSION from the source.
#   2. Ensure a WASI SDK is available, verify its checksum if freshly downloaded.
#   3. Build the tooling and every non-internal plugin with `--locked`.
#   4. Generate index.json + Jekyll Markdown into plugins_index/ (move to the
#      blog repo manually afterwards).
#   5. With --publish, upload the .wasm/.toml blobs to the per-ABI GitHub Release.
#
# Usage:
#   scripts/publish-plugins.sh [--out-dir DIR] [--publish]
#
# Env overrides:
#   REPO               GitHub owner/name       (default: jha-naman/treetags)
#   OUT_DIR            generated-content dir   (default: plugins_index)
#   WASI_SDK_PATH      existing WASI SDK       (preferred if set)
#   WASI_SDK_VERSION   version to fetch        (default: 30)
#   WASI_SDK_SHA256    pinned tarball hash     (optional; else upstream SHA256SUMS)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REPO="${REPO:-jha-naman/treetags}"
# Generated content lands here (tracked in this repo); move it into the blog
# site repo under treetags/ afterwards to publish.
OUT_DIR="${OUT_DIR:-plugins_index}"
PUBLISH=0
for arg in "$@"; do
  case "$arg" in
    --publish) PUBLISH=1 ;;
    --out-dir) : ;;                       # handled below
    --out-dir=*) OUT_DIR="${arg#*=}" ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

DIST="$ROOT/target/plugin-dist"
BP=./target/release/treetags-build-plugin
BS=./target/release/treetags-build-site

# 1. ABI version (single source of truth in the code).
ABI=$(grep -oP 'PLUGIN_ABI_VERSION:\s*u32\s*=\s*\K[0-9]+' src/plugin/mod.rs | head -1)
[ -n "$ABI" ] || { echo "could not read PLUGIN_ABI_VERSION" >&2; exit 1; }
ASSET_BASE="https://github.com/${REPO}/releases/download/plugin-store-v${ABI}"
echo "==> ABI $ABI  repo $REPO  out $OUT_DIR"

# 2. WASI SDK — prefer an existing install; otherwise download + verify.
ensure_wasi_sdk() {
  if [ -n "${WASI_SDK_PATH:-}" ] && [ -d "$WASI_SDK_PATH" ]; then
    echo "==> using WASI_SDK_PATH=$WASI_SDK_PATH"
    return
  fi
  local ver="${WASI_SDK_VERSION:-30}"
  local dir="$HOME/.cache/treetags/wasi-sdk-${ver}"
  if [ -d "$dir/bin" ]; then export WASI_SDK_PATH="$dir"; echo "==> cached WASI SDK $dir"; return; fi

  local arch; case "$(uname -m)" in
    x86_64) arch=x86_64 ;; aarch64|arm64) arch=arm64 ;;
    *) echo "unsupported arch $(uname -m); set WASI_SDK_PATH manually" >&2; exit 1 ;;
  esac
  local tarball="wasi-sdk-${ver}.0-${arch}-linux.tar.gz"
  local base="https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${ver}"
  local tmp; tmp=$(mktemp -d)
  echo "==> downloading $tarball"
  curl -fsSL "$base/$tarball" -o "$tmp/sdk.tgz"

  local expected="${WASI_SDK_SHA256:-}"
  if [ -z "$expected" ]; then
    curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"
    expected=$(grep -E "[[:space:]]${tarball}\$" "$tmp/SHA256SUMS" | awk '{print $1}' | head -1)
  fi
  [ -n "$expected" ] || { echo "no checksum for $tarball" >&2; exit 1; }
  local actual; actual=$(sha256sum "$tmp/sdk.tgz" | awk '{print $1}')
  if [ "$expected" != "$actual" ]; then
    echo "WASI SDK checksum mismatch: expected $expected got $actual" >&2; exit 1
  fi
  echo "==> checksum OK ($actual)"
  mkdir -p "$dir"; tar xzf "$tmp/sdk.tgz" -C "$dir" --strip-components=1
  rm -rf "$tmp"; export WASI_SDK_PATH="$dir"
}
ensure_wasi_sdk

# 3. Build tooling and plugins with locked dependencies.
echo "==> building tooling (--locked)"
cargo build --release --locked --bin treetags-build-plugin --bin treetags-build-site

rm -rf "$DIST"
built=()
for manifest in plugins/*/plugin.toml; do
  [ -e "$manifest" ] || continue
  dir=$(dirname "$manifest")
  if grep -qE '^[[:space:]]*internal[[:space:]]*=[[:space:]]*true' "$manifest"; then
    echo "==> skipping internal plugin $dir"
    continue
  fi
  echo "==> building plugin $dir"
  "$BP" --output-dir "$DIST" "$dir"
  built+=("$(basename "$dir")")
done
[ "${#built[@]}" -gt 0 ] || { echo "no publishable plugins found" >&2; exit 1; }

# 4. Generate index.json + Markdown into the site.
echo "==> generating site content into $OUT_DIR"
"$BS" \
  --abi "$ABI" \
  --asset-base-url "$ASSET_BASE" \
  --out-dir "$OUT_DIR" \
  --generated-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  "$DIST"/*/

# 5. Release the blobs (opt-in — this is outward-facing and hard to undo).
if [ "$PUBLISH" -ne 1 ]; then
  echo
  echo "Generated content into $OUT_DIR/ and built plugins: ${built[*]}"
  echo "Blobs NOT uploaded. Re-run with --publish to upload to GitHub Release plugin-store-v${ABI}."
  echo "Then move $OUT_DIR/ into the blog site repo under treetags/ and commit there."
  exit 0
fi

echo "==> staging release assets"
UPLOAD="$ROOT/target/plugin-upload"
rm -rf "$UPLOAD"; mkdir -p "$UPLOAD"
for d in "$DIST"/*/; do
  name=$(basename "$d")
  cp "$d/plugin.wasm" "$UPLOAD/$name.wasm"
  cp "$d/plugin.toml" "$UPLOAD/$name.toml"
done

TAG="plugin-store-v${ABI}"
if ! gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  gh release create "$TAG" --repo "$REPO" \
    --title "Plugin store (ABI ${ABI})" \
    --notes "Durable blob store for treetags plugins built against ABI ${ABI}. Only the latest compatible build of each plugin is kept."
fi
echo "==> uploading assets to $TAG"
gh release upload "$TAG" --repo "$REPO" --clobber "$UPLOAD"/*.wasm "$UPLOAD"/*.toml
echo "==> done. Move $OUT_DIR/ into the blog site repo under treetags/ and commit there to publish the index + pages."
