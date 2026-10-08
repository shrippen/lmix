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
| H.4 | Live-Monitore über GPU-Speicher (DMABuf) statt Thumbnails | NV | G.2, OUT.11 | offen |
| H.5 | Benchmark WPE gegen CEF (`BENCH-HTML.md`), inkl. H.264/AAC über NVDEC | NV | M7.0, E.9 | offen |
| H.6 | M1b-Abnahme auf AMD und Intel | AMD, INTEL | M5b | offen |
| H.7 | Echte Quellen: V4L2-Kamera, DeckLink SDI ein/aus | CAP | SRC.1, OUT.4 | offen |
| H.8 | Ton über die Slot-Kopplung (gebaut, B23): Lippensynchronität PGM bei Datei und SRT, Drift über 1 h, AFV-Blende ohne Knacken am Lautsprecher | NV | M2b Schritt 5 | offen |
| H.9 | Warmlauf des Mischers: Zeit vom Start bis zum ersten möglichen Übergang (VM: wenige Sekunden, CPU) | NV | M2b Schritt 2 | offen |
| H.10 | Kopplung mit Basiszeit 0 und `start_time_selection=first` auf CUDA- und GL-Mischer (VM: nur CPU geprüft): PGM bewegt sich, kein Aufholen, keine wachsende Latenz über 1 h | NV, AMD | M2b Schritt 4 | offen |
| H.11 | Console als Client: Bildabruf über `lmix-core` bei 16 Inputs (Last, Verzögerung der Kacheln), Doppelklick-Take am echten Bildschirm | NV | M2b Schritt 4 | offen |
| H.12 | Dateidialog der Console über xdg-portal auf KDE Plasma (nativer Breeze-Dialog), Geräteliste mit echter Kamera (Auswahl → Input online) | CAP | M2b, B24/B25 | offen |
| H.13 | Monitor-Bus auf Kopfhörer (`--monitor-out auto`): PFL/AFL hörbar, Umschalten ohne Knacken, kein Drift gegen das Programm über 1 h | NV | PFL/AFL | offen |
