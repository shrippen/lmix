# strom: was auf `dev` liegt und wie es nach upstream kommt

Regel: `claude.md`, Arbeitsregeln („Jede Änderung an strom geht auch nach upstream“).
Jedes Thema auf `dev` steht hier mit seinem Stand, bis upstream es gemergt hat oder der
User es als lmix-eigen entschieden hat.

Stand 2026-11-01: `dev` (c426a7e) hat 172 Commits ohne Merges, die upstream `main` (4479d8c) nicht
hat (`git log --no-merges 4479d8c..gitea/dev`).

Stand 2026-11-01: bei Eyevinn liegen fünf PRs von uns (#1185–#1189, unten), auf `main` 4479d8c
rebasiert. Issue-Entwürfe liegen bereit und gehen erst nach Freigabe raus; weitere PR-Branches
auf upstream `main` sind vorbereitet und warten auf den Review.

Weg: **PR** = gleich als PR (klein, ein Fehler mit Test); **Issue** = zuerst ein Issue
(Feature, Entscheidung für die Maintainer; upstream zieht das vor). Die Einteilung ist ein
Vorschlag nach den Titeln, vor dem Einreichen am Code prüfen. Stand: offen, sofern nicht
anders vermerkt.

Abgleich mit upstream: `git fetch https://github.com/Eyevinn/strom main` und
`git cherry FETCH_HEAD gitea/dev` (ein `-` heißt: gleicher Patch schon upstream).

## Fehlerbehebungen

| Thema | Commits / Gitea-PR | Weg | Stand, Hinweis |
|---|---|---|---|
| Media Player: Bus-Watch, die beim Stoppen entfernt wird (CRITICAL „has no signal watches“) | 21d8ec5 (kam mit #1) | – | upstream erledigt: upstream zählt die `add_signal_watch`-Aufrufe und entfernt genau so viele |
| Vorschaubild-Abgriff liefert kein Bild älter als `max_age` (3 s, mindestens zwei `update_interval`); eine Quelle, die keine Bilder mehr schickt, sah sonst live aus, solange der Abgriff abgefragt wird (`cache_ttl` war ungenutzt) | 7e40604, 594092c, #68 | PR | offen; auf `dev` (#68 gemergt). Upstream-Branch `thumbnail-max-age-upstream` (a591891) auf `main` 4479d8c: hängt nicht an #1187/#1189 (mit #1189 nur ein Konflikt im Testmodul), darum auf `main`. Upstream hat `cache_ttl` gestrichen (#897): zwei triviale Konflikte (Feld und `Default`) gelöst, Commit-Text angepasst. Text `strom-upstream/texts/thumbnail-max-age.md`, wartet auf Review und Freigabe |
| Vorschaubild: 504 auf DEBUG statt ERROR (eigener `MakeClassifier` im `TraceLayer`; ein abgefragter Abgriff einer toten Quelle schrieb eine ERROR-Zeile je Abfrage) | 1fcd191, #71 | PR | offen; Gitea #71 offen, Submodul auf `lmix-thumb-log` (8dd4a82 = `dev` + Fix). Eigener Upstream-PR, nicht im `max_age`-PR (anderes Thema, `lib.rs`): Branch `thumbnail-504-quiet-upstream` (1dd2b39) auf `main` 4479d8c, Text `strom-upstream/texts/thumbnail-504-quiet.md`, wartet auf Review und Freigabe |
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
| DSK bleibt an, wenn er während des Ausblendens eingeschaltet wird | 04b87d1 | PR | offen; setzt das Ein-/Ausblenden der DSK-Ebenen (d47a50a, unten) voraus, gehört dann in dessen PR |
| Losgelassenes Standbild lässt die Haupt-Pipeline fallen | #15 | PR | offen |
| PiP-Schnitt zeigt das neue Bild vor dem Ausblenden des alten | #35 | PR | offen |
| CUDA-Thumbnails ohne cudaconvertscale | #37 | PR | offen; hängt am CUDA-Mischer |
| AAC-Encoder-Delay aus der Zeitachse | #38 | PR | offen |
| Start, Stopp, Löschen und Update eines Flows unter einer Sperre je Flow | #45, #47 | PR | offen; Race, Tests vorhanden |
| Event-Filter-Test ohne Log-Zähler | #44 | PR | offen |
| Thumbnail-Zweig starten, bevor das Tee ihn speist (sonst hält die Quelle an) | #55 | PR | offen; Test vorhanden |
| Media Player hält beim Pausieren das letzte Bild | 7682d41 | PR | offen |
| DeckLink Output mit direktem Media-Timing | #50 | Issue | Abgleich: upstream #1131 (f294622, Video an den Modus anpassen) und #1168 (a340f49, Audio anpassen) sind gemergt und berühren weder den Kartentakt-Seed noch den Vorlauf. Der Seed (eefe858, 56245a9, 031721f) ist auf `dev` durch #53 (decklink2sink) wieder entfernt; übrig bleibt der Vorlauf (`lead_time`, 1e98a41), der im decklink2sink-Issue-Entwurf (#53) mitläuft |
| CUDA-Mischer auf echter Hardware (Alpha nach Fade, Stinger, Thumbnails) | #29 | PR | offen; hängt am CUDA-Mischer |
| Vorschaubild-Abgriff: Abhängen ohne Zustandssperre (Verklemmung mit dem Appsink-Callback) | 5f32f15, #63 | PR | **Eyevinn #1187** (Branch `thumbnail-detach-deadlock`, 523a390) |
| JSON-Speicher: Schreiben unter einer Sperre (gemeinsame Temp-Datei, verlorene Flows) | 8e7cbff, #62 | PR | **Eyevinn #1186** (Branch `json-storage-lock`, d44fd98) |
| Mischer: Multiview-, Vorschau- und Kachel-Queues werfen alte Bilder weg statt Program aufzuhalten | 4ba8ad8 (upstream-fähig), 76513f5 (nur `dev`), #65 | PR | **Eyevinn #1188** (Branch `mixer-monitor-queues`, eed0070, nur 4ba8ad8; Vorschau/Kacheln gibt es upstream nicht) |
| Vorschaubild-Abgriff verwirft sein Bild beim Abhängen (kein altes Bild nach dem Wiederanhängen) | 8c77ec8, #66 | PR | **Eyevinn #1189** (Branch `thumbnail-drop-stale`, e4fac6c); auf #1187 gestapelt, das Löschen nach dem Stoppen wie auf `dev` |
| SRT-Passphrasen, Stream-Keys und Tokens in Logs und Bus-Fehlertexten maskieren (`redact`) | #59 | PR | offen; auf `dev` gemergt (d4a4a0f). Seit 2bf5674 fail-closed: nur schlichte URL-Teile sichtbar, Namen nach Endung; seit e5b75db werden Anführungszeichen vom Textanfang verfolgt, jede Maske in einem offenen läuft bis zu dessen Partner; seit 10893ce läuft `text()` unter `catch_unwind` (ein Panic maskiert alles, statt strom im Bus-Callback abzubrechen), HTTP nur mit Herkunft (`origin`), seit e8907cd (#69, unten) mit Pfadwörtern; seit 0e21812 Nachschlagen per Index (#67, unten); lmix `redact_text` ist eine Portierung mit denselben Test-Eingaben. Branch `redact-secrets-in-logs` auf upstream `main` 4479d8c rebased (b7de5ed, mit #70; `bus.rs` ohne Konflikt, dort `as_deref` für clippy, weil `debug_info` jetzt `String` ist), PR-Text bereit, noch nicht eingereicht |
| Vision Mixer holt Verspätung auf: Probe am Src-Pad jedes Compositors misst den Rückstand gegen die Uhr und schickt dem Aggregator QoS, `videoaggregator` lässt verspätete Bilder aus; das eigene Event geht nicht an die Eingänge (Peer-Probe per Seqnum) | 7a59b78, #73 | Issue | offen; Gitea #73 noch nicht auf `dev`. Branch `mixer-self-qos` von upstream 8fce2ee, lässt sich auf 4479d8c anwenden. Entscheidung für die Maintainer (immer an oder Block-Eigenschaft), darum Issue mit Testfall und Branch als PR-Angebot; Entwurf `~/.cache/claude/strom-upstream/texts/mixer-self-qos.md`. Submodul zeigt bis zum Merge auf `lmix-mixer-self-qos` (d4ed896 = `dev` + Fix) |
| Queue-Warnung: eigene `min-threshold-time` zählt mit, einmal je Überschreitung | #60 | PR | **Eyevinn #1185** (Branch `queue-warn-delay`, f4c2e3e) |
| `redact` in linearer Zeit: Partner, Wortende und `@` aus einem Index statt eines Caches mit einem Eintrag (ein Anführungszeichen über den ganzen Text machte das Maskieren quadratisch, 1 MB 11–124 s, jetzt unter 0,1 s; Ausgabe unverändert) | 677d223, #67 | PR | offen; auf `dev` gemergt (c426a7e). Gehört zum `redact`-PR (#59): auf dessen Branch `redact-secrets-in-logs` als 0e21812 nachgezogen |
| `redact`: Restlücken geschlossen (Shell-`'\''`, einzelnes Anführungszeichen davor, roher Leerraum im Passwort, `&` vor unbekanntem Namen, Token im RTSP-/SRT-Pfad, `name: wert` bis Zeilenende, `@` im Pfad) und Lesbarkeit zurück (Partner und Rest hinter einer Maske, begrenzte URL-Masken, bekannte Paare hinter Leerraum, HTTP-Pfadwörter wie `whip`, `index.m3u8`) | 5923a21, #69 | PR | offen; auf `dev` gemergt (4377d67). Gehört zum `redact`-PR (#59): auf dessen Branch `redact-secrets-in-logs` als e8907cd nachgezogen; lmix `redact_text` portiert, gleiche Test-Eingaben |
| `redact`: HTTP-Pfadsegment höchstens 10 Ziffern (`live-1234567890-9876543210-…` gilt nicht mehr als Wort), Shell-`'\''` in einem Wert, der selbst ein Anführungszeichen öffnet (`'passphrase="my"'\''s secret'` zeigte `secret`), Doku: `name:`-Wert endet an Leerraum vor bekanntem Paar oder `!` (Review #69) | 97cb2c5, #70 | PR | offen. Gehört zum `redact`-PR (#59), dort im rebasten Branch enthalten; lmix `redact_text` portiert, gleiche Test-Eingaben |

## Features

| Thema | Commits / Gitea-PR | Weg | Stand, Hinweis |
|---|---|---|---|
| `--host`/`STROM_HOST` | 4bce876 | PR | offen; klein |
| `eos_on_stop`: Stopp schickt EOS und wartet begrenzt (5 s), damit Muxer abschließen und RTMP sich abmeldet | 596c10d | Issue | Issue-Entwurf in Arbeit (neue Flow-Eigenschaft, anderes Stoppverhalten: Entscheidung der Maintainer); lmix nutzt es für alle Ausgänge |
| `STROM_FORCE_SOFTWARE_CONVERT` | bd86930 | – | upstream erledigt: 1e9b9c0 (#1142) hat die globale Konvertierungswahl samt CUDA-GL-Probe entfernt, nichts einzureichen |
| CUDA-Compositor-Backend des Vision Mixers, mit CUDA-Kontext, CUDA/VA-Brücken, GPU-Skalierung und JPEG in Thumbnails, Alpha-Konvertierung, NVDEC-Deinterlace | 63cf137, 8725d4c, 7f367ec, 1079575, 4beaf4d, 77e8f4b, 176a1f2, 7615d6f | Issue | Issue-Entwurf fertig, wartet auf Freigabe; danach in mehrere PRs teilen |
| CPU-Mischer: videoconvert und Speicherbrücken | b38ace4 | Issue | teilweise upstream gelöst: videoconvert und Download von CUDA/GL (1e9b9c0, 624082c #1121, 4e0917e #1138); VA-Download im CPU-Mischer (`vapostproc`) fehlt upstream (`download_front` lehnt VAMemory ab), steht im VA-Issue-Entwurf (#21) |
| VA-Surfaces in den GL-Mischer | #21 | Issue | jetzt Issue: upstream lehnt VA-Speicher mit klarer Meldung ab, unser Weg ist ohne VA-Hardware ungeprüft; Issue-Entwurf fertig, wartet auf Freigabe |
| Auslastung von Hardware-Decoder/-Encoder, eigener Speicher in den System-Stats | dd6293c, 34302d6 | Issue | Issue-Entwurf in Arbeit (neue API-Felder oder Endpunkte: Entscheidung der Maintainer, wie bei der Block-Statistik #61/#64) |
| Bis 32 Mischer-Eingänge, neun DSK-Ebenen | b778653, #6, #16 | Issue | Issue-Entwurf fertig, wartet auf Freigabe |
| Mischer starten beim ersten Puffer | #1 | Issue | Issue-Entwurf fertig, wartet auf Freigabe; mit `sample_rate` des Audiomischers (ed647f2); `eos_on_stop` eigene Zeile |
| DSK-Ebenen ein- und ausblenden (`duration_ms` in `DskToggleRequest`) | d47a50a (kam mit #5) | Issue | Issue-Entwurf fertig, wartet auf Freigabe (neues API-Feld); 04b87d1 gehört dazu |
| identity ts-offset im Lauf änderbar | #5 (401053c) | PR | offen |
| Media Player gibt Video im Systemspeicher aus | #3 | PR | offen |
| Playlist-Loop im Lauf umschalten | #17 | Issue | Issue-Entwurf in Arbeit (neue API-Felder oder Endpunkte: Entscheidung der Maintainer, wie bei der Block-Statistik #61/#64) |
| Rand-Underlays live, Startzeit zuerst | 9274bbd | PR | offen |
| HTML-Input: Seite per API steuern | #22 | Issue | Issue-Entwurf fertig, wartet auf Freigabe |
| Show Sound / Show Playback (Klang- und Bild-Cues in Zonen, Wiederholen, Pause) | #23–#28, #30, #33, #51 | Issue | Issue-Entwurf fertig, wartet auf Freigabe (nur Frage, ob upstream es will) |
| Audioausgänge mit Kanalpositionen | #31 | Issue | Issue-Entwurf in Arbeit (neue API-Felder oder Endpunkte: Entscheidung der Maintainer, wie bei der Block-Statistik #61/#64) |
| Vorschau-Ausgang des Mischers (`pvw_out`) | #32 | Issue | Issue-Entwurf fertig, wartet auf Freigabe, zusammen mit dem Kachel-Ausgang #52 |
| Take an einer T-Bar-Position halten | #34 | Issue | Issue-Entwurf fertig, wartet auf Freigabe |
| PadMap; Mischer-Eingänge im Lauf hinzufügen und entfernen; Benchmark | #39, #41, #40 | Issue | Issue-Entwurf fertig, wartet auf Freigabe, ein Issue mit #42, #43, #56, #57 |
| Monitor folgt einem Aux-Bus; Aux-Busse im Lauf | #42, #43 | Issue | im Issue-Entwurf zu den Mischer-Eingängen im Lauf |
| Inter Input wechselt den Kanal im Lauf | #46 | Issue | Issue-Entwurf in Arbeit (neue API-Felder oder Endpunkte: Entscheidung der Maintainer, wie bei der Block-Statistik #61/#64) |
| DeckLink-Karten als Videoquellen auflisten | #49 | Issue | Issue-Entwurf in Arbeit (neue API-Felder oder Endpunkte: Entscheidung der Maintainer, wie bei der Block-Statistik #61/#64) |
| Audiomischer: Kanäle im Lauf hinzufügen und entfernen; Bildmischer ab einem Eingang mit freien Plätzen, Kachelbild für einen Teil der Eingänge | #56, #57 | Issue | im Issue-Entwurf zu den Mischer-Eingängen im Lauf |
| Byte-Zähler der Ausgänge (`/flows/{id}/output-bytes`: RTMP, Live Recorder) | #54 | Issue | Issue-Entwurf in Arbeit (neue API-Felder oder Endpunkte: Entscheidung der Maintainer, wie bei der Block-Statistik #61/#64) |
| Block-Statistik: Warteschlange des Inter Input (wartend, verworfen) und Encoder-Zeit des Video Encoder (`/flows/{id}/rtp-stats`) | #61, #64 | Issue | Entwurf fertig, wartet auf Freigabe (zwei Buffer-Probes am Encoder und neue API-Typen sind eine Entscheidung der Maintainer); Branch `lmix-output-load` auf upstream `main` 4479d8c |
| Kachel-Ausgang des Mischers (`tiles_out`) | #52, #57 | Issue | im Issue-Entwurf zum Vorschau-Ausgang |
| DeckLink-Ausgang über `decklink2sink` (spätes Bild beim zweiten Start) | #53 | Issue | Issue-Entwurf fertig, wartet auf Freigabe; braucht GStreamer 1.30, upstream-Ziel ist 1.28 |
| QoS-Events an blockgebauten Sinks verwerfen | #58 | PR | offen; wird geprüft: ob der Fehler auf upstream `main` auftritt (Sinks, die Blöcke selbst bauen) und ob der Branch ohne lmix-Teile auf `main` passt |

## Von upstream übernommen (nichts zu tun)

#18 (RTMP-Ausgang, 4:2:0), #19 (begrenzter Stopp, Bus-Watch), #20 (Abgleich mit 0.7.1).
