#!/usr/bin/env bash
# Collects the Qt Console's texts (qsTr in its QML and in Kante) into
# workspace/console-qt/i18n/qml_<lang>.ts and fills in the German texts of
# i18n/de.py. The build turns them into .qm files (build.rs, lrelease).
# English is the source language; German is kept complete.
set -euo pipefail
cd "$(dirname "$0")/../workspace/console-qt"
LUPDATE="$(command -v lupdate || command -v lupdate-qt6 || echo /usr/lib/qt6/bin/lupdate)"
for lang in de; do
    "$LUPDATE" -silent -no-obsolete -locations relative -source-language en -target-language "$lang" \
        qml -extensions qml -ts "i18n/qml_${lang}.ts"
done
# German texts from i18n/de.py; lists what is missing there.
python3 i18n/fill.py
