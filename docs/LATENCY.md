# Latenz und Kapazität

Messwerte der Performance-Abnahmen. Neueste oben.

## Messmethode

Werkzeug: Selbsttest `lmix-selftest` (kopflos, früher Teil der Console), gegen ein Release-Backend.
Abnahme in einem Schritt (baut, erzeugt den Testclip, startet ein eigenes
Backend auf Port 8199):

```bash
scripts/acceptance-m1b.sh            # 16 Inputs, 600 s
scripts/acceptance-m1b.sh 24 60      # Kapazität
```

Von Hand:

```bash
cd workspace/strom && cargo build --release --features no-gui
cd ../selftest && cargo build --release
../strom/target/release/strom --host 127.0.0.1 --port 8111 --no-auto-restart &
./target/release/lmix-selftest --server http://127.0.0.1:8111 \
  --patterns 0 --file /pfad/1080p30.mp4 [... 16×] \
  --encode --secs 600
```

- `--encode`: PGM und Multiview laufen je durch einen
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

## 2026-10-30 — SDI-Loop mit decklink2sink (H.7)

Wie unten, Kartenausgang jetzt über `decklink2sink` (DECISIONS 30.10.),
1080p25, CUDA-Mischer, 3 Läufe je Zeile, Last unter 1,1 (andere Agenten
davor und danach beschäftigt).

| Aufbau | Input hinter Program |
|---|---|
| decklink2, Vorlauf 20 ms (Standard) | 433, 400, 433 ms |
| decklink2, Vorlauf 0 ms | 433, 366, 400 ms |
| decklink2, 3 Bilder vorgehalten, leerer Rechner | 267, 234, 267 ms |
| decklink2, 3 Bilder vorgehalten, Load 32 (Soak + CPU-Brenner) | 267, 300, 267 ms |

Der Vorlauf ändert nichts mehr; was zählt, ist der Puffer der Karte
(GStreamers Standard 7 Bilder = 280 ms bei 25p). lmix stellt 3 ein
(`--card-frames`, DECISIONS 30.10., Nachtrag Puffer der Karte).

## 2026-10-27 — SDI-Loop: Program über die Karte zurück (H.7)

`scripts/measure-sdi-latency.sh` (DeckLink Quad 2, Kabel von Anschluss 1
auf 2, CUDA-Mischer mit 30 fps, RTX 3090): Ein Clip blitzt einmal pro
Sekunde auf Program. Program geht als Kartenausgang hinaus und kommt als
Karten-Input zurück. Der aufgenommene Multiview zeigt beide;
`scripts/sdi-latency.py` misst, wie viel später die Kachel des Inputs
blitzt. Das ist der Weg aus dem Mischer hinaus, durch beide Anschlüsse und
zurück in den Mischer. Auflösung: ein Mischerbild (33 ms).

| Modus | Läufe | Input hinter Program |
|---|---|---|
| 1080p25 | 3 | 266, 300, 267 ms |
| 1080i50 | 2 | 300, 267 ms |

150 ms davon sind der Vorlauf am Kartenausgang (`CARD_LEAD_MS`, DECISIONS
27.10.). Er sinkt erst, wenn die übrige Ursache später Bilder nach einem
Neustart geklärt ist (todo). Ein Gerät hinter dem Kartenausgang sieht das
Bild also etwa 150 ms plus Kartenweg nach Program.

## 2026-10-23 — Program an den Ausgängen, mit und ohne Sync

Bildalter am Eingang des Encoders einer MKV-Aufnahme auf Program
(Buffer-Age-Probe im Ausgangs-Flow), RTX 3090, Testbilder, unter fremder
Last. Vorher (`sync=true` an den inter-Ausgängen des Mischers) zeigte
`/api/latency` 56 ms, der Encoder bekam 90 ms. Details: DECISIONS
23.10. „Mischer-Ausgänge ohne Sync“.

