# Persona-Durchläufe

Testmethode zu `LASTENHEFT.md` §9. Jede Persona ist ein typischer Nutzer
mit Ziel, Technik und Erfolgskriterium. Ein Durchlauf spielt den Einsatz
Schritt für Schritt gegen das Lastenheft (Schreibtischtest) und ab M3 gegen
das laufende Programm (Abnahmetest).

Bewertung je Schritt: ✅ abgedeckt · ⚠ abgedeckt, aber Lücke oder Hürde ·
✗ fehlt · ⏸ bewusst später (Priorität L).

Erster Durchlauf: 2026-10-08, gegen das Lastenheft. Befunde → §8.

---

## P1 Ehrenamtliche Technik in einer Kirchengemeinde

**Steckbrief**: Miriam, 52, macht seit zwei Jahren sonntags den Stream.
Keine Ausbildung, liest Anleitungen, hat Angst, live etwas kaputt zu machen.
Technik: ein Mini-PC mit Intel-Grafik, 3 PTZ-Kameras über NDI, USB-Mischpult,
Beamer im Saal für Liedtexte. Ziel: Gottesdienst auf YouTube, Liedtexte im
Saal und im Stream. **Erfolg**: Start bis Sendung in unter 10 Minuten, keine
Panne, die das Publikum sieht.

| # | Schritt | Anforderung | Bewertung |
|---|---|---|---|
| 1 | Programm starten, letzte Show öffnet sich | UI.12, SHW.5 | ✅ |
| 2 | Kameras sind da, PTZ-Positionen "Kanzel", "Altar", "Band" als eigene Inputs | SRC.10, PER.3 | ✅ |
| 3 | Liedtexte als Folien aus PDF oder Textdatei laden, mit Pfeiltaste weiter | SRC.28 (K), GFX.8 | ⚠ Präsentation nur Kann; Textfolien aus Liste fehlen als Vorlage |
| 4 | Beamer zeigt nur Liedtext, Stream zeigt Kamera + Liedtext unten | OUT.2, OUT.3, SWI.10 | ✅ (Ausgang 2 = Mix-Input mit Liedtext) |
| 5 | Ton vom Mischpult, Audio folgt Video aus | SRC.9, AUD.5 | ✅ |
| 6 | Stream zu YouTube starten | STR.1–3 | ✅ |
| 7 | Bedienung ohne Überforderung: nur die nötigen Knöpfe | UI.7 (K) | ⚠ Einfacher Modus nur Kann |
| 8 | Intel-Grafik reicht für 3 Kameras + Stream | NFA.1–2 | ⚠ Budget nur für NVIDIA gemessen (M5b) |
| 9 | Versehentliches Stoppen verhindern | AUT.11, UI.10 | ✅ |
| 10 | Nächste Woche: Show-Vorlage "Gottesdienst" statt alles neu | SHW.3 | ⚠ keine Vorlagen/Assistent |

**Befunde**: B1 Einfacher Modus, B2 Textfolien/Präsentation, B3 Show-Vorlagen
und Einrichtungsassistent.

---

## P2 Freiberuflicher Konferenztechniker

**Steckbrief**: Deniz, 38, Veranstaltungstechniker, betreut Hybrid-Konferenzen.
Technik: Workstation mit NVIDIA-Karte, 4 Kameras über SDI (DeckLink), 2 Beamer,
Notebooks der Vortragenden, Stream + Aufnahme je Vortrag. Ziel: zwei Tage,
30 Vorträge, ohne Ausfall. **Erfolg**: jeder Vortrag als eigene Datei, Stream
ohne Unterbrechung, Vortragende sehen ihre Restzeit.

