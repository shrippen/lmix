# Lastenheft: Show-Modus (Showrunner) für lmix

Stand 2026-10-16, Entwurf 3: abgeglichen mit dem QLab-5-Handbuch; Licht,
Objekt-Audio, Effekt-Plugins und Timecode aufgenommen. Entscheidungen in §6
(User 2026-10-16), offen nur S-E.3.
Ergänzt `LASTENHEFT.md` (Bildmischer) um einen zweiten Bedienmodus über
demselben `lmix-core`. Abweichungen von `claude.md`, `ROADMAP.md` oder
`LASTENHEFT.md` stehen in §6 zur Entscheidung, nicht stillschweigend.

Dieses Dokument beschreibt **was** der Show-Modus können muss, nicht **wie**.

**Leitlinie**: Vorbild ist QLab 5. Der Mischer-Modus denkt in Inputs und
entscheidet im Moment (Preview → Program). Der Show-Modus denkt in einer
vorbereiteten Folge von Cues und arbeitet sie mit GO ab. Beide Modi teilen
Engine, Inputs, Ausgänge, Ton, Aktionskatalog, Auslöser, APIs und
Show-Datei. QLab ist die Vorlage, kein Zwang: lmix weicht ab, wo es
sinnvoll ist (Linux, Drehbuch-Betrieb, HTTP, Zonen statt Patches im
Dokument), und jede Abweichung steht hier mit Grund.

---

## 0. Clean-Room-Regeln

Es gelten CR.1 bis CR.8 aus `LASTENHEFT.md` sinngemäß, mit QLab statt vMix:

