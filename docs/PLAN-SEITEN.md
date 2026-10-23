# Plan: eine App mit Seiten (Konzept A)

Stand 2026-10-23. **Entschieden (User 23.10.)**: Konzept A der Entwürfe
„lmix Arbeitsbereiche“ (https://claude.ai/artifact/BFRjbxndxLaEMsYaT72ZVf):
eine Oberfläche mit einer Seitenleiste am Fuß, wie bei Resolve, statt
zweier Programme `lmix-mixer` und `lmix-show`. Weitere Pulte (Ton, Licht)
kommen als Seiten dazu, nicht als eigene Programme. Mehrere Oberflächen
greifen weiter parallel auf `lmix-core` zu, auch übers Netz.

Dieser Plan beschreibt die Reihenfolge. Umgesetzt wird erst, wenn die
Screens mit dem User als Click-Dummy ausgearbeitet sind (Schritt 1).
Verbindlich ist der Text hier („Click-Dummy 2“ und die Entscheidungen);
die Leinwand ist Anschauung und liegt nicht im Repo.

## Grundsätze

- **Eine Shell, eine Codebasis.** Ein Programm `lmix` mit den Seiten Mix,
  Show, Audio, Outputs, Setup (später Light). Seiten bauen auf gemeinsamen
  Bausteinen auf (Monitor, Kanalzug, Cue-Liste, Pad-Raster, Inspector,
  Statusleiste), nicht jede Seite eigene.
- **Seitenwechsel hält nichts an.** Der Ablauf liegt in `lmix-core`
  (unverändert, S-E.1); eine Seite ist nur eine Ansicht. Ein Punkt in der
  Leiste zeigt, wo etwas läuft (Cue auf Standby, Lichtschleife, Aufnahme).
- **Immer erreichbar**: Panik, Statuszeile (Aufnahme, Stream, Last,
  Verbindung, Uhr) und auf Seiten ohne eigenes Bild ein kleines Program-Bild.
- **Prozessmodell bleibt** (claude.md, Entscheidung 4): Shell und Kern sind
  getrennte Prozesse, Grenze ist die lmix-API. Ein Absturz der Shell hält
  keinen Cue an.
- **Arbeitsplätze statt Programme** (Konzept C): dieselbe App verbindet sich
  mit einem Kern, lokal oder im Netz, und zeigt je nach Profil nur
  bestimmte Seiten (Lichtpult: Light + Show; Inspizient: nur GO).
- Vor dem ersten Release: Namen, Einstellungen und Skripte dürfen brechen
  (Regel „Breaking Changes“); lokale Einstellungen werden migriert.

## Was heute wohin wandert

| Heute | Seite |
|---|---|
| `MixerMain.qml`, Monitore, `TransitionPanel`, `InputTile`, `LayoutBar` | Mix |
| `ShowMain.qml`, `ShowView`, `CueListsDialog`, `TransportBar` | Show |
| `ChannelStripPanel`, `ChannelStripDialog`, `AuxBusDialog`, `ZonesDialog` | Audio |
| `OutputBar`, `AddOutputDialog`, `TargetsDialog`, `FormatDialog` | Outputs |
| `DataSourcesDialog`, `TriggersDialog`, `ShortcutsDialog`, Rechner-Einstellungen, Stil | Setup |
| `AppShell.qml` (Datei-Menü, Stil, Hinweise) | Shell: Kopfzeile und Seitenleiste |

Dialoge, die heute nur aus Platzmangel Dialoge sind, werden Teile einer
Seite; kurze Abfragen (Input hinzufügen, Bestätigungen) bleiben Dialoge.
Welche, steht unter „Click-Dummy 2“; was davon schon Seite ist, unter „Stand“.

## Bild im Netz

Lokal bleibt es beim rohen Bild über den Unix-Socket (`unixfdsink`,
GUI-SHELL.md). Für Arbeitsplätze im Netz bringt strom die Wege schon mit;
lmix-core legt je Verbindung einen Ausgang für den Arbeitsplatz an:

| Verbindung | Weg in strom | Erwartung |
|---|---|---|
| lokal | Live-Socket (heute) | volle Rate, kein Kodieren |
| LAN | WHEP-Ausgang (WebRTC, `blocks/builtin/whep`) oder SRT (`mpegtssrt`) | Vorschau kodiert, 25 fps, wenige 100 ms |
| WAN, schwach | JPEG-Vorschaubilder (`thumbnail`) über HTTP | Standbilder 1–2 fps |
| LAN, Studio | NDI-Ausgang | volle Qualität, viel Bandbreite |

Zu messen (Schritt 4): Latenz und Last je Weg, Zahl der gleichzeitigen
Arbeitsplätze, Verhalten bei Verbindungsabbruch. Nichts davon ändert den
Kern-Ablauf; es sind Ausgänge wie andere auch.

## Schritte

0. **Doku** (eigener PR, vor dem Code): claude.md (Kontext, Entscheidung 7:
   „ein Kern, eine Shell mit Seiten“ statt „zwei Oberflächen“), DECISIONS
   (Eintrag 23.10.), Lastenhefte und ROADMAP, wo „Bedienmodus“ oder die
   Programmnamen als Anforderung stehen, todo (Licht). Was beschreibt, welche
   Programme heute laufen (GUI-SHELL, BUILD, HANDOVER, HARDWARE-TODO,
   Skripte), ändert sich mit dem Code in Schritt 2.
1. **Click-Dummy mit dem User**, Screen für Screen, als interaktive
   Artboards auf der Leinwand: Seitenleiste und Kopfzeile, Mix, Show,
   Audio, Outputs, Setup, Verbinden/Profil. Je Screen: was drauf ist, was
   Dialog bleibt, Tastatur, was ein entfernter Arbeitsplatz darf. Die
   Shell-Entscheidungen stehen in DECISIONS (23.10.), der Inhalt je Seite
   hier unter „Click-Dummy 2“; dieser Text ist die Referenz für die
   Umsetzung, die Leinwand Anschauung.
2. **Shell mit Seitenleiste**: ein Binary `lmix` statt `lmix-mixer` und
   `lmix-show`, Seiten Mix und Show aus dem Bestand, Leiste mit Panik und
   Status. Kurzbefehle, Einstellungen und `.desktop`-Dateien migriert;
   `run-local.sh`, `start-*.sh`, `test-console-qt.sh`, `test-m2b-vm.sh`,
   `test-global-keys.sh` angepasst. Bestehende Tests bleiben grün.
3. **Seiten Audio, Outputs, Setup** aus den heutigen Dialogen, nach dem
   Dummy.
4. **Arbeitsplätze**: Verbinden mit einem Kern (Adresse, gefundene Kerne),
   Profile mit sichtbaren Seiten, Bild im Netz nach der Tabelle oben;
   Messungen auf Hardware.
5. **Licht** (später, siehe todo): eigener Lastenheft-Teil, dann Kern, dann
   Seite.

## Entschieden für die Shell (User 23.10., Click-Dummy 1)

- **Panik global und je Seite**: „Panic all“ in der Leiste stoppt die Show
  und blendet Program auf Schwarz; Aufnahmen und Streams laufen weiter
  (Vorschlag aus dem Dummy, siehe „Offen“). Jede Seite hat dazu eine eigene
  nur für ihren Bereich (Mix: Fade to
  black, Show: Stop show, Audio: Audio off, Outputs: Stop outputs).
- **Kurzbefehle global plus aktive Seite**: globale Tasten (Panik, GO, Cut,
  Seiten, Speichern) gelten immer, die Tasten einer Seite nur, solange sie
  sichtbar ist. Eine Übersicht „Keys“ in der Kopfzeile zeigt beide.
- **Seiten lösbar**: eine Seite lässt sich auf einen zweiten Bildschirm
  lösen und zurückholen; die Leiste bleibt im Hauptfenster und zeigt die
  gelöste Seite markiert.
- **Rechte im Netz**: alle Arbeitsplätze dürfen alles; die Leiste zeigt je
  Seite, wer sie zuletzt bedient hat.
- **Name**: Programm `lmix`; Seitennamen englisch (UI zuerst Englisch),
  deutsch über Übersetzung.

## Offen

Schritt 1 ist abgeschlossen (Click-Dummies 1 und 2, unten; vom User
angenommen 23.10.). Offen für die Umsetzung:

- Was „Panic all“ genau umfasst, wenn eine Aufnahme läuft (im Dummy:
  Aufnahmen und Streams laufen weiter).
- Tasten je Seite über die heutigen hinaus (im Dummy vorgeschlagen).

## Click-Dummy 2: Seiten mit Inhalt (23.10.)

Je Seite ein klickbares Artboard (`D-Mix` … `D-Setup` auf der Leinwand),
die Leiste verlinkt sie. Abgeglichen mit den Vorbildern über die
Lastenhefte (Clean Room: nur die dort ausgewerteten Handbücher), danach
ergänzt:

- **Mix**: Kategoriefarbe und Tasten je Kachel (Cut, GO, Overlay 1/2, Ton,
  Einstellungen; UI.2, INP.7), 8 Overlay-Kanäle (OVL.1), T-Bar bedienbar
  (SWI.5), Tonmischer am Rand mit Audio folgt Video (UI.6, AUD.1, AUD.5),
  Senken-Tasten in der Fußleiste (UI.1).
- **Show**: Standby-Notiz groß (SUI.3), Doppel-GO-Sperre sichtbar (RUN.6),
  Standby auf Nummer (RUN.2), Gruppen einklappbar, Ziel-Spalte (SUI.2),
  Farbe je Cue und „gelaufen“ (CUE.1, SUI.12), Stopp/Pause je laufendem Cue
  (SUI.4), Raster „Pads“ (RUN.12), Vorhören (RUN.10), Bühnenvorschau und
  Zonen-Leiste (SUI.7, SUI.8), Warnliste (SUI.5), dunkle Raumansicht (SUI.6).
- **Audio**: dB-Skala, Spitzenwert, Farbbänder (AUD.12), Bus-Tasten je Zug
  (AUD.2), Matrix Inputs × Busse (AUD.13), Abhöre AFL/PFL (AUD.4),
  Audio folgt Video je Zug (AUD.5), Begrenzer je Zone (SND.14).
- **Outputs**: Ton-Bus je Ausgang (REC.1), Clean Feed (OVL.5), Dateiname
  und Teilung (REC.4, REC.5), Marke ins Protokoll (REC.8), Verzögerung,
  Puffer, Encoder-Last (STR.5, STR.7).
- **Setup**: Show-Vorlagen und Einrichtungsassistent (UI.13), Startoptionen
  (UI.12), Sperren je Ein-/Ausgabeklasse (RUN.9).

Bewusst nicht im Dummy: Replay, Gäste-Anrufe, Scripting (nicht im
Lastenheft bzw. später), die Reiter der Input-Einstellungen (UI.8, bleiben
ein Dialog aus der Kachel).

Nach Bildschirmfotos (Ausnahme, DECISIONS 23.10.) zusätzlich:

- **Show** nach Art der Arbeitsfläche eines Cue-Programms: GO mit
  editierbarer Notiz, Leiste zum Anlegen je Cue-Art, Transportgruppe (alles
  stoppen, pausieren, fortsetzen, ausblenden), Seitenleiste mit Reitern
  Aktiv/Listen/Pads (laufende Cues als Fortschrittsfüllung mit negativer
  Restzeit), Inspector unten mit Reitern (Basics, Triggers, Time & loops,
  Levels, Actions), Schalter Edit/Show, Fußzeile mit Zählung und Warnungen,
  Zeitfelder mit Fortschritt in der Liste.
- **Mix**: Titelleiste der Kachel in Tally-Farbe, Overlay-Tasten 1–4, GO,
  Cut, Ton je Kachel; Overlay-Raster 2×4 in der Übergangsspalte; Ansicht
  „Buttons“ mit Program-/Preview-Tastenreihen wie an einem Hardware-Mischer;
  „Pause inputs“, Basic-Modus.
- **Audio**: je Zug Mini-Kurven für EQ und Dynamik, Spitzenwert in dB,
  Pan, Umschalter ON/AFV, Abhör-Taste mit Kopfhörer.

## Stand

- **Schritt 2 erledigt**: ein Programm `lmix` mit Kopfzeile, Seitenleiste,
  „Panic all“, Tasten-Übersicht, Strg+1…n, `--page`; Einstellungen aus
  `mixer.conf` und `show.conf` übernommen (GUI-SHELL.md).
- **Schritt 3 erledigt, erste Fassung**: Audio (Kanalzüge, Kanalzug des
  gewählten Inputs, Busse, Zonen), Outputs (Liste, Start/Stopp mit Rückfrage,
  Einzelheiten, Multiview), Setup (Show, Steuerung, dieser Rechner).

Seit #134 dazu (eigener PR): Mix-Ansicht „Buttons“ (Program-/Preview-
Reihen), Show-Schalter Edit/Show, Audio-Matrix Inputs × Busse (AUD.13) und
Spitzenwert in dB je Zug; ein Klick auf den Program-Input tut nichts, ohne
Meldung (Owner 23.10.).

