# Auf echter Hardware zu testen

Entwicklung läuft auch in einer VM ohne GPU (kein `/dev/dri`, kein NVIDIA,
GStreamer 1.24 aus Ubuntu 24.04). Was dort nicht prüfbar ist, landet hier,
automatisch bei jeder Arbeit, die es betrifft. Erledigtes wird mit Datum und
Messwert abgehakt, Messwerte gehen nach `LATENCY.md`.

Hardware: **NV** = NVIDIA-Host (RTX 3090), **AMD**, **INTEL**,
**CAP** = Capture-Karte/Kamera, **NET** = zweiter Rechner im LAN.

| # | Was | Hardware | Herkunft | Stand |
|---|---|---|---|---|
| H.1 | Bleibt GPU-Speicher über `intersink`/`intersrc` erhalten (CUDA/GL), oder kopiert die Kopplung über die CPU? Pipeline-Graph und GPU-Last prüfen | NV | ROADMAP 2b.5, M2b Schritt 2 | offen |
| H.2 | Slot-Mischer (eine Quelle je Flow, Kopplung über `inter`): Latenz Quelle → PGM ≤ 2 Frames, 16 × 1080p30 ohne Drop (Selftest wie M1b) | NV | M2b Schritt 2, NFA.2–3 | offen |
| H.3 | Quelle im Slot tauschen während Sendung: kein Aussetzer, kein Schwarzbild auf PGM | NV | M2b Abnahme, INP.1 | offen |
| H.4 | Live-Monitore über GPU-Speicher (DMABuf) statt Thumbnails | NV | G.2, OUT.11 | teilweise 2026-10-09: Qt-Console zeigt alle Inputs live über Unix-Sockets (memfd, `glupload`), 30 fps bei 18 Ansichten; DMABuf ohne Kopie offen |
| H.5 | Benchmark WPE gegen CEF (`BENCH-HTML.md`), inkl. H.264/AAC über NVDEC | NV | M7.0, E.9 | offen |
| H.6 | M1b-Abnahme auf AMD und Intel | AMD, INTEL | M5b | offen |
| H.7 | Echte Quellen: V4L2-Kamera, DeckLink SDI ein/aus | CAP | SRC.1, OUT.4 | offen |
| H.8 | Ton über die Slot-Kopplung (gebaut, B23): Lippensynchronität PGM bei Datei und SRT, Drift über 1 h, AFV-Blende ohne Knacken am Lautsprecher | NV | M2b Schritt 5 | offen |
| H.9 | Warmlauf des Mischers: Zeit vom Start bis zum ersten möglichen Übergang (VM: wenige Sekunden, CPU) | NV | M2b Schritt 2 | offen |
| H.10 | Kopplung mit Basiszeit 0 und `start_time_selection=first` auf CUDA- und GL-Mischer (VM: nur CPU geprüft): PGM bewegt sich, kein Aufholen, keine wachsende Latenz über 1 h | NV, AMD | M2b Schritt 4 | offen |
| H.11 | Console als Client: Bildabruf über `lmix-core` bei 16 Inputs (Last, Verzögerung der Kacheln), Doppelklick-Take am echten Bildschirm | NV | M2b Schritt 4 | erledigt 2026-10-09 mit der Qt-Console (RTX 3090, Wayland): 18 Ansichten je 30 fps, Console 34 % eines Kerns, GPU +4 %, Kachelalter ≤ 3 ms (`GUI-SHELL.md`); Doppelklick unter Xvfb geprüft, am echten Bildschirm noch nicht |
| H.12 | Dateidialog der Console über xdg-portal auf KDE Plasma (nativer Breeze-Dialog), Geräteliste mit echter Kamera (Auswahl → Input online) | CAP | M2b, B24/B25 | offen |
| H.13 | Monitor-Bus auf Kopfhörer (`--monitor-out auto`): PFL/AFL hörbar, Umschalten ohne Knacken, kein Drift gegen das Programm über 1 h | NV | PFL/AFL | offen |
| H.14 | NDI-Quelle (NDI-SDK fehlt in der VM): Bild und Ton, Geräteliste zeigt NDI-Quellen im Netz | NET | 2b.3 | offen |
| H.15 | WHIP-Ingest von Browser und OBS auf `/whip/<endpoint>`: Bild, Ton, Latenz | NET | 2b.3 | offen |
| H.16 | Audio-Gerät (Mikrofon, Line-In) als Input: Pegel, Lippensynchronität zur Kamera, Signalverlust beim Abziehen | CAP | B29 | offen |
| H.17 | Signalverlust halten/schwarz auf CUDA- und GL-Mischer (Pad-Namen `sink_N` dort gleich?), Kamera abziehen statt Clip pausieren | NV, CAP | 2b.3 | offen |
| H.18 | Ausgänge auf Hardware-Encoder (NVENC über `videoenc` auto): Last, Latenz, SRT zu echtem Ziel über 1 h, Aufnahme 1 h ohne Drop | NV, NET | M3 3.1 | offen |
| H.19 | NDI-Ausgang (NDI-SDK): in NDI Studio Monitor sichtbar mit Ton | NET | M3 3.1 | offen |
| H.20 | Bildschirm-Ausgang: Vollbild unter KDE Plasma auf zweitem Monitor (Fensterregel), DRM-Anschluss ohne Desktop, Tearing, Latenz zum PGM | NV, Beamer | M3 3.3 | offen |
| H.21 | Signalverlust „halten“ beim echten Abbruch (Kamera ab, SRT-Sender weg): kein schwarzer Frame im Moment des Verlusts (VM-CPU-Compositor zeigt teils einen), Failback ohne Aussetzer auf PGM | NV, CAP | M3 3.4 | offen |
| H.22 | Latenz Quelle → PGM über `GET /api/latency` auf CUDA- und GL-Mischer (VM: 116 ms Median, CPU); Budget im CI-Test auf Hardware-Wert senken | NV, AMD | M3 3.4 | offen |
| H.23 | SRT-Input (Kamera-Encoder, OBS) mit echter Quelle: Bild, Ton, Abbruch und Wiederkehr des Senders (VM: strom-Block decodiert MPEG-TS aus `srt-live-transmit` nicht, siehe DECISIONS) | NET | M3 3.4 | offen |
| H.24 | NFA.7: 8 h Soak mit Last (16 Inputs, 2 Streams, Einzelaufnahme) ohne Speicher- und Latenzdrift; danach 48 h ohne Neustart | NV, NET | NFA.7 | teilweise 2026-10-09: 295 min (CUDA, 16 Inputs, Aufnahme + ISO + 2 × SRT lokal), Latenz 56–63 ms ohne Anstieg, alle Bilder angekommen; **strom-RSS wächst ~32 MB/h** (763 → 998 MB), Ursache offen (VM: nach 12 min flach, nicht nachstellbar; Vergleichsläufe in DECISIONS „strom-Speicher (H.24)“); 8 h und 48 h offen |
| H.25 | Kanalzug mit echtem Mikrofon und Kamera: Verzögerung lippensynchron einstellen (Regler während der Sendung, kein Knacken/Aussetzer), Gate und Kompressor hörbar, EQ/Hochpass gegen Rumpeln | CAP | M4 4.7 | offen |