| # | Schritt | Anforderung | Bewertung |
|---|---|---|---|
| 1 | SDI-Kameras einbinden | SRC.1 | ✅ |
| 2 | Folien vom Notebook der Vortragenden ohne Software-Installation | SRC.18 (K), CAL ⏸ | ⚠/⏸ Bildschirm entfernt nur Kann; Browser-Freigabe hängt an Gästen (später) |
| 3 | Bauchbinden aus CSV mit Name/Titel je Vortrag | DAT.1, DAT.4, GFX.3 | ✅ |
| 4 | Countdown "Pause endet in" | GFX.9 | ✅ |
| 5 | Bühnenmonitor für Vortragende: Restzeit, Uhr, Nachricht von der Regie | — | ✗ fehlt |
| 6 | Beamer 1 Folien, Beamer 2 Kamera + Folien, Stream mit Bauchbinde | OUT.2–3, SWI.10, OVL.5 | ✅ |
| 7 | Aufnahme je Vortrag: neue Datei per Taste, Name aus Datenquelle | REC.1, REC.5 | ⚠ Dateiname aus Datenquelle fehlt |
| 8 | Einzelaufnahme aller Kameras für den Schnitt | MCR.1 | ✅ |
| 9 | Zwei Tage Betrieb ohne Neustart | NFA.7 | ✅ (8 h Soak; 2 Tage nicht gefordert) |
| 10 | Tonregie sitzt an eigenem Rechner | — | ✗ nur eine GUI vorgesehen |

**Befunde**: B4 Bühnenmonitor, B5 Bildschirm vom Notebook per Browser,
B6 Aufnahme-Dateiname aus Daten, B7 mehrere Bedienplätze, B8 Dauerbetrieb
über mehrere Tage.

---

## P3 Streaming einer Amateur-Fußballliga

**Steckbrief**: Jonas, 24, Vereinsmitglied, technikaffin, kleines Budget.
Technik: Gaming-Notebook, 2 Kameras über SRT von Handys/Encodern, Stream Deck,
mobiles Internet. Ziel: Spiele live mit Spielstand und Uhr, Tore wiederholen.
**Erfolg**: Spielstand per Tastendruck, Tor-Wiederholung in unter 10 s.

| # | Schritt | Anforderung | Bewertung |
|---|---|---|---|
| 1 | Handy-Kameras über SRT, wackliges Netz | SRC.12, NFA.5 | ✅ |
| 2 | Handy direkt im Browser als Kamera (WHIP) | SRC.14 | ✅ |
| 3 | Spielstand-Grafik mit Teamnamen, Tore per Stream-Deck-Taste | GFX.12, API.9, AUT.1 | ✅ |
| 4 | Spieluhr läuft, Halbzeit stoppt | GFX.9 | ✅ |
| 5 | Tor: letzte 8 s in Zeitlupe zeigen | RPL ⏸, SRC.19 ⏸ | ⏸ Wiederholung später; keine einfache Alternative |
| 6 | Stinger zwischen Wiederholung und Live | SWI.7 | ✅ |
| 7 | Mobilfunk schwankt: Stream passt sich an oder puffert | STR.3, STR.5 | ⚠ keine automatische Bitratenanpassung |
| 8 | Notebook-Akku, Hitze: Warnung vor Überlast | PER.7 | ✅ |

**Befunde**: B9 einfache Wiederholung über Video-Verzögerung (SRC.19) vorziehen,
B10 adaptive Bitrate bei instabiler Leitung.

---

## P4 Streamerin auf Twitch

**Steckbrief**: Lea, 29, Vollzeit-Creatorin, kennt OBS gut. Technik: Desktop
mit AMD-Grafik, Kamera, Greenscreen, Spiel auf demselben Rechner, Alerts und
Chat als Webseiten. Ziel: horizontaler Stream auf Twitch und gleichzeitig
hochkant auf TikTok. **Erfolg**: Umstieg von OBS in einem Abend, keine
Ruckler im Spiel.

| # | Schritt | Anforderung | Bewertung |
|---|---|---|---|
| 1 | Spiel als Fenster aufnehmen, mit Spielton | SRC.17, SRC.9 | ✅ |
| 2 | Kamera mit Greenscreen keyen | PRC.6 | ✅ |
| 3 | Alerts und Chat als Webseiten mit Transparenz, Videos mit H.264 | GFX.6, GFX.20 | ✅ |
| 4 | Zwei Streams: 16:9 auf Twitch, 9:16 mit anderem Layout auf TikTok | STR.1, STR.6, SWI.10, OUT.2 | ✅ (Mix-Input als eigenes Layout) |
| 5 | Mikrofon: Rauschunterdrückung | AUD.10, AUD.11 | ✅ (Plugins) |
| 6 | AMD-Grafik: Encode und Spiel parallel | NFA.1 | ⚠ AMD erst in M5b geprüft |
| 7 | Szenen aus OBS übernehmen | — | ✗ kein Import; Umstieg kostet Zeit |
| 8 | Hotkeys global, auch wenn das Spiel im Vordergrund ist | AUT.1 | ⚠ globale Tasten außerhalb des Fensters nicht gefordert |

