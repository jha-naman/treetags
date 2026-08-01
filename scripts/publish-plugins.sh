#!/usr/bin/env bash
#
# Publish already-built treetags plugin blobs to GitHub Releases.
#
# Needs `gh` and push credentials — host only. Builds nothing: run
# scripts/build-plugins.sh first to produce target/plugin-dist.
#
# Pipeline:
#   1. Determine the current PLUGIN_ABI_VERSION from the source.
#   2. Stage the built .wasm/.toml blobs from target/plugin-dist.
#   3. Create/update the per-ABI GitHub Release and upload the blobs.
#
# Usage:
#   scripts/publish-plugins.sh [--yes]
#
# Env overrides:
#   REPO   GitHub owner/name   (default: jha-naman/treetags)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REPO="${REPO:-jha-naman/treetags}"
ASSUME_YES=0
for arg in "$@"; do
  case "$arg" in
    -y|--yes) ASSUME_YES=1 ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

DIST="$ROOT/target/plugin-dist"

# 1. ABI version (single source of truth in the code).
ABI=$(grep -oP 'PLUGIN_ABI_VERSION:\s*u32\s*=\s*\K[0-9]+' src/plugin/mod.rs | head -1)
[ -n "$ABI" ] || { echo "could not read PLUGIN_ABI_VERSION" >&2; exit 1; }
echo "==> ABI $ABI  repo $REPO"

# 2. Stage the built blobs (build-plugins.sh must have run).
[ -d "$DIST" ] || { echo "no build output at $DIST; run scripts/build-plugins.sh first" >&2; exit 1; }
shopt -s nullglob
dirs=("$DIST"/*/)
shopt -u nullglob
[ "${#dirs[@]}" -gt 0 ] || { echo "no built plugins in $DIST; run scripts/build-plugins.sh first" >&2; exit 1; }

echo "==> staging release assets"
UPLOAD="$ROOT/target/plugin-upload"
rm -rf "$UPLOAD"; mkdir -p "$UPLOAD"
staged=()
for d in "${dirs[@]}"; do
  name=$(basename "$d")
  cp "$d/plugin.wasm" "$UPLOAD/$name.wasm"
  cp "$d/plugin.toml" "$UPLOAD/$name.toml"
  staged+=("$name")
done

TAG="plugin-store-v${ABI}"
echo "==> ready to publish ${#staged[@]} plugin(s) to $REPO release $TAG: ${staged[*]}"
if [ "$ASSUME_YES" -ne 1 ]; then
  read -r -p "Upload these blobs to GitHub? [y/N] " reply
  case "$reply" in
    y|Y|yes|YES) ;;
    *) echo "aborted."; exit 0 ;;
  esac
fi

# 3. Create/update the release and upload the blobs.
if ! gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  gh release create "$TAG" --repo "$REPO" \
    --title "Plugin store (ABI ${ABI})" \
    --notes "Durable blob store for treetags plugins built against ABI ${ABI}. Only the latest compatible build of each plugin is kept."
fi
echo "==> uploading assets to $TAG"
gh release upload "$TAG" --repo "$REPO" --clobber "$UPLOAD"/*.wasm "$UPLOAD"/*.toml
echo "==> done. Move plugins_index/ into the blog site repo under treetags/ and commit there to publish the index + pages."
