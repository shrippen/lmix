#!/usr/bin/env bash
# Prints the path of a strom binary for the current submodule commit:
# the prebuilt one from the Gitea package registry (CI, .gitea/workflows/
# strom.yml) if available, else a local release build.
#
#   STROM_BIN="$(scripts/fetch-strom.sh)"
#
# Needs GITEA_TOKEN (read:package) for the download; without it, or when
# the commit has no package yet, it builds locally (slow, 20+ min cold).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVER="https://git.arianw.de"
OWNER="shrippen"
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

if [[ -n "${GITEA_TOKEN:-}" ]]; then
    mkdir -p "$PREBUILT_DIR"
    url="$SERVER/api/packages/$OWNER/generic/strom/$SHA/$FILE"
    if curl -sf -H "Authorization: token $GITEA_TOKEN" "$url" | zstd -dc | tar -x -C "$PREBUILT_DIR"; then
        log "prebuilt $SHA"
        echo "$PREBUILT"
        exit 0
    fi
    rm -rf "$PREBUILT_DIR"
    log "no package for $SHA, building locally"
else
    log "GITEA_TOKEN not set, building locally"
fi

(cd "$ROOT/workspace/strom" && env -u CARGO_TARGET_DIR cargo build --release --features no-gui --bin strom >&2)
echo "$LOCAL"
