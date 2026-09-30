# Latenz und Kapazität

Messwerte der Performance-Abnahmen. Neueste oben.

## Messmethode

Werkzeug: Selftest der Console (headless), gegen ein Release-Backend.
Abnahme in einem Schritt (baut, erzeugt den Testclip, startet ein eigenes
Backend auf Port 8199):

```bash
scripts/acceptance-m1b.sh            # 16 Inputs, 600 s
scripts/acceptance-m1b.sh 24 60      # Kapazität
```

Von Hand:

```bash
cd workspace/strom && cargo build --release --features no-gui
cd ../app-desktop && cargo build --release
../strom/target/release/strom --host 127.0.0.1 --port 8111 --no-auto-restart &
./target/release/app-desktop --server http://127.0.0.1:8111 --selftest \
  --selftest-patterns 0 --selftest-file /pfad/1080p30.mp4 [... 16×] \
  --selftest-encode --selftest-secs 600
```

- `--selftest-encode`: PGM und Multiview laufen je durch einen
  Hardware-Encoder (`builtin.videoenc`, H.264, `hardware`) in einen
  `fakesink sync=true`.
- **fps**: strom-Buffer-Age-Probes am Encoder-Eingang und an der Senke,
  Anzahl Puffer / Zeit.
- **Alter**: Laufzeit der Pipeline-Uhr minus Puffer-Zeitstempel an der
  Messstelle. Das ist die Latenz ab Zeitstempel (Media-Player: Eintritt in
  die Hauptpipeline), **nicht** Glas-zu-Glas; Decode-Zeit im Media-Player
  ist nicht enthalten. Echte Glas-zu-Glas-Messung mit Zeitmarker-Bildern
  folgt mit M2 (OMT) bzw. M3 (Latenz-CI).
- Budget: fps ≥ 29, PGM-Alter nach Encode p95 ≤ 167 ms (5 Frames @ 30 fps).
- Testdatei: 1080p30 H.264 (Constrained Baseline, 8 Mbit/s, GOP 60,
  `videotestsrc pattern=ball` + `timeoverlay`, per NVENC erzeugt).

## 2026-09-30 — M1b, NVIDIA RTX 3090, CUDA-Mixer

Host: RTX 3090, Treiber 615.71, GStreamer 1.28.7, 24 Threads.
strom `dev` e23f0b5, Mixer-Backend `cuda`.

**Abnahme: 16 × 1080p30 (NVDEC) → Vision Mixer → PGM 1080p30 +
Multiview 720p30 → 2 × NVENC, 10 min Dauerlauf, Take alle 1,5 s
(cut/fade/dip/wipe→fade):**

| Messstelle | fps | Alter p50 | p95 | max |
|---|---:|---:|---:|---:|
| PGM vor Encoder | 30,0 | 30 ms | 33 ms | 74 ms |
| PGM nach Encoder | 30,0 | 60 ms | 60 ms | 76 ms |
| Multiview vor Encoder | 30,0 | 32 ms | 33 ms | 76 ms |
| Multiview nach Encoder | 30,0 | 60 ms | 60 ms | 77 ms |

- 269 Takes, 0 Fehler; Take → WS-Bestätigung p95 11 ms.
- Backend-CPU Mittel 19 % (eines Kerns), RSS max 1,3 GB.
- GPU 2 %, NVDEC 54 %, VRAM 4,8 GB (inkl. Desktop).
- Speicherpfad (pad-caps): NVDEC → CUDA → `cudacompositor` → CUDA → NVENC.
  Einzige Downloads: 16 Thumbnail-Taps, 320×180 bei 1 fps.
- Start bis alle Inputs Bild zeigen: im Dauerlauf 15,3 s (seriell, ~1 s
  Abstand). Ursache: jeder Thumbnail-Tap warf die Pipeline kurz auf PAUSED.
  Behoben (strom `Keep the pipeline PLAYING…`): 16 Inputs nach 2,3 s.

**Vergleich derselbe Host, andere Pfade** (Selftest, 30 s):

