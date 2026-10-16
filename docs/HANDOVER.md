# Übergabe: Stand M4 (in Arbeit), lokale Übernahme

Stand 2026-10-09, PR-Liste 2026-10-12. Die Arbeit läuft lokal weiter (Arch, RTX 3090), Abnahme
auf echter Hardware.

## Offene PRs, Stand 2026-10-12 (Merge-Reihenfolge)

Gestapelt, jeder auf dem davor; von unten nach oben mergen (die Basis
des nächsten wird danach `main`, Gitea stellt sie beim Mergen um):

| PR | Branch | Basis | Inhalt |
|---|---|---|---|
| strom #7, #8 | `lmix-live-underlays`, `lmix-process-memory` | `dev` | Layout-Underlays live, eigener Speicher in den Stats |
| #8 | `m8-web` | `m4-pip` | M6/M8: Webseiten, Kurzbefehle, Auslöser, Cue-Listen, OSC, Textprotokoll, Metriken |
| #9 | `m6-open` | `m8-web` | Offene Punkte ohne Hardware |
| #17 | `m7-titles` | `m6-open` | M7 Teil A: Titel und Datenquellen ohne Render-Engine |
| #18 | `m9-console` | `m7-titles` | M9: Transportleiste, Stopp-Rückfrage, TSL, PTZ, Pakete, Layout-Fangen, MIDI-Lampen |
| #19 | `m9-data` | `m9-console` | XLSX/ODS, Zeilen zu Spalten, Folien aus Datei, Inputs einklappen |
| #20 | `m9-layout` | `m9-data` | Layout-Editor: Mehrfachauswahl, Sperre |
| #21 | `m9-keys` | `m9-layout` | Google Sheets API, Kurzbefehle für alle Shows, Zustand als Diff, Tally über HTTP, Wiedergabezeit-Auslöser, Tasten mit Bild, Inputs anhängen, fehlende Dateien, Beispiel-Clients |
| #22 | `m9-security` | `m9-keys` | Zugriff: nur LAN, Passwort, offene Seiten (API.7) |
| #23 | `m9-transport` | `m9-security` | Springen im Clip, Playlist-Eintrag abspielen (UI.5, UI.4) |

Unabhängig von `main`: #14 (CI-Token fürs strom-Submodul; der Token
braucht noch Leserechte, siehe `todo.md`), #15 (H.24-Vergleich), #16
(Qt-eigene Übersetzungen); Kante #53 (Dialog in Kante Light), danach
`scripts/sync-kante.sh`.

## Branches und PRs (Gitea), Stand 2026-10-09

| PR | Branch | Basis | Inhalt | Stand |
|---|---|---|---|---|
| #1–#3 | `lastenheft`, `m2b`, `m3` | `main` | Lastenheft bis M3 | gemergt |
| #4 | `claude-todo` (mit `console-qt`) | `main` | Restart-502, Selbsttest, Qt-Console statt egui | gemergt |
| #5 | `m4` | `main` | M4: Overlays (4.1), T-Bar (4.3), Kanalzug und Aux-Busse (4.7) | gemergt |
| #6 | `m4-stinger` | `main` | M4: Stinger (4.2) | offen |

strom-Fork (`shrippen/strom`), Ziel `dev`:
- Gemergt: #1 `lmix-start-time`, #2 `lmix-sample-rate`, #3
  `lmix-player-sysmem`, #5 `lmix-live-offset` (enthält #4
  `lmix-dsk-fade`, daher geschlossen).
- Offen: #6 `lmix-stinger-layer` (bis zu 8 DSK-Ebenen, Stinger-Ebene).
  Submodul von `m4-stinger` zeigt hierher.

Offen in dieser Reihenfolge: strom #6, dann lmix #6.

Seit 2026-10-09 auf `console-qt` und `claude-todo`:
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
git checkout main
git submodule update --init workspace/strom
```

Systempakete wie in `BUILD.md` („Systemabhängigkeiten“) bzw. die
apt-Liste in `scripts/cloud-setup.sh` (Ubuntu 24.04 / Kubuntu).

strom: `scripts/fetch-strom.sh` lädt das CI-Binary (ghcr, ohne Token)
oder baut lokal (`cargo build --release --features
no-gui`, 20–25 min kalt). Mischer-Backend wählt `lmix-core --mixer`
(Standard `cuda` für NVIDIA; ohne NVIDIA `gl` oder `cpu`).

## Starten (drei Prozesse)

```bash
STROM="$(scripts/fetch-strom.sh)"
"$STROM" --host 127.0.0.1 --port 8099 --no-auto-restart &          # Engine
(cd workspace/lmix-core && cargo run --release -- --strom http://127.0.0.1:8099) &   # Steuerung, API :8200
(cd workspace/console-qt && cargo run --release)                   # Qt-Console (Prototyp)
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

- **Merges** strom #6, lmix #6.
- B31 RTMP-Ausgang wartet auf strom #771.

## Bekannte Befunde

- GL-Mischer: SRT-Ausgang ohne Video (DECISIONS, „Hardware-Läufe“).
- Qt-Console: Ziel Linux allgemein; Ubuntu 24.04 über gepinntes KDE neon (BUILD.md), ungeeignet Debian 12.

- SRT-Input-Block in strom decodiert in der VM nicht (DECISIONS,
  „M3 Schritt 4“).
- Halten bei Quellverlust: in der VM teils ein schwarzer Frame (H.21).
- MP4-Aufnahme reserviert ~93 MB moov; MKV ist Standard.
