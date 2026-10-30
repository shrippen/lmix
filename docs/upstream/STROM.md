# strom: was auf `dev` liegt und wie es nach upstream kommt

Regel: `claude.md`, Arbeitsregeln („Jede Änderung an strom geht auch nach upstream“).
Jedes Thema auf `dev` steht hier mit seinem Stand, bis upstream es gemergt hat oder der
User es als lmix-eigen entschieden hat.

Stand 2026-10-29: `dev` (d4c588f/f5cf6bd) hat 132 Commits, die upstream `main` (7c80005) nicht
hat (`git cherry`: keiner davon upstream), und liegt 83 Commits hinter upstream `main`
(Basis 8fce2ee, 15.10.). Bei Eyevinn liegt noch kein Issue und kein PR von uns.

Weg: **PR** = gleich als PR (klein, ein Fehler mit Test); **Issue** = zuerst ein Issue
(Feature, Entscheidung für die Maintainer; upstream zieht das vor). Die Einteilung ist ein
Vorschlag nach den Titeln, vor dem Einreichen am Code prüfen. Stand: offen, sofern nicht
anders vermerkt.

Abgleich mit upstream: `git fetch https://github.com/Eyevinn/strom main` und
`git cherry FETCH_HEAD gitea/dev` (ein `-` heißt: gleicher Patch schon upstream).

## Fehlerbehebungen

| Thema | Commits / Gitea-PR | Weg | Stand, Hinweis |
|---|---|---|---|
| Panic im eigenständigen nativen Frontend | 779cc60 | PR | offen |
| GL-Renderer-Probe auf GStreamer 1.28 | a7a2aa9 | PR | offen; prüfen, ob upstream seit #1123 (1.28-Baseline) selbst gelöst |
| NVENC-Rate-Control auf GStreamer 1.28 | dcf2b83 | PR | offen; wie oben |
| build.rs beobachtet das echte Git-Verzeichnis | 01bdcc0 | PR | offen |
| Pipeline bleibt PLAYING, wenn ein Thumbnail-Abgriff andockt | da76592 | PR | offen |
| Deinterlace nur bei interlaced | e23f0b5 | PR | offen |
| Buffer-Alter auch für Puffer vor der Uhr zählen | fb62d2b | PR | offen |
| EFP-SRT-H.264-Decoder nach Rang statt Hersteller | ecd2ed6 | PR | offen |
| GL-Eingänge und Underlays auf NVIDIA ohne udmabuf | 887248d, 2e4a749, #36 | PR | offen; als ein PR |
| Inaktive Pads in den Compositoren ignorieren | 2ccb62b, bb4fcd5 | PR | offen |
| Encoder: GPU-Speicher vor Software-Konvertierung herunterladen, vor NVENC auf der GPU konvertieren | 79b3b31, 3e8301e | PR | offen |
| DSK bleibt an, wenn er während des Ausblendens eingeschaltet wird | 04b87d1 | PR | offen |
| Losgelassenes Standbild lässt die Haupt-Pipeline fallen | #15 | PR | offen |
| PiP-Schnitt zeigt das neue Bild vor dem Ausblenden des alten | #35 | PR | offen |
| CUDA-Thumbnails ohne cudaconvertscale | #37 | PR | offen; hängt am CUDA-Mischer |
| AAC-Encoder-Delay aus der Zeitachse | #38 | PR | offen |
| Start, Stopp, Löschen und Update eines Flows unter einer Sperre je Flow | #45, #47 | PR | offen; Race, Tests vorhanden |
| Event-Filter-Test ohne Log-Zähler | #44 | PR | offen |
| Thumbnail-Zweig starten, bevor das Tee ihn speist (sonst hält die Quelle an) | #55 | PR | offen; Test vorhanden |
| Media Player hält beim Pausieren das letzte Bild | 7682d41 | PR | offen |
| DeckLink Output mit direktem Media-Timing | #50 | PR | offen; überschneidet sich mit upstream #1131, #1168, zuerst abgleichen |
| CUDA-Mischer auf echter Hardware (Alpha nach Fade, Stinger, Thumbnails) | #29 | PR | offen; hängt am CUDA-Mischer |

## Features

| Thema | Commits / Gitea-PR | Weg | Stand, Hinweis |
|---|---|---|---|
| `--host`/`STROM_HOST` | 4bce876 | PR | offen; klein |
| `STROM_FORCE_SOFTWARE_CONVERT` | bd86930 | Issue | offen |
| CUDA-Compositor-Backend des Vision Mixers, mit CUDA-Kontext, CUDA/VA-Brücken, GPU-Skalierung und JPEG in Thumbnails, Alpha-Konvertierung, NVDEC-Deinterlace | 63cf137, 8725d4c, 7f367ec, 1079575, 4beaf4d, 77e8f4b, 176a1f2, 7615d6f | Issue | offen; groß, in mehrere PRs teilen |
| CPU-Mischer: videoconvert und Speicherbrücken | b38ace4 | Issue | offen |
| VA-Surfaces in den GL-Mischer | #21 | PR | offen; nach dem Brücken-Issue |
| Auslastung von Hardware-Decoder/-Encoder, eigener Speicher in den System-Stats | dd6293c, 34302d6 | PR | offen |
| Bis 32 Mischer-Eingänge, neun DSK-Ebenen | b778653, #6, #16 | Issue | offen |
| Mischer starten beim ersten Puffer | #1 | Issue | offen |
| identity ts-offset im Lauf änderbar | #5 | PR | offen |
| Media Player gibt Video im Systemspeicher aus | #3 | PR | offen |
| Playlist-Loop im Lauf umschalten | #17 | PR | offen |
| Rand-Underlays live, Startzeit zuerst | 9274bbd | PR | offen |
| HTML-Input: Seite per API steuern | #22 | Issue | offen |
| Show Sound / Show Playback (Klang- und Bild-Cues in Zonen, Wiederholen, Pause) | #23–#28, #30, #33, #51 | Issue | offen; groß und nah an lmix: erst fragen, ob upstream es will, sonst lmix-eigen |
| Audioausgänge mit Kanalpositionen | #31 | PR | offen |
| Vorschau-Ausgang des Mischers (`pvw_out`) | #32 | Issue | offen |
| Take an einer T-Bar-Position halten | #34 | Issue | offen |
| PadMap; Mischer-Eingänge im Lauf hinzufügen und entfernen; Benchmark | #39, #41, #40 | Issue | offen |
| Monitor folgt einem Aux-Bus; Aux-Busse im Lauf | #42, #43 | Issue | offen |
| Inter Input wechselt den Kanal im Lauf | #46 | PR | offen |
| DeckLink-Karten als Videoquellen auflisten | #49 | PR | offen |

## Von upstream übernommen (nichts zu tun)

#18 (RTMP-Ausgang, 4:2:0), #19 (begrenzter Stopp, Bus-Watch), #20 (Abgleich mit 0.7.1).