| Pfad | Inputs | CPU | RSS | Anmerkung |
|---|---|---:|---:|---|
| CPU-Compositor (2026-09-19) | 16 × 720p Testbild | ~1700 % | 2,4 GB | 2,2 s Latenz |
| GL-Mixer nach Probe-Fix | 16 × 720p Testbild | 160 % | 2,2 GB | |
| CUDA-Mixer | 16 × 720p Testbild | 154 % | 0,8 GB | Testbild in 10-Bit-RGB (`cudaupload` wählte BGR10A2) |
| CUDA-Mixer, Testbild als NV12 | 16 × 720p Testbild | 68 % | 0,6 GB | Console fordert NV12 an |
| GL-Mixer, Testbild als NV12 | 16 × 720p Testbild | 113 % | 0,7 GB | |
| CUDA-Mixer, eigener Kontext je Pipeline | 16 × 1080p NVDEC | 69 % | 4,1 GB | 9,6 GB VRAM |
| CUDA-Mixer, geteilter Kontext | 16 × 1080p NVDEC | 17 % | 1,0 GB | 4,6 GB VRAM |

**Kapazitätsgrenze** (Selftest mit Encodern, 30 s, fps je Input per Probe
am Thumbnail-Tap; strom-Kappung auf 32 Inputs angehoben):

| Inputs | NVDEC | Backend-CPU | Inputs fps (min) | Ausgänge | Ergebnis |
|---|---:|---:|---:|---|---|
| 16 × 1080p30 | 54 % | 17 % | 30,0 | 30,0 fps, PGM 60 ms | PASS |
| 24 × 1080p30 | 81 % | 25 % | 30,0 | 30,0 fps, PGM 60 ms | PASS |
| 24 × 720p30 | 38 % | 25 % | 30,0 | 30,0 fps, PGM 60 ms | PASS |
| 32 × 1080p30 | 100 % | 53 % | 27,5 | 30,0 fps, PGM 74 ms | FAIL (NVDEC voll) |
| 32 × 720p30 | ~50 % | 33 % | 30,0 | 30,0 fps | PASS (3/3 nach Probe-Fix) |

- Grenze auf diesem Host: **24 Inputs** 1080p30 im Budget; 32 × 720p30
  ebenfalls. 32 × 1080p30 scheitert real an NVDEC (Mittel 97 %).
- Frühere Aussetzer einzelner Inputs bei 32 × 720p waren ein Messartefakt:
  Die Probes zählten Puffer vor der Pipeline-Uhr nicht (strom-Fix "Count
  buffer-age samples…"). Nach dem Fix 3 von 3 Läufen ohne Befund.
- Stresslauf: 30 × (2 Testbilder + 14 Dateien, Encoder, 10 s),
  30/30 ohne Befund.
- **Offen**: Einmal in ~20 Läufen (4 Inputs, direkt nach einem Neubau)
  froren alle Quellen nach dem ersten Bild ein, Ausgänge liefen weiter
  (Mixer wiederholt Frames); Flow meldete PLAYING. Nicht reproduziert.
- Startfehler mit 32 Media-Playern (2 von ~12 Läufen am Vormittag: ein
  Input meldet `not-negotiated` an seinem appsrc, Flow startet nicht):
  seit dem NVJPEG-Thumbnail-Zweig in 26 Läufen (davon 20 × 32 × 720p)
  nicht mehr aufgetreten. Ursache nicht belegt; Verdacht auf den früheren
  `cudadownload`-Thumbnail-Zweig unter Last.
- Die Ausgänge halten 30 fps auch bei verhungernden Inputs (der Mixer
  wiederholt Frames); deshalb misst der Selftest seit 2026-09-30 jeden
  Input.

**SRT-Eingänge** (4 × 1080p30 H.264 in MPEG-TS über SRT, Loopback,
SRT-Latenz 125 ms Standard, 30 s): alle Inputs 30,0 fps, Backend-CPU 9 %.
Speicherpfad srtsrc → decodebin (NVDEC) → CUDA → Mixer, kein Download,
kein Deinterlacer (progressiv). PGM-Alter nach Encode 198 ms: enthält die
konfigurierte SRT-Latenz; Budget von 167 ms gilt für lokale Quellen.

## AMD / Intel

Nicht gemessen, keine Hardware (M5b). Pfade: GL-Mixer, VA-API.
