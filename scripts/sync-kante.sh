#!/usr/bin/env bash
# Copy Kante's QML module into the Qt Console (the README's way: a copy in
# the project, built into the binary's resources). Never edit the copy;
# change Kante and run this again.
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