**Befunde**: B11 globale Tastenkürzel, B12 Import von OBS-Szenen (Kann),
B13 AMD-Abnahme früher.

---

## P5 Broadcast-Ingenieur im Regionalsender

**Steckbrief**: Thomas, 47, Systemintegration, denkt in Signalwegen und
Redundanz. Technik: Server im Rack ohne Monitor, SDI über DeckLink, Tally
über TSL, Steuerung durch eine Automation, Überwachung im Leitstand.
Ziel: lmix als Grafik- und Ersatzmischer in eine bestehende Anlage.
**Erfolg**: läuft headless, meldet seinen Zustand, übersteht Fehler.

| # | Schritt | Anforderung | Bewertung |
|---|---|---|---|
| 1 | Betrieb ohne Monitor und ohne GUI, Show beim Start laden | — | ✗ headless nicht gefordert |
| 2 | GUI von einem anderen Rechner verbinden | — | ✗ (siehe B7) |
| 3 | SDI ein und aus, Key+Fill | SRC.1, OUT.4, OUT.5 | ✅ |
| 4 | Tally per TSL UMD an den Mischer | PER.1 | ⚠ nur "generisch", TSL nicht genannt |
| 5 | Steuerung durch Automation | API.1–10 | ✅ |
| 6 | Zustand an Monitoring (Metriken, Alarme) | — | ✗ fehlt |
| 7 | Timecode in Aufnahmen, Takt von außen | — | ✗ fehlt |
| 8 | GUI-Absturz ändert nichts an Program | NFA.6 | ✅ |
| 9 | Steuerung über OSC aus der Lichtregie | — | ✗ OSC nicht gefordert |

**Befunde**: B14 Headless-Betrieb, B15 Monitoring-Schnittstelle, B16 Timecode,
B17 TSL UMD ausdrücklich, B18 OSC als Steuerweg.

---

## P6 Medienzentrum einer Hochschule

**Steckbrief**: Frau Dr. Petersen, 61, Dozentin; der Raum wird vom
Medienzentrum vorbereitet. Technik: festes Pult im Hörsaal, eine Kamera,
Pult-PC mit Folien, Funkmikrofon. Ziel: Vorlesung aufzeichnen und live ins
Lernportal. **Erfolg**: ein Knopf, nichts falsch machen können.

| # | Schritt | Anforderung | Bewertung |
|---|---|---|---|
| 1 | Medienzentrum legt Show-Vorlage "Hörsaal 3" an | SHW.1 | ⚠ keine Vorlagen (B3) |
| 2 | Aufnahme und Stream starten nach Stundenplan automatisch | — | ✗ kein Zeitplan |
| 3 | Dozentin sieht nur "Start", "Pause", "Ende" | UI.7 (K), WEB.1 | ⚠ Web-Tastenseite geht, einfacher Modus nur Kann |
| 4 | Folien vom Pult-PC als Bildschirmaufnahme | SRC.17 | ✅ |
| 5 | Kamera + Folien als Bild-in-Bild | LAY.1 | ✅ |
| 6 | Aufnahme landet mit Datum und Kursname im Ordner | REC.5 | ⚠ Kursname aus Zeitplan fehlt (B6) |

**Befunde**: B19 Zeitplan (Show laden, Aufnahme/Stream starten und stoppen),
dazu B1, B3, B6.

---

## P7 Video-Operator im Theater