Aus dem Dummy noch offen, **nur Oberfläche** (der Kern kann es):
- Mix: Titelleiste der Kachel in Tally-Farbe (braucht Textfarben auf Rot
  und Grün in Kante), Overlay-Raster 2×4 und Overlay-Tasten je Kachel,
  schmaler Tonmischer am Rand.
- Show: Inspector unten mit Reitern, Seitenleiste
  Aktiv/Listen, laufende Cues als Fortschrittsfüllung mit negativer
  Restzeit, Leiste zum Anlegen je Cue-Art, Zeitfelder mit Fortschritt.
- Audio: Mini-Kurven EQ und Dynamik je Zug, ON/AFV als Umschalter.
- Seiten lösbar auf einen zweiten Bildschirm (Entscheidung Click-Dummy 1).
- Leiste: wer eine Seite zuletzt bediente (braucht die Kennung je
  Verbindung im Kern, siehe Schritt 4).

**Braucht den Kern** (Lastenheft offen): Pause je Cue und alles (RUN.5),
Vorhören (RUN.10), Pads/Raster (RUN.12), Sperren je Ein-/Ausgabeklasse
(RUN.9), Markierung und „gelaufen“ (SUI.12), Zonen-Leiste mit Pegeln
(SUI.8), Pause inputs (INP.12), Dateiteilung und Namensmuster (REC.4,
REC.5), Marken ins Protokoll (REC.8), Sendeverzögerung (STR.5), Puffer und
Encoder-Last (STR.7), Vorlagen und Assistent (UI.13).

**Vor Schritt 4**: Anmeldung der Arbeitsplätze am Kern (Schlüssel bzw.
Pairing, TLS im WAN); „alle dürfen alles“ gilt nur für angemeldete Plätze
(Review #133).
