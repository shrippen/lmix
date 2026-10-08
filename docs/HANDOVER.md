# Übergabe: Stand M3, lokale Übernahme

Stand 2026-10-08. Die Arbeit in der Cloud-Session endet hier. Weiter geht es
lokal mit Abnahme auf echter Hardware.

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
- `lmix-sample-rate` (auf `lmix-start-time`, kein PR): Mischer-Eigenschaft
  `sample_rate`. Submodul von `m3` zeigt seit 2026-10-09 hierher.

Merge-Reihenfolge: strom #1, dann lmix #1, #2, #3 (jeweils auf Gitea).

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
(cd workspace/app-desktop && cargo run --release -- --core http://127.0.0.1:8200)    # Console
```

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

- **Entscheidung 3.5 GUI-Shell**: `docs/GUI-SHELL.md` (Empfehlung Qt
  Quick/Kirigami; `claude.md` sagt bisher egui).
- **Repo-Secret** `REGISTRY_TOKEN` (lmix, `read:repository`,
  `write:package`): ohne es laufen die Workflows `strom` und `vm-check`
  nicht.
- **Merges** strom #1, lmix #1–#3.
- B31 RTMP-Ausgang wartet auf strom #771.

## Bekannte Befunde

- SRT-Input-Block in strom decodiert in der VM nicht (DECISIONS,
  „M3 Schritt 4“).
- Halten bei Quellverlust: in der VM teils ein schwarzer Frame (H.21).
- MP4-Aufnahme reserviert ~93 MB moov; MKV ist Standard.