**Steckbrief**: Kim, 33, Bühnenvideo, arbeitet nach Cues aus dem Inspizienten-
Pult. Technik: 2 Projektoren, Zuspieler-Videos, Live-Kamera auf der Bühne,
Lichtpult sendet OSC/MIDI. Ziel: Szenenwechsel exakt auf Zuruf.
**Erfolg**: jeder Cue mit einer Taste, framegenau, auch nach Probenänderung.

| # | Schritt | Anforderung | Bewertung |
|---|---|---|---|
| 1 | Cue-Liste: jede Zeile setzt Inputs, Ebenen, Überblendung, Ton | AUT.8, AUT.7 | ⚠ Ablaufliste kennt Inputs, keine Mehrfach-Aktionen je Cue |
| 2 | "GO" springt zur nächsten Zeile, auch rückwärts | AUT.8 | ✅ (manueller Modus) |
| 3 | Lichtpult löst Cues per OSC oder MIDI aus | AUT.1, — | ⚠ MIDI ja, OSC fehlt (B18) |
| 4 | Videos mit In/Out-Punkt und Schleife | INP.10 | ✅ |
| 5 | Projektor 2 zeigt Kamera, Projektor 1 Zuspieler | OUT.3, SWI.10 | ✅ |
| 6 | Bild auf schräge Leinwand anpassen (Eckpunkte, Kantenblende) | — | ✗ kein Warping (vMix kann das auch nicht) |
| 7 | Ton der Zuspieler langsam ausblenden | AUD.14 | ✅ |

**Befunde**: B20 Cue-Liste mit Aktionen je Zeile, B21 Ausgangs-Warping (Kann),
B18 OSC.

---

## Befunde (Zusammenfassung)

| # | Befund | Personas | Umsetzung im Lastenheft |
|---|---|---|---|
| B1 | Einfacher Modus für Ungeübte | P1, P6 | UI.7 auf S (entschieden) |
| B2 | Textfolien/Präsentation (PDF, Liedtexte) | P1 | SRC.28 auf S (entschieden); neu GFX.21 Textfolien |
| B3 | Show-Vorlagen und Einrichtungsassistent | P1, P6 | neu UI.13 |
| B4 | Bühnenmonitor für Sprechende | P2 | neu OUT.17 |
| B5 | Bildschirm vom Notebook per Browser teilen | P2 | neu SRC.31; SRC.18 bleibt K (entschieden) |
| B6 | Dateinamen aus Daten/Zeitplan | P2, P6 | neu REC.9 |
| B7 | Mehrere Bedienplätze gleichzeitig | P2, P5 | neu OPS.2 |
| B8 | Dauerbetrieb mehrere Tage | P2 | NFA.7 erweitert |
| B9 | Einfache Wiederholung über Video-Verzögerung | P3 | SRC.19 bleibt L (entschieden) |
| B10 | Adaptive Bitrate | P3 | neu STR.10 |
| B11 | Globale Tastenkürzel | P4 | neu AUT.12 |
| B12 | Import von OBS-Szenen | P4 | neu SHW.10 (K) |
| B13 | AMD früher abnehmen | P4 | M5b wie geplant (entschieden) |
| B14 | Headless-Betrieb | P5 | neu OPS.1 |
| B15 | Monitoring-Schnittstelle | P5 | neu OPS.3 |
| B16 | Timecode | P5 | neu PER.9 |
| B17 | TSL UMD ausdrücklich | P5 | PER.1 präzisiert |
| B18 | OSC als Steuerweg | P5, P7 | neu AUT.13 |
| B19 | Zeitplan | P6 | neu OPS.4 |
| B20 | Cue-Liste mit Aktionen je Zeile | P7 | neu AUT.14 |
| B21 | Ausgangs-Warping, Kantenblende | P7 | neu OUT.18 (K) |

---

## Durchlauf 2: M2b Schritte 1–3 (2026-10-08, VM-Test + Schreibtisch)

Neu: `lmix-core` (headless Steuerprozess, API, Slot-Mischer über strom,
Show-Datei, Autosave). Geprüft mit `scripts/test-m2b-vm.sh` (CPU-Mischer,
Testbilder) und gegen die Szenarien.

