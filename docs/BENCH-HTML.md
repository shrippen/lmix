# Benchmark HTML-Rendering: WPE WebKit gegen CEF (M7.0)

Entscheidet E.9 (`LASTENHEFT.md` §6): welche Browser-Engine Titel
(GFX.1–19) und Webseiten-Inputs (GFX.6, GFX.20) rendert. Ergebnis in
`LATENCY.md` (Messwerte) und `DECISIONS.md` (Wahl).

## 1. Kandidaten

| | CEF (`cefsrc`) | WPE WebKit (`wpesrc`) |
|---|---|---|
| Herkunft | gstcefsrc (Centricular), in strom über `cefsrc`/`cefbin` angebunden | `wpe`-Plugin in gst-plugins-bad, nutzt libwpewebkit + WPEBackend-fdo |
| Lizenz | Plugin LGPL-2.1+, CEF BSD-3 | Plugin LGPL, WebKit LGPL/BSD |
| Engine | Chromium (gstcefsrc pinnt derzeit CEF 139) | WebKit |
| Bildausgabe | `video/x-raw` BGRA im Systemspeicher, Bildrate variabel (`0/1`) | `video/x-raw(memory:GLMemory)` RGBA (GPU) oder BGRA im Systemspeicher |
| Ton | `cefdemux` → F32LE | Pads `audio_%u` (F32LE/F64LE/S16LE) |
| JavaScript von außen | Eigenschaft `listen-for-js-signals`, `js-flags` | Aktion `run-javascript`, `load-bytes` |
| Anzeige-Server | braucht X (`xvfb-run` ohne Bildschirm) | headless über EGL |
| H.264/AAC | Standard-Build nein; eigener Build mit `proprietary_codecs=true`, `ffmpeg_branding=Chrome` | über GStreamer-Medienpfad, nutzt installierte Decoder |

Quellen: `gstcefsrc.cc` (Caps), README gstcefsrc,
<https://gstreamer.freedesktop.org/documentation/wpe/wpesrc.html>.

Erwartung vor der Messung (zu prüfen, nicht vorwegnehmen):
- WPE: Bild bleibt im GL-Speicher bis in den Mischer (Leitprinzip
  "Frames bleiben auf der GPU"); CEF lädt jedes Bild von der CPU hoch.
- CEF: höhere Kompatibilität mit fremden Seiten.

## 2. Varianten im Test

| Kürzel | Variante |
|---|---|
| CEF-std | gstcefsrc mit Standard-CEF (wie strom-full) |
| CEF-codec | gstcefsrc mit eigenem CEF-Build inkl. H.264/AAC (nur für Tests bauen, nicht verteilen) |
| WPE | `wpesrc` aus der Distribution (Ubuntu/Debian `gstreamer1.0-wpe`), GL-Ausgabe |
| WPE-sys | `wpesrc` mit BGRA-Systemspeicher (Vergleich zum CEF-Pfad) |

## 3. Testseiten

Eigene Seiten in `bench/html/` (lmix-Repo, MIT), lokal über `file://` oder
einen kleinen HTTP-Server. Alle 1920×1080, transparenter Hintergrund, wo
sinnvoll.

| # | Seite | Prüft |
|---|---|---|
| T1 | Bauchbinde mit CSS-Animation (Einfliegen, Ausblenden) und Alpha, Konvention `play/stop/next/update` | Grundlast, Alpha, GFX.1/GFX.4 |
| T2 | Canvas 2D + WebGL (Partikel, 60 fps) | GPU-Pfad der Engine |
| T3 | lottie-web, eine komplexe Animation in Schleife | GFX.5 |
| T4 | `<video>` H.264/AAC MP4, 1080p60 | GFX.20 |
| T5 | HLS über hls.js (MSE), H.264/AAC | GFX.20, MSE |
| T6 | Twitch-Player-Einbettung (öffentlicher Kanal) | GFX.20, fremde Seite |
| T7 | YouTube-Einbettung | GFX.20, fremde Seite |
| T8 | Latenz-Seite: `update({n})` schreibt eine Zahl und färbt ein Markerfeld; Zeit bis zum Bild im Mischer | Bedien-Latenz |
| T9 | 8 Instanzen T1–T3 gemischt gleichzeitig | GFX.15 Budget |
| T10 | Kompatibilitätsliste fremder Seiten: Chat-Overlay, Alert-Widget, SPX-Vorlage, CasparCG-HTML-Vorlage, Google-Sheets-Anzeige, Wetter-Widget | GFX.6, Konvention |

## 4. Messgrößen und Werkzeuge

