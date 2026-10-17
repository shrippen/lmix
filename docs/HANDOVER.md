# Übergabe: Stand M9 und Show-Modus S1 (in Arbeit), lokale Übernahme

Stand 2026-10-09, PR-Liste 2026-10-17 (bis #88). Die Arbeit läuft lokal weiter (Arch, RTX 3090), Abnahme
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
| #56 | `s1-release` | GO erst nach Loslassen (RUN.6) |
| #57 | `s1-check` | Nicht erreichbare Ziele in der Warnliste, „Ziele prüfen“ (SUI.5) |
| #58 | `s1-trust` | Vertraute Adressen ohne Schlüssel, `--trust` (CTL.5) |
| #59 | `s1-osc-topics` | OSC-Abo je Thema und Feld (CTL.3); enthält den Fix aus #45 |
| #60 | `se13-stage-measure` | S-E.13 Bühnen gemessen, nur Doku und Messskript |
| #61 | `todo-1017` | Übergabe und Todo nach dem 17.10. |
| #62 | `lastenheft-stand` | Lastenheft: Stand von 68 Zeilen nachgezogen |
| #63 | `aud14-fades` | Pegel blenden: Eingang, Bus, Master über Zeit (AUD.14) |
| #64 | `src4-alpha` | Standbilder mit Alpha (PNG, WebP, TIFF) (SRC.4) |
| #65 | `aud1-stereo` | Pegel je Kanal an der Kachel (AUD.1) |
| #66 | `aud4-monitor` | Abhörpegel, Standardverzögerung neuer Eingänge (AUD.4/AUD.6) |
| #67 | `out2-multiview` | Multiview als Ausgangsquelle (OUT.2) |
| #68 | `out1-format` | Programmformat: Auflösung und Bildrate (OUT.1) |
| #69 | `handover-1017b` | Übergabe: PRs #62–#68 |
| #70 | `strom-still-release` | strom-Pin mit Fix für das Leck pausierter Clips (strom #15) |
| #71 | `ovl1-eight` | Acht Overlay-Kanäle (OVL.1, strom #16); vorweg `cargo fmt` über den Stapel |
| #72 | `str1-streams` | Alle Streams zusammen starten/stoppen (STR.1) |
| #73 | `api4-playback` | Abspielstand im Zustand (API.4) |
| #74 | `inp10-at-end` | Clip am Ende: Schleife oder stehen bleiben (INP.10, strom #17) |
| #75 | `aud2-seven-aux` | Sieben Aux-Busse A–G (AUD.2) |
| #76 | `vm-act-timeout` | VM-Check: Aktionen mit Zeitlimit, Threads von lmix-core bei Hänger |
| #77 | `str3-encoding` | Codec, Rate Control, Keyframes je Ausgang (STR.3) |
| #78 | `swi5-tbar-sound` | Ton folgt der T-Bar (SWI.5) |
| #79 | `handover-1017c` | Übergabe: PRs #69–#78, strom-Stapel |
| #80 | `engine-call-timeout` | Engine-Aufrufe mit Zeitlimit 60 s (Hänger von lmix-core) |
| #81 | `out10-multiview` | Multiview: Program links/rechts, Pegel an/aus (OUT.10) |
| #82 | `lay2-box-visible` | Layout-Box ein-/ausblenden (LAY.2) |
| #83 | `shw3-new-format` | Neue Show mit Formatwahl (SHW.3) |
| #84 | `personas-1017` | Persona-Durchlauf 11, Übergabe bis #84 |
| #85 | `str2-rtmp` | RTMP-Ausgang (STR.2, B31), strom #18 |
| #86 | `strom-upstream-fixes` | strom-Pin mit upstream #874/#788 (strom #19) |
| #87 | `vm-picture-retry` | VM-Check: fehlendes Bild ist ein Fehlversuch |
| #88 | `aud12-meter` | Pegel stumm blass, Spitzenwert 2 s (AUD.12) |

Rote GitHub-Läufe auf `s1-feeds` und `s1-script-folder` sind die wackelige
Failover-Prüfung, die #45 behebt (der Stapel bekommt sie über `main`).

Breaking (vor dem ersten Release erlaubt, je PR beschrieben): Cue `label`
→ `name`, `cue_back`/`cue_jump` → `cue_standby`, `next` → `standby`,
OSC `/lmix/cue/next` → `/lmix/cue/standby`, OSC-Abos laufen nach 60 s
ohne Lebenszeichen ab; Show-Dateien Version 1 werden beim Öffnen
umgewandelt (alte Datei bleibt als `.v1.bak`).

Offen für S1 (DECISIONS „Show-Modus …“, PERSONAS Durchlauf 10): Klang-
und Bild-Cues, Zonen, Bühnen (Stufen 2/3), warten auf die Entscheidungen
S-E.3 (Klang, #39) und S-E.13 (Bühnen, #60); Vorschlag beide B, als ein
strom-Block „Show-Wiedergabe“. Ton lässt sich ohne GPU bauen, Bild mit dem
CPU-Mischer vorbereiten; GPU-Pfade auf echter Hardware.

## Branches und PRs (Gitea), Stand 2026-10-09

| PR | Branch | Basis | Inhalt | Stand |
|---|---|---|---|---|
| #1–#3 | `lastenheft`, `m2b`, `m3` | `main` | Lastenheft bis M3 | gemergt |
| #4 | `claude-todo` (mit `console-qt`) | `main` | Restart-502, Selbsttest, Qt-Console statt egui | gemergt |
| #5 | `m4` | `main` | M4: Overlays (4.1), T-Bar (4.3), Kanalzug und Aux-Busse (4.7) | gemergt |
| #6 | `m4-stinger` | `main` | M4: Stinger (4.2) | offen |

strom-Fork auf Gitea (`shrippen/strom`), Ziel `dev`, seit 17.10. gestapelt:
#15 `lmix-still-release` (Leck pausierter Clips) → #16 `lmix-nine-dsk`
(9 DSK-Ebenen) → #17 `lmix-player-loop` (Schleife live) → #18
`lmix-rtmp-output` (RTMP und 4:2:0 aus upstream) → #19
`lmix-upstream-fixes` (Stopp begrenzt, Bus-Watches). Das Submodul von
lmix #74 zeigt auf #17, #85 auf #18, #86 auf #19; nach dem Merge auf `dev` neu pinnen.

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

## Bekannte Befunde

- GL-Mischer: SRT-Ausgang ohne Video (DECISIONS, „Hardware-Läufe“).
- Qt-Console: Ziel Linux allgemein; Ubuntu 24.04 über gepinntes KDE neon (BUILD.md), ungeeignet Debian 12.

- SRT-Input-Block in strom decodiert in der VM nicht (DECISIONS,
  „M3 Schritt 4“).
- Halten bei Quellverlust: in der VM teils ein schwarzer Frame (H.21).
- MP4-Aufnahme reserviert ~93 MB moov; MKV ist Standard.