| Persona | Schritt | Ergebnis |
|---|---|---|
| P1 Ehrenamt | Absturz während des Gottesdienstes, Neustart | ✅ letzte Show kommt aus der Autosave zurück, Mischer lief weiter (SHW.5, NFA.6) |
| P1 Ehrenamt | Show "Gottesdienst" öffnen | ✅ Show-Datei; ⚠ noch keine GUI dafür (Console spricht strom direkt) |
| P2 Konferenz | Zweiter Bedienplatz | ✅ Grundlage: API und WebSocket für beliebig viele Clients; ⚠ Rechte je Platz fehlen (OPS.2) |
| P3 Sport | Spielstand per Stream Deck | ⚠ Aktionen über API da; Titel fehlen (M7) |
| P5 Broadcast | Headless-Betrieb | ✅ `lmix-core` ohne GUI; ⚠ kein systemd-Dienst, keine Metriken (OPS.1, OPS.3) |
| P5 Broadcast | Steuerung durch Automation | ✅ Aktionen per HTTP, Katalog als JSON-Schema; ⚠ kein schlanker TCP-Ereigniskanal (API.5) |
| P7 Theater | Quelle tauschen während Vorstellung | ✅ Slot-Flow wird ersetzt, Mischer läuft weiter; ⚠ Bild ohne Aussetzer nur auf GPU prüfbar (H.3) |

**Befunde**: B22 Console muss Client von `lmix-core` werden (2b.2), bevor
Bediener davon etwas haben; B23 Ton fehlt in den Slots (nur Video gekoppelt).

---

## Durchlauf 3: M2b Schritt 4 (2026-10-08, VM-Test + Console auf Xvfb)

Neu: Console als Client von `lmix-core` (`--core`): Input-Leiste mit
Bildern und Tally, Klick = PVW, Doppelklick = Take, Tasten 1–9, vier
Übergangs-Buttons, FTB, Input hinzufügen, umbenennen, entfernen,
Show-Datei. Geprüft mit `CONSOLE_SHOT=… scripts/test-m2b-vm.sh`.

| Persona | Schritt | Ergebnis |
|---|---|---|
| P1 Ehrenamt | Kamera in Vorschau, dann auf Sendung | ✅ Klick, Doppelklick; Rahmen grün/rot wie gewohnt |
| P1 Ehrenamt | Show "Gottesdienst" öffnen | ⚠ Pfad eintippen, kein Dateidialog, keine Liste letzter Shows (B24) |
| P2 Konferenz | Zweiter Bedienplatz mit eigener Console | ✅ beide sehen denselben Zustand (WebSocket); ⚠ Rechte je Platz (OPS.2) |
| P3 Sport | Input "Kamera 2" in "Torkamera" umbenennen | ✅ Kachel-Menü |
| P4 Streamer | Webcam hinzufügen | ⚠ Geräte-ID eintippen, keine Geräteliste über `lmix-core` (B25) |
| P6 Hörsaal | Übergang von Fade auf Cut umstellen | ✅ nach Fix: Hinweis am Button, Rechtsklick (B26 behoben) |
| P7 Theater | Zuspieler während der Vorstellung austauschen | ✅ nach Fix: Kachel-Menü "Quelle ersetzen" (B27 behoben); Bild auf PGM läuft weiter (VM, CPU) |
| P5 Broadcast | Bild eines Inputs in eigenes Tool holen | ✅ `GET /api/pictures/inputs/{n}` |

**Befunde**: B24 Dateidialog und letzte Shows; B25 Geräteliste über
`lmix-core` (SRC.1); B26 Übergangs-Einstellung nicht auffindbar (behoben:
Hinweis); B27 Quelle ersetzen fehlte in der GUI (behoben). Nebenbei: der
Test fand einen echten Fehler (PGM schwarz bei Kopplung), siehe
`DECISIONS.md`.

**Nachtrag 2026-10-08**: B24 behoben (Dateidialog über xdg-portal, Liste der
letzten Shows aus `lmix-core`, für alle Bedienplätze gleich), B25 behoben
(Geräteliste `GET /api/devices`, Auswahl in der Console). Dazu Farbmarke und
Typ-Kennung je Kachel (`set_input_color`). P2 (zweiter Platz auf anderem
Rechner): Dialog zeigt die lokalen Dateien, `lmix-core` öffnet den Pfad auf
seinem Rechner; Hinweis am Button, echte Lösung mit Datei-Upload später.

