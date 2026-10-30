#!/usr/bin/env bash
# Prints a directory for GST_PLUGIN_PATH holding GStreamer's decklink2
# plugin (decklink2sink, decklink2combiner …), which strom's DeckLink
# output uses. GStreamer ships it from 1.30 on; for older GStreamer it is
# built here from a fixed commit of GStreamer main against the installed
# GStreamer (DECISIONS 30.10.). The old decklinkvideosink shows every
# frame late once an output starts a second time in the same process
# (GStreamer work item 5391) and will not be fixed.
#
#   GST_PLUGIN_PATH="$(scripts/decklink-plugin.sh)" strom …
#   LMIX_PLUGIN_DIR=…   # where to put it (default ~/.local/share/lmix/gstreamer);
#                       # rebuilt there when GStreamer or the commit changes
#
# Prints nothing when the installed GStreamer has decklink2 already.
# Needs git, g++, pkg-config and the GStreamer development files
# (Fedora: gstreamer1-devel gstreamer1-plugins-base-devel; Ubuntu:
# libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev).
set -euo pipefail

# GStreamer main, 2026-10-29. The commit id names the tree, so a fetch of
# it is the code reviewed here, wherever it comes from.
REPO="https://gitlab.freedesktop.org/gstreamer/gstreamer.git"
COMMIT="6b0f6493f604c6740eb018891689fbb4f250ed77"
DIR="subprojects/gst-plugins-bad/sys/decklink2"
MODULES="gstreamer-video-1.0 gstreamer-audio-1.0 gstreamer-base-1.0 gstreamer-1.0"

log() { echo "decklink-plugin: $*" >&2; }

VERSION="$(pkg-config --modversion gstreamer-1.0)" || { log "no GStreamer development files"; exit 2; }
if env -u GST_PLUGIN_PATH gst-inspect-1.0 decklink2sink >/dev/null 2>&1; then
    log "GStreamer $VERSION has decklink2"
    exit 0
fi
OUT="${LMIX_PLUGIN_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/lmix/gstreamer}"
BUILT="$VERSION ${COMMIT:0:12}"
if [[ -f "$OUT/libgstdecklink2.so" && "$(cat "$OUT/built" 2>/dev/null)" == "$BUILT" ]]; then
    echo "$OUT"
    exit 0
fi

mkdir -p "$OUT"
WORK="$(mktemp -d -p "$OUT")"
trap 'rm -rf "$WORK"' EXIT
log "building decklink2 (GStreamer ${COMMIT:0:12}) for GStreamer $VERSION"
git -C "$WORK" init -q
git -C "$WORK" sparse-checkout set --no-cone "/$DIR/"
git -C "$WORK" fetch -q --depth 1 --filter=blob:none "$REPO" "$COMMIT"
git -C "$WORK" -c advice.detachedHead=false checkout -q FETCH_HEAD
[[ "$(git -C "$WORK" rev-parse HEAD)" == "$COMMIT" ]] || { log "fetched the wrong commit"; exit 1; }

SRC="$WORK/$DIR"
# gst_plugin_set_static_features_flag() is new in 1.29 and only marks the
# features for the registry; older GStreamer goes without.
compat=()
if ! pkg-config --atleast-version=1.29 gstreamer-1.0; then
    compat=(-D'gst_plugin_set_static_features_flag(p)=((void)(p))')
fi
# As meson builds it (sys/decklink2/meson.build), without config.h.
# shellcheck disable=SC2046
g++ -std=c++14 -shared -fPIC -O2 -Wno-undef -Wno-missing-declarations -Wno-multichar \
    -DGST_USE_UNSTABLE_API -DHAVE_POSIX_MEMALIGN "${compat[@]}" \
    -DVERSION="\"$VERSION\"" -DPACKAGE='"gst-plugins-bad"' -DPACKAGE_NAME='"GStreamer Bad Plug-ins (lmix)"' \
    -DGST_PACKAGE_ORIGIN='"https://git.arianw.de/shrippen/lmix"' \
    -I"$SRC" -I"$SRC/linux" \
    "$SRC"/gstdecklink2combiner.cpp "$SRC"/gstdecklink2demux.cpp "$SRC"/gstdecklink2deviceprovider.cpp \
    "$SRC"/gstdecklink2input.cpp "$SRC"/gstdecklink2device.cpp "$SRC"/gstdecklink2output.cpp \
    "$SRC"/gstdecklink2sink.cpp "$SRC"/gstdecklink2sinkbin.cpp "$SRC"/gstdecklink2src.cpp \
    "$SRC"/gstdecklink2srcbin.cpp "$SRC"/gstdecklink2utils.cpp "$SRC"/plugin.cpp \
    "$SRC"/linux/DeckLinkAPIDispatch.cpp "$SRC"/linux/DeckLinkAPIDispatch_v10_11.cpp \
    $(pkg-config --cflags --libs $MODULES) -ldl -lpthread -lm \
    -o "$OUT/libgstdecklink2.so.part"
mv "$OUT/libgstdecklink2.so.part" "$OUT/libgstdecklink2.so"
# The patched decklink plugin that lived here before.
rm -f "$OUT/libgstdecklink.so"
echo "$BUILT" >"$OUT/built"
log "built $OUT"
echo "$OUT"
