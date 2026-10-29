# GUI-Shell: egui oder Qt Quick (ROADMAP 3.5, G.1)

Stand 2026-10-09. **Entschieden (User): Qt Quick/Kirigami über cxx-qt**,
Umstieg in den drei Schritten der Empfehlung unten.

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

## Schritt 1: Prototyp (Stand 2026-10-09)

`workspace/console-qt` (Binary `lmix-console`, 2026-10-18 `lmix-mixer` und `lmix-show`, seit 2026-10-23 `lmix` mit Seiten), Client von `lmix-core` wie
die egui-Console:

- Kirigami-Fenster mit Kante (Stil per `--theme kante|kante-light|system`,
  Standard Kante). `scripts/sync-kante.sh` kopiert `Kante/qml/Kante`
  unverändert nach `console-qt/qml/Kante`, `build.rs` legt es unter
  `qrc:/qt/qml/Kante` in die Ressourcen, also `import Kante` ohne Pfad.
- Vorschau- und Programm-Monitor, Übergangsknöpfe (Cut, Slot 1–4),
  Input-Leiste als `GridView`: jede Kachel live, Klick = Vorschau,
  Doppelklick = direkt auf Programm, Tally als Balken plus Wort (PGM/PRV).
- Bilder: `GET /api/live/program`, `/api/live/preview` bzw.
  `/api/live/inputs/{id}` liefert Socket und Transport; `unixfdsrc ! glupload`
  aus dem Hauptspeicher, mit dem CUDA-Mischer `cudaipcsrc ! cudadownload` von
  der GPU der Engine (eine Kopie auf der GPU), dann `glcolorconvert !
  qml6glsink(sync=false)` in ein `GstGLQt6VideoItem`. Bricht der Socket
  (Engine-Neustart, neuer Flow), fragt die Ansicht nach 1 s neu.
- Kacheln: alle Slots in einem Bild (`GET /api/live/tiles`, Raster und
  Zellgröße dazu), in RGBA (`tiles_format`), so dass die Console nichts
  umrechnet. Der Mischer skaliert die Inputs auf seiner GPU in ein Raster;
  die Console holt jedes Bild als GL-Textur aus einem `appsink`, und jede
  Kachel zeichnet ihren Ausschnitt direkt (`AtlasCell`, `cpp/atlas_cell.h`:
  ein `QSGImageNode` mit `sourceRect`, keine Ebene je Kachel). `TilesAtlas`
  hält nur den `qml6glsink`, dessen GL-Kontext Qt und GStreamer teilen; er
  bekommt keine Bilder. Desktop-OpenGL (die CUDA-GL-Kopie braucht PBOs) und
  der threaded Render-Loop sind gesetzt.
- Ein Frame des Fensters für alle Bilder: Programm und Kachelbild kommen
  rund 1 ms versetzt; die Bridge hält das erste höchstens 4 ms an, bis das
  andere da ist (`src/frame_gate.rs`), sonst zeichnet Qt jedes Bild in einem
  eigenen Frame. Dauerhafte Animationen (z. B. eine atmende Pille) halten
  das Fenster auf der Bildrate des Bildschirms; in der Kopfzeile ist darum
  „lmix-core“ verbunden ruhig (Pille „Done“).
- Rust-Backend (cxx-qt, `src/bridge.rs`): Zustand über WebSocket, Aktionen
  über HTTP, eine Pipeline je Item.
- Messsonde: mit `RUST_LOG=lmix=debug` meldet jede Ansicht alle
  5 s fps und Bildalter (Engine-Zeitstempel bis GUI-Sink, ohne Anzeige).

### Messung (H.11), RTX 3090, Wayland (Plasma), CUDA-Mischer, 16 Inputs

| Größe | Wert |
|---|---|
| Ansichten | 18 (16 Kacheln 640×360, Vorschau, Programm 1080p) |
| Bildrate je Ansicht | 30,0 fps, keine Abbrüche in 35 s |
| Bildalter Programm | p50 93 ms, p95 97 ms (Mischer-Latenz; Transport ≈ 0) |
| Bildalter Kacheln | p50 0–1 ms, p95 ≤ 3 ms (einige 54 ms, siehe unten) |
| CPU Console | 34 % eines Kerns |
| RAM Console | 386 MB RSS |
| GPU gesamt | 11–12 % statt 8 % ohne Console |
| CPU strom für die 17 Live-Flows | +38 % eines Kerns (videoscale auf der CPU) |