---

## Durchlauf 4: M2b Schritt 5, Ton (2026-10-08, VM-Test + Console auf Xvfb)

Neu: Ton je Slot in einen Audio-Mischer; je Input an/aus ("M"), AFV,
Lautstärke; Pegel auf Kacheln und unter PROGRAM.

| Persona | Schritt | Ergebnis |
|---|---|---|
| P1 Ehrenamt | Kameraton nur hören, wenn die Kamera auf Sendung ist | ✅ AFV an der Kachel, blendet mit dem Übergang |
| P1 Ehrenamt | Zuspieler leiser machen | ⚠ Regler nur im Kachel-Menü, schwer zu finden (B28) |
| P6 Hörsaal | Mikrofon der Lehrkraft immer an | ⚠ geht nur, wenn das Mikrofon Ton eines Inputs ist; reine Audio-Inputs fehlen (B29) |
| P3 Sport | Kommentar-Mikrofon dauerhaft, Stadionton per AFV | ⚠ wie B29; AFV für Stadionkameras ✅ |
| P4 Streamer | Sehen, ob Ton ankommt | ✅ Pegel je Kachel und Programm |
| P5 Broadcast | Pegel in eigenes Monitoring | ✅ `GET /api/audio/levels`, WS `levels`; ⚠ nur Spitzenpegel, keine Lautheit (LUFS, AUD) |
| P7 Theater | Zuspieler-Ton mit dem Bild einblenden | ✅ AFV + Übergang (800 ms im Test) |

**Befunde**: B28 Lautstärke direkt an der Kachel bzw. Audio-Mischpult-
Ansicht (vMix-Audiomixer, M4); B29 reine Audio-Inputs (Mikrofon,
Line-In) als eigener Quellentyp (2b.3, AUD).

**Nachtrag PFL/AFL (2026-10-08)**: P5 Broadcast und P3 Sport können einen
stummen Kommentar-Input per PFL vorhören, bevor er auf Sendung geht ✅;
P7 Theater prüft per AFL, wie ein Zuspieler im Programm klingt ✅. ⚠ Der
Monitor ist nur auf der Engine-Maschine hörbar; ein Bedienplatz an einem
anderen Rechner hört ihn noch nicht (mit OUT, M3).

---

## Durchlauf 5: M2b Schritt 6 (2026-10-08, VM-Test + Console auf Xvfb)

Neu: Farbe, Bild, Playlist mit Transport, NDI, WHIP, Audio-Gerät;
Signalverlust halten/schwarz; Lautstärke an der Kachel; PFL/AFL.

| Persona | Schritt | Ergebnis |
|---|---|---|
| P1 Ehrenamt | Liedtext-Folie als Bild, Begrüßungsvideo als Playlist | ✅ Bild, Playlist mit Play/Pause an der Kachel |
| P1 Ehrenamt | Zuspieler leiser | ✅ Regler direkt an der Kachel (B28 behoben) |
| P6 Hörsaal | Mikrofon der Lehrkraft dauerhaft | ✅ Audio-Input (Mikrofon), Ton an, kein AFV (B29 behoben); ⚠ Gerät nur auf Hardware prüfbar |
| P4 Streamer | Gast per Browser (WHIP) | ✅ WHIP-Input; ⚠ Link zum Teilen fehlt in der Console (B30) |
| P5 Broadcast | Kamera fällt aus | ✅ je Input "letztes Bild halten" oder "schwarz" |
| P7 Theater | Zuspieler pausiert, Bild soll stehen bleiben | ✅ Standard "halten" |
| P3 Sport | NDI-Kamera eines zweiten Rechners | ✅ NDI-Input und Geräteliste; ⚠ nur mit NDI-SDK (H.14) |

**Befunde**: B30 WHIP: Console soll die Ingest-Adresse anzeigen und
kopieren lassen. Sonst keine neuen Lücken für M2b.
