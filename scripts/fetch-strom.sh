#!/usr/bin/env bash
# Prints the path of a strom binary for the current submodule commit:
# the prebuilt one from ghcr.io (public, no login; built by CI,
# .github/workflows/ci.yml) if available, else a local release build.
#
#   STROM_BIN="$(scripts/fetch-strom.sh)"
#
# Needs curl, jq and zstd for the download; when the commit has no
# package yet, it builds locally (slow, 20+ min cold).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REGISTRY="https://ghcr.io"
IMAGE="shrippen/lmix-strom"
FILE="strom-ubuntu24.04-x86_64.tar.zst"

SHA="$(git -C "$ROOT" ls-tree HEAD workspace/strom | awk '{print $3}')"
PREBUILT_DIR="$ROOT/workspace/strom/target/prebuilt/$SHA"
PREBUILT="$PREBUILT_DIR/strom"
LOCAL="$ROOT/workspace/strom/target/release/strom"

log() { echo "fetch-strom: $*" >&2; }

if [[ -x "$PREBUILT" ]]; then
    echo "$PREBUILT"
    exit 0
fi

# Anonymous pull: token, manifest, then the tarball layer (the blob
# redirects to storage, hence -L).
download() {
    local token digest
    token="$(curl -sf "$REGISTRY/token?scope=repository:$IMAGE:pull" | jq -r .token)" || return 1
    digest="$(curl -sf -H "Authorization: Bearer $token" \
        -H "Accept: application/vnd.oci.image.manifest.v1+json" \
        "$REGISTRY/v2/$IMAGE/manifests/$SHA" \
        | jq -re --arg f "$FILE" '.layers[] | select(.annotations["org.opencontainers.image.title"] == $f) | .digest')" || return 1
    curl -sfL -H "Authorization: Bearer $token" "$REGISTRY/v2/$IMAGE/blobs/$digest" \
        | zstd -dc | tar -x -C "$PREBUILT_DIR"
}

mkdir -p "$PREBUILT_DIR"
if download; then
    log "prebuilt $SHA"
    echo "$PREBUILT"
    exit 0
fi
rm -rf "$PREBUILT_DIR"
log "no package for $SHA, building locally"

(cd "$ROOT/workspace/strom" && env -u CARGO_TARGET_DIR cargo build --release --features no-gui --bin strom >&2)
echo "$LOCAL"