Zum Vergleich: die egui-Console holt JPEG-Thumbnails über HTTP.

Einordnung:
- Flüssige Kacheln für alle Inputs sind damit erreicht (Ziel aus
  `fluid-input-monitors`), bei geringer Last auf beiden Seiten.
- Die Kacheln lädt die Console noch als Systemspeicher hoch (memfd über
  den Socket, `glupload`). DMABuf ohne Kopie (H.4) bleibt der nächste
  Schritt, lohnt aber erst bei mehr oder größeren Ansichten.
- Einige Kacheln zeigen konstant 54 ms Alter: vermutlich die Phase
  zwischen Quelle und Slot-Flow; kein Wachstum, kein Einfluss auf die
  Bildrate. Klären, wenn echte Kameras dran sind.
- Die 93 ms beim Programm sind die Latenz des Mischers selbst (gleich mit
  CPU- und CUDA-Mischer), nicht des Monitors; siehe `LATENCY.md`.

Gefundene Fehler (behoben, lmix-core):
- Mit dem CUDA-Mischer trägt der Programmkanal CUDA-Speicher. Bildschirm-
  und NDI-Ausgang sowie der Live-Socket lasen ihn auf der CPU und
  scheiterten mit `not-negotiated`; jetzt `cudadownload` plus Capsfilter
  auf Systemspeicher davor. Encoder nehmen weiter GPU-Speicher.
- Aktionen zwischen „API bereit“ und „Engine bereit“ änderten die Show,
  meldeten aber 502. Jetzt merkt sich lmix-core sie, `restore` gibt sie
  der Engine (auch gestartete Ausgänge).

## Schritt 2: Funktionen übertragen (Stand 2026-10-09)

Übertragen aus der egui-Console:
- Show: Neu, Öffnen (Portal-Dialog), Zuletzt, Speichern, Speichern unter.
- Latenzmessung in der Werkzeugleiste.
- Übergänge: Cut, vier Knöpfe (Rechtsklick: Effekt, Länge), FTB.
- Kacheln: Mute, Lautstärke, Pegel (grün bis −18, gelb bis −6, rot
  darüber; Tooltip mit dBFS), PFL; Menü: Umbenennen, Quelle ersetzen,
  Farbmarke, Signalverlust, Audio folgt Video, PFL/AFL, Wiedergabe,
  WHIP-/Browser-Link kopieren, Entfernen.
- Dialoge „+ Input“ (alle Quellarten, Geräteliste, Dateiwahl,
  Farbwahl) und „+ Output“ (SRT, Aufnahme, NDI, Bildschirm; Programm oder
  ein Input).
- Ausgangsleiste: Start/Stopp per Klick, Laufzeit, Datenrate,
  Leitungszustand im Tooltip, Entfernen per Rechtsklick.
- Tasten 1–9: Vorschau.

Neu gegenüber egui: Kacheln per Drag & Drop umsortieren; Vorschau-Monitor
in voller Größe. Seit 19.10. (VSP) zeigt er den Vorschau-Ausgang des
strom-Mischers (`/api/live/preview`): Inputs und Layouts, wie ein Take sie
auf Programm bringt; ein Klick auf eine Kachel baut keine Pipeline mehr.

Technik: QML schickt Aktionen im JSON der API, Rust prüft sie
(`Core.act`); Formulare baut Rust (`forms.rs`, getestet), Texte liegen in
QML (`Texts.qml`, übersetzbar). Inputs,
Ausgänge und Übergänge liegen in Modellen mit fester ID (`KeyedModel`),
damit ein Zustands-Update nicht jede Kachel samt Pipeline neu baut.
Breeze-`MenuItem` ruft `RegExp()`, was in Delegates mit JS-Array-Modell
scheitert; Menüs aus Listen baut deshalb `ChoiceMenu` ohne Repeater.

Offen:
- Demo-Modus mit den gemeinsamen Demodaten (`claude.md`-Regel).

## Schritt 3: Sprache, Kante, Parität (Stand 2026-10-09)

