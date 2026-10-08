# GUI-Shell: egui oder Qt Quick (ROADMAP 3.5, G.1)

Stand 2026-10-08. **Vorschlag, Entscheidung beim User** (die Vorgabe in
`claude.md` „native egui-Variante“ stammt vom User; ein Wechsel ist seine
Entscheidung).

## Ausgangslage

- Die Console ist seit M2b ein reiner Client von `lmix-core` (HTTP +
  WebSocket, Bilder als JPEG). Die Shell lässt sich also tauschen, ohne
  Engine oder Kern anzufassen; die Grenze ist die lmix-API.
- Heute: egui 0.34 (`app-desktop`, Console ~1600 Zeilen, 29 Tests).
- Gestaltung: Kante, Kante Light, System/Breeze (`agent.md`, GUI-Regel).

## Kandidaten

- **egui** (weiter wie bisher).
- **Qt Quick / Kirigami** über cxx-qt (Rust ↔ QML, MIT/Apache-2.0).
- **Tauri** (Web-Oberfläche im nativen Fenster): scheidet aus, weil
  `claude.md` eine native GUI verlangt (kein Web als Produktziel).

## Kriterien

| Kriterium | egui | Qt Quick / Kirigami |
|---|---|---|
| Video in der GUI (1.1): GPU-Decode direkt in die Textur | eigene Arbeit: wgpu kann DMABuf/GL-Texturen nicht stabil importieren; heute JPEG-Thumbnails | `qml6glsink` (GStreamer, `gstreamer1.0-qt6` 1.24 in Ubuntu 24.04): GL-Textur ohne CPU-Kopie in ein QML-Item |
| Mehrere Bildschirme (3.3) | Viewports seit 0.24; Bildschirm-Ausgang liegt ohnehin in der Engine | ausgereift (Fenster je Screen, Plasma-Fensterregeln) |
| Dichte der Input-Leiste (2b.2) | gut (Immediate Mode, eigene Layouts) | gut (GridView, Delegates); Kante-Komponenten fertig |
| Kante-Anbindung | fehlt; müsste erst in Kante entstehen (aus `tokens/palette.json`), Breeze-Rückfall nur nachgebaut | vorhanden: `qml/Kante` mit `KanteStyle.kind` Kante / Light / System (echtes Breeze über Kirigami) |
| KDE-Plasma-Integration (Zielsystem) | fremd (eigene Widgets, eigener Dateidialog über rfd/Portal) | nativ (Kirigami, Portal-Dialoge, Farbschema, globale Kürzel) |
| Barrierefreiheit | AccessKit, Grundfunktionen | Qt Accessibility (AT-SPI), Screenreader unter Plasma |
| Lizenz | MIT/Apache | Qt LGPLv3 (dynamisch gelinkt mit MIT vereinbar), Kirigami LGPL-2.0+, cxx-qt MIT/Apache; kein GPL-Code |
| Bau/Verteilung | ein Rust-Binary | Qt 6 als Systempaket (Plasma hat es), Build über `cxx-qt-build` in Cargo (findet Qt per qmake) |
| Umstiegskosten | keine | Console neu in QML (~1600 Zeilen egui), Logik (Client, Zustand) bleibt in Rust |

## Empfehlung

**Qt Quick / Kirigami über cxx-qt**, schrittweise:
1. Prototyp: ein QML-Fenster mit Kante-Stil, Programm-Monitor über
   `qml6glsink`. Das Bild kommt prozessübergreifend aus der Engine
   (`inter` gilt nur innerhalb von strom), z. B. über
   `unixfdsink`/`unixfdsrc` (GStreamer ≥ 1.24, DMABuf ohne Kopie), Input-Leiste aus dem `lmix-core`-Zustand. Messen: Latenz
   Monitor gegen PGM, CPU/GPU-Last bei 16 Kacheln (H.11, Hardware).
2. Bei Erfolg: Console-Funktionen nach QML übertragen, Client-Logik
   (`core_client.rs`) als Rust-Backend der QML-Oberfläche weiterverwenden.
3. Die egui-Console bleibt bis zur Gleichwertigkeit als Werkzeug
   (Selftest, VM-Screenshot) erhalten.

Begründung: Die zwei schwersten Kriterien, Video ohne Kopie in der GUI und
die vorgeschriebene Kante/Breeze-Gestaltung, sind in Qt schon gelöst und in
egui offene Eigenentwicklung. Die Prozesstrennung (GUI-Absturz lässt die
Sendung laufen) bleibt unverändert, weil die Grenze die lmix-API ist.

Bleibt es bei egui, ist der nächste Schritt eine egui-Anbindung in Kante
(Theme aus `palette.json`, Stil-Umschaltung) und ein eigener
Textur-Importpfad für Live-Monitore (H.4).
