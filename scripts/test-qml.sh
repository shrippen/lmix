#!/usr/bin/env bash
# QML tests of the console without engine or window: components get a
# state from tests/qml/fixtures and are checked by counts (items rebuilt by a
# state update, actions sent by a drag), so they run in CI. Offscreen;
# tests/qml/stubs stands in for GStreamer's QML item and the Console's
# AtlasCell.
# Needs Qt 6 with qmltestrunner and Kirigami.
#
#   scripts/test-qml.sh
#   scripts/test-qml.sh -functions test_a_fader_drag_sends_few_moves
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
QML="$ROOT/workspace/console-qt/qml"

runner=""
for candidate in qmltestrunner6 qmltestrunner-qt6 /usr/lib/qt6/bin/qmltestrunner /usr/lib64/qt6/bin/qmltestrunner qmltestrunner; do
    if command -v "$candidate" >/dev/null 2>&1; then
        runner="$candidate"
        break
    fi
done
[[ -n "$runner" ]] || { echo "qmltestrunner not found (Qt 6 declarative tools)" >&2; exit 2; }

export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}"
exec "$runner" -import "$QML" -import "$ROOT/workspace/console-qt/tests/qml/stubs" -input "$ROOT/workspace/console-qt/tests/qml" "$@"