| # | Regel |
|---|---|
| SCR.1 | Quelle für Vorbild-Verhalten: nur das öffentliche QLab-5-Handbuch (<https://qlab.app/docs/v5/>). Kein Zerlegen von QLab-Programmen, keine QLab-Dateiformate als Vorlage. Programme, die an QLab andocken (Companion, Stream-Deck-Plugin), sind Referenz für die API, nicht für QLab-Verhalten. |
| SCR.2 | Eigene Worte, eigene Namen. Kein Handbuchtext, keine Screenshots. Kein „QLab“, kein „Workspace“-Format, keine QLab-Icons in Oberfläche und Dateien. |
| SCR.3 | Handbuch ausgewertet am 2026-10-16 (97 Seiten). Zuordnung Anforderung → Handbuchseite: `LASTENHEFT-SHOW-QUELLEN.md`. Lücken werden dort und hier ergänzt, dann umgesetzt. |
| SCR.4 | Anforderungen ohne Vorbild im Handbuch sind mit **(lmix)** markiert. |

---

## 1. Begriffe

| Begriff | Bedeutung | QLab-Pendant |
|---|---|---|
| Show-Modus | Bedienmodus mit Cue-Listen und GO, neben dem Mischer-Modus | — |
| Cue | Ein Schritt im Ablauf: Nummer, Name, Art, Ziel, Zeiten, Folge, Notiz | Cue |
| Cue-Liste | Geordnete Folge von Cues mit eigenem Standby. Eine Show hat eine oder mehrere | Cue List |
| Raster | Freie Tastenfläche ohne Standby und ohne Reihenfolge | Cue Cart |
| Standby | Der Cue, den das nächste GO in dieser Liste startet | Playhead |
| GO | Startet den Standby-Cue samt seiner Folge und rückt Standby hinter die Folge | GO |
| Vorwartezeit | Zeit zwischen Startbefehl und Beginn der Aktion | Pre Wait |
| Nachwartezeit | Zeit ab Beginn der Aktion, nach der die Folge greift | Post Wait |
| Folge | Nichts, nächsten Cue nach Nachwartezeit starten, nächsten Cue nach Ende starten | Continue Mode |
| Gruppe | Cue mit Kindern, in einem von mehreren Modi (CUE.4) | Group Cue |
| Unscharf | Cue führt seine Aktion nicht aus, Zeiten und Folge bleiben | Disarmed |
| Ziel | Worauf ein Fade-, Stopp-, Start- oder Aktions-Cue wirkt (Cue, Zone, Bühne) | Target |
| Zone | Logischer Klangausgang mit Namen („vorn“, „hinten“); pro Rechner auf Gerät und Kanäle gelegt (lmix) | Cue-Ausgang eines Output Patch |
| Bühne | Bildfläche, auf der Bild-Cues in nummerierten Ebenen liegen; pro Rechner einem Bildschirm zugeordnet | Stage |
| Ausgabeziel | Benanntes externes Ziel (HTTP-Basis, OSC-Host) in den Rechner-Einstellungen | Network Patch |
| Panik | Alles über die Panikdauer ausblenden, dann anhalten | Panic |
| Harter Stopp | Alles sofort anhalten, ohne Ausblendung | Hard Stop |
| Notfall-Liste | Cue-Liste, die bei Panik zusätzlich läuft (lmix) | — |
| Drehbuch | Externe Textdateien, aus denen Cue-Listen erzeugt werden (lmix) | — |
| Variable | Benannter Wert der Show, von Cues gesetzt, über API lesbar (lmix) | — |

---

## 2. Systemmodell

```
 Drehbuch (Markdown) ──Import-Werkzeug──► API ─┐
                                               ▼
 Mischer-Modus ─┐                     ┌──── lmix-core ─────────────────────┐
 Show-Modus ────┼── Aktionskatalog ──►│ Show-Zustand · Cue-Listen · Ablauf │
 Companion/OSC ─┤                     │ (Standby je Liste, laufende Cues,  │
 Tasten/MIDI ───┘                     │  Zeiten, Folgen, Panik)            │
                                      └──┬──────────────┬──────────────┬───┘
                                         ▼              ▼              ▼
                                   Engine (strom)   Ausgabeziele    Ereignisse
                                   Bühne: Ebenen    HTTP, OSC       Standby, laufende
                                   Zonen → Geräte   (Rechner-       Cues, Restzeit,
                                   (Rechner-Patch)   Einstellung)   verworfenes GO → Abo
```

- Der Ablauf läuft in `lmix-core`, nicht in der Oberfläche (E.4, NFA.6).
  Ein Absturz der Console hält keinen Cue an.
- Die Show nennt nur logische Namen (Zone, Bühne, Ausgabeziel). Welches
  Gerät, welcher Bildschirm, welche Adresse und welches Geheimnis dahinter
  stehen, liegt in den Rechner-Einstellungen. Eine Show läuft dadurch auf
  einem anderen Rechner nach neuem Patch unverändert (lmix; QLab speichert
  Patches im Dokument).
- Der Drehbuch-Import ist ein eigenes Werkzeug außerhalb von `lmix-core`
  und spricht nur die API (Prozessregel `claude.md` 4).

---

## 3. Funktionale Anforderungen

Priorität wie `LASTENHEFT.md`: **M** Muss, **S** Soll, **K** Kann, **X**
ausgeschlossen. **MS**: Meilenstein (§7). Stand: ✅ vorhanden · ◐ teilweise ·
– fehlt; Bezug auf bestehende IDs in Klammern.

### 3.1 Cue-Modell (CUE)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| CUE.1 | Cue hat Nummer, Name, Art, Notiz (mehrzeilig), Farbe, feste ID. Nummer ist freier Text, darf leer sein, ist in der ganzen Show eindeutig; Umsortieren ändert keine Nummer. Ohne Namen wird einer aus Art und Ziel gebildet. Nummern ohne Leerzeichen und Sonderzeichen empfohlen (Fernsteuerung), Import warnt | M | S1 | ✅ (Drehbuch warnt bei Nummern mit Leerzeichen oder Zeichen) |
| CUE.2 | Vorwartezeit und Nachwartezeit je Cue, 0 bis 1 h, Auflösung 10 ms. Nachwartezeit zählt ab Beginn der Aktion, also nach der Vorwartezeit | M | S1 | ✅ |
| CUE.3 | Folge je Cue: keine; nächsten starten nach Nachwartezeit; nächsten starten nach Ende (dann gilt Nachwartezeit = Dauer) | M | S1 | ✅ |
| CUE.4 | Gruppen-Modi: **Zeitleiste** (alle Kinder starten zugleich, versetzt nur durch ihre Vorwartezeiten; Kinder haben keine Folge), **erstes starten** (erstes Kind startet, Rest über Folgen; Standby springt hinter die Gruppe) | M | S1 | ✅ |
| CUE.5 | Gruppen-Modi **hinein** (Standby geht ins erste Kind; Gliederung, z. B. Szene), **Wiedergabeliste** (Kinder nacheinander, Schleife, Mischen, Überblendung; GO = nächstes Kind) | S | S2 | – |
| CUE.6 | Gruppen-Modus **Zufall**: ein scharfes, nicht laufendes Kind; jedes kommt einmal dran, bevor sich eines wiederholt | K | S3 | – |
| CUE.7 | Aktions-Cue: beliebige Aktionen des Katalogs (heutiger Cue, AUT.14) | M | S1 | ✅ |
| CUE.8 | Notiz-Cue: tut nichts, trägt eine Regieanweisung oder trennt Abschnitte | M | S1 | ✅ |
| CUE.9 | Warte-Cue: läuft nur ab, Dauer = Nachwartezeit | S | S1 | ✅ |
| CUE.10 | Start-Cue: startet einen anderen Cue oder eine Cue-Liste, ohne Standby zu bewegen | S | S1 | ✅ |
| CUE.11 | Andere beim Start ausblenden und stoppen: Geschwister, Liste oder alles, mit Zeit | M | S1 | ◐ (stoppt; Ausblendung mit Klang und Bild ab Stufe 2) |
| CUE.12 | Unscharf: Aktion entfällt, Zeiten und Folge bleiben; optional „überspringen, wenn unscharf“. Scharf-/Unscharf-Cue mit Ziel | S | S2 | – |
| CUE.13 | Springen-Cue: setzt Standby auf ein Ziel | S | S2 | ◐ (`cue_standby` als Aktion) |
| CUE.14 | Zweitauslöser je Cue: was ein Start an einen laufenden Cue tut (nichts, Stopp mit Ausblendung, harter Stopp, Neustart, Schleife beenden); optional beim Loslassen der Taste | S | S2 | – |
| CUE.15 | Schleife sauber beenden: laufender Durchgang spielt zu Ende; optional danach nächsten Cue starten oder Ziel stoppen | S | S2 | – |
| CUE.16 | Direkter Auslöser je Cue (Taste, MIDI, OSC, Uhrzeit): startet Cue samt Folge, Standby unverändert | S | S2 | ◐ (Kurzbefehle) |
| CUE.17 | Pause-, Zurücksetzen- und Laden-Cue mit Ziel | K | S3 | – |
| CUE.18 | Ducken: solange ein Cue läuft, andere Klänge der Liste um x dB senken, mit Zeit | K | S2 | – |
| CUE.19 | Defekter Cue (fehlende Datei, fehlendes Ziel) in einer Folge: Zeiten und Folgen laufen weiter, Fehler wird gemeldet | M | S1 | ✅ (Fehler als Ereignis `cue` `failed`) |
| CUE.20 | Grenzen: mindestens 1000 Cues je Liste, 64 Kinder je Gruppe, Gruppen 3 Ebenen tief | S | S1 | ✅ |
| CUE.21 | Heutige Cue-Listen (AUT.14) werden beim Laden ins neue Cue-Modell migriert und verhalten sich wie bisher (keine Zeiten, keine Folge); kein Rückwärtslesen alter Formate (S-E.5) | M | S1 | ✅ (Show-Datei Version 2) |

### 3.2 Ablauf (RUN)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| RUN.1 | GO startet den Standby-Cue samt Folge; Standby rückt auf den ersten Cue nach der Folge bzw. Gruppe (bei Gruppe „hinein“ ins erste Kind) | M | S1 | ✅ (ohne „hinein“, S2) |
| RUN.2 | Standby setzen, unabhängig von laufenden Cues: einen Cue vor/zurück, eine ganze Folge vor/zurück, auf Nummer, auf Anfang | M | S1 | ✅ (`cue_standby`) |
| RUN.3 | Mehrere Cues laufen gleichzeitig; Liste der laufenden Cues mit abgelaufener Zeit, Restzeit, Fortschritt | M | S1 | ✅ (`running_cues` im Zustand; Anzeige SUI.4) |
| RUN.4 | Einzelnen laufenden Cue stoppen (mit seiner Ausblendung) oder hart stoppen | M | S1 | ◐ (`cue_stop`; Ausblendung ab Stufe 2) |
| RUN.5 | Alles pausieren / fortsetzen, über alle Listen; einzelnen Cue pausieren | S | S2 | – |
| RUN.6 | Schutz vor Doppel-GO: Mindestabstand einstellbar (Standard 300 ms, lmix-Wert), wirkt auf GO über alle Bedienwege, nicht auf direkte Auslöser, Folgen, Stopp, Panik; verworfenes GO sichtbar und als Ereignis; Option „erst nach Loslassen wieder scharf“ | M | S1 | ✅ (Abstand, Ereignis `go_ignored`, `set_go_rearm` `release` für MIDI, OSC `/lmix/cue/release`, Leertaste) |
| RUN.7 | Mehrere Cue-Listen je Show, jede mit eigenem Standby; eine aktive Liste für GO; „Liste X starten“ = GO in X, ohne aktive Liste zu wechseln | M | S1 | ✅ |
| RUN.8 | Zeitgenauigkeit: Wartezeiten und Folgen ± 10 ms über 4 h | M | S1 | ◐ (ohne Drift; freier Rechner 1 h: spätestens 5,2 ms; unter Last bis 146 ms, DECISIONS 17.10.; Echtzeit-Priorität und 4 h frei offen) |
| RUN.9 | Sperren je Ein- und Ausgabeklasse (Netz rein/raus, HTTP, MIDI, OSC): z. B. Probe ohne Lichtpult oder kein fremdes GO; betroffene Cues zeigen Warnung; per Aktion und API schaltbar | S | S2 | – |
| RUN.10 | Vorhören: einzelnen Cue ohne Vorwartezeit und Folge abspielen, Standby bleibt; Umleitung je Ausgabeart (unverändert, stumm, andere Zone, Monitorfenster) | S | S2 | – |
| RUN.11 | Show zurücksetzen: harter Stopp, vorübergehende Änderungen verwerfen, alle Standbys auf den Anfang | S | S1 | ✅ |
| RUN.12 | Raster: Tastenfläche ohne Standby für spontane Effekte, je Feld ein Cue; Companion-Taste je Feld | S | S2 | – |
| RUN.13 | Ab Zeitpunkt starten (auch negativ ab Ende), gilt für die ganze Folge | K | S3 | – |
| RUN.14 | Start mitten in der Show mit Zustand nachziehen: dauerhafte Klänge und Bühne des Abschnitts herstellen (lmix) | K | S3 | – |

### 3.3 Klang (SND)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| SND.1 | Klang-Cue spielt eine Datei (WAV, FLAC, OGG, Opus, MP3; bis 24 Kanäle; Abtastrate und Bittiefe beliebig, automatisch angepasst). Komprimierte Formate werden vorab dekodiert oder vorgeladen, damit SND.12 gilt | M | S1 | – |
| SND.2 | Lautstärke je Cue (dB, 0 dB = unverändert). Einblendzeit und Ausblendzeit beim Stoppen als Komfort im Klang-Cue; wirken intern wie Fades und werden von späteren Fades übersteuert (lmix) | M | S1 | – |
| SND.3 | Wiederholungen: n-mal oder endlos, lückenlos (keine Pause, kein Klick an der Nahtstelle) | M | S1 | – |
| SND.4 | Mindestens 16 Klang-Cues gleichzeitig ohne Aussetzer (lmix) | M | S1 | – |
| SND.5 | Kanalzuordnung je Klang-Cue: Dateikanal → Zone mit Pegel je Kreuzung; Vorlagen je Zone (Stereo → L/R, Stereo → Mono −3 dB); Gesamtpegel je Cue | M | S1 | – |
| SND.6 | Fade-Cue, absolut: ändert nur die gesetzten Parameter eines Ziels (Cue oder Zone) über eine Dauer; optional „Ziel danach stoppen“; Zielwert „still“ = untere Reglergrenze; Fades auf verschiedene Parameter desselben Ziels laufen gleichzeitig | M | S1 | – (AUD.14) |
| SND.7 | Fade relativ (dB addieren); ein absoluter Fade hat Vorrang vor laufenden relativen | S | S2 | – |
| SND.8 | Fade-Kurve: S-Kurve als Standard | M | S1 | – |
| SND.9 | Weitere Kurven (linear, parametrisch) und Pegelbereich (Reglerskala, dB, linear) | S | S2 | – |
| SND.10 | Zonen: mindestens 8 benannte Zonen mit 1–8 Kanälen; pro Rechner Zuordnung Zone → Gerät und Kanäle mit Pegel, mehrere Ziele je Zone möglich. Die Show nennt nur Zonennamen (lmix; QLab hält Patches im Dokument) | M | S1 | ◐ (Zonen mit Namen und Kanälen in der Show, Zuordnung zu Gerät, Kanälen und Pegel je Rechner, mehrere Ausgänge; Abspielen mit Stufe 2) |
| SND.11 | Lautstärke und Mute je Zone, als Aktion, als Fade-Ziel und in der Oberfläche | M | S1 | ◐ (Aux-Bus 1–2) |
| SND.12 | Latenz GO → hörbarer Klang ≤ 50 ms | M | S1 | – |
| SND.13 | Reglergrenzen der Show: höchster Wert (Standard +12 dB) und niedrigster (Standard −60 dB = still); gelten für Cues, Fades und Fernsteuerung | M | S1 | – |
| SND.14 | Signal-Begrenzer je Zone (Höchstpegel in dBFS), Schutz vor Lautstärkesprüngen (lmix) | S | S1 | – |
| SND.15 | Zonen auf verschiedenen Geräten bleiben über 4 h synchron (Taktangleichung); Abweichung im Selftest gemessen | M | S1 | – |
| SND.16 | Fehlendes Gerät einer Zone: Warnung an Zone und Cues, Show läuft weiter; Gerät kommt zurück → Zone geht wieder | M | S1 | ◐ (Warnung für Zonen ohne Ausgang auf diesem Rechner; Geräteausfall zur Laufzeit mit Stufe 2) |
| SND.17 | Klang-Cue erscheint nicht als Input-Kachel im Mischer (lmix) | S | S1 | – |
| SND.18 | Start- und Endpunkt in der Datei | S | S2 | – |
| SND.19 | Fade der Kreuzungspegel (Klang zwischen Zonen verschieben) | S | S2 | – |
| SND.20 | Trim je Cue, bewusst nicht per Fade oder Fernsteuerung änderbar | K | S2 | – |
| SND.21 | Abschnitte (Marken) mit eigener Wiederholzahl, 0 = überspringen; Marken aus WAV übernehmen | K | S3 | – |
| SND.22 | Pegelkurve über die Dauer eines Cues | K | S3 | – |
| SND.23 | Live-Ton-Cue: Eingang (Mikrofon, Mischer-Input) in Zonen | K | S3 | – |

### 3.4 Bild und Bühne (VIS)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| VIS.1 | Mehrere Bühnen je Show (mindestens 4), jede eine eigene Bildfläche mit eigenen Ebenen; die Show nennt nur Bühnennamen. Pro Rechner wird jede Bühne einem oder mehreren Ausgängen zugeordnet: Vollbild auf einem Bildschirm/Projektor oder Fenster auf dem Desktop (Probe ohne Projektor, Kontrollbild) | M | S1 | ◐ (OUT.3, Display Vollbild/Fenster) |
| VIS.1a | Bühne zusätzlich als Netzausgang (NDI/OMT) | S | S2 | ◐ (OUT.6) |
| VIS.2 | Bild-Cue: Video oder Standbild auf eine Bühne; Standbild läuft bis Stopp; Video: Wiederholungen wie SND.3, am Ende halten (letztes Bild bleibt) oder stoppen | M | S1 | ◐ |
| VIS.3 | Ebenen: Ebenennummer je Bild-Cue (1–999, dazu „ganz oben“, „ganz unten“); auf gleicher Ebene liegt der zuletzt gestartete oben. „Kulisse“ ist Konvention (z. B. Ebene 1) | M | S1 | ◐ (Overlays, OVL) |
| VIS.4 | Deckkraft je Bild-Cue (0–100 %); Bild-Fade-Cue für Deckkraft mit Kurve und „Ziel danach stoppen“. Überblenden = neuer Cue auf derselben Ebene blendet ein, alter blendet aus und stoppt (CUE.11) | M | S1 | – |
| VIS.5 | Füllmodus je Bild-Cue: einpassen, füllen, strecken | M | S1 | – |
| VIS.6 | Mischmodus je Bild-Cue: mindestens normal, aufhellen, addieren, multiplizieren (Effektvideos auf Schwarz) | S | S1 | – |
| VIS.7 | Bühnen-Dimmer: Gesamthelligkeit einer Bühne per Fade (0–100 %), unabhängig vom Inhalt (lmix) | M | S1 | – |
| VIS.8 | Bühne zeigt ohne laufende Cues, beim Start und nach Panik dauerhaft Schwarz, nie Testbild oder Desktop | M | S1 | – |
| VIS.9 | Freie Geometrie: Position, Skalierung, Drehung, Beschnitt; per Fade änderbar | S | S2 | – |
| VIS.10 | Ton aus Videodateien in Zonen (wie SND.5) | S | S2 | – |
| VIS.11 | Bühne als virtuelle Fläche mit eigener Auflösung, Teilbereiche auf mehrere Ausgänge | S | S2 | – |
| VIS.12 | HTML-Ebene auf der Bühne mit Variablen (lmix) | S | S2 | – (GFX.2) |
| VIS.13 | Videoeffekte je Cue (Helligkeit, Weichzeichner …), Parameter per Fade | K | S3 | – |
| VIS.14 | Text-Cue: gestalteter Text als Bild-Ebene | K | S3 | ◐ (GFX) |
| VIS.15 | Live-Bild (Kamera, NDI, Mischer-Input) als Bild-Cue | K | S3 | ◐ (Inputs) |
| VIS.16 | Kantenüberblendung, Eckpunkt-Verzerrung, Maske je Ausgang | K | S3 | – (OUT.18) |
| VIS.17 | Bühne als Input im Mischer, für die gemischte Theatershow: feste Teile fährt die Show, in freien Teilen übernimmt der Mischer; Show und Mischer laufen auch je allein (S-E.10) (lmix) | S | S2 | ◐ (Inputs) |

### 3.5 Externe Ziele (EXT)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| EXT.1 | Ausgabeziele benannt in den Rechner-Einstellungen: Art (HTTP, OSC über UDP/TCP), Basis-URL bzw. Host:Port, Geheimnis-Verweis, Zeitlimit, mehrere gleichzeitige Ziele (Haupt/Reserve); Cues nennen nur den Zielnamen | M | S1 | ◐ (HTTP; OSC-Ziele angelegt, Senden mit EXT.6) |
| EXT.2 | Geheimnisse (Tokens, Passwörter) nur in den Rechner-Einstellungen; nie Klartext in Show-Datei oder Drehbuch (lmix) | M | S1 | ✅ (`targets.json`, nur für den Nutzer lesbar) |
| EXT.3 | HTTP-Cue: Ziel, Methode, Pfad, Kopfzeilen, Rumpf (JSON/Text); Fehler werden angezeigt und blockieren den Ablauf nie (lmix; QLab hat kein HTTP) | M | S1 | ✅ (Aktion `http_request` in einem Aktions-Cue, Ereignis `target`) |
| EXT.4 | Home-Assistant-Kurzform: Dienst + Entität + Daten auf ein HA-Ziel (lmix; Gegenstück zu QLabs Gerätebeschreibungen) | S | S1 | ✅ (`ha_service`) |
| EXT.5 | Ziel testen: Nachricht probeweise senden, Ergebnis mit Grund | S | S1 | ✅ (`test_target`) |
| EXT.6 | OSC-Cue: Adresse, Argumente (Zahl, Text, Wahrheitswert, leer) an ein Ziel | S | S2 | – |
| EXT.7 | Platzhalter in ausgehenden HTTP- und OSC-Nachrichten für Live-Werte (Standby-Nummer, Variable) | S | S2 | – |
| EXT.8 | Netz-Cue mit Dauer: wiederholt senden oder einen Wert blenden | K | S3 | – |
| EXT.9 | MIDI-Cue: Note, CC, Programmwechsel, MSC | K | S3 | – |
| EXT.10 | Art-Net/sACN-Cue für einfache DMX-Werte mit Fade | K | S3 | – |

### 3.6 Variablen (VAR) (lmix)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| VAR.1 | Show-Variablen (Zahl, Text), per Cue setzen, erhöhen, verringern | S | S2 | ◐ (AUT.5 dynamische Werte) |
| VAR.2 | Variablen über API, OSC und Ereignis-Abo lesbar und setzbar | S | S2 | – |
| VAR.3 | Bedingter Cue: läuft nur, wenn eine Variable eine Bedingung erfüllt | K | S3 | – |

### 3.7 Drehbuch-Import (IMP) (lmix)

QLab kennt keinen Import aus Texten; am nächsten kommt das Anlegen von Cues
über die Fernsteuerung. Der Import nutzt deshalb nur die API.

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| IMP.1 | Eigenes Werkzeug liest Markdown-Dateien eines Ordners (z. B. Obsidian-Vault) und erzeugt Cue-Listen über die API | M | S1 | ✅ (`lmix-script`, `docs/DREHBUCH.md`) |
| IMP.2 | Cues stehen als Codeblöcke (`lmix-cue`) im Text; Reihenfolge im Text = Reihenfolge der Liste; Überschriften werden Notiz-Cues als Szenenmarken (S2: Gruppen „hinein“, CUE.5) | M | S1 | ✅ |
| IMP.3 | Eine Datei = eine Cue-Liste; Name aus Dateiname oder Frontmatter | M | S1 | ✅ |
| IMP.4 | Medienpfade relativ zum Ordner; fehlende Dateien und doppelte Nummern als Fehler mit Datei und Zeile | M | S1 | ✅ (fehlende Medien als Warnung, CUE.19) |
| IMP.5 | Neu laden auf Befehl und bei Dateiänderung; Standby bleibt auf derselben Cue-Nummer; laufende Cues laufen weiter | M | S1 | ✅ (`--watch`) |
| IMP.6 | Cue-Nummern im Drehbuch sind die feste Zuordnung; der Import nummeriert nicht selbst | M | S1 | ✅ |
| IMP.7 | Vorlagen: Standardwerte je Cue-Art (z. B. Fade-Dauer) und benannte Cue-Bausteine („Szene“ = Bild + Licht + Atmo als Gruppe) | S | S2 | – |
| IMP.8 | Formatbeschreibung als JSON-Schema | S | S1 | ✅ (`lmix-script schema`) |
| IMP.9 | Im Drehbuch-Betrieb sind Cues in der Oberfläche nicht bearbeitbar (eine Quelle der Wahrheit); Hinweis in der Oberfläche | M | S1 | ✅ (Quelle der Liste, Console sperrt) |

### 3.8 Bedienung und Fernsteuerung (CTL)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| CTL.1 | Befehle: GO, Standby (CUE.1 / RUN.2), Liste wählen und starten, Stopp, harter Stopp, Panik, Panik in Zeit x, Pause/Fortsetzen, Zurücksetzen. Über Tasten, MIDI, OSC über UDP und TCP (SLIP), HTTP; Ports einstellbar. Cues adressierbar über Nummer, feste ID, `standby`, `aktiv` | M | S1 | ◐ (GO, Standby, Liste wählen und starten, Stopp, harter Stopp, Panik, Panik in Zeit x, Zurücksetzen; Cues über Nummer, ID, `standby`, Listen über `active`; Pause S2) |
| CTL.2 | Ereignis-Abo für den Ablauf: Standby (Nummer, Name, Notiz) je Liste, Cue gestartet/gestoppt, laufende Cues mit abgelaufener Zeit und Dauer, Panik, verworfenes GO, Fehler von Ausgabezielen. Restzeit fertig gerechnet (lmix), Takt begrenzt (Standard 4 Hz, einstellbar) | M | S1 | ◐ (Ereignis `running` mit Restzeit, 4 Hz, `--cue-events-hz`; `cue`, `panic`, `target`; Standby über den Zustand) |
| CTL.3 | Abo verwalten: an- und abmelden je Ereignis und Feld; Ablauf nach 60 s ohne Lebenszeichen | M | S1 | ◐ (OSC-Abos enden 60 s nach der letzten Nachricht, `/lmix/ping`; Auswahl je Ereignis und Feld: TCP-Themen, sonst S2) |
| CTL.4 | Feste OSC-Abonnenten in den Einstellungen (Host:Port, Ereignisauswahl, Nachrichtenvorlage mit Platzhaltern für Nummer, Name, Restzeit): Companion-Tasten ohne eigenes Modul und ohne Anmeldung | M | S1 | ✅ (`set_feed` auf `osc_udp`-Ziele, Platzhalter) |
| CTL.5 | Zugriffsschutz für Steuerung von außen: Rechte Ansehen, Steuern, Bearbeiten für OSC, HTTP, TCP, Web. Mehrere Zugangsschlüssel mit eigenen Rechten; Stufe „ohne Schlüssel“ getrennt für localhost und eine LAN-Adressliste (Standard: nur localhost). Panik einzeln ohne Schlüssel freigebbar. Falsche Schlüssel verlängern die Wartezeit, Abweisungen werden protokolliert | M | S1 | ✅ (Schlüssel mit Ansehen/Steuern/Bearbeiten für HTTP und TCP, `--open-panic` auch für OSC, wachsende Wartezeit und Protokoll; Adressliste mit Recht `--trust` für HTTP, TCP, OSC; ohne Schlüssel und Passwort bleibt das LAN frei wie bei API.7, DECISIONS 17.10.) |
| CTL.6 | Globale Taste für GO und Panik, auch wenn die Console nicht im Vordergrund ist; in der Show-Ansicht ist Escape immer Panik, zweimal Escape harter Stopp, nicht umlegbar | S | S1 | ◐ (Leertaste GO, Escape Panik, zweimal hart, in der Show-Ansicht; global über Kurzbefehle) |
| CTL.7 | Antworten auf Anfragen mit Status ok/Fehler/abgelehnt und Daten; für reine Aktionen abschaltbar; Lebenszeichen-Befehl | S | S2 | ◐ (API.3) |
| CTL.8 | Companion-Modul: Aktionen, Rückmeldungen, Variablen für den Show-Modus | S | S2 | – (API.11) |
| CTL.9 | Web-Fernbedienung `/show` für Handy/Tablet: GO, Panik, Standby mit Notiz, laufende Cues; Nur-Lesen-Modus; hält den Doppel-GO-Schutz ein | S | S2 | – |
| CTL.10 | Lernen: eine eingehende OSC- oder MIDI-Nachricht per Mitschnitt einer Bedienfunktion zuordnen, MIDI mit Wertbereichen | S | S2 | ◐ (MIDI-Lernen) |
| CTL.11 | Protokoll aller Auslösungen (Quelle, Zeit, Ergebnis, verworfene GOs) | S | S2 | – |
| CTL.12 | MSC rein (Geräte-ID, GO/STOP/RESUME/ALL_OFF/STANDBY±/RESET) und raus | K | S3 | – |
| CTL.13 | Optional startet beim Laden der Show eine bestimmte Cue-Liste; ab Werk aus (Ausnahme zu SAF.4, für P10) | K | S2 | – |

### 3.9 Sicherheit (SAF)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| SAF.1 | Panik: alle Klänge und Bild-Cues über die Panikdauer ausblenden und stoppen, alle Wartezeiten und Folgen abbrechen, über alle Listen; Standby bleibt; zweite Panik während der Ausblendung = harter Stopp | M | S1 | ◐ (alle Cues und Wartezeiten aus, Standby bleibt, zweite Panik = harter Stopp; Ausblenden mit Stufe 2) |
| SAF.2 | Panikdauer als Show-Einstellung, 0–10 s, Standard 1 s | M | S1 | ✅ (`set_panic_time`) |
| SAF.3 | Harter Stopp als eigener Befehl: sofort still, Bühne schwarz | M | S1 | ◐ (`cue_hard_stop`; still und schwarz mit Zonen und Bühnen) |
| SAF.4 | Notfall-Liste: läuft bei Panik und hartem Stopp zusätzlich (z. B. HTTP: Raumlicht hell) (lmix) | M | S1 | ✅ (`set_emergency_list`) |
| SAF.5 | Panik und harter Stopp über jeden Bedienweg erreichbar, auch ohne Console (OSC, HTTP, MIDI, globale Taste) | M | S1 | ✅ (OSC `/lmix/panic`, `/lmix/stop/hard`, HTTP, MIDI und globale Taste über Kurzbefehle) |
| SAF.6 | Nach Neustart von `lmix-core` oder Engine startet nichts von selbst: Stille, Bühne schwarz, Standbys auf dem letzten Stand (lmix; Ausnahme CTL.13) | M | S1 | ◐ (nichts startet, Standby aus der Sicherung; Stille und Schwarz mit Stufe 2/3) |
| SAF.7 | Zeit von Panik bis Stille ≤ Panikdauer + 100 ms | M | S1 | – |
| SAF.8 | Show-Sperre: Bearbeiten gesperrt, GO und Panik frei; Rückfrage beim Beenden; Bildschirmschoner und Ruhezustand unterdrückt | S | S2 | – |

### 3.10 Oberfläche (SUI)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| SUI.1 | Show-Ansicht in der Console als eigener Modus (umschaltbar zum Mischer) | M | S1 | ✅ (Umschalter Mischer/Show) |
| SUI.2 | Cue-Liste mit Spalten Nummer, Name, Art, Ziel, Vorwartezeit, Dauer, Nachwartezeit, Folge; Standby deutlich markiert; Gruppen einklappbar | M | S1 | ◐ (Nummer, Name, Notiz, Art mit Dauer, Vor/Nach/Folge unter „Erweitert“, Gruppen einklappbar; Ziel steht im Namen) |
| SUI.3 | Großer GO-Knopf, Panik-Knopf, Standby-Name und Standby-Notiz in großer Schrift | M | S1 | ✅ |
| SUI.4 | Laufende Cues mit Fortschrittsbalken, Restzeit, Stopp und Pause je Cue | M | S1 | ◐ (Fortschritt, Restzeit, Stopp je Cue; Pause mit RUN.5, S2) |
| SUI.5 | Warnliste: defekte Cues, fehlende Medien, fehlende Zonen-Geräte, nicht erreichbare Ausgabeziele; vor der Show prüfbar | S | S1 | ◐ (fehlende Start-Ziele, Ausgabeziele, Inputs, Mediendateien; Erreichbarkeit der Ziele: `check_targets`, „Ziele prüfen“, jedes Senden; Zonen-Geräte mit Stufe 2) |
| SUI.6 | Dunkle Raumansicht: stark gedimmt, kein Weiß, für Bedienung im dunklen Raum (lmix) | S | S1 | ✅ (Knopf „Dunkel“: Kante dunkel und ein Schleier über dem Fenster) |
| SUI.7 | Kleine Bühnenvorschau | S | S1 | ◐ (Live-Monitore) |
| SUI.8 | Zonen-Leiste: Pegel und Lautstärke je Zone | S | S1 | – |
| SUI.9 | Gestaltung nach Kante (UI.9), Texte aus Katalogen de/en (NFA.11) | M | S1 | ✅ |
| SUI.10 | Cues in der Oberfläche anlegen, bearbeiten, umsortieren, umnummerieren (wenn kein Drehbuch-Betrieb) | S | S2 | ◐ (Cue-Dialog AUT.14) |
| SUI.11 | Standby unabhängig von der Auswahl (bearbeiten, während ein anderer Cue auf Standby steht) | S | S2 | – |
| SUI.12 | Cue markieren (Prüfliste) und Zweitfarbe „bereits gelaufen“ | K | S2 | – |
| SUI.13 | Felder und Funktionen je nach Modus zeigen oder verbergen; Seltenes hinter „Erweitert“ (S-E.5) (lmix) | S | S1 | ◐ („Erweitert“ in der Show-Ansicht) |

### 3.11 Show-Datei (SSH)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| SSH.1 | Cue-Listen mit allen neuen Feldern in der Show-Datei; Format-Version steigt, ältere Shows werden beim Laden migriert (CUE.21) | M | S1 | ✅ |
| SSH.2 | Im Drehbuch-Betrieb speichert die Show nur den Verweis auf den Drehbuch-Ordner, nicht die erzeugten Cues | S | S1 | ◐ (Show kennt ihren Drehbuch-Ordner, lmix-core hält `lmix-script --watch` dafür am Laufen; die Cues bleiben zusätzlich in der Show) |
| SSH.3 | Zonen-, Bühnen- und Ausgabeziel-Zuordnung liegen nicht in der Show, sondern in den Rechner-Einstellungen; ex- und importierbar | M | S1 | ✅ (Ziele, Abonnenten und Zonen-Zuordnung in `targets.json`; Bühnen mit Stufe 3) |
| SSH.4 | Paket (SHW.6) nimmt Klang- und Bilddateien der Cues mit | S | S2 | ◐ (SHW.6) |
| SSH.5 | Show-Datei auf Wunsch zusammen mit den Rechner-Einstellungen (Zonen, Bühnen, Ausgabeziele, Licht-Patch) exportieren und importieren, z. B. für den Umzug auf gleiche Technik; Geheimnisse bleiben draußen, nur ihre Verweise (EXT.2) (lmix) | S | S1 | ✅ (`export_package`/`import_package` mit `machine: true`, ohne Geheimnisse) |

### 3.12 Licht (LGT)

Licht läuft in S1 über Home Assistant (EXT.4, Entscheidung User
2026-10-16). Ab S3 steuert der Show-Modus Licht auch selbst über DMX.

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| LGT.1 | DMX-Ausgabe über Art-Net (Version 3 und 4) und sACN (E1.31) im Netz; mehrere Universen | S | S3 | – |
| LGT.2 | DMX-Ausgabe über USB-DMX-Schnittstellen mit verbreitetem Protokoll (Enttec-Pro-kompatibel) | K | S3 | – |
| LGT.3 | Geräte (Instrumente) mit Namen, Definition und Gruppenzugehörigkeit stehen in der Show; DMX-Adresse, Universum und Ausgabeweg je Gerät stehen in den Rechner-Einstellungen (wie SSH.3) (lmix) | S | S3 | – |
| LGT.4 | Gerätedefinition: Parameter mit DMX-Kanal, 8 oder 16 Bit, Anzeige in Prozent oder DMX-Wert, Standardparameter, Ruhewert; virtuelle Parameter für Farbe (additiv, subtraktiv, Farbton/Sättigung/Helligkeit) und Schwenken/Neigen | S | S3 | – |
| LGT.5 | Gerätebibliothek mit eigenen Definitionen; Import aus offenen Gerätebibliotheken (lmix) | S | S3 | – |
| LGT.6 | Licht-Gruppen, verschachtelbar; Befehl an eine Gruppe wirkt auf alle Geräte mit dem Parameter | S | S3 | – |
| LGT.7 | Licht-Cue: Liste von Befehlen (Gerät oder Gruppe, Parameter, Wert), Dauer, Kurve getrennt für steigende und fallende Werte; ohne Ziel | S | S3 | – |
| LGT.8 | Werte, die ein Licht-Cue nicht nennt, bleiben stehen (Ursprung beim zuletzt setzenden Cue); Option je Cue „vorherige Licht-Cues der Liste einrechnen“, damit nach Sprüngen (RUN.2) dasselbe Bild entsteht | S | S3 | – |
| LGT.9 | Befehlssprache als Text: Zuweisung, Bereiche, Ad-hoc-Gruppen, „unverändert lassen“, „Ruhewert“, Wert aus anderem Cue ziehen (auch anteilig). Dieselbe Sprache im Drehbuch (IMP) | S | S3 | – |
| LGT.10 | Licht-Pult: Werte live setzen (auch über Zeit), Farbwähler, Schwenk/Neige-Feld; Cue aufnehmen und aktualisieren aus dem Live-Zustand; Ursprungs-Cue je Wert anzeigen | S | S3 | – |
| LGT.11 | Submaster: Cue als Regler, höchster Wert gewinnt; Parken einzelner Werte | K | S3 | – |
| LGT.12 | DMX-Statusansicht: alle Kanäle je Universum mit Wert | S | S3 | – |
| LGT.13 | Verhalten bei Panik und hartem Stopp einstellbar: Licht halten, auf Ruhewerte, oder Notfall-Liste (SAF.4) setzt ein festes Bild (lmix) | S | S3 | – |
| LGT.14 | Warnungen: Gerät ohne Adresse, Adresskonflikt, fehlende Definition, fehlende USB-Schnittstelle | S | S3 | – |
| LGT.15 | Licht-Cue als Fade-Ziel nicht nötig: ein Licht-Cue ist selbst ein Fade | — | — | (Hinweis) |

### 3.13 Objekt-Audio (OBJ)

Klang wird nicht nur über Kanäle (SND.5), sondern über die Position eines
Objekts auf einer Fläche verteilt. Beide Wege gelten gleichzeitig.

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| OBJ.1 | Klang-Karten: 2D-Fläche je Show mit Marken; eine Marke hat Position und einen Pegelsatz je Zonen-Kanal, dazu Gewicht (Einfluss) und Schatten (verdeckt Marken dahinter) | S | S3 | – |
| OBJ.2 | Filter auf der Karte: Trennlinie mit Position, Winkel, Breite; Objekte hören nur Marken auf ihrer Seite; Ausnahmen je Zonen-Kanal | K | S3 | – |
| OBJ.3 | Objekte: Cue-Objekte (nur Klang ihres Cues) und Karten-Objekte (Klang aus mehreren Cues); Position und Ausdehnung | S | S3 | – |
| OBJ.4 | Objekt-Matrix je Klang-Cue: Dateikanal → Objekt mit Pegel, neben der Kanal-Matrix (SND.5); Gesamtpegel gilt für beide | S | S3 | – |
| OBJ.5 | Fade-Pfad: Objekt folgt über die Fade-Dauer einem 2D-Pfad (Punkte, Freihand, Kreis; spiegeln, umkehren), mit Kurve | S | S3 | – |
| OBJ.6 | Wärmebild: Pegel eines Zonen-Kanals über die ganze Fläche | K | S3 | – |
| OBJ.7 | Testobjekt: eine Datei abspielen und auf der Karte verschieben, zum Einrichten | K | S3 | – |

### 3.14 Effekt-Plugins (FX)

Gleicher Unterbau wie AUD.11 im Mischer (Plugins je Input und Bus; LV2,
VST3, CLAP nach E.8). Der Show-Modus nutzt ihn je Cue und je Zone.

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| FX.1 | Plugin-Kette je Klang-Cue: LV2, VST3, CLAP; Reihenfolge, an/aus, Editor-Fenster des Plugins | S | S2 | – (AUD.11) |
| FX.2 | Plugin-Kette je Zone; Zonen-Kanäle für Plugins koppelbar (z. B. Stereo-Hall auf zwei Mono-Kanälen) | S | S2 | – |
| FX.3 | Plugin-Parameter per Fade-Cue änderbar | S | S2 | – |
| FX.4 | Nachklang: ein Cue endet erst, wenn der Nachklang seiner Plugins abgeklungen ist; Stopp mit Ausblendung lässt ihn ausklingen, harter Stopp schneidet ab | S | S2 | – |
| FX.5 | Fehlendes Plugin oder unpassende Kanalzahl: Warnung, Cue spielt ohne dieses Plugin | M | S2 | – |
| FX.6 | Plugins laufen in einem gemeinsamen Plugin-Host-Prozess (bei Bedarf mehrere, z. B. je Zone), damit ihr Absturz weder Engine noch Ablauf anhält (NFA.6, S-E.12) (lmix) | S | S2 | – |
| FX.7 | Nur Effekt-Plugins, keine Klangerzeuger (Instrumente) | — | — | (Abgrenzung) |

### 3.15 Timecode (TC)

| ID | Anforderung | Prio | MS | Stand |
|---|---|---|---|---|
| TC.1 | Timecode rein je Cue-Liste aktivierbar: LTC über einen Audio-Eingang, MTC über MIDI; Bildraten 23,976, 24, 25, 29,97 (mit und ohne Drop-Frame), 30 | S | S3 | – (PER.9) |
| TC.2 | Timecode-Auslöser je Cue: Zeit als Timecode (h:m:s:f) oder Echtzeit (h:m:s.ms); Übernahme der laufenden Zeit per Knopf | S | S3 | – |
| TC.3 | Mitlaufen: Liste folgt Start, Stopp und Sprüngen des Timecodes; beim Start wahlweise Cues vor dem ersten Bild nachholen; beim Stopp Timecode-Cues stoppen, pausieren oder weiterlaufen lassen | S | S3 | – |
| TC.4 | Freilauf: Aussetzer bis 0–2 s (einstellbar) überbrücken | S | S3 | – |
| TC.5 | Timecode-Cue raus: LTC auf einen Zonen-Kanal oder MTC auf einen MIDI-Ausgang; Bildrate, Startzeit, optional Endzeit; mehrere gleichzeitig; LTC läuft im Takt des Audiogeräts | S | S3 | – |
| TC.6 | Schutz: LTC hat keinen Standardkanal und muss ausdrücklich zugewiesen werden; Warnung, wenn der Kanal zu einer Lautsprecher-Zone gehört (lmix) | M | S3 | – |
| TC.7 | Timecode-Anzeige für ein- und ausgehende Ströme, auch groß für Sicht aus Entfernung | S | S3 | – |
| TC.8 | Gemeinsamer Decoder mit dem Mischer (PER.9) | S | S3 | – |

---

## 4. Nichtfunktionale Anforderungen (SNF)

| ID | Anforderung | Prio |
|---|---|---|
| SNF.1 | Plattform wie NFA.1 (Linux); Ton über PipeWire, ALSA als Rückfall | M |
| SNF.2 | Show-Modus läuft auch ohne GPU-Pipeline und ohne Kamera-Inputs mit geringer Last (Ton + eine Bühne in 1080p) | M |
| SNF.3 | Dauerlauf: 4 h mit mehreren Schleifen-Klängen und Bühnenvideo ohne Drift, Klick oder Speicherzuwachs | M |
| SNF.4 | Zeitgenauigkeit und Latenz siehe RUN.8, SND.12, SND.15, SAF.7; automatisch gemessen im Selftest | M |
| SNF.5 | Crash-Isolation wie NFA.6: Console-Absturz hält keinen Cue an und löst nichts aus | M |
| SNF.6 | Prüfbarkeit wie NFA.13; Ablauf-Engine mit Tests für Zeiten, Folgen, Gruppen, Panik, Doppel-GO | M |

---

## 5. Bewusst ausgeschlossen (SX)

| Was | Grund |
|---|---|
| Tonhöhe und Abspielrate ändern | Selten gebraucht, Aufwand hoch; bewusste Abweichung vom Vorbild |
| Skript-Cue | Sicherheit; Ersatz: Aktions-Cue, HTTP-Cue, später Skripte nach AUT.9 |
| Gleichzeitiges Bearbeiten an mehreren Rechnern | Eine Quelle der Wahrheit (IMP.9); Fernsteuerung bleibt möglich |
| MIDI-Datei-Cue | Ohne Bedarf in den Personas |
| Import von QLab-Dateien | Clean-Room (SCR.2) |

Lichtsteuerung, Objekt-Audio, Effekt-Plugins und Timecode standen in
Entwurf 2 hier und sind seit 2026-10-16 (User) Anforderungen (§3.12–3.15).

---

## 6. Entscheidungen

### 6.1 Entschieden (User 2026-10-16)

| # | Frage | Entscheidung | Anmerkung des Users |
|---|---|---|---|
| S-E.1 | Ort der Ablauf-Engine | In `lmix-core` (E.4); Console und Companion sind Clients | |
| S-E.2 | Drehbuch-Import | Eigenes Werkzeug über die API (Prozessregel 4); `lmix-core` kennt kein Markdown | |
| S-E.4 | Zonen | Zone = logischer Ausgang mit Rechner-Zuordnung (SND.10), nicht die heutigen zwei Aux-Busse; Aux-Busse können später auf Zonen abgebildet werden. Zonen über mehrere Geräte brauchen Taktangleichung (SND.15) | |
| S-E.5 | Cue-Modell | Bestehenden `Cue` umbauen, ohne Rückwärtsverträglichkeit; vorhandene Shows werden beim Laden migriert (CUE.21, SSH.1) | Die Oberfläche zeigt Felder und Funktionen je nach Modus oder verbirgt sie, Seltenes hinter „Erweitert“ (SUI.13) |
| S-E.6 | Patches in Show oder Rechner | Rechner-Einstellungen (SSH.3); Abweichung vom Vorbild, damit eine Show ohne Änderung auf anderer Hardware läuft | Show-Datei auf Wunsch samt Rechner-Einstellungen ex- und importieren (SSH.5) |
| S-E.7 | Windows | Bleibt ausgeschlossen (NFA.1); Show-Modus erbt die Plattform | Auf die Stärken von Linux konzentrieren, nicht auf die Schwächen |
| S-E.8 | Name des Modus | „Show“; der Mischer heißt „Mixer“, deutsch „Mischer“ (Oberfläche zuerst Englisch) | |
| S-E.9 | Bühnen in S1 | Mehrere Bühnen, je auf Vollbild oder Fenster (VIS.1); eine Bühne über mehrere Projektoren mit Teilbereichen und Kantenüberblendung erst S2/S3 (VIS.11, VIS.16) | |
| S-E.10 | Mischer und Show auf denselben Ausgängen | Ein Bildausgang gehört zu jeder Zeit entweder dem Mischer (Program, Ausgang 1–4) oder einer Bühne; umschaltbar per Aktion. Eine Bühne kann als Input im Mischer erscheinen (VIS.17) | Show und Mischer laufen je für sich allein. Ziel ist die gemischte Theatershow: feste Teile fährt die Show, in freien, improvisierten Teilen übernimmt der Mischer |
| S-E.11 | Licht-Patch | Geräte und Gruppen in der Show, Adressen und Ausgabeweg pro Rechner (LGT.3), wie Zonen und Bühnen | Wie S-E.6: Export und Import samt Rechner-Einstellungen (SSH.5) |
| S-E.12 | Plugin-Isolation | Ein gemeinsamer Plugin-Host-Prozess (FX.6); ein Plugin-Absturz trifft die Plugins, nie Engine oder Ablauf. Lizenz der Plugin-SDKs gegen NFA.9 prüfen | In Grenzen; denkbar ist auch ein Host je Bus bzw. Zone |

### 6.2 Offen

| # | Frage | Stand |
|---|---|---|
| S-E.3 | Klang-Cues intern: verdeckte strom-Inputs oder ein Abspieler je Zone | Zurückgestellt (User 2026-10-16, Folgen noch unklar). Festgelegt ist nur SND.17. Vor S1 Stufe 2 beide Wege messen (5–10 Klänge zugleich: Startzeit GO → Ton, Speicher, CPU) und mit Zahlen erneut vorlegen |

---

## 7. Meilensteine

### S1 — Spieltisch (Show-Modus für eine Pen-&-Paper-Kampagne)

Alles, was eine Spielleitung für ein Minitheater braucht: Projektion hinter
der Spielleitung, Klang vorn und hinter der Spielerin, Licht und Duft über
Home Assistant, Bedienung über Companion, Drehbuch in Obsidian.

**Umfang** (alle Anforderungen mit MS = S1):
CUE.1–4, 7–11, 19–21 · RUN.1–4, 6–8, 11 · SND.1–6, 8, 10–17 · VIS.1–8 (ohne VIS.1a) ·
EXT.1–5 · IMP.1–6, 8, 9 · CTL.1–6 · SAF.1–7 · SUI.1–9, 13 · SSH.1–3, 5 · SNF.1–6.

Licht in S1 nur über Home Assistant (EXT.4); eigene Lichtsteuerung ab S3.

**Voraussetzungen aus `LASTENHEFT.md`**: OUT.3 (Vollbild), AUD.14
(Pegel-Fades), API.5 (Abo), API.7 (Sicherheit).

**Reihenfolge innerhalb von S1** (jede Stufe abnahmefähig):
1. Cue-Modell und Ablauf-Engine in `lmix-core` (CUE, RUN, SSH.1), Tests.
2. Zonen mit Rechner-Zuordnung, Klang-Cues, Fades mit S-Kurve, Grenzen,
   Begrenzer (SND, SSH.3).
3. Bühne, Bild-Cues mit Ebenen, Bild-Fades, Bühnen-Dimmer (VIS).
4. Ausgabeziele, HTTP-Cue, Geheimnisse, Home Assistant (EXT).
5. Panik, harter Stopp, Notfall-Liste (SAF); Ereignis-Abo, feste
   Abonnenten, Zugriffsschutz (CTL).
6. Drehbuch-Import (IMP).
7. Show-Ansicht in der Console (SUI).

**Abnahme S1** (Persona P8, §9):
- Ein Drehbuch-Ordner mit einer Session-Datei, darin rund 25 Cues in
  Szenen gegliedert, wird importiert; Fehler zeigen Datei und Zeile.
- Szenenwechsel als Gruppe (Zeitleiste): Licht-Szene per HTTP an Home
  Assistant; neue Kulisse auf Ebene 1 blendet über 4 s ein, die alte blendet
  aus und stoppt; die Atmo der alten Szene blendet über 3 s mit S-Kurve aus
  und stoppt (CUE.11); nach 2 s Vorwartezeit startet die neue Atmo als
  Schleife in Zone „vorn“; ein Rauch-Video liegt im Mischmodus „aufhellen“
  auf Ebene 2.
- Eine Stereo-Stimme läuft als Mono nur in Zone „hinten“ auf einem zweiten
  Gerät und bleibt 4 h synchron zu „vorn“.
- GO und Panik über Companion; Standby-Name und Restzeit erscheinen auf
  Companion-Tasten über einen festen OSC-Abonnenten; ein doppelt gedrücktes
  GO wird verworfen und angezeigt.
- Drehbuch wird während des Ablaufs geändert und neu geladen: Standby
  bleibt, laufende Atmo läuft weiter.
- Panik über einen physischen Taster ohne Schlüssel (nur Panik freigegeben):
  Stille nach ≤ 1,1 s, Bühne schwarz, Notfall-Liste schaltet Raumlicht an;
  zweite Panik wirkt sofort.
- Eine fehlende Datei in einer Szenengruppe bricht den Szenenwechsel nicht
  ab und steht in der Warnliste.
- 4 h Dauerlauf mit zwei Schleifen und Bühnenvideo ohne Klick und Drift.
- Persona-Durchlauf P8, P9, P10, P7 und einer weiteren (NFA.15).

### S2 — Theater-Grundstand

Bearbeiten in der Oberfläche, Gruppen „hinein“ und Wiedergabeliste,
unscharfe Cues, Springen-Cue, Zweitauslöser, Schleife beenden, direkte
Auslöser, Pause, Vorhören, Sperren je Ein-/Ausgabeklasse, Raster, relative
Fades, weitere Kurven, Start-/Endpunkte, Kreuzungs-Fades, freie Geometrie,
Videoton, virtuelle Bühne, HTML-Ebene (nach GFX.2), Effekt-Plugins je Cue
und Zone (FX), OSC-Cue, Platzhalter,
Variablen, Companion-Modul, Web-Fernbedienung, Lernen, Protokoll,
Show-Sperre, Vorlagen, Paket.

**Abnahme S2** (zusätzlich): Persona P7 fährt eine gemischte Vorstellung, feste
Szenen per Show, eine improvisierte Szene im Mischer mit der Bühne als Input
(VIS.17).

**Abnahme S2**: Persona P9 spielt eine Theatervorstellung mit 120 Cues ohne
Drehbuch-Werkzeug, nur in der Oberfläche angelegt.

### S3 — Erweiterungen: Licht, Objekt-Audio, Timecode

Eigene Lichtsteuerung über Art-Net/sACN mit Geräten, Gruppen, Licht-Cues,
Befehlssprache und Licht-Pult (LGT); Objekt-Audio mit Klang-Karten und
Fade-Pfaden (OBJ); Timecode rein und raus (TC). Dazu Gruppe Zufall,
Pause-/Laden-/Zurücksetzen-Cues, Start ab Zeitpunkt, Zustand nachziehen,
Abschnitte, Pegelkurve, Live-Ton und Live-Bild, Text-Cue, Videoeffekte,
Verzerrung, Netz-Cue mit Dauer, MIDI/MSC, bedingte Cues.

**Abnahme S3**: Persona P9 fährt eine Vorstellung mit eigenem Licht über
Art-Net (20 Geräte, Farbe, Schwenk), einem Klangobjekt, das über einen
Pfad durch vier Zonen wandert, und einer Szene, die auf eingehenden LTC
läuft; Sprung in die Mitte ergibt dasselbe Lichtbild (LGT.8).

---

## 9. Personas für den Show-Modus

Testmethode wie `LASTENHEFT.md` §9 (NFA.15). Drei Personas für den
Show-Modus stehen in `PERSONAS.md`: P8 Spielleitung am Spieltisch, P9
Tondesign an einer freien Bühne, P10 Ausstellungs- und Erlebnistechnik.
Show-Abnahmen spielen sie zusammen mit P7 (Theater-Video) durch.

---

## 8. Quellen

- QLab-5-Handbuch, <https://qlab.app/docs/v5/> (abgerufen 2026-10-16,
  97 Seiten; lokal außerhalb des Repos). Zuordnung Anforderung →
  Handbuchseite: `LASTENHEFT-SHOW-QUELLEN.md`.
- Bestand lmix: `LASTENHEFT.md`, `ROADMAP.md`, `lmix-types/src/automation.rs`,
  `lmix-types/src/action.rs` (Stand 2026-10-16).
