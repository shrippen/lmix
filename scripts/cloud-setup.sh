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
apt-get install -y -qq git curl jq zstd zip unzip imagemagick xvfb ffmpeg mold clang pkg-config libssl-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-tools gstreamer1.0-gl \
    gstreamer1.0-nice libnice-dev libcairo2-dev graphviz xdotool

# Qt Console: Qt 6 and Kirigami from KDE neon, pinned to Qt/KF6 and the
# newer base libraries they need (docs/BUILD.md, "Qt-Console").
mkdir -p /etc/apt/keyrings
curl -sSo /etc/apt/keyrings/neon-archive.asc https://archive.neon.kde.org/public.key
echo "deb [signed-by=/etc/apt/keyrings/neon-archive.asc] https://archive.neon.kde.org/user noble main" \
    > /etc/apt/sources.list.d/neon.list
printf 'Package: *\nPin: origin archive.neon.kde.org\nPin-Priority: 100\n\nPackage: qt6-* libqt6* qml6-* kf6-* libkf6* gstreamer1.0-qt6 qmake6* qt6-l10n-tools libwayland* libinput* libpcre2*\nPin: origin archive.neon.kde.org\nPin-Priority: 600\n' \
    > /etc/apt/preferences.d/neon-qt6
apt-get update -qq
apt-get install -y -qq --no-install-recommends qt6-base-dev qt6-declarative-dev qt6-l10n-tools \
    qt6-tools-dev-tools qt6-qpa-plugins qml6-module-org-kde-kirigami kf6-kirigami-dev gstreamer1.0-qt6

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
