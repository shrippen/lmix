"""Writes the German texts of de.py into qml_de.ts and lists sources
without one. Run after scripts/update-translations.sh:
    python3 workspace/console-qt/i18n/fill.py
"""
import pathlib
import sys

sys.dont_write_bytecode = True
import xml.etree.ElementTree as ET

HERE = pathlib.Path(__file__).parent
sys.path.insert(0, str(HERE))
from de import DE  # noqa: E402

path = HERE / "qml_de.ts"
tree = ET.parse(path)
missing = set()
for message in tree.iter("message"):
    source = message.find("source").text
    comment = message.find("comment")
    translation = message.find("translation")
    key = (source, comment.text) if comment is not None else None
    if key in DE or source in DE:
        translation.text = DE[key] if key in DE else DE[source]
        translation.attrib.pop("type", None)
    else:
        missing.add(source)
ET.indent(tree)
with open(path, "wb") as f:
    f.write(b'<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE TS>\n')
    tree.write(f, encoding="utf-8")
for source in sorted(missing):
    print("untranslated:", source)
sys.exit(1 if missing else 0)
