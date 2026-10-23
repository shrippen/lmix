# Plan: eine App mit Seiten (Konzept A)

Stand 2026-10-23. **Entschieden (User 23.10.)**: Konzept A der Entwürfe
„lmix Arbeitsbereiche“ (https://claude.ai/artifact/BFRjbxndxLaEMsYaT72ZVf):
eine Oberfläche mit einer Seitenleiste am Fuß, wie bei Resolve, statt
zweier Programme `lmix-mixer` und `lmix-show`. Weitere Pulte (Ton, Licht)
kommen als Seiten dazu, nicht als eigene Programme. Mehrere Oberflächen
greifen weiter parallel auf `lmix-core` zu, auch übers Netz.

Dieser Plan beschreibt die Reihenfolge. Umgesetzt wird erst, wenn die
Screens mit dem User als Click-Dummy ausgearbeitet sind (Schritt 1).

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
Welche genau, klärt Schritt 1.

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
   (Eintrag 23.10.), ROADMAP und Lastenhefte, wo „Bedienmodus“ oder die
   Programmnamen stehen (GUI-SHELL, BUILD, HANDOVER, HARDWARE-TODO), todo
   (Licht). Owner bestätigt den PR in Worten (ändert claude.md).
1. **Click-Dummy mit dem User**, Screen für Screen, als interaktive
   Artboards auf der Leinwand: Seitenleiste und Kopfzeile, Mix, Show,
   Audio, Outputs, Setup, Verbinden/Profil. Je Screen: was drauf ist, was
   Dialog bleibt, Tastatur, was ein entfernter Arbeitsplatz darf. Ergebnis
   je Screen in DECISIONS; der Dummy ist Referenz für die Umsetzung.
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

## Offen, im Click-Dummy zu klären

- **Panik**: eine Taste für alles (Show-Panik, Ton aus, Licht Blackout?)
  oder je Seite eine eigene, mit der globalen in der Leiste?
- **Kurzbefehle**: heute je Programm (`globalShortcuts`). Künftig global plus
  je Seite? Was gilt, wenn die Seite gerade nicht sichtbar ist?
- **Mehrere Fenster**: eine Seite auf einen zweiten Bildschirm lösen
  (Show auf Monitor 2, Mix auf Monitor 1)?
- **Rechte im Netz**: darf jeder Arbeitsplatz alles, oder gehört eine Seite
  einem Platz (Licht nur vom Lichtpult)? Anzeige, wer was bedient.
- **Name**: Programm `lmix`; Seitennamen englisch (UI zuerst Englisch),
  deutsch über Übersetzung.
