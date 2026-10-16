# Roadmap: lmix auf vMix-Niveau

Stand 2026-09-27. Grundlage: vMix 29 (Feb. 2026), strom `dev` (`bd86930`),
app-desktop `23e0606`. Ergänzt die Meilensteine in `claude.md`, ersetzt sie
nicht. Feststehende Entscheidungen und Ausschlüsse von dort gelten weiter.

Ziel seit 2026-10-08: Clean-Room-Reimplementierung von vMix. Was lmix können
muss, steht mit IDs in `LASTENHEFT.md`; diese Roadmap ordnet die IDs
Meilensteinen zu (§3a).

Legende Aufwandsort: **C** = nur app-desktop, **S** = strom-Patch
(upstream-PR), **B** = neuer Block.

---

## 1. Wie vMix funktioniert

### 1.1 Das Kernmodell: alles ist ein Input

```
  Inputs (beliebig viele, zur Laufzeit hinzufügen/entfernen)
  Kamera · NDI · SRT · OMT · Datei · Playlist/List · Bild · Farbe ·
  Browser · Titel (GT) · vMix Call · Replay · Mix · Zoom · PowerPoint
      │   jeder Input trägt: Audio-Kanal, Effekte (Farbkorrektur, Key,
      │   Crop/Zoom/Pan), bis zu 10 Layer (andere Inputs), Loop, Label
      ▼
  PREVIEW ──[Cut · Fade · Merge · Stinger 1–8 · T-Bar]──► OUTPUT
                                                          │
                                   Overlay 1–8 (je eigene Transition,
                                   auch auf Preview, auch aus Inputs)
                                                          ▼
                    Stream (3 Ziele) · Record · MultiCorder (ISO) ·
                    Fullscreen · Karte (SDI/HDMI) · NDI/SRT/OMT · VirtualCam
```

Der Operator denkt in **Inputs**, nicht in Pipelines. Ein Input ist Quelle,
Audiokanal und Grafikebene zugleich; derselbe Input kann Fullscreen, Layer
eines anderen Inputs, Overlay oder Replay-Quelle sein.

### 1.2 Funktionsumfang (gruppiert)

| Bereich | vMix |
|---|---|
| Schalten | Preview/Output, 4 Transition-Buttons (13 Typen), 8 Stinger, T-Bar, Quick Play/GO, bis 15 Zusatz-Mixe (Mix-Input) |
| Overlays | 8 Overlay-Kanäle mit Ein-/Ausblend-Transition, Position, Rand |
| Layer | 10 Layer pro Input, Crop/Zoom/Pan/Rotate, Chroma-/Luma-Key |
| Grafik | GT Title Designer (Titel, Ticker, Scoreboards, Timer), Datenquellen (Excel, Google Sheets, JSON, XML, RSS) |
| Audio | Kanal pro Input, Master + Busse A–G, Mute/Solo, EQ/Comp/Gate, VST3, Delay, Audio-follows-Video, Auto-Mix |
| Ausgänge | Stream (RTMP/SRT, 3 Ziele), Record (MP4/MOV/MXF/AVI), MultiCorder (jeder Input einzeln), External/Fullscreen, NDI-/SRT-/OMT-Out |
| Replay | Ringbuffer bis 8 Kameras, Zeitlupe, Events/Tags, Highlights, Quad-View |
| Gäste | vMix Call (WebRTC, bis 8), Zoom-Integration |
| Steuerung | Shortcuts (Tastatur, MIDI, X-keys, Stream Deck), Trigger (OnTransitionIn …), Activators (Tally/LED), VB.NET-Scripting, HTTP-/TCP-API, Web Controller, PTZ (VISCA/NDI) |
| Messen | Waveform, Vectorscope, Parade, Stream-Statistik, Tally |
| Projekt | Preset-Datei (.vmix) mit allen Inputs, Layern, Audio, Ausgängen |

Editionen begrenzen v. a. Input-Zahl, Auflösung, MultiCorder, Replay-Kameras,
Call-Gäste, Scripting. **vMix 29 spricht OMT** (Senden, Empfangen,
MultiCorder ohne Re-Encode) — relevant für M2 als Interop-Gegenstelle.

---

## 2. Abgleich: vMix ↔ strom ↔ lmix heute

