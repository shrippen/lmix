# Übergabe: Stand M9 und Show-Modus S1 (in Arbeit), lokale Übernahme

Stand 2026-10-09, PR-Liste 2026-10-17. Die Arbeit läuft lokal weiter (Arch, RTX 3090), Abnahme
auf echter Hardware.

## Offene PRs, Stand 2026-10-17 nachts (Merge-Reihenfolge)

CI läuft auf GitHub (Status `github / test` am Commit auf Gitea). PR-Agent
scheitert am Kostenlimit des Schlüssels (`todo.md`), grüne CI genügt.

Unabhängig von `main`, zuerst:

| PR | Branch | Inhalt |
|---|---|---|
| #45 | `vm-failover-later` | VM-Check: Quelle stirbt später in der Blende (Failover-Prüfung wackelte) |
| #51 | `console-test-coords` | Lokaler Console-Check: wartet auf das Bild nach dem Cut, ignoriert CRITICALs von strom beim Start |
| #39 | `se3-measure` | S-E.3 gemessen (Klang-Cues), nur Doku und Messskript |

Show-Modus S1 und Rest, gestapelt, jeder auf dem davor (von oben nach
unten mergen; Gitea stellt die Basis des nächsten auf `main` um):

| PR | Branch | Inhalt |
|---|---|---|
| #38 | `s1-cues` | Cue-Modell und Ablauf in lmix-core (Stufe 1) |
| #40 | `s1-script` | Drehbuch-Import `lmix-script` (Stufe 6) |
| #41 | `s1-targets` | Ausgabeziele HTTP, Home Assistant (Stufe 4) |
| #42 | `s1-safety` | Panik, harter Stopp, Notfall-Liste (Stufe 5) |
| #43 | `s1-feeds` | Restzeit als Ereignis, feste OSC-Abonnenten (CTL.2, CTL.4) |
| #44 | `s1-show-view` | Show-Ansicht der Console (Stufe 7) |
| #46 | `s1-timing` | Cue-Zeitgenauigkeit in `/metrics`, Zugangsschlüssel mit Rechten |
| #47 | `s1-checks` | Warnliste, Rechner-Einstellungen im Paket, Persona-Durchlauf 10 |
| #48 | `s1-script-folder` | Show merkt sich ihren Drehbuch-Ordner |
| #49 | `s1-targets-ui` | Ziele und Abonnenten in der Console |
| #50 | `s1-polish` | Dauer an Aktions-Cues, dunkler Raum, OSC-Abo-Ablauf, Übergangsleiste rollt (Fix für `main`) |
| #52 | `m9-settings` | Älteres Show-Format beim Speichern gesichert; Einstellungen ex-/importieren (SHW.7/8) |
| #53 | `s1-zones` | Zonen in der Show, Ausgänge je Rechner (SND.10 ohne Engine); Gruppe endet nach Neuladen |
| #54 | `s1-ctl` | Panik in Zeit x, Cues über `standby`, Listen über `active` (CTL.1) |

Rote GitHub-Läufe auf `s1-feeds` und `s1-script-folder` sind die wackelige
Failover-Prüfung, die #45 behebt (der Stapel bekommt sie über `main`).

Breaking (vor dem ersten Release erlaubt, je PR beschrieben): Cue `label`
→ `name`, `cue_back`/`cue_jump` → `cue_standby`, `next` → `standby`,
OSC `/lmix/cue/next` → `/lmix/cue/standby`, OSC-Abos laufen nach 60 s
ohne Lebenszeichen ab; Show-Dateien Version 1 werden beim Öffnen
umgewandelt (alte Datei bleibt als `.v1.bak`).

Offen für S1 (DECISIONS „Show-Modus …“, PERSONAS Durchlauf 10): Klang-
und Bild-Cues, Zonen, Bühnen (Stufen 2/3), warten auf S-E.3 und GPU.

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
