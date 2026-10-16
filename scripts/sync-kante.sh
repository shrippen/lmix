#!/usr/bin/env bash
# Copy Kante's QML module into the Qt Console (the README's way: a copy in
# the project, built into the binary's resources), and Kante's CSS tokens
# and fonts into lmix-core's title templates (offline: the engine renders
# them without network). Never edit the copies; change Kante and run this
# again.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
kante="${KANTE:-$here/../Kante}"
dest="$here/workspace/console-qt/qml/Kante"
[ -f "$kante/qml/Kante/qmldir" ] || { echo "Kante not found at $kante (set KANTE)" >&2; exit 1; }
rm -rf "$dest"
mkdir -p "$dest"
cp -r "$kante/qml/Kante/." "$dest/"
cp "$kante/LICENSE" "$dest/LICENSE" 2>/dev/null || true
echo "$(git -C "$kante" rev-parse --short HEAD 2>/dev/null || echo unknown)" > "$dest/VERSION"
echo "Kante $(cat "$dest/VERSION") -> $dest"

# Titles: tokens, @font-face rules and the fonts they use.
titles="$here/workspace/lmix-core/titles/kante"
rm -rf "$titles"
mkdir -p "$titles/fonts"
cp "$kante/tokens/variables.css" "$kante/css/fonts.css" "$titles/"
# Only the faces fonts.css declares.
for face in $(grep -o "fonts/[A-Za-z0-9-]*\.ttf" "$kante/css/fonts.css"); do
  cp "$kante/$face" "$titles/fonts/"
done
cp "$kante/fonts/OFL.txt" "$titles/fonts/"
git -C "$kante" rev-parse --short HEAD > "$titles/VERSION" 2>/dev/null || echo unknown > "$titles/VERSION"
echo "Kante $(cat "$titles/VERSION") -> $titles"
