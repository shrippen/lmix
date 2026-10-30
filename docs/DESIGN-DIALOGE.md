# Dialoge, Menüs und Popovers: Entscheidungen

Stand 2026-10-30. **Entschieden (User)** über die Wahlseite der Design-Durchsicht.
Quelle der Tafeln: Canvas „lmix: Popups und Menüs“
(https://claude.ai/artifact/65MRcnoQmnS57Rfe8itDve). Dort ist die gewählte Spalte jeder
Tafel markiert, und die Notiz des Users steht darunter. Die Antworten auf die Nachfragen
liegen in einem eigenen Artefakt, „lmix Nachfragen“
(https://claude.ai/artifact/8L3Lw8VsEJUGm4Rafnpyn8). Beide Runden der Nachfragen
sind beantwortet (Abschnitt „Antworten auf die Nachfragen“).

Dieses Dokument ist die verbindliche Vorgabe für den Umbau der QML-Oberfläche
(`workspace/console-qt/qml`). Was hier nicht steht, bleibt wie heute. Bausteine, die
fehlen, kommen nach Kante (`Kante/qml/Kante`) und werden dann mit
`scripts/sync-kante.sh` übernommen.

## Querschnitts-Regeln

Diese Regeln gehen den einzelnen Tafeln vor. Widerspricht ein gewählter Vorschlag einer
Regel, gilt die Regel.

1. **Entfernen: immer mit Rückfrage, danach ein Toast zum Zurückholen.**
   - Der Bestätigungsdialog hat die Rolle `destructive`: roter Balken, als Titel die
     Frage, die Folge in einem Satz.
   - Der rote Knopf trägt Verb und Objekt („Bus entfernen“). Abbrechen steht links
     daneben und ist der Standard bei Enter.
   - Nach dem Entfernen erscheint ein Toast mit „Rückgängig“; Ctrl+Z nimmt den
     obersten. Mehrere Toasts stapeln sich.
   - Das gilt für Busse, Zonen, Ziele, Layouts, Cues, Auslöser, Belegungen, Vorlagen und
     Ausgänge (Tafel 2, Mischung aus A und B).
   - Ohne Toast, weil lmix-core es nicht zurückholen kann, gehen nur: einen laufenden
     Ausgang stoppen und die Werkseinstellungen. Dort bleibt es beim Dialog.
2. **Meldungen: Toasts unten rechts, Banner nur für Dauerzustände** (Tafel 4, B).
   - Fehler bleiben stehen, bis man sie schließt.
   - Banner zeigen nur Zustände, die andauern, etwa „älteres Format“ oder
     „lmix-core offline“.
   - **Das Hilfesystem gehört nicht dazu.** Dazu zählen das F1-Overlay und neue Regeln
     für Tooltips. Der User bearbeitet es gesondert; bis dahin bleiben die Tooltips wie
     heute.
3. **Dialograhmen** (Tafel 1, Mischung):
   - Aus A: `KanteDialog` bekommt die Größen S, M und L (24, 36 und 48 Rastereinheiten,
     höchstens 90 % des Fensters). Der Inhalt scrollt zwischen festem Kopf und Fuß, und
     die Knöpfe stehen immer im Fuß.
   - Aus B: **Liveeditoren öffnen als Seitenblatt** am rechten Rand, nicht modal. Vorschau
     und Programm bleiben sichtbar, ‹ › wechselt den Input. Liveeditoren sind die Dialoge,
     deren Werte sofort wirken: Farbe, Kanalzug, PTZ. Titel gehört nicht dazu: Er bleibt ein
     Dialog mit Tabs und bekommt die Vorschau im Dialog (D11 A, Nachfrage beantwortet).
   - Formulare (Hinzufügen, Bestätigen) bleiben mittige Dialoge.
4. **Gemeinsame Basis für Liveeditoren visueller Inputs** (Notiz zu D01, D02):
   - Ein Gerüst nimmt jeden Editor auf, der das Bild eines Inputs ändert. Es besteht aus
     dem Seitenblatt mit Kopf (Input, ‹ ›), Vorher/Nachher, Zurücksetzen und den
     Messwerkzeugen (Scopes).
   - Die Scopes umfassen Waveform, RGB-Parade, Vektorskop und Histogramm (LASTENHEFT
     OUT.13).
   - Gewählt ist B: Die Scopes stehen als Leiste unter der Vorschau des Seitenblatts.
   - Dazu kommt der Knopf „Als Fenster lösen“. Das Fenster zeigt alle vier Scopes, lässt
     sich auf einen zweiten Monitor ziehen (Ort und Größe bleiben gespeichert), folgt dem
     Blatt oder hält einen Input fest. Schließen holt die Scopes in die Leiste zurück. Die
     Werte misst die Engine; die GUI zeigt sie nur an. Entwurf in Runde 2 der Nachfragen, bestätigt.
5. **Icons in Menüs:**
   - Icons bekommen die häufigen Handlungen, und zwar nicht übermäßig, aber überall dort,
     wo es ein eindeutiges Icon gibt (Notiz zu M03).
   - Die Einträge laufen über `KanteMenuItem` (Icon, Wert, Taste). Die Regel aus dem
     Antworten-Artefakt ist so übernommen.
6. **Datei- und Ordnerdialoge** (S01, Mischung):
   - Ein gemeinsames Bauteil für alle 13 Stellen (B). Es regelt Titel mit Ziel, Filter,
     Fokus-Rückgabe und die Umwandlung der Pfade.
   - Es sieht aus wie A und bleibt der Plattform-Dialog.
   - Je Aufruf sind Anpassungen möglich: Titel, Filter, Startordner, Mehrfachauswahl,
     Ordner statt Datei.
   - Der Entwurf im Antworten-Artefakt (Aufrufparameter, Zweck merkt den Ordner) ist so
     übernommen.

## Je Tafel

„Heute“ heißt: bleibt, wie es ist. Bei einer Mischung steht, was aus welchem Vorschlag
kommt.

### Live-Editoren

| Tafel | Wahl | Was umgesetzt wird | Notiz des Users |
|---|---|---|---|
| D01 Farbkorrektur | B | Seitenblatt rechts, nicht modal; ‹ › wechselt den Input, „Vorher“ zeigt das Bild ohne Korrektur. Dazu die gemeinsame Basis mit Scopes als Leiste, lösbar als eigenes Fenster (Regel 4) | Der Liveeditor braucht eine gemeinsame Basis. Diese müssen wir entwickeln. Für die Vorschau braucht es noch Hilfswerkzeuge wie Scopes, die alle Liveeditoren für visuelle Inputs haben sollen. |
| D02 Kanalzug | B | Seitenblatt wie bei der Farbe; ‹ › blättert durch die Inputs, ohne zu schließen | Siehe D01 (gemeinsame Basis; beim Ton gibt es keine Bild-Scopes, aber dasselbe Gerüst) |
| D03 Master, Kopfhörer und Busse | A | Jeder Bus als Karte mit Zahlenwert und Abgriff im Klartext. **Abweichend von A:** Entfernen mit Rückfrage und Toast (Regel 1). **Neu:** Name und Farbe der Busse sind hier bearbeitbar; der Entwurf im Antworten-Artefakt ist so übernommen | Entfernen mit Rückfrage! Außerdem soll man hier Namen und Farbe der Busse bearbeiten können. |
| D04 Belegungen | B | Liste links, Bearbeiten rechts, Änderungen gelten sofort; „Belegung bearbeiten“ entfällt als zweiter Dialog | – |
| D05 Zonen | B | Liste links, gewählte Zone rechts, Änderungen gelten sofort (gleiches Muster wie D04) | – |
| D06 PTZ | geändert | **Nur das Seitenblatt** (Regel 3), kein Popover: Steuerkreuz aus Icons mit 44 px, Tempo, Zoom, **Fokus als Regler mit Knopf „Auto“** (bei Auto ruht der Regler und zeigt den Wert, Ziehen schaltet Auto aus), Positionen 1–8 als Raster mit Abrufen oder Speichern als sichtbarem Modus; die Adresse in den Einstellungen. **Dazu in der Detail-Kachel** (Ansicht → Detail) bei PTZ-Inputs die Positionen 1–4 und vier Pfeile für kleine Korrekturen (Umschalt: größerer Schritt), abschaltbar im Popover Ansicht. Die Knöpfe liegen **auf dem Bild** (B, Runde 2): Pfeile an den Rändern, Positionen unten links; sie erscheinen bei Maus über dem Bild oder Fokus auf der Kachel, dann gehen auch die Pfeiltasten. Die Kachel wird nicht höher | A und B umsetzen. Nachfrage: „Ich will nur das Seitenblatt. Darin für Fokus auch einen Slider und den Knopf für Auto. Für PTZ-Inputs in der erweiterten Kachelansicht die Positionen 1–4 und die Pfeiltasten als Option, damit man ohne extra etwas aufzurufen kleine Korrekturen machen kann.“ |
| D07 Cue-Listen | Clickdummy, bestätigt | Show-Seite wie im Clickdummy: Listen links, Cues auf der Seite bearbeiten, Inspector rechts; der Dialog „Cues…“ entfällt. Der Dummy aus Runde 2 ist gegen `LASTENHEFT-SHOW.md` geprüft und bestätigt (Standby je Liste, Folgen und Wartezeiten, Gruppe als Zeitleiste, Notiz- und Warte-Cues, unscharfe Cues, Pausieren, Vorhören, Zurücksetzen, Panik über die Panikdauer und harter Stopp, Notfall-Liste, Warnliste, Dunkel, Bühnenvorschau, Erweitert); was ihm fehlt, steht unter „Offene Punkte“ | Das brauche ich wohl als Clickdummy der ganzen Seite. Nachfrage: „Sieht auf den ersten Blick gut aus, vergleiche nochmal mit Lastenheft von qlab.“ |
| D08 Datenquellen | B | Zweistufig: erst Datei und Art, dann die Vorschau prüfen; danach eine schlanke Liste. Abspielen gehört an den Titel-Kanal | – |
| D09 Input-Einstellungen | Heute | Bleibt bei den Tabs | – |
| D10 Ziele und Abonnenten | A | Zwei Tabs, je eine Liste; Bearbeiten öffnet das Formular als Dialog mit Speichern im Fuß | – |
| D11 Titel | A | Vier Tabs: Kanäle · Vorlage und Felder · Datensätze · Position; Abspielen, Weiter und Stopp immer im Fuß. **Dazu eine Live-Vorschau im Dialog** (Nachfrage: A); kein Seitenblatt | Braucht noch eine Livevorschau. Mach Vorschläge, wie das aussehen könnte. |

### Formulare

| Tafel | Wahl | Was umgesetzt wird | Notiz des Users |
|---|---|---|---|
| F01 Aktionen | A | Titel immer „Aktionen · Art Name“; der primäre Knopf sagt, was er tut, und ist gelb | – |
| F02 Ausgang stoppen | A | Vorbild aller Bestätigungen: Frage als Titel, Folge in einem Satz, roter Knopf mit Verb und Objekt; „Nicht mehr fragen“ klein unten links im Fuß | – |
| F03 Programmformat (auch Neue Show) | A | Größe S, Seitenverhältnis als Vorschau, die Folge als Callout, der primäre Knopf gelb. **Dazu weitere Seitenformate**, im Dialog zuerst das Seitenverhältnis wählen, dann die Größe (Nachfrage: A) | Es sollte auch andere Seitenformate geben. Wie sähe der Dialog dann aus? |
| F04 Inputs aus einer Show | A | Liste mit fester Höhe und Scrollen, „Alle/Keine“, die Anzahl im primären Knopf, der Dateiname im Titel | – |
| F05 Stinger | B | Zeitleiste mit Vorschaubild; Cut-Marke und Ende ziehen. **Dazu:** Pfeiltasten gehen Bild für Bild weiter, Umschalt 10 Bilder (Entwurf im Antworten-Artefakt so übernommen) | Mit den Pfeiltasten kann man Bild für Bild durchgehen. |
| F06 Belegung bearbeiten | B | Entfällt als eigener Dialog (siehe D04 B) | – |
| F07 Overlay-Kanal | Mischung | Aus B die Box im 16:9-Rahmen zum Ziehen (die Vorschau ist wichtig), aus A die Gruppen und die Prozentfelder für genaue Werte (Entwurf übernommen). **Neu:** Seitenverhältnis der Box festlegbar (Frei, Wie Quelle, 16:9, 4:3, 1:1, 9:16); gekoppelt folgt Höhe der Breite und umgekehrt, in Pixeln des Programms, die Ecke hält das Verhältnis (Entwurf in Runde 2, bestätigt) | Mischung aus A und B. B hat die Vorschau, das ist wichtig. Prozentwerte lassen sich dort aber nicht genau einstellen; das kann Vorschlag A besser. |
| F08 Auslöser | A | Je Auslöser eine Karte in Klartext; die Liste scrollt, „+ Auslöser“ unten links | – |
| F09 Input hinzufügen | A | Feste Höhe, nur der rechte Teil scrollt, die Vorschau oben | – |
| F10 Ausgang hinzufügen | Mischung | Die Arten als Kacheln (aus B), sonst das Formularraster aus A (Entwurf übernommen) | Arten als Kacheln, sonst wie Vorschlag A. |
| F11 Layout | B | Nur die gewählte Box zeigt ihre Werte; ein Klick auf die Leinwand wählt; der Dialog bleibt gleich hoch | – |
| F12 Werkseinstellungen | B | Der Knopf wird erst aktiv, wenn das Wort getippt ist; dazu der Weg, vorher eine Sicherung zu ziehen. Roter Balken und Knopf im Fuß nach Regel 1 | – |
| F13 Input umbenennen | A | Größe S, eine gemeinsame Instanz für alle Kacheln, der Text vorausgewählt, Enter bestätigt | – |
| F14 Bus entfernen | A | Muster der Bestätigungen: roter Balken, „Bus entfernen“ rot, die Folgen mit Zahlen; danach der Toast (Regel 1) | – |

### Menüs

| Tafel | Wahl | Was umgesetzt wird | Notiz des Users |
|---|---|---|---|
| 3 Kontextmenüs (Übergang, Ausgang) | A | Effekte direkt im Menü, der gewählte mit Haken; die Dauer als Chips; alle Kontextmenüs mit `KanteMenuItem` und Abschnitten; Entfernen zuletzt und rot | – |
| M01 Datei | A | „Zuletzt“ zeigt Dateinamen groß, Ordner und Datum klein darunter | – |
| M02 Input-Kachel | A | Untermenüs, die für die Quelle nichts tun, entfallen; „Link kopieren“ und „Dynamischer Input“ ziehen in die Input-Einstellungen | Übernommen von der entfallenen Tafel 4 „Menü der Input-Kachel“ |
| M03 ⋯ Weitere Input-Aktionen | A | `KanteMenuItem` mit Icon; „Input hinzufügen…“ im Menü; „In der Zeile ▸“ bleibt als Untermenü | Generell wären Icons für Menüeinträge gut. Nicht übermäßig, aber wo möglich für häufige Handlungen. (Regel 5) |
| M04 Eingeklappte Kachel | B | Kein eigenes Menü: Doppelklick klappt aus, Rechtsklick zeigt das normale Kachelmenü. Geht der Klick eines Inputs auf Sendung (Cut, Schnellstart), klappt nur das Menü aus: ein Doppelklick ließe den Cut auf den zweiten Klick warten | – |
| M05 Layout | A | Icons, Entfernen rot und abgesetzt; mit Rückfrage und Toast (Regel 1) | – |
| M06 Vorlagen | A | Am Knopf verankert; Entfernen rot und unten, mit Bestätigung | – |
| M07 Ausgang | B | Volles Menü mit Kopf und Bitrate: Stoppen, Einstellungen, Adresse, Statistik; Entfernen gesperrt, solange er sendet. „Einstellungen…“ öffnet den Ausgangsdialog zum Bearbeiten (`update_output`); beim Senden zeigt er die Werte gesperrt mit Hinweis (DECISIONS 2026-10-30) ✅ | – |
| M08 Übergangsknopf und Schnellstart | A | Eine Ebene: Effekte als Liste, die Dauer als Chips; Schnellstart ebenso, ohne Stinger | – |
| M10 Overlay-Kanal | A | Statuskopf (an/aus, Inhalt), Icons, der Zustand des Schalters rechts | – |
| M11 Bindung (Titel) | A | Abschnitte je Quelle, Spalten ohne Wiederholung, der aktuelle Wert als Beispiel | – |
| M12 Cue-Farbe | A | Swatch-Picker (`KanteSwatchPicker`), die gewählte mit Ring | – |

### Popovers und Sonstiges

| Tafel | Wahl | Was umgesetzt wird | Notiz des Users |
|---|---|---|---|
| P01 Ansicht | A | Zwei Spalten, ohne doppelten Titel; Größen als kleine Kachelbilder, Schalter statt Häkchen | – |
| P02 Tonspalte | A | `KanteSwitch`, feste Breite M, Gruppen mit Überschrift; Entfernen als roter Textknopf unten rechts, mit Rückfrage und Toast | – |
| S01 Datei- und Ordnerdialoge | Mischung | Regel 6 | A sieht als Vorschlag gut aus, es soll aber wie in B ein gemeinsames Bauteil werden. Es darf trotzdem Varianten und Anpassungen je Aufruf geben. |
| S02 Tasten | A | Tastenkappen in zwei Spalten mit Suche; die eigenen Belegungen als dritte Gruppe. Umgesetzt in `KeysDialog.qml` (DECISIONS 2026-10-30) | – |
| 1 Dialograhmen | Mischung | Regel 3 | Bei Liveeditoren ist ein Seitenblatt nützlich und sinnvoll, sonst Vorschlag A |
| 2 Destruktive Aktionen | Mischung | Regel 1 | Entfernen bekommt immer einen Bestätigungsdialog und danach einen Toast zum Zurückholen. |
| 4 Meldungen | B | Regel 2 | Alle Meldungen landen als Toast unten rechts, Banner für Dauerzustände. Das Hilfesystem ist ein eigenes Thema, mit dem ich mich separat beschäftigen möchte. |

## Antworten auf die Nachfragen

Runde 1, beantwortet am 2026-10-29. Die Tabellen oben sind entsprechend nachgezogen.

| Nachfrage | Antwort | Notiz des Users |
|---|---|---|
| Liveeditor-Gerüst mit Scopes (D01, Regel 4) | B, Scopes als Leiste | Zusätzliche Option, die Scopes als extra Fenster herauszulösen. |
| Show-Seite (D07) | passt so | Mit dem Show-Lastenheft vergleichen (Runde 2). |
| Titel-Vorschau (D11) | A, Vorschau im Dialog | – |
| Seitenformate (F03) | A, erst das Seitenverhältnis | – |
| Stinger bildweise (F05) | passt so | – |
| Overlay-Box (F07) | passt so | Option, ein Seitenformat festzulegen; die Werte passen sich dann selbst an (Runde 2). |
| Ausgang hinzufügen (F10) | passt so | – |
| Busse (D03) | passt so | – |
| PTZ (D06) | ändern | Nur das Seitenblatt, mit Fokus-Regler und Auto; Positionen 1–4 und Pfeile in der Detail-Kachel als Option (Runde 2). |
| Dateidialog (S01, Regel 6) | passt so | – |
| Icons in Menüs (M03, Regel 5) | passt so | – |

Runde 2, beantwortet am 2026-10-30.

| Nachfrage | Antwort |
|---|---|
| PTZ in der Detail-Kachel (D06) | B, auf dem Bild |
| Seitenverhältnis der Overlay-Box (F07) | passt so |
| Scopes als eigenes Fenster lösen (D01, Regel 4) | passt so |
| Show-Seite gegen das Show-Lastenheft (D07) | passt so |

## Offene Punkte

- D07 Cue-Listen: Der Dummy ist bestätigt. Für diese Punkte des Show-Lastenhefts braucht die
  Seite noch einen Platz:
  weitere Gruppenmodi (CUE.4–6), Start-Cue (CUE.10), andere beim Start ausblenden
  (CUE.11), Zweit- und direkte Auslöser (CUE.14, CUE.16), Sperren je Klasse (RUN.9),
  Raster ohne Standby (RUN.12), Zonen-Leiste (SUI.8), Umsortieren und Umnummerieren
  (SUI.10), Prüfliste (SUI.12) und die Show-Sperre (SAF.8).
- D06 PTZ, Knöpfe auf dem Bild: Bei Kachelgröße S sind die Ziele klein. Beim Bau prüfen, ob
  sie dort die 24 px aus WCAG 2.5.8 halten; sonst zeigt S nur die Pfeiltasten über den Fokus.
- D11 Titel: entschieden (A, Vorschau im Dialog). Damit ist Titel kein Liveeditor mehr
  im Sinn von Regel 3; die Regel ist angepasst. Eine Overlay-Vorschau in der Engine
  (OVL.3) wird dafür nicht gebraucht.
- D09 bleibt bei den Tabs. Farbe und Kanalzug öffnen dagegen als Seitenblatt. Ob
  „Optionen“ der Kachel später auch dorthin führt, ist offen.
- Hilfesystem (F1, Tooltips): eigenes Thema.