- Sprache: Quelltexte englisch, Deutsch als Übersetzung
  (`i18n/qml_de.ts`, in das Binary gebaut, folgt der Systemsprache). Rust
  liefert nur Werte; Fehler von lmix-core übersetzt die Console anhand des
  Codes (`error.error`).
- Kante erweitert (PR #36): sieben Farb-Labels (`KanteStyle.labelColor`),
  `KanteAudioMeter` mit Skala (Kante 1.29). lmix-Farbmarken sind diese sieben (alte Shows:
  teal → cyan, pink → purple).

### Parität mit der egui-Console

Die egui-Console fliegt raus, sobald die Qt-Console alles kann, was sie als
Client von lmix-core konnte (Entscheidung des Users).

| egui (Client von lmix-core) | Qt-Console |
|---|---|
| Show neu, öffnen, zuletzt, speichern (unter) | ja |
| Latenzmessung | ja |
| Monitore Vorschau/Programm, Pegel | ja, live statt Bildabruf |
| Übergänge 1–4, Effekt und Länge, Cut, FTB | ja |
| Kachel: Nummer, Quellart-Kürzel, Name, Farbmarke, Tally, Status | ja |
| Kachel: Klick Vorschau, Doppelklick Programm, Menü „Cut to program“ | ja |
| Kachel-Menü: Umbenennen, Quelle ersetzen, Farbe, Signalverlust, Ton, PFL/AFL, Wiedergabe, Senden an, Entfernen | ja |
| Ton: Mute, Lautstärke, Pegel, PFL/AFL, Solo aus | ja |
| „+ Input“ mit allen Quellarten, Datei-/Farbwahl, Geräteliste mit „neu suchen“ | ja |
| Ausgänge: hinzufügen, Start/Stopp, Laufzeit, Leitungszustand, entfernen | ja |
| Tasten 1–9 | ja |

**Entfernt am 2026-10-09** (`app-desktop`, `run.sh`): Der kopflose
Selbsttest ist ein eigenes Binary (`workspace/selftest`, `lmix-selftest`),
VM-Test und Screenshot laufen über die Qt-Console (`test-m2b-vm.sh`
`CONSOLE_SHOT`, `test-console-qt.sh`). Mit der egui-Console gingen der
M1b-Modus direkt an strom und das Leistungs-Overlay (F3); Messwerte je
Ansicht liefert die Qt-Console mit `RUST_LOG=lmix=debug`.

## Eine App mit Seiten (2026-10-23)

Ein Programm `lmix` statt `lmix-mixer` und `lmix-show` (DECISIONS
2026-10-23, Plan und Reihenfolge in `docs/PLAN-SEITEN.md`):

- `AppShell.qml` ist das Fenster: Kopfzeile (File-Menü, Show-Datei,
  Aufnahme/Stream, Verbindung, Tasten-Übersicht, Uhr), die Seiten in einem
  `StackLayout`, darunter die Seitenleiste mit „Panic all“ (Show-Panik und
  Fade to black; Aufnahmen und Streams laufen weiter) und, wo die Seite kein
  Program-Bild hat, Program klein.
- `AppPage.qml` ist eine Seite: Titelzeile mit Werkzeugen und der eigenen
  Panik der Seite, darunter der Inhalt. `MixPage.qml` (vorher MixerMain),
  `ShowPage.qml` (vorher ShowMain), `AudioPage.qml` (Kanalzüge mit Fader,
  Pegel, Mute, AFL, AFV, daneben der Kanalzug des gewählten Inputs; Busse
  aus `BusPanel.qml`, das auch der Busse-Dialog nutzt; Zonen),
  `OutputsPage.qml` (Liste mit Zustand, Laufzeit, Verbindung, Start/Stopp
  mit Rückfrage `StopOutputDialog.qml`, Einzelheiten, Multiview) und
  `SetupPage.qml` (Show, Steuerung, dieser Rechner; öffnet die vorhandenen
  Dialoge).
- Seitenwechsel über die Leiste oder Strg+1…n; `--page mix|show|audio|outputs|setup` beim Start,
  sonst die zuletzt offene Seite. Seitentasten (1–9 im Mix, Leertaste und
  Escape in der Show) wirken nur auf der sichtbaren Seite; die Belegungen
  der Show (AUT.1–3) immer.
- Einstellungen in `lmix.conf`, einmal aus `mixer.conf` und `show.conf`
  übernommen (`app.rs`).