| Größe | Werkzeug | Schwelle (GFX.15, NFA.3) |
|---|---|---|
| Bildrate und Drops am Mischer-Eingang | strom Buffer-Age-Probes / Selftest | 60 fps, 0 Drops über 10 min |
| Takt | Abstand der Zeitstempel; variable CEF-Rate gegen festen Ausgangstakt | Jitter < ½ Frame |
| GPU-Last, Encoder/Decoder | NVML (wie Selftest), später fdinfo für AMD/Intel | Summe 8 Titel < 30 % GPU |
| CPU | `pidstat` über alle Prozesse der Engine (CEF-Renderer, WebKit-WebProcess zählen mit) | Summe 8 Titel < 2 Kerne |
| Speicher | RSS aller Prozesse, VRAM per NVML | je Titel angeben |
| Latenz `update` → Bild im Mischer | T8, Markerfeld per Pixelprobe | ≤ 2 Frames |
| Kopien im Pfad | Pipeline-Graph (`GST_DEBUG_DUMP_DOT_DIR`): GLMemory oder Systemspeicher, Anzahl `glupload`/`gldownload` | GPU-Pfad bevorzugt |
| Start | Zeit bis erstes Bild | < 2 s |
| Absturz | Renderer-Prozess per `kill -9` beenden | Program läuft, Input offline, Neustart ok |
| Kompatibilität | T4–T7, T10: läuft / läuft mit Fehlern / läuft nicht | GFX.20 Pflicht |

## 5. Ablauf

1. Umgebung festhalten: Host (RTX 3090), Treiber, GStreamer-, CEF-, WPE-Version.
2. Einzeltests T1–T8 je Variante, 2 min, headless:
   ```bash
   # CEF (braucht X)
   xvfb-run --server-args="-screen 0 1920x1080x24" gst-launch-1.0 \
     cefbin name=cef cefsrc::url=file:///…/t1.html cefsrc::gpu=true \
     cef.video ! video/x-raw,width=1920,height=1080,framerate=60/1 ! \
     glupload ! glcolorconvert ! fakesink sync=true
   # WPE (GL-Ausgabe)
   gst-launch-1.0 wpesrc location=file:///…/t1.html draw-background=false ! \
     "video/x-raw(memory:GLMemory),width=1920,height=1080,framerate=60/1" ! \
     fakesink sync=true
   ```
   Die Befehle sind ein Ausgangspunkt; Caps und Eigenschaften beim
   Aufbau prüfen und hier korrigieren.
3. Lasttest T9 in strom: 8 Titel als Inputs im GPU-Mischer (Backend wie
   M1b), PGM + Multiview + 2 Encodes, 10 min, Selftest misst.
4. Kompatibilität T10 von Hand, Bildschirmfoto je Seite und Variante.
5. Absturztest und Startzeit.
6. Ergebnis in `LATENCY.md` (Tabelle §7), Entscheidung in `DECISIONS.md`.

## 6. Entscheidungsregel

1. **GFX.20 Pflicht**: Eine Variante, die T4–T7 nicht schafft, scheidet
   aus. CEF-codec zählt nur, wenn das Verteilen geklärt ist
   (Patentlizenz), sonst nur als Beleg.
2. **Budget GFX.15**: 8 Titel 1080p60 ohne Drop im vollen M1b-Aufbau.
3. Danach nach Gewicht: Kompatibilität T10 (40 %), GPU/CPU (30 %),
   Latenz (20 %), Speicher und Start (10 %).
4. Zwei Engines nebeneinander nur, wenn keine allein 1 und 2 erfüllt
   (z. B. WPE für Titel, CEF für fremde Seiten); dann im Lastenheft
   festhalten.

## 7. Ergebnisvorlage

| Test | CEF-std | CEF-codec | WPE | WPE-sys |
|---|---|---|---|---|
| T1 fps / Drops | | | | |
| T2 fps / GPU % | | | | |
| T3 fps / CPU % | | | | |
| T4 H.264/AAC | | | | |
| T5 HLS (MSE) | | | | |
| T6 Twitch | | | | |
| T7 YouTube | | | | |
| T8 Latenz (Frames) | | | | |
| T9 8 Titel: Drops, GPU %, CPU, RAM, VRAM | | | | |
| T10 Kompatibilität (x/6) | | | | |
| Kopien im Pfad | | | | |
| Start (s) | | | | |
| Absturz übersteht | | | | |

## 8. Arbeitspakete

| # | Paket |
|---|---|
| B.1 | `wpesrc` auf dem Host installieren und headless starten (WPEBackend-fdo, EGL) |
| B.2 | CEF-codec bauen (nur Testmaschine, nicht verteilen) |
| B.3 | Testseiten T1–T10 in `bench/html/`, Latenz-Seite mit Markerfeld |
| B.4 | Selftest um HTML-Inputs erweitern (Engine wählbar) |
| B.5 | Messläufe, Ergebnis, Entscheidung E.9 |
