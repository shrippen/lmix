#!/usr/bin/env bash
# Setup script for the Claude Code cloud environment (Ubuntu 24.04).
# Paste into the environment settings, field "Setup script". Needs the
# environment variable GITEA_TOKEN (read:repository, read:package).
#
# Installs GStreamer and the build tools, clones lmix with its strom
# submodule to /root/work/lmix and fetches the prebuilt strom binary,
# so a session can run scripts/test-m2b-vm.sh right away.
set -euo pipefail

WORK=/root/work
REPO="https://git.arianw.de/shrippen/lmix.git"

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq git curl jq zstd zip unzip imagemagick xvfb mold clang pkg-config libssl-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-tools gstreamer1.0-gl \
    gstreamer1.0-nice libnice-dev libcairo2-dev graphviz

if [[ -z "${GITEA_TOKEN:-}" ]]; then
    echo "GITEA_TOKEN not set: skipping lmix clone and strom download" >&2
    exit 0
fi

auth=(-c "http.extraheader=Authorization: token $GITEA_TOKEN")
mkdir -p "$WORK"
if [[ ! -d "$WORK/lmix/.git" ]]; then
    git "${auth[@]}" clone -q "$REPO" "$WORK/lmix"
fi
git "${auth[@]}" -C "$WORK/lmix" submodule update --init --depth 50 workspace/strom

"$WORK/lmix/scripts/fetch-strom.sh" >/dev/null