| Funktion | strom (Engine) | lmix Console | Lücke / Weg |
|---|---|---|---|
| Preview/Program, Cut, Auto | ✅ | ✅ | — |
| Transitions | ✅ Fade, Slide, Push, Dip; Wipes/FX nur GPU | ✅ Auswahl + Dauer | Stinger, T-Bar fehlen (**S**) |
| Live-Monitore | Thumbnails fest 1 fps; Multiview nur per WHEP | 1 fps | **Kernlücke** (siehe M1-Rest) |
| Inputs zur Laufzeit | Mixer-Inputs fest zur Build-Zeit (2–16) | Quellen in einen Flow gebacken, Änderung = Neubau | **Kernlücke** (Input-Modell, M2b) |
| Quellentypen | V4L2, Datei/Playlist, NDI, SRT, WHIP/WHEP, AES67, DeckLink, PipeWire, CEF (strom-full) | Testbild, V4L2, Datei, URL | NDI/SRT/OMT/WHIP/HTML in Builder (**C**) |
| Layer/PiP, Crop | ✅ 4 PiPs, Zonen, Punch-in | ❌ | PiP-Editor (**C**) |
| Overlays | DSK 0–4, nur an/aus, keine Transition, nicht im Multiview | ❌ | 8 Kanäle + Transition (**S**) |
| Farbkorrektur, Key | ✅ Looks (nur GPU) | ❌ | UI (**C**), GPU nötig |
| FTB | ✅ | ❌ | Button (**C**) |
| Audio-Mixer | ✅ 128 Kanäle, Gruppen, Aux, Gate/Comp/EQ, PFL/AFL | Fader + Peak-Meter | Busse, Solo, EQ-UI, **Audio-follows-Video** (**C**) |
| Titel/Grafik | CEF-HTML (Build separat) | ❌ | HTML-Titel + Datenquellen (**C**/**B**) |
| Stream | SRT, WHIP, NDI; RTMP-Block upstream offen (#771) | ❌ | Ausgangsleiste (**C**) |
| Record / MultiCorder | Recorder-Block | ❌ | pro Input via `inter` (**C**) |
| Replay | ❌ | ❌ | Ringbuffer-Block (**B**) |
| Gäste | WHIP-Input | ❌ | Browser-Gast per WHIP (**C**) |
| Shortcuts | — | Tasten 1–8, Take | Editor, Trigger (**C**) |
| API/Automation | REST, WS, MCP | — | vorhanden auf Engine-Ebene |
| Projekt speichern | Flows persistiert | ❌ | Show-Datei (**C**) |
| Performance | GL-Mixer, NVDEC/NVENC vorhanden; gemessen lief aber der CPU-Compositor: 8 Inputs ok, 16 → 2,2 s Latenz | — | **GPU-Pipeline, M1b** |

**Befund**: Die Engine deckt Schalten, Audio und Transport breit ab. Was lmix
von vMix trennt, liegt fast vollständig im **Bedienmodell** (Inputs statt
Flows, Live-Bild, Overlays, Ausgangsleiste) und in drei Engine-Grenzen:
feste Input-Zahl, 1-fps-Thumbnails, CPU-Compositor.

**GPU-Befund (2026-09-27)**: Host hat RTX 3090, OpenGL 4.6 (NVIDIA-EGL),
NVDEC/NVENC, `cudacompositor`. `glvideomixerelement` und `cudacompositor`
laufen headless (ohne `DISPLAY`) fehlerfrei. "Kein GL auf dem Host" in
`DECISIONS.md` (2026-09-19) ist damit falsch; der CPU-Fallback kam aus
stroms Erkennung oder dem Konvertierungs-Workaround, nicht von der Hardware.

---

## 3. Roadmap

Jede Abnahme enthält einen Persona-Durchlauf (`PERSONAS.md`, mindestens 5
Personas, `LASTENHEFT.md` NFA.15).

Reihenfolge nach Wert für den Kern-Workflow
(Input → Preview → Take → Output). Jede Stufe endet abnahmefähig.

### Leitprinzip: Performance zuerst

Ziel: maximale Zahl paralleler Inputs/Outputs live auf einer Maschine.

- Frames bleiben vom Decode bis zum Encode im GPU-Speicher. Jeder
  `gldownload`/`cudadownload` im Hot-Path ist ein Bug, außer an Senken,
  die zwingend CPU-Speicher brauchen.
- Jedes Feature hat ein Messbudget (Latenz, GPU-/CPU-Last pro Input) und
  wird vor dem Merge gemessen; Selftest ist das Werkzeug.
- NVIDIA, AMD und Intel gleichrangig: jeder Hersteller bekommt seinen
  schnellsten Pfad, keiner fällt auf CPU zurück.
- Kennzahl über alle Meilensteine: **max. Inputs @ Budget** (1080p30,
  PGM + Multiview + 2 Encodes, < 5 Frames Latenz, kein Drop). Regression
  bricht CI.

### M1b — GPU-Pipeline (nächster Schritt, vor allem anderen)

Architektur: herstellerneutraler Kern, herstellerspezifische Ränder.

```
            Decode (pro Hersteller)          Kern (neutral)          Encode (pro Hersteller)
 NVIDIA   NVDEC ──CUDA-GL-Interop──┐                              ┌──CUDA-GL── NVENC
 AMD      VA-API ──DMABuf──────────┼──► OpenGL/EGL: Mixer, FX, ───┼──DMABuf─── VA-API
 Intel    VA-API/QSV ──DMABuf──────┘    Skalierung, Thumbnails    └──DMABuf─── VA-API/QSV
```

- Kern bleibt GL, weil stroms Mixer und Shader-FX schon GL sind und GL auf
  allen drei Herstellern läuft. Vulkan-Video als späterer neutraler Pfad
  beobachten (GStreamer hat heute Vulkan-Decode, aber keinen vollen
  Vulkan-Compositor).
- Herstellerwahl als eigene Schicht (`GpuProfile`: Erkennung per
  GL-Renderer/DRM-Treiber → passende Decoder, Encoder, Upload-Elemente);
  Blocks fragen die Schicht, statt Elementnamen fest zu verdrahten.

| # | Punkt | Ort |
|---|---|---|
| G.1 | Diagnose: warum stroms Auto-Auswahl CPU wählte (`has_hardware_gl()`-Probe im Backend-Log, Reihenfolge der Probe) und warum `autovideoconvert` mit CUDA-GL `not-negotiated` liefert. `STROM_FORCE_SOFTWARE_CONVERT` in `run.sh` durch echten Fix ersetzen | S |
| G.2 | Vision Mixer fest auf GPU-Backend (`gpu`), CPU nur als expliziter Notfall; Start bricht laut ab statt still zu degradieren | C (+S) |
| G.3 | `GpuProfile`-Schicht: Hersteller erkennen, Elementwahl kapseln (Decode, Encode, Upload/Download, JPEG). Baut auf stroms `gpu.rs` auf | S |
| G.4 | Decode auf GPU, zero-copy in GL: NVDEC (CUDA-GL), VA-API (DMABuf→GL) für AMD/Intel; H.264/H.265/JPEG aus Datei, SRT, NDI, OMT, V4L2-MJPEG | S/B |
| G.5 | Encode aus GL-Speicher: NVENC (Interop in `gpu.rs` vorhanden), VA-API-Encode per DMABuf für AMD/Intel | S |
| G.6 | Thumbnails/Monitor-Vorschau auf GPU skalieren und kodieren (NVJPEG bzw. VA-API-JPEG, sonst GL-Skalierung + kleines CPU-JPEG) | S |
| G.7 | Messen pro Hersteller: GL-Kern vs. herstellereigene Compositor (`cudacompositor`, `vacompositor`). Ein herstellereigener Pfad nur, wenn er messbar schneller ist, und dann hinter `GpuProfile`. Ergebnis in `DECISIONS.md` | — |
| G.8 | Selftest erweitert: N Inputs × M Outputs, misst GPU-Last (NVML bzw. `amdgpu`/`i915`/`xe`-fdinfo), Kopien pro Frame, Latenz; liefert "max. Inputs @ Budget" pro Hersteller | C |

Testlage: aktuell nur NVIDIA-Hardware verfügbar. AMD-/Intel-Pfade werden
trotzdem jetzt gebaut und optimiert (VA-API/DMABuf nach Doku und
GStreamer-Referenz), aber erst in M5b auf echter Hardware geprüft. Bis dahin
gilt für sie: Elementwahl per Unit-Test gegen eine simulierte
Element-Registry abgesichert, Pipeline-Beschreibung pro Profil als
Snapshot-Test, im Code als `unverified` markiert.

**Abnahme** (NVIDIA-Host; AMD/Intel siehe M5b):
16 × 1080p30 Inputs, PGM + Multiview + 2 Hardware-Encodes,
Latenz < 5 Frames, keine Drops über 10 min; Pipeline-Graph zeigt keinen
Download im Hot-Path; Messwerte in `docs/LATENCY.md`. Danach Kapazitätsgrenze
ausreizen (32+ Inputs) und dokumentieren.

**Stand 2026-09-30** (Details `DECISIONS.md`, Messwerte `LATENCY.md`):

| # | Stand |
|---|---|
| G.1 | ✅ Ursachen: GL-Probe (1.28), GLES/CUDA, Thumbnail-Tap pausierte Pipeline. `run.sh` ohne Override |
| G.2 | ✅ Console setzt Backend immer explizit, `auto` ohne GPU bricht ab |
| G.3 | ◐ Console: `gpu_profile.rs`. strom: EFP-Decoder nach Rang statt `nvh264dec`. Offen: Elementwahl als eigene Schicht in strom |
| G.4 | ◐ NVDEC aus Datei und SRT → CUDA ohne Kopie; Deinterlacer nur bei interlaced; GL-Mixer nimmt CUDA/VA (Brücke). Offen: V4L2-MJPEG, NDI, OMT (M2) |
| G.5 | ◐ NVENC aus CUDA ✅. VA-Encode aus GL (DMABuf): offen, ohne Hardware nicht prüfbar |
| G.6 | ◐ Skalierung auf GPU (CUDA, GL, VA unverifiziert); JPEG per NVJPEG auf NVIDIA (10 fps × 16: 18 % statt 39 % CPU). VA-JPEG offen |
| G.7 | ✅ `cudacompositor` auf NVIDIA (6 % vs. 108 % CPU) → Backend `cuda` |
| G.8 | ✅ Selftest: fps je Input/Ausgang, Alter, CPU, GPU/NVDEC/NVENC, VRAM. AMD/Intel-fdinfo offen |

Abnahme NVIDIA ✅: 16 × 1080p30, 10 min, alle Budgets
(`scripts/acceptance-m1b.sh`). Kapazität: 24 × 1080p30 und 32 × 720p30 im
Budget; 32 × 1080p30 NVDEC-begrenzt. Offen: sporadischer Startfehler bei
32 Media-Playern.

### M1-Rest — Console abschließen

| # | Punkt | Ort |
|---|---|---|
| 1.1 | **Live-Monitore**: PGM/PVW/Multiview flüssig. Entscheidung: WHEP-Empfang des `multiview_out` in der Console vs. upstream-PR konfigurierbares Thumbnail-Intervall. Empfehlung: WHEP (echte Framerate, keine REST-Last), Hardware-Encode (M1b), Decode in der Console auf der GPU (NVDEC bzw. VA-API). | C (+S) |
| 1.2 | Audio-follows-Video: Take schaltet zugeordnete Audiokanäle | C |
| 1.3 | FTB-Button, DSK-Toggles | C |
| 1.4 | Live-State-Test: Quelle weg während Transition (Akzeptanz aus `claude.md`) | C |

**Abnahme**: PGM-Monitor ≥ 25 fps sichtbar; Take mit AFV; Akzeptanz M1 erfüllt.

**Stand 2026-09-30** (Branch `m1-rest`, nicht in `main`, wartet auf
M1b-Freigabe):

| # | Stand |
|---|---|
| 1.1 | ◐ Entschieden: Eingänge immer flüssig (wie vMix), Umsetzung später (`DECISIONS.md`). Vorarbeit: PROGRAM-Monitor zeigt jetzt das echte PGM (Tap am Ausgang, 1 fps) statt des Input-Thumbnails |
| 1.2 | ✅ Audio pro Input (Testton/Dateiton) → Audio-Mixer; AFV-Schalter pro Kanal, folgt PGM aus WS |
| 1.3 | ✅ FTB (mit AUTO-Dauer) und DSK-Toggles; Selftest: FTB macht PGM schwarz |
| 1.4 | ✅ `--selftest-dropout`: Quelle stoppt mitten in der Blende; Blende schließt ab, Cut zurück, Ausgänge ohne Einbruch; WS-Spiegel = Engine-Zustand |

### M2 — OMT (wie geplant) + Interop

- OmtBlock Input (Option a, entschieden). **B**
- Neu: **OMT-Output** im selben Block-Modul — geringer Mehraufwand, macht
  lmix als Quelle für vMix 29 nutzbar. **B**
- Abnahme erweitert: vMix 29 → OMT → lmix PGM, und lmix PGM → OMT → vMix;
  Latenz in `docs/LATENCY.md`.

### M2b — Input-Modell (neu, Voraussetzung für alles Weitere)

vMix-Grundprinzip: Inputs kommen und gehen, ohne dass Program stockt.

```
  Quellen-Flow 1: V4L2 → inter_output "in1" ┐
  Quellen-Flow 2: NDI  → inter_output "in2" ├─► Mixer-Flow: inter_input ×N → Vision Mixer → PGM
  Quellen-Flow 3: Datei→ inter_output "in3" ┘                          └─► Audio-Mixer
```

| # | Punkt | Ort |
|---|---|---|
| 2b.1 | Ein Flow pro Quelle, Kopplung über `builtin.inter_*`; Mixer-Flow mit festen Slots (z. B. 16), Quelle in Slot tauschen ohne Mixer-Neustart | C |
| 2b.2 | Input-Leiste im vMix-Stil: Kacheln mit Typ, Label, Farbe, Audio, Klick = PVW, Doppelklick = Take | C |
| 2b.3 | Quellentypen im Builder: NDI, SRT, OMT, WHIP, Bild, Farbe, Playlist (mit Transport: Play/Pause/Seek/Loop) | C |
| 2b.4 | **Show-Datei** (`.lmix`, JSON): Inputs, Mixer-Konfig, Audio, Ausgänge, Hotkeys; Laden baut alle Flows | C |
| 2b.6 | **Steuerkern** (`LASTENHEFT.md` E.4): Show-Zustand und Aktionskatalog (API.1) in eigenem Prozess `lmix-core`; GUI wird Client. Voraussetzung für NFA.6 und alle APIs | C (neues Crate) |
| 2b.5 | Vorab messen: bleibt GPU-Speicher über `intersink`/`intersrc` erhalten (geteilter GL-/CUDA-Kontext) oder erzwingt die Kopplung Download/Upload? Falls Kopie: Kopplung ändern (geteilter Kontext, ein Flow mit dynamischen Pads), nicht Performance opfern. Ergebnis in `DECISIONS.md` | S? |

**Abnahme**: Input während laufender Sendung hinzufügen/entfernen, PGM ohne
Aussetzer; Show speichern, Neustart, Show laden → identischer Zustand.

**Stand 2026-10-08** (Branch `m2b`, VM-Test `scripts/test-m2b-vm.sh` grün,
GPU-Punkte in `HARDWARE-TODO.md`):

| # | Stand |
|---|---|
| 2b.6 | ✅ `lmix-core` + `lmix-types`, Aktionskatalog, HTTP/WS-API, Bilder je Input/PGM |
| 2b.1 | ✅ Slot-Flows über `inter`, Quellentausch auf Sendung (VM, CPU); Zeitbasis 0 + strom-Patch `start_time_selection` (`DECISIONS.md`) |
| 2b.4 | ✅ Show-Datei v1, Autosave, Wiederaufnahme nach Absturz (ohne Audio/Ausgänge/Hotkeys, die es noch nicht gibt) |
| 2b.2 | ✅ Console `--core`: Kacheln mit Bild, Tally, Typ, Farbmarke, Klick/Doppelklick, Menü; Dateidialog, letzte Shows (B24), Geräteliste (B25); ⚠ Audio der Kachel fehlt (B23) |
| 2b.3 | ✅ Farbe, Bild, Playlist mit Transport, NDI, WHIP, Audio-Gerät; Signalverlust halten/schwarz je Input; ⚠ OMT fehlt (kein strom-Block), NDI/WHIP/Geräte nur auf Hardware prüfbar (H.14–H.16) |
| B23 | ✅ Ton über die Slots: Kanal je Slot, an/aus, AFV, Lautstärke (an der Kachel, B28), PFL/AFL, Pegel; Audio-Inputs (B29) |
| 2b.5 | ⬜ nur auf GPU messbar (H.1) |

### M3 — Ausgänge + Robustheit (wie geplant, konkretisiert; GPU liegt jetzt in M1b)

| # | Punkt | Ort |
|---|---|---|
| 3.1 | Ausgangsleiste: Stream (SRT jetzt, RTMP sobald #771 upstream), Record, NDI/OMT-Out; Status: Bitrate, Drops, Laufzeit | C |
| 3.2 | MultiCorder: Recorder pro Input am `inter`-Bus | C |
| 3.3 | Fullscreen-Output auf zweitem Monitor (Sink im Engine-Prozess, nicht in der GUI — Crash-Isolation) | C |
| 3.4 | Failover, Latenz-CI (wie `claude.md`) | C |
| 3.5 | GUI-Shell-Entscheidung egui vs. Qt/Tauri — Kriterien jetzt: Video-Textur-Performance (1.1, GPU-Decode direkt in Textur), Multi-Display (3.3), Dichte der Input-Leiste (2b.2), Kante-Anbindung mit Kante Light und Breeze-Rückfall (`agent.md`, GUI-Regel) | — |

**Stand 2026-10-08** (Branch `m3`):

| # | Stand |
|---|---|
| 3.1 | ✅ Ausgänge SRT, Aufnahme (MKV/MP4/TS), NDI als eigene Flows; Start/Stopp, Status mit Laufzeit und Fehler; Senderate, Paketverluste und RTT für SRT (B32); RTMP wartet auf strom #771, OMT ohne strom-Block |
| 3.2 | ✅ MultiCorder als Ausgangsquelle "Input": Aufnahme, SRT oder NDI je Input (ISO) |
| 3.3 | ✅ Bildschirm-Ausgang im Engine-Prozess: Fenster, Vollbild (Wayland), DRM-Anschluss; ⚠ Vollbild/DRM nur auf Hardware prüfbar (H.20) |
| 3.4 | ✅ Failover: Quelle weg → Input offline, PGM hält, Slot kommt von selbst wieder (2–10 s Backoff); Ausgang bricht → Fehler, Neustart; Latenz-API und Budget-Check im VM-Test, CI `vm-check`; ⚠ SRT-Input in strom unzuverlässig (H.23) |
| 3.5 | ✅ Qt Quick/Kirigami über cxx-qt (User, 2026-10-09); Umstieg schrittweise nach `GUI-SHELL.md` |

### M4 — Produktionswerkzeuge (wie geplant, erweitert)

| # | Punkt | Ort |
|---|---|---|
| 4.1 | Overlays: 8 Kanäle, Ein-/Ausblend-Transition, im Multiview sichtbar | S |
| 4.2 | Stinger-Transition (Video mit Alpha + Cut-Punkt) | S |
| 4.3 | T-Bar (manueller Transition-Fortschritt) | S |
| 4.4 | HTML-Titel statt GT: Vorlagen (Lower Third, Ticker, Uhr, Countdown, Scoreboard) über CEF, Felder aus der Console; Datenquellen JSON/CSV | C/B |
| 4.5 | Instant Replay: Ringbuffer pro Input, Markieren, Zeitlupe, Abspielen als Input | B |
| 4.6 | PiP-/Layer-Editor, Looks/Farbkorrektur-UI | C |
| 4.7 | Audio: Busse, Solo/PFL, EQ/Dynamik-UI, Delay pro Kanal | C |

Stand:

| # | Stand |
|---|---|
| 4.1 | ✅ 4 Overlay-Kanäle (wie vMix; strom-DSK hat höchstens 4) über die DSK-Ebenen des Mischers, Ein-/Ausblenden (strom-Patch `lmix-dsk-fade`), Tally rot, in Show-Datei; Console: Knöpfe 1–4 je Kachel und Overlay-Leiste; ⚠ Position/Größe eines Overlays: über ein Layout (4.6) |
| 4.2 | ✅ Stinger: Clip-Input (Alpha, z. B. WebM/VP9) auf eigener DSK-Ebene unter den Overlays, Cut darunter am Cut-Punkt, Länge je Taste; gesperrt solange er läuft; strom-Patch `lmix-stinger-layer` (8 DSK); Console: Effekt „Stinger“ mit Dialog, Länge aus dem Clip |
| 4.3 | ✅ T-Bar: Vorschau wird beim Ziehen über Programm eingeblendet (Mischung), am Ende Take, zurück auf 0 ohne Wirkung; Position im Zustand; Console-Schieber; ⚠ nur Blende (andere Effekte am Ende als Cut), Ton folgt erst beim Take |
| 4.6 | ✅ Looks je Input (Farbkorrektur, nur GL-Mischer, sonst Ablehnung mit Grund); Layouts: Hintergrund und bis zu 8 Boxen mit Rahmen als PiP-Komposition, in Vorschau/Programm wie ein Input, Tally für alle Inputs darin; strom-Patch `lmix-live-underlays`; Boxen frei ziehen und zuschneiden; Console: „Farbkorrektur…“ je Kachel, Layout-Leiste und -Dialog mit Fläche; ⚠ keine Bewegung zwischen Layouts |
| 4.7 | ✅ Kanalzug je Input: Gain, Pan, Hochpass, 4-Band-EQ, Gate, Kompressor, Verzögerung (live, strom-Patch `lmix-live-offset`); 2 Aux-Busse (1 vor, 2 nach dem Fader) mit Sends je Input, Master, Pegel, Ausgang mit Aux-Ton (Mix-Minus); alles in Show-Datei; Console: „Kanalzug…“, „Aux…“, Ausgangsdialog „Ton“; Solo/PFL/AFL seit M2b; ⚠ vor/nach fest je Bus |
| 4.4 | → M7 (Titel) |
| 4.5 | → „Später“ (Wiederholung) |

### M5 — Robustheit (wie geplant)

8-h-Soak, `kill -9` der GUI, CI grün. Zusätzlich: Show mit 16 Inputs,
2 Streams, MultiCorder als Soak-Last.

### M5b — Hersteller-Abnahme AMD/Intel (sobald Hardware da ist)

- M1b-Abnahme auf je einer AMD- und Intel-Maschine wiederholen, Werte in
  `docs/LATENCY.md`, `unverified`-Markierungen entfernen oder Pfade fixen.
- Idealerweise als CI-Runner, damit "max. Inputs @ Budget" pro Hersteller
  regressionsgeschützt ist.

### M6 — Steuerung & Remote (Vorschlag, nach Freigabe)

- Shortcut-Editor, Trigger (OnTransitionIn/Out, OnCountdown) — **C**
- Tally über WS-Events (Ausgabe an einfache HTTP-Tally-Geräte) — **C**
- (später) Gäste per Browser-WHIP (vMix-Call-Ersatz); braucht erreichbaren
  Media-Port, klärt die offene RTSP-/Bind-Frage aus `DECISIONS.md` mit — **C**
- Scopes (Waveform, Vectorscope) auf PGM — **C**
- PTZ über VISCA-IP — **C**, optional

Stand:

| # | Stand |
|---|---|
| AUT.1–3 | ✅ Kurzbefehle (Taste, bis 16 Aktionen) in der Show; Console-Editor, Web-Seite `/keys` |
| AUT.7 | ✅ Auslöser je Input: Programm an/aus, Vorschau, Overlay ein/aus, Clip-Ende und Countdown davor, mit Verzögerung |
| AUT.10, AUT.11 | ✅ Geschlossenen Input zurückholen; Sperre gegen Schließen/Stoppen |
| AUT.13 | ✅ OSC: Aktionen per Adresse oder JSON; Tally, Programm, Vorschau, T-Bar, FTB, nächster Cue an Abonnenten; ganzer Zustand auf Anfrage |
| AUT.14 | ✅ Cue-Listen mit GO, Zurück, Sprung; Console-Dialog |
| AUT.4 | ✅ MIDI aus Gerätedateien: Note/Regler je Kurzbefehl, Regler auf T-Bar, Lernen in der Console; ⚠ keine Rückmeldung an den Controller |
| AUT.12 | ✅ Globale Tasten über das GlobalShortcuts-Portal (KDE, GNOME 48+); ⚠ nicht unter X11 ohne Portal |

### M7 — Titel & Daten (HTML), direkt nach M4

**M7.0 Benchmark** vor der Engine-Wahl (E.9): `cefsrc` (vorhanden) gegen
WPE WebKit (`wpesrc`) mit 8 animierten Titeln 1080p60 (CSS, Canvas,
lottie-web), Alpha; messen: GPU/CPU je Titel, Drops, Latenz, Speicher,
Kopien bis in den Mischer. Pflicht-Test GFX.20: H.264/AAC-Video, HLS über
MSE, Twitch- und YouTube-Einbettung (§5.3).
Testplan: `BENCH-HTML.md`.

Titel-Fähigkeiten nach dem Vorbild CasparCG (`LASTENHEFT.md` §3.11):
Vorlagen-Konvention, Lebenszyklus, Titel-Ebenen, Datensätze.

Titel sind Muss (`LASTENHEFT.md` §3.11): HTML-Vorlagen mit Feld-API,
Animationen per CSS/JS und Lottie, Vorlagen-SDK, Datenquellen. Ersetzt M4.4.
IDs: GFX.1–19, DAT.1–5, SWI.7, OVL.7. Entscheidungen E.9 (Engine), E.10 (Lottie).

**Abnahme**: Bauchbinde aus CSV-Zeile, Ein-/Ausblendanimation als Overlay,
Lottie-Bauchbinde mit Textfeld, Countdown mit Auslöser bei 0, 8 animierte
Titel 1080p60 ohne Drop.

Stand (2026-10-11, Teil A ohne Render-Engine, DECISIONS „M7 Teil A“):

| Teil | Stand |
|---|---|
| Grafikkanal als Seite, CasparCG-Lebenszyklus, JSON/XML-Daten, Feld hochzählen | ✅ GFX.1, 3, 4 (ohne Overlay-Warten) |
| Vorlagen eingebaut (Kante-Satz, Textfolien) und aus Ordner, Neuladen | ✅ GFX.12, 13, 14, 19; GFX.21 teils (Folien aus Datei, Beamer fehlt) |
| Datensätze (vor/zurück, CSV), Datenquellen CSV/XLSX/ODS/JSON/Text/Adresse mit Bindung, Zeilen zu Spalten, Auto-Weiter | ✅ GFX.8, DAT.2, DAT.3, DAT.5; DAT.1, 4 teils |
| Console: Titel- und Datenquellen-Dialog | ✅ GFX.7 |
| M7.0 Benchmark, Rendern in strom (GFX.2), Kanal als Input/Overlay | offen (GPU, `cefsrc`/`wpesrc`) |
| Auslöser auf Titelmeldungen (Countdown bei 0) | ✅ GFX.9 |
| Ebenen-Transformation, animiert | ✅ GFX.18 |
| Lottie in Vorlagen (Bauchbinde mit Textfeldern) | ✅ GFX.5 teils |
| Titel als Stinger | offen (Engine) |

### M8 — Fernbedienung, API-Gleichstand

IDs: WEB.1–5, TEL.1–3, API.2–11, STR.8.

**Abnahme**: Bitfocus Companion steuert lmix über das eigene Modul
(API.9, API.11); Tally auf dem Handy.

Stand:

| # | Stand |
|---|---|
| WEB.1–3 | ✅ `/mixer`, `/tally`, `/keys` aus lmix-core, Kante-Aussehen; Anmeldung mit `--password`, Seiten einzeln offen (API.7) |
| API.5 | ✅ Textzeilen über TCP: Aktionen, Tally/Zustand/Pegel/Ausgänge im Abo |
| OPS.1 | ✅ Show beim Start (`LMIX_SHOW`), SIGTERM, systemd-Benutzer-Units |
| OPS.3 | ✅ `/metrics` (Prometheus) mit Maschine, GPU, Speicher von strom (strom-Patch `lmix-process-memory`) und QoS je Flow |

### M9 — Gleichstand

Restliche S-Anforderungen: SWI.6 (Verschmelzen), SWI.10 (Mix-Inputs),
LAY.6 (Ebenen-Editor), PRC.9 (Effektkette), AUD.9/11 (Matrix, Plugins),
OUT.4/5 (Karte, Key+Fill), PER.2–5 (PTZ, Uhren), UI.3–12, SHW.6–9.

**Abnahme**: Abdeckungsmatrix (`LASTENHEFT.md`) ohne offene M/S.

Stand (2026-10-12, ohne GPU; DECISIONS „M9 Anfang“):

| Teil | Stand |
|---|---|
| Transportleiste, Stopp-Rückfrage, Startoptionen, Kachelgröße/Reihen/Einklappen, einfacher Modus | ✅ UI.5 teils, UI.10, UI.12 teils, UI.3, UI.7 |
| Input-Einstellungen in Reitern | ◐ UI.8 (ohne Key, Effekte, Position, Ebenen, Tally) |
| Dynamische Inputs/Werte | ✅ AUT.5 |
| Tally über TSL UMD 3.1/5.0 | ✅ PER.1 teils (ohne GPIO) |
| PTZ über VISCA (IP, UDP), Positionen per Auslöser | ◐ PER.2, PER.3 (an echter Kamera ungeprüft) |
| Show-Paket mit Medien, Daten, Vorlagen | ✅ SHW.6 |
| Layout-Editor: Ziehen, Fangen, Mehrfachauswahl, Sperre, Rückgängig | ✅ LAY.6 |
| Clips spielen/pausieren auf Sendung von selbst; alle Overlays aus, letzter Input je Kanal | ✅ INP.11, OVL.8 |
| Schnellstart mit eigenem Effekt; Klick- und GO-Aktion je Input | ✅ SWI.8, INP.8, INP.9 |
| Overlay-Kanäle: Effekt, Dauer, Auto-Aus, Bild im Bild; Overlay mit dem nächsten Take | ◐ OVL.2, OVL.3 (ohne Rand, Schiebe-/Wischeffekte, Vorschau-Bild: Engine) |
| Layout-Vorlagen außerhalb der Show, Ex-/Import | ✅ LAY.5 |
| Verschmelzen, Mix-Inputs, Effektkette, Karten-Ausgang, Key+Fill | offen (Engine/Hardware) |

### Später — Wiederholung, Gäste

Zurückgestellt (User 2026-10-08): RPL.1–10, SRC.19–20, CAL.1–6, SRC.24.
Ersetzt M4.5 und den Gäste-Punkt in M6.

---

## 3b. Lösungsansätze (angenommen 2026-10-08)

| Baustelle | Ansatz | Meilenstein |
|---|---|---|
| Input-Modell | Ein Flow je Quelle, feste Mischer-Slots, Slot tauschen statt neu bauen; vorher messen, ob die Kopplung GPU-Speicher hält, sonst dynamische Pads; Offline-Input mit Ersatzbild | M2b |
| Steuerkern | `lmix-core` hält Show, Aktionskatalog, Auslöser, Ablauflisten; Aktionskatalog als ein Rust-Enum mit Metadaten, daraus API-Doku, Tastenbelegungs-Dialog, Mappings | M2b |
| Live-Monitore | Bilder lokal per DMABuf/Shared Memory als GPU-Textur in die GUI, Kacheln als ein Mosaik-Frame; WHEP nur für entfernte Clients | M1-Rest/M3 |
| Overlays, Stinger, T-Bar | Ein strom-Patch: Übergangsfortschritt 0–1 setzbar; Overlay-Kanal = Ebene mit eigenem Übergangszustand; Stinger-Schnitt framegenau im Mischer | M4 |
| API für Fremdprogramme | Eigene versionierte API, Aktionen parametrisiert, eigenes Companion-Modul; Prüfstein: was das Companion-vMix-Modul kann | M8 |
| Controller | MIDI, HID, Gamepad, Stream Deck direkt; Rückmeldungen aus Zustandsereignissen | M6 |
| GUI-Shell | Entscheidung nach Prototyp (32 flüssige Kacheln, zweiter Monitor); Vollbild-Ausgänge nie in der GUI | M3 |
| Titel | HTML in strom (`cefsrc`), Konvention und Fähigkeiten nach CasparCG, kein CasparCG-Server (T.2) | M7 |

---

## 3a. Lastenheft → Meilenstein

| Bereich (`LASTENHEFT.md`) | Meilenstein |
|---|---|
| INP.1–3, INP.13, SRC.2–9, SHW.1–5, API.1, UI.1/2/5/8 | M2b |
| SRC.10–12 (NDI, OMT, SRT ein) | M2 / M2b |
| OUT.1–3, OUT.6–11, REC.1–6, MCR.1–3, STR.1–7 | M3 |
| OVL.1–6, SWI.3–5, LAY.1–2, PRC.1–8, AUD.2–16 | M4 |
| NFA.5–7, NFA.13 | M5 |
| NFA.1–2 (AMD/Intel) | M5b |
| AUT.1–11, PER.1, PER.6–7, OUT.12–13 | M6 |
| GFX, DAT | M7 (nach M4) |
| WEB, TEL, API.2–11 | M8 |
| OPS.1–3, AUT.12–14, OUT.17, UI.13, GFX.21 | M6/M7 (nach Thema) |
| Rest S, dann K | M9 |
| RPL, CAL | Später |

---

## 4. Bewusst nicht

Maßgeblich ist `LASTENHEFT.md` §5. Änderungen gegenüber dem Stand
2026-09-27, offen in `LASTENHEFT.md` E.1/E.8: Control-Surfaces, 4K und
Web-Fernbedienung werden für den Gleichstand gebraucht; Scripting und
Audio-Plugins (LV2, VST3 für Linux, CLAP) sind K statt ausgeschlossen.
Weiter ausgeschlossen: Windows/macOS, XAML-Titel, Zoom-, DVD-, Flash-Input,
Virtual Sets.

## 5. Offene Entscheidungen

1. Live-Monitore: WHEP vs. Thumbnail-Intervall-PR (M1-Rest).
2. M2b als eigener Meilenstein vor M3 (Empfehlung) — ändert die Reihenfolge
   in `claude.md`.
3. ~~Lizenz CEF-Pfad~~ — geprüft 2026-10-08: `gstcefsrc` ist LGPL-2.1-or-later
   (`LICENSE`, Datei-Header, Plugin-Metadaten "LGPL", Commit `204b01b`), CEF
   BSD-3-Clause. strom lädt `cefsrc` zur Laufzeit als GStreamer-Plugin, kein
   Linken, kein Quellcode im Repo → vereinbar mit Entscheidung 6 (wie
   GStreamer selbst, ebenfalls LGPL). Regeln: gstcefsrc-Code nie ins Repo
   kopieren (Änderungen upstream oder eigenes Repo); beim Ausliefern von
   Paketen LGPL-Text, Quellverweis und CEF-/Chromium-Hinweise beilegen.
   Standard-CEF-Builds spielen kein H.264/AAC ab (Patente). Gebraucht wird es
   aber (`LASTENHEFT.md` GFX.20, Webseiten als Quellen). Wege, im Benchmark
   M7.0 zu prüfen:
   (a) WPE WebKit (`wpesrc`, LGPL): Medien laufen über GStreamer, also über
       die installierten Decoder (NVDEC, VA-API, gst-libav); lmix liefert
       keinen Patent-Codec selbst aus. Kompatibilität mit fremden Seiten
       prüfen.
   (b) Eigener CEF-Build mit `proprietary_codecs=true`,
       `ffmpeg_branding=Chrome`: volle Chromium-Kompatibilität, aber
       Software-Decoder für H.264/AAC im ausgelieferten Paket (Patentlizenz
       beim Verteilen), großer Build.
   (c) CEF-Build nur mit Hardware-Dekodierung: ohne Patent-Software, aber
       ohne passende GPU kein H.264.
   Empfehlung: (a) als Favorit, (b) als Rückfall, falls WPE an fremden
   Seiten scheitert.
4. ~~Testhardware AMD/Intel~~ — entschieden 2026-09-27: derzeit keine
   verfügbar; herstellerneutral bauen, Abnahme später (M5b).
5. Clean-Room-Fragen E.1–E.8 in `LASTENHEFT.md` §6.
6. ~~strom-CRITICALs~~ — geklärt 2026-10-08: strom verwirft Upstream-QoS
   an Sinks mit `PadProbeReturn::Drop`; gstreamer-rs leert dabei die Daten,
   GStreamer 1.24 gibt dann NULL frei (eine CRITICAL je Frame). lmix
   schaltet QoS an seiner PGM-Senke ab. In strom trifft es jede Sink mit
   QoS: als Fehler an strom melden (Probe `drop_upstream_qos_events`).
7. strom-Patch `lmix-start-time` (Branch im strom-Fork) nach `dev` mergen;
   das Submodul zeigt bis dahin auf den Branch.

Quellen: vMix-Features <https://www.vmix.com/software/features.aspx>,
vMix 29 <https://blog.vmix.com/vmix-29-is-available-now/>, strom-Doku
`workspace/strom/docs/VISION_MIXER_OPERATOR_GUIDE.md`,
`AUDIO_MIXER_OPERATOR_GUIDE.md`, `FEATURE_SUGGESTIONS.md`.
