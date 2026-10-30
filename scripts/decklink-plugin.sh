#!/usr/bin/env bash
# Prints a directory for GST_PLUGIN_PATH holding GStreamer's decklink
# plugin with lmix's patch: a card output started again in the same
# process (stop and start, the card switch, a new mode) otherwise shows
# every frame late (packaging/gstreamer/decklink-start-on-frame-grid.patch,
# DECISIONS 27.10.). Built once per GStreamer version and patch, from the
# gst-plugins-bad release matching the installed GStreamer.
#
#   GST_PLUGIN_PATH="$(scripts/decklink-plugin.sh)" strom …
#   LMIX_PLUGIN_DIR=…           # where to put it (default ~/.local/share/lmix/…);
#                               # rebuilt there when GStreamer or the patch changes
#
# Needs curl, g++, pkg-config and the GStreamer development files
# (Fedora: gstreamer1-devel gstreamer1-plugins-base-devel; Ubuntu:
# libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PATCH="$ROOT/packaging/gstreamer/decklink-start-on-frame-grid.patch"
MODULES="gstreamer-video-1.0 gstreamer-audio-1.0 gstreamer-base-1.0 gstreamer-1.0"

log() { echo "decklink-plugin: $*" >&2; }

VERSION="$(pkg-config --modversion gstreamer-1.0)" || { log "no GStreamer development files"; exit 2; }
STAMP="$(sha256sum "$PATCH" | cut -c1-12)"
OUT="${LMIX_PLUGIN_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/lmix/gstreamer-$VERSION-$STAMP}"
BUILT="$VERSION $STAMP"
if [[ -f "$OUT/libgstdecklink.so" && "$(cat "$OUT/built" 2>/dev/null)" == "$BUILT" ]]; then
    echo "$OUT"
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
URL="https://gstreamer.freedesktop.org/src/gst-plugins-bad/gst-plugins-bad-$VERSION.tar.xz"
log "building for GStreamer $VERSION"
curl -sfL -o "$WORK/src.tar.xz" "$URL"
want="$(curl -sfL "$URL.sha256sum" | awk '{print $1}')"
echo "$want  $WORK/src.tar.xz" | sha256sum -c --quiet
tar -xJf "$WORK/src.tar.xz" -C "$WORK" --strip-components=1 "gst-plugins-bad-$VERSION/sys/decklink"
patch -s -d "$WORK" -p1 <"$PATCH"

SRC="$WORK/sys/decklink"
mkdir -p "$OUT"
# As meson builds it (sys/decklink/meson.build), without config.h.
# shellcheck disable=SC2046
g++ -std=c++11 -shared -fPIC -O2 -Wno-undef -DGST_USE_UNSTABLE_API \
    -DVERSION="\"$VERSION\"" -DPACKAGE='"gst-plugins-bad"' -DPACKAGE_NAME='"GStreamer Bad Plug-ins (lmix)"' \
    -DGST_PACKAGE_ORIGIN='"https://git.arianw.de/shrippen/lmix"' \
    -I"$SRC" \
    "$SRC"/gstdecklinkplugin.cpp "$SRC"/gstdecklink.cpp "$SRC"/gstdecklinkaudiosink.cpp \
    "$SRC"/gstdecklinkvideosink.cpp "$SRC"/gstdecklinkaudiosrc.cpp "$SRC"/gstdecklinkvideosrc.cpp \
    "$SRC"/gstdecklinkdeviceprovider.cpp "$SRC"/linux/DeckLinkAPIDispatch.cpp \
    $(pkg-config --cflags --libs $MODULES) -ldl -lpthread -lm \
    -o "$OUT/libgstdecklink.so.part"
mv "$OUT/libgstdecklink.so.part" "$OUT/libgstdecklink.so"
echo "$BUILT" >"$OUT/built"
log "built $OUT"
echo "$OUT"
