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
| H.8 | Ton über die Slot-Kopplung (sobald gebaut, B23): Lippensynchronität PGM, Drift über 1 h | NV | M2b | offen |
| H.9 | Warmlauf des Mischers: Zeit vom Start bis zum ersten möglichen Übergang (VM: wenige Sekunden, CPU) | NV | M2b Schritt 2 | offen |