| Aufbau | vorher | ohne Sync |
|---|---|---|
| CUDA, 2 Inputs, 6 + 6 Läufe | 90 ms | 0–62 ms |
| CUDA, 8 Inputs, 3 Läufe | 90 ms | 33–64 ms |
| CPU, 8 Inputs, 3 Läufe (Load 125) | 90 ms | 45–85 ms |
| GL, 4 Inputs, 3 Läufe (Load 132) | 90 ms | 9–68 ms |
| Ton an der Aufnahme | 128–130 ms | 44–101 ms |

## 2026-10-09 — M3, Dauerlauf NFA.7, NVIDIA RTX 3090, CUDA-Mischer

`scripts/soak-hardware.sh`, 295 min, 16 Inputs (Testbilder), Programmwechsel
alle 30 s, MKV-Aufnahme, ISO-Aufnahme eines Inputs, 2 × SRT an lokale
Empfänger, Messung alle 5 min über `GET /api/latency`.

| Zeit | Median | p95 | strom CPU | strom RSS | GPU |
|---|---|---|---|---|---|
| 0 min | 84 ms | 89 ms | 183 % | 828 MB | 10 % |
| 10 min | 56 ms | 56 ms | – | 763 MB | – |
| 60 min | 56 ms | 56 ms | 181 % | 869 MB | 9 % |
| 120 min | 56 ms | 56 ms | 176 % | 903 MB | 9 % |
| 180–230 min | 63 ms | 63–65 ms | 173–192 % | 937–959 MB | 9–11 % |
| 295 min | 56 ms | 56 ms | 177 % | 998 MB | 10 % |

Latenz ohne Drift (der erste Wert fällt in den Anlauf). Aufnahme und
ISO je 531 254 Bilder in 17 707 s, SRT 531 190 bzw. 531 192 empfangen,
0 verloren. **strom-RSS wächst gleichmäßig um ~32 MB/h**, Ursache offen
(keine Warnungen im Log).

## 2026-10-08 — M3, Slot-Mischer über `lmix-core`, VM (CPU)

Messung im Betrieb über die API von `lmix-core`, ohne Console:

```bash
curl -X POST 'http://127.0.0.1:8200/api/latency/measure?seconds=10'
sleep 11; curl http://127.0.0.1:8200/api/latency
# {"samples":10,"median_ms":116,"p95_ms":120,"max_ms":120}
```

- Messstelle: Ausgang `pgm_out` des Mischers (strom-Buffer-Age-Probe).
  Alle lmix-Flows laufen mit Basiszeit 0, `inter` behält die
  Zeitstempel: das Alter dort ist die Zeit seit der Aufnahme in der
  Quelle (Slot-Flow), über Slot-Flow, Kopplung und Compositor hinweg.
- strom meldet etwa einen Wert je Sekunde (jüngster Puffer), daher
  „samples“ = Sekunden der Messung.
- VM ohne GPU, CPU-Compositor 1080p30, 3 Slots plus Ausgänge:
  **Median 116 ms, p95 120 ms**.
- Regression: `scripts/test-m2b-vm.sh` misst 5 s und scheitert über
  `LATENCY_BUDGET_MS` (Standard 150 ms, VM); CI-Job `vm-check` (`.github/workflows/ci.yml`).
- Ziel auf Hardware: ≤ 2 Frames Quelle → PGM (H.2).

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

**Interlaced über SRT** (1 × 1080i, H.264 Main 8-Bit 4:2:0, MBAFF/Field,
plus 1 Testbild): NVDEC dekodiert, der Deinterlacer wird nur für diese
Quelle eingefügt. Erst `cudadownload ! deinterlace` (yadif, CPU): Backend
50 % CPU. Seit strom "Deinterlace NVDEC frames on the GPU" auf der GPU
(`cudaconvert` → RGBA → `cudadownload` nach GL → `gldeinterlace
greedyh`): 23 %, Bild korrekt.
yadif verdoppelte auf Halbbildrate (1080i50 → 50p), `gldeinterlace`
behält die Bildrate (→ 25p). Entschieden 2026-09-30: GPU bleibt.

## AMD / Intel

Nicht gemessen, keine Hardware (M5b). Pfade: GL-Mixer, VA-API.
