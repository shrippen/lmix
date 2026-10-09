# Übergabe: Stand M3, lokale Übernahme

Stand 2026-10-09. Die Arbeit läuft lokal weiter (Arch, RTX 3090), Abnahme
auf echter Hardware.

## Branches und PRs (Gitea, gestapelt)

| PR | Branch | Basis | Inhalt | Stand |
|---|---|---|---|---|
| #1 | `lastenheft` | `main` | Lastenheft, Roadmap | fertig, Merge offen |
| #2 | `m2b` | `lastenheft` | lmix-core, Slot-Mischer, Show-Datei, Console als Client, Ton, Quellen | fertig, Merge offen |
| #3 | `m3` | `m2b` | Ausgänge, MultiCorder, Bildschirm, Failover, Latenz, Leitungszustand | fertig bis auf 3.5 (Entscheidung) |
| — | `m4` | `m3` | **WIP** Overlays (4.1), nicht getestet, kein PR | angehalten |

strom-Fork (`shrippen/strom`):
- `lmix-start-time` (PR #1): Startzeit `first`, Media-Player-Bus-Watch,
  `eos_on_stop`. Submodul von `m2b`/`m3` zeigt hierher.
- `lmix-dsk-fade` (auf `lmix-start-time`, kein PR): DSK-Blende mit
  `duration_ms`, nur für `m4`.
- `lmix-sample-rate` (auf `lmix-start-time`, PR #2): Mischer-Eigenschaft
  `sample_rate`. Submodul von `m3` zeigt seit 2026-10-09 hierher.

Merge-Reihenfolge: strom #1, #2, dann lmix #1, #2, #3 (jeweils auf Gitea).

Seit 2026-10-09 lokal auf `m3` (nicht gepusht):
- Live-Bilder über Unix-Sockets (`/api/live/…`, `?size=full` für die
  Vorschau).
- Qt-Console `workspace/console-qt`: Schritt 1 gemessen, Schritt 2 (Funktionen
  der egui-Console) übertragen, Test `scripts/test-console-qt.sh`.
- lmix-core: CUDA-Programm für Bildschirm/NDI/Live heruntergeladen,
  GL-Mischer gibt Systemspeicher aus, Aktionen vor „Engine bereit“ warten auf
  den Restore, ein neu gestartetes strom bekommt die Show zurück.
- `scripts/soak-hardware.sh`: 60 min CUDA bestanden (DECISIONS,
  „Hardware-Läufe“).

## Lokal einrichten

```bash
git clone https://git.arianw.de/shrippen/lmix.git && cd lmix
git checkout m3
git submodule update --init workspace/strom
```

Systempakete wie in `BUILD.md` („Systemabhängigkeiten“) bzw. die
apt-Liste in `scripts/cloud-setup.sh` (Ubuntu 24.04 / Kubuntu).

strom: `scripts/fetch-strom.sh` lädt das CI-Binary (mit `GITEA_TOKEN`,
`read:package`) oder baut lokal (`cargo build --release --features
no-gui`, 20–25 min kalt). Mischer-Backend wählt `lmix-core --mixer`
(Standard `cuda` für NVIDIA; ohne NVIDIA `gl` oder `cpu`).

## Starten (drei Prozesse)

```bash
STROM="$(scripts/fetch-strom.sh)"
"$STROM" --host 127.0.0.1 --port 8099 --no-auto-restart &          # Engine
(cd workspace/lmix-core && cargo run --release -- --strom http://127.0.0.1:8099) &   # Steuerung, API :8200
(cd workspace/console-qt && cargo run --release)                   # Qt-Console (Prototyp)
(cd workspace/app-desktop && cargo run --release -- --core http://127.0.0.1:8200)    # egui-Console
```

Die Qt-Console braucht Qt 6 mit Kirigami und das GStreamer-Plugin `qml6`
(Arch `gst-plugin-qml6`, Ubuntu `gstreamer1.0-qt6`); sie muss auf dem
Rechner der Engine laufen (Live-Bilder über Unix-Sockets in
`$XDG_RUNTIME_DIR/lmix/live`). Kante aktualisieren:
`scripts/sync-kante.sh`.

Kopfhörer für PFL/AFL: `lmix-core --monitor-out auto`. Externe Adresse
für WHIP-Links: `--public-url`.

## Prüfen

1. Ohne GPU zuerst den Gesamttest:
   `scripts/test-m2b-vm.sh` (in der Cloud zuletzt grün, Latenz p95 121 ms).
2. Dann die Hardware-Liste `docs/HARDWARE-TODO.md`, H.1–H.23. Für M3
   zuerst:
   - H.18 NVENC-Ausgänge, 1 h SRT und Aufnahme
   - H.20 Vollbild/DRM auf Beamer
   - H.21 Halten beim echten Kabelziehen
   - H.22 Latenz auf CUDA/GL (`GET /api/latency`, Knopf „Latency“)
   - H.23 SRT-Kamera als Input
3. Messwerte nach `docs/LATENCY.md`, Abgehaktes mit Datum in
   `HARDWARE-TODO.md`.

## Offen beim User

- **Merges** strom #1, #2, lmix #1–#3.
- B31 RTMP-Ausgang wartet auf strom #771.

## Bekannte Befunde

- GL-Mischer: SRT-Ausgang ohne Video (DECISIONS, „Hardware-Läufe“).
- Qt-Console: Ziel Linux allgemein; ungeeignet Ubuntu 24.04 und Debian 12 (kein Kirigami 6, BUILD.md).

- SRT-Input-Block in strom decodiert in der VM nicht (DECISIONS,
  „M3 Schritt 4“).
- Halten bei Quellverlust: in der VM teils ein schwarzer Frame (H.21).
- MP4-Aufnahme reserviert ~93 MB moov; MKV ist Standard.
