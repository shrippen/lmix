# Lastenheft: lmix als Clean-Room-Reimplementierung von vMix

Stand 2026-10-08. Einzige Grundlage: vMix User Guide 29
(<https://www.vmix.com/help29/>). Ergänzt `claude.md` und `ROADMAP.md`; Abweichungen von
dort stehen in §6 zur Entscheidung, nicht stillschweigend.

Dieses Dokument beschreibt **was** lmix können muss, nicht **wie**. Es ist
die einzige Funktionsquelle für die Umsetzung (siehe §0).

**Leitlinie**: vMix ist die Vorlage, kein Zwang. lmix weicht ab, wo es
sinnvoll ist: Altlasten fallen weg oder werden durch Heutiges ersetzt
(§5a), ein sauberer Schnitt zu vMix-Namen und -Formaten ist zulässig. Jede
Abweichung steht mit Grund in diesem Dokument.

**Show-Modus**: Die Seite Show (Showrunner nach QLab-Vorbild; eine App mit
Seiten, DECISIONS 2026-10-23)
hat ein eigenes Lastenheft, `LASTENHEFT-SHOW.md` (IDs CUE, RUN, SND, VIS,
EXT, VAR, IMP, CTL, SAF, SUI, SSH, LGT, OBJ, FX, TC; Meilensteine S1–S3).
Es setzt auf diesem Lastenheft auf und ändert es nicht.

---

## 0. Clean-Room-Regeln

Ziel: funktional gleichwertig, rechtlich unabhängig.

| # | Regel |
|---|---|
| CR.1 | Quelle für dieses Lastenheft: nur das öffentliche vMix-29-Handbuch. Andere Handbuchversionen, Herstellerseiten, Foren und Drittsoftware werden nicht ausgewertet. Kein Disassemblieren, Dekompilieren oder Debuggen von vMix-Programmen, kein Mitschnitt interner Protokolle von vMix-Komponenten untereinander. Ausnahme: Programme, die an vMix andocken (Bitfocus Companion, Stream-Deck-Plugins, Tally-Software), dienen als Referenz dafür, was die lmix-API leisten muss (API.9); sie sind keine Quelle für vMix-Verhalten, Code wird nicht übernommen. |
| CR.2 | Anforderungen in eigenen Worten. Kein Handbuchtext, keine Screenshots, keine vMix-Grafiken im Repo. Die heruntergeladene Hilfe bleibt außerhalb des Repos. |
| CR.3 | Umsetzung arbeitet nur gegen dieses Lastenheft, `ROADMAP.md` und offene Standards. Wer Code schreibt, liest dafür nicht im vMix-Handbuch nach; Lücken im Lastenheft werden hier ergänzt (mit Quelle), dann umgesetzt. |
| CR.4 | Keine vMix-Assets: keine Titelvorlagen, Stinger, LUTs, Virtual Sets, Icons, Sounds, Shortcut-Vorlagen. Eigene Vorlagen werden neu gestaltet. |
| CR.5 | Eigene Namen in Oberfläche und Dateien: kein "vMix", "GT", "vMix Call", "MultiCorder", ".vmix". Begriffe siehe §1. Der Vergleich mit vMix steht nur in Doku. |
| CR.6 | lmix hat eine eigene API (§3.15). Ein vMix-Kompatibilitätsmodus ist optional (API.8, K); falls gebaut, nur aus den Handbuchseiten zu HTTP-/TCP-API und Funktionsreferenz. |
| CR.8 | Ergänzungen, die nicht aus dem Handbuch stammen (Linux-Ersatz für Windows-Technik, eigene Ziele), sind mit **(lmix)** markiert. |
| CR.7 | Jede Anforderung trägt eine ID. Commits, die eine Anforderung umsetzen, nennen sie (`Implements AUD.12`). |

---

## 1. Begriffe

| lmix-Begriff | Bedeutung | vMix-Pendant |
|---|---|---|
| Input | Jede Quelle im Projekt: Kamera, Datei, Netz, Titel, Mix, Gast … Hat Nummer, Name, feste ID, Bild, Ton, Einstellungen | Input |
| Preview (PVW) | Nächster Input, vorbereitet | Preview |
| Program (PGM) | Sichtbarer Haupt-Input | Output / Active |
| Take | Preview → Program mit Übergang | Transition |
| Übergang | Effekt + Dauer für einen Take | Transition Effect |
| Stinger | Übergang aus einer Animation mit Schnittpunkt | Stinger |
| Overlay-Kanal | Eine von 8 Ebenen über Program | Overlay 1–8 |
| Ebene (Layer) | Ein Input als Teil eines anderen Inputs, mit Position/Crop | Layer / MultiView (alt) |
| Mix | Zusätzlicher Mini-Mischer mit eigenem PVW/PGM, als Input nutzbar | Mix Input |
| Ausgang 1–4 | Gerenderte Signale mit wählbarer Quelle | Output 1–4 |
| Senke | Ziel eines Ausgangs: Vollbild, Karte, Stream, Aufnahme, Netz | Fullscreen, External, … |
| Bus | Audio-Summe: Master, A–G | Bus M, A–G |
| Abhörweg | Kopfhörer-Summe mit Solo | Headphones |
| Einzelaufnahme | Rohaufnahme je Input | MultiCorder |
| Wiederholung | Ringpuffer-Aufnahme + Ereignisse + Abspielkanäle | Instant Replay |
| Aktion | Benannte Funktion mit Parametern; alles Steuerbare ist eine Aktion | Shortcut Function |
| Tastenbelegung | Eingabe (Taste, MIDI, …) → Aktionen | Shortcut |
| Rückmeldung | Zustand → Lampe/Motorfader am Controller | Activator |
| Auslöser | Ereignis eines Inputs → Aktionen | Trigger |
| Ablaufliste | Zeitgesteuerte Folge von Inputs | PlayList |
| Titel | Grafikvorlage mit Feldern und Animationen | Title / GT |
| Datenquelle | Tabelle aus Datei/Netz, an Titelfelder gebunden | Data Source |
| Gast | Entfernter Teilnehmer per Browser | vMix Call |
| Show | Projektdatei mit allen Inputs und Einstellungen | Preset |
| Tally | Signal "auf Sendung"/"in Vorschau" je Input | Tally |

---

## 2. Systemmodell

### 2.1 Bild

```
 Quellen ─► Input ─► Input-Verarbeitung ─────────────┐
            (Decode,   (Deinterlace, Spiegeln,      │ als Ebene in anderen
             Formate)   Farbe, Key, Effekte,        │ Inputs (bis 10, 2 Ebenen
                        Position/Crop, Ebenen)      │ tief), in Mix-Inputs,
                                                    ▼ in Overlays
   ┌──────────── Hauptmischer ─────────────┐   ┌── Mix 2…16 ──┐
   │ PVW ──[Cut/Übergang/T-Bar/Stinger]─► PGM │   │ PVW ─► PGM   │──► als Input
   └───────────────┬────────────────────────┘   └──────────────┘
                   ▼
         Overlay-Kanäle 1–8 (je Ein-/Ausblendung, PiP)
                   ▼
         FTB (wirkt auf Senken, nicht auf die PGM-Anzeige)
                   ▼
   Ausgang 1 (= Program)   Ausgang 2–4 (Quelle wählbar:
                           Program, Preview, Multiview 1/2, Mix n, Input n;
                           Overlays je Ausgang abwählbar = "clean feed")
                   ▼
   Senken: Vollbild 1–2 · Karte (SDI/HDMI, Key+Fill) · Stream 1–5 ·
           Aufnahme 1–2 · NDI/OMT/SRT · virtuelle Kamera · Gäste-Rückweg
```

### 2.2 Ton

```
 Input-Ton (bis 16 Kanäle) ─► Kanalmischer ─► Matrix 16×16 ─► Gain/EQ/Comp/Gate/Plugins
                                                   │
                                Fader, Mute, Balance, Delay
                                                   ▼
                     Sends je Bus: M · A · B · C · D · E · F · G
                         │                    │
                         ▼                    └─► Bus A–G optional → Master
                 Master ─► Aufnahme/Stream/Karte (Mehrkanal-Kombis M, MA, MAB …)
                 Abhörweg = Master, außer Solo (AFL/PFL)
                 Gäste-Rückweg = gewählter Bus minus eigener Gast (automatisch)
```

### 2.3 Steuerung

Alle Bedienwege rufen dieselben Aktionen auf:

```
 GUI · Tastatur · MIDI/HID · Web-Fernbedienung · HTTP-API · TCP-API ·
 Auslöser · Ablaufliste · Skript
        └──────────────► Aktionskatalog ──► Show-Zustand ──► Engine
                                                │
                       Zustandsereignisse ◄─────┘ (Tally, Rückmeldung,
                                                   XML/JSON-Zustand, GUI)
```

Folge: Der Aktionskatalog (§3.15) ist die zentrale Schnittstelle. Die GUI
ist ein Client dieses Katalogs, kein Sonderweg.

---

## 3. Funktionale Anforderungen

Priorität: **M** Muss (Kern für Live-Produktion), **S** Soll (Gleichstand
mit vMix), **K** Kann, **L** später (bewusst zurückgestellt, Entscheidung
2026-10-08), **X** ausgeschlossen (§5).
Stand lmix: ✅ vorhanden · ◐ teilweise · – fehlt. "E" = Engine (strom)
kann es, Console nicht.

### 3.1 Input-Modell (INP)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| INP.1 | Inputs zur Laufzeit hinzufügen, entfernen, ersetzen, ohne Aussetzer auf Program | M | ✅ |
| INP.2 | Jeder Input hat laufende Nummer (1…n), Namen (änderbar), feste ID (UUID); Ansprache in Aktionen über alle drei | M | ✅ |
| INP.3 | Reihenfolge per Drag & Drop ändern; Nummern folgen der Reihenfolge | M | ✅ |
| INP.4 | "Quelle ändern": neue Quelle, alle Einstellungen bleiben | S | ✅ |
| INP.5 | Virtueller Input: Kopie, die Bild der Quelle teilt, eigene Einstellungen hat | S | – |
| INP.6 | Input schließen mit Rückgängig | S | ✅ |
| INP.7 | Kategorien: 6 feste Farben + 16 eigene, Beschriftung, Filter in der Input-Leiste, Suchkategorie nach Namen | S | ◐ (7 feste Farben; ohne eigene Farben, Beschriftung, Filter, Suche) |
| INP.8 | Klick-Aktion je Input wählbar (Standard: in Preview) | S | ✅ |
| INP.9 | GO-Aktion je Input wählbar (Standard: Schnellstart, §3.5) | S | ✅ |
| INP.10 | Wiedergabe-Inputs: Play, Pause, Neustart, Positionieren, Schleife, Geschwindigkeit, In-/Out-Marke | M | ◐ (Play, Pause, Neustart, Positionieren; Schleife oder Stehenbleiben am Ende je Input; ohne Geschwindigkeit, In-/Out-Marke) |
| INP.11 | Automatik je Input: bei Take abspielen / von vorn starten / nach Wegschalten pausieren | S | ✅ |
| INP.12 | Live-Pause: Live-Quelle einfrieren, ohne Aufnahme/Wiederholung/Ton anzuhalten | S | – |
| INP.13 | Offline-Zustand bei fehlender Quelle; Wiederverbinden; Datei neu zuordnen | M | ✅ |
| INP.14 | Seitenverhältnis je Input überschreiben | S | – |
| INP.15 | Feste Bildverzögerung je Live-Input in Frames | S | – |
| INP.16 | Input als Schnappschuss (PNG/JPEG, Alpha erhalten) | K | – |
| INP.17 | Bis 1000 Inputs im Projekt (Länge der Tally-Liste der TCP-API); Live-Dekodierung nur durch Hardware begrenzt, Ziel ≥ 16 Live-Quellen gleichzeitig (lmix) | M | ◐ (16 × 1080p30 gemessen; ein Input je Mischer-Slot, Standard 16, `--slots`) |

### 3.2 Quelltypen (SRC)

| ID | Quelle | Details | Prio | Stand |
|---|---|---|---|---|
| SRC.1 | Capture-Gerät | V4L2, DeckLink (SDI/HDMI), Format/Auflösung/Bildrate/Interlaced/PsF wählbar, eingebetteter Ton bis 16 Kanäle | M | ◐ (V4L2 über die Geräteliste, Auflösung und Bildrate wählbar (2026-10-25); ohne Pixelformat (strom bietet es nicht), ohne Interlaced/PsF, ohne eingebetteten Ton; DeckLink-Karte als Input mit Modus oder automatisch, Halbbilder entflochten, eingebetteter Ton Stereo (2026-10-27, H.7 SDI-Loop bestanden); ohne PsF, ohne 16 Kanäle) |
| SRC.2 | Videodatei | gängige Container/Codecs (MP4, MOV, MXF, MKV, TS, AVI), Alpha (ProRes 4444, QT-Animation, VP9), Interlaced erkannt | M | ✅ |
| SRC.3 | Liste | mehrere Video-/Audiodateien als ein Input; Auto-Weiter, Auto-Erstes, Schleife, Mischen, Einträge abwählen/umsortieren, Gesamtrestzeit, M3U ein/aus | S | ◐ (Liste mit Weiter/Zurück, Eintrag wählen, Schleife; ohne Mischen, Abwählen, Restzeit, M3U) |
| SRC.4 | Bild | PNG, JPEG, TIFF, BMP, WebP; Alpha | M | ✅ (Alpha bei PNG, WebP, TIFF in Programmgröße; geprüft mit PNG auf dem CPU-Mischer, GPU H.33) |
| SRC.5 | Fotos | Ordner als ein Input; vor/zurück; Diashow mit Intervall und Übergang | S | – |
| SRC.6 | Bildsequenz | nummerierte Bilder als Animation mit Alpha; Begleitton gleichen Namens; bei Interlaced ein Bild je Halbbild | S | – |
| SRC.7 | Farbe, Testbild | Volltonfarbe, Farbbalken | M | ✅ |
| SRC.8 | Audiodatei | MP3, WAV, FLAC, AAC | M | ◐ (über den Datei-Input, ungeprüft) |
| SRC.9 | Audiogerät | ALSA/PipeWire/JACK (lmix); Kanal oder Stereopaar; System-Ton; Ton einer Anwendung | M | ◐ (Audio-Gerät über PipeWire in der Console; ohne Kanalwahl, System-/Anwendungston; H.16 offen) |
| SRC.10 | NDI | Suche mit Vorschaubildern, nach Gerät gruppiert; niedrige Bandbreite; nur Ton; Puffer erhöhen; Alpha; Quelle per Menü wechseln | M | ◐ (Auswahl aus der Geräteliste; ohne Vorschaubilder, Gruppierung, Bandbreite, nur Ton, Puffer; H.14 offen) |
| SRC.11 | OMT | wie NDI; zusätzlich Qualitätswunsch an Sender | M | – (kein strom-Block) |
| SRC.12 | SRT | Caller/Listener/Rendezvous, Latenz, Decoder-Verzögerung, Passphrase + Schlüssellänge, Stream-ID, HW-Decode, H.264/HEVC + AAC in TS, Mehrspur-Ton zu Mehrkanal | M | ◐ (SRT-Adresse in der Console, Optionen nur in der URI; H.23 offen) |
| SRC.13 | Netzstreams | RTSP (UDP/TCP), RTMP, TS über UDP/TCP (Server und Pull), HLS; Puffer; Niedriglatenz-Modus | S | ◐ (URL über den Media-Player: http(s), RTSP; ohne Puffer, Niedriglatenz, TS-Server) |
| SRC.14 | WebRTC/WHIP | eingehender WHIP-Stream (lmix) | S | ◐ (Console: WHIP-Adresse und Browser-Seite; auf Hardware ungeprüft, H.15) |
| SRC.15 | Webseite | Chromium-Rendering, URL, Größe, eigenes CSS, transparenter Hintergrund, Ton, Maus/Tastatur-Bedienung, zurück/vor/neu laden | S | ◐ (CEF E, eigener Build) |
| SRC.16 | Titel | §3.11 | M | ◐ (Titelkanäle, Vorlagen, Daten; Grafikkanal als Input über strom `html_input`, mit Chromium ungeprüft) |
| SRC.17 | Bildschirm lokal | Bildschirm oder Fenster dieses Rechners (PipeWire-Portal) | S | – |
| SRC.18 | Bildschirm entfernt | Fenster/Bildschirm anderer Rechner per NDI/OMT (Fremdwerkzeug genügt) | K | – |
| SRC.19 | Video-Verzögerung | Ringpuffer eines Inputs oder von Program, Länge in s, Abspieltempo (Zeitlupe), Ausschnitt als Clip speichern → Liste oder neuer Input | L | – |
| SRC.20 | Wiederholungskanal | §3.10 | L | – |
| SRC.21 | Mix | §3.6 | S | – |
| SRC.22 | Program als Input | Ausgang 1 als Input (nur einmal; Selbstbezug liefert Standbild) | S | – |
| SRC.23 | Preview als Input | aktuelle Preview als Input | S | – |
| SRC.24 | Gast | §3.14 | L | – |
| SRC.25 | Zeichenebene | transparente Ebene, im Browser bemalbar (§3.17) | K | – |
| SRC.26 | Produktionsuhren | §3.18 als Input | K | – |
| SRC.27 | Virtuelles Set | Keying-Talent in 3D-Szene mit Kamerafahrten | X | – |
| SRC.28 | Präsentation | Folien als Bilder (über LibreOffice-Export) | S | – |
| SRC.29 | DVD, Flash, WMV, DirectShow, XAML/WPF | | X | – |
| SRC.30 | Zoom-Meeting | | X | – |
| SRC.31 | Bildschirm teilen per Browser | Vortragende teilen Bildschirm oder Fenster über eine Webseite von lmix (WebRTC), ohne Installation; Zugang per Link/Code (P2) | S | – |

### 3.3 Input-Verarbeitung (PRC)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| PRC.1 | Position: Zoom, Pan X/Y, Rotation (auch 3D-Achsen), Crop links/rechts/oben/unten; auflösungsunabhängig gespeichert (Zoom 1 = 100 %, Pan in Bildbreiten) | M | ◐ (nur Rechteck und Zuschnitt einer Layout-Box; ohne Zoom/Pan/Rotation je Input) |
| PRC.2 | Quellrotation in 90°-Schritten mit Auto-Zoom (Hochkant-Kameras) | S | – |
| PRC.3 | Spiegeln, Deinterlace (Blend/Verwerfen/besser), Schärfen | S | ◐ (Deinterlace GPU) |
| PRC.4 | Farbanpassung: Schwarz/Weiß-Dehnung, R/G/B, Alpha, Sättigung, vormultipliziertes Alpha, 16–235 → 0–255 | S | ◐ (nur Helligkeit, Kontrast, Sättigung über Looks) |
| PRC.5 | Farbkorrektur: Lift/Gamma/Gain je Kanal + Luma, Farbton, Sättigung, Auto-Korrektur, Reset, Presets ex-/importieren | S | ◐ (Looks in der Console: Helligkeit, Kontrast, Sättigung, Farbton, Gamma, Temperatur, Tönung, Reset; nur GL-Mischer; ohne Lift/Gain je Kanal, Auto, Presets) |
| PRC.6 | Farbkey + Chroma-Key mit Auto-Farbwahl, Toleranz, Spill-Unterdrückung, Kantenglättung, Presets | M | ◐ (E, GPU) |
| PRC.7 | Luma-Key | S | – |
| PRC.8 | Key/Fill: anderer Input liefert Alpha | S | – |
| PRC.9 | Effektkette je Input, sortierbar, einzeln abschaltbar, je Effekt Ausschnitt: Weichzeichner, 3D-LUT (.cube, PNG-LUT) mit Stärke, Alpha-Maske aus PNG | S | – |
| PRC.10 | Ränder um Input/Ebene: Farbe, Dicke, Eckradius | S | ◐ (nur Layout-Boxen: Rahmen an/aus, weiß; ohne Farbe, Eckradius) |
| PRC.11 | Einstellungen von anderem Input kopieren (Auswahl der Bereiche); Position kopieren/einfügen | S | – |
| PRC.12 | Alle Parameter per Aktion setzbar, Position/Zoom/Crop auch animiert über Zeit | M | ◐ (vorhandene Parameter per Aktion; ohne Animation) |

### 3.4 Ebenen (LAY)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| LAY.1 | Bis 10 Ebenen je Input, jede ein anderer Input; Z-Reihenfolge per Nummer | M | ◐ (bis 4 Layouts: Hintergrund + 8 Boxen, Reihenfolge nach Boxnummer; nicht je Input) |
| LAY.2 | Je Ebene Position, Crop, Rand, sichtbar an/aus, Rotation 90° | M | ◐ (Rechteck, Zuschnitt, Rahmen, Sperre, sichtbar an/aus; ohne Rotation) |
| LAY.3 | Verschachtelung: Ebenen von Ebenen werden gerendert (mind. 2 Stufen) | S | – |
| LAY.4 | "Ebenen flach rechnen": Input samt Ebenen als ein Bild für Weiterverwendung | K | – |
| LAY.5 | Layout-Vorlagen: speichern, laden, ex-/importieren, außerhalb der Show | S | ✅ |
| LAY.6 | Ebenen-Editor: Maus verschieben/skalieren/croppen, Mehrfachauswahl, Fangen an Kanten/Mitte/Schutzbereichen/anderen Ebenen, Sperre, Rückgängig/Wiederholen, Pixelwerte zur Ausgabegröße | S | ✅ (Zuschnitt per Zahl, nicht per Maus) |
| LAY.7 | Ebene ein/aus, tauschen, Inhalt wechseln und Rechteck setzen per Aktion, auch animiert | S | ◐ (Inhalt und Rechteck per `set_layout`; ohne Ein/Aus je Box, Tauschen, Animation) |

### 3.5 Schalten (SWI)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| SWI.1 | Preview/Program-Paar; Input in Preview per Klick, Taste, Aktion | M | ✅ |
| SWI.2 | Cut (Tausch PVW/PGM) und Cut direkt (Input sofort auf PGM) | M | ✅ |
| SWI.3 | 4 Übergangstasten, je Effekt + Dauer einstellbar; Taste 1 steuert T-Bar | M | ✅ (T-Bar mit dem Effekt von Taste 1 seit 19.10.: Blende, Dip, Schieben, Push; Schnitt, Wischen und Stinger als Blende) |
| SWI.4 | Effektkatalog: Cut, Blende, 3D-Zoom, Schieben, Wischen, Würfel, Würfelzoom, Fliegen, Fliegen+Drehen, Kreuzzoom, Verschmelzen (SWI.6), Stinger 1–8 | M | ◐ (Cut, Blende, Dip, Schieben, Push, Wischen, Stinger; ohne 3D-Zoom, Würfel, Fliegen, Kreuzzoom, Verschmelzen) |
| SWI.5 | T-Bar: manueller Fortschritt 0–100 %, per Maus, MIDI-Fader, Aktion | M | ◐ (Maus, MIDI, Aktion; Ton mit „folgt Bild“ blendet mit; Effekt von Taste 1 seit 19.10., SWI.3; bei Fade to Black gesperrt; nur Inputs, kein Layout auf Program oder Vorschau) |
| SWI.6 | Verschmelzen: Ebenen, die in PVW und PGM vorkommen, wandern an ihre neue Position; Rest blendet | S | – |
| SWI.7 | Stinger 1–8: Animations-Input, Dauer, Schnittpunkt, Zusatzeffekt, unter oder über Overlays; Dauer/Schnittpunkt aus Titelanimation übernehmbar | S | ◐ (4 Tasten, Clip + Schnittpunkt, über strom bildgenau; Dauer = Cliplänge; nur unter Overlays, nur zwischen Inputs (keine Layouts), ohne Zusatzeffekt, ohne Titelanimation; Track-Matte nur auf GL, DECISIONS 18.10. STG) |
| SWI.8 | Schnellstart: Take mit eigenem Effekt (Standard ½ s Blende) und Start der Wiedergabe; optional per Klick ins Preview-Fenster | S | ✅ (Doppelklick ins Preview-Fenster) |
| SWI.9 | FTB: blendet Senken auf Schwarz (optional mit Ton), Program-Anzeige bleibt bedienbar | M | ✅ |
| SWI.10 | Mix-Inputs (E.11: Umfang wie vMix): bis 15 zusätzliche Mischer mit PVW/PGM, Cut/Übergang, Overlays und Stinger je Mix wählbar; "nur Ausgang"-Ansicht; Mix in Mix ohne Verzögerung bei aufsteigender Reihenfolge | S | – |
| SWI.11 | Alle Wiedergabe-Inputs halten während Übergang ihr Timing (keine Ruckler, keine Ton-Lücke) | M | ✅ |
| SWI.12 | Interne Latenz Input → Program ≤ 2 Frames (ohne Quell- und Senkenlatenz) | M | ◐ (< 5 Frames gemessen) |

### 3.6 Overlays (OVL)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| OVL.1 | 8 Overlay-Kanäle über Program; ein Input je Kanal; Alpha und Key des Inputs gelten | M | ✅ |
| OVL.2 | Je Kanal: Typ (Vollbild/PiP mit Position), Ein-/Ausblendeffekt + Dauer (rückwärts beim Ausblenden), Auto-Aus nach Zeit, Rand | M | ◐ (Effekt nur Schnitt/Blende, ohne Rand: Engine) |
| OVL.3 | Overlay in Preview vorbereiten; geht beim nächsten Take mit auf Program | S | ◐ (Tally, nicht im Vorschau-Bild: Engine) |
| OVL.4 | PiP-Overlay per Aktion auf Vollbild vergrößern und zurück | K | – |
| OVL.5 | Overlay je Ausgang abwählbar (Clean Feed) | M | – |
| OVL.6 | Overlays auch auf Mix-Inputs (Aktion mit Mix-Liste) | S | – |
| OVL.7 | Titel als Overlay: Ein-/Ausblendung nutzt die Titelanimation statt Kanaleffekt | S | – |
| OVL.8 | "Alle Overlays aus"; letzter Input je Kanal wiederholen | S | ✅ |

### 3.7 Audio (AUD)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| AUD.1 | Jeder Input mit Ton erscheint als Kanalzug: Fader, Mute, Pegelanzeige L/R | M | ✅ (Pegel links und rechts auf der Kachel, `channels_db` in den Pegeln) |
| AUD.2 | Busse: Master und Kopfhörer fest, dazu bis 16 frei angelegte, benannte Busse; Send je Input und Bus; Abgriff vor/nach dem Fader je Bus; Bus optional in Master; Bus-Ausgang auf Gerät (später über eine Zone, S-E.4) oder nur intern | M | ◐ (Master mit Pegel und Stumm; Busse anlegen, umbenennen, entfernen, Abgriff, Pegel, Stumm, Abhören; Send je Input; Bus auf Ausgang; Bus nicht in den Master; Hinzufügen, Entfernen und Abgriffwechsel zur Laufzeit, ohne Neuaufbau) |
| AUD.3 | Sendepegel je Input und Bus (Bus-Mixer) | S | ✅ (Send je Input auf jeden Bus und den Kopfhörer, Matrix auf der Audio-Seite und im Kanalzug) |
| AUD.4 | Kopfhörer (Abhörweg) mit eigener Lautstärke je Input und gesamt; Solo AFL (Standard) / PFL, mehrfach-Solo, zerstörungsfrei; einen Bus abhören | M | ✅ (Send je Input an/aus mit Pegel, Abgriff vor/nach den Fadern, Lautstärke und Stumm gesamt; AFL als Standard, PFL im Menü, Mehrfach-Solo und Bus-Abhören schalten den Kopfhörer auf die Solos) |
| AUD.5 | Audio folgt Video: Take schaltet Ton der beteiligten Inputs; je Input an/aus; global an/aus | M | ✅ |
| AUD.6 | Gain (dB), Balance, Delay (ms), Standard-Delay für neue Inputs | M | ✅ (Gain, Pan, Delay; Standard-Delay für neue Inputs, `set_default_delay`) |
| AUD.7 | Kanalwahl: Mono links/rechts, Stereo, getrennt Mono mit eigenen Fadern | S | – |
| AUD.8 | Kanalmischer bis 16 Kanäle mit Pegel und Anzeige | S | – |
| AUD.9 | Kanalmatrix 16 × 16 (Quelle → Bus-Kanal), Presets speichern/anwenden | S | – |
| AUD.10 | EQ, Kompressor, Noise Gate je Input und Bus | M | ◐ (EQ, Gate, Kompressor, Hochpass je Input auch in der Console; nicht je Bus) |
| AUD.11 | Plugins je Input und Bus: LV2, VST3, CLAP (E.8), Reihenfolge, an/aus, Editor-Fenster | S | – |
| AUD.12 | Pegelanzeigen in dBFS, 5 Farbbänder, Spitzenwert über ~2 s, vor/nach Fader, gedämpft-Anzeige | M | ◐ (dBFS, Spitzenwert hält 2 s, stumm = blass, links/rechts; drei statt fünf Farbbänder (Kante-Meter), nur vor dem Fader) |
| AUD.13 | Bus-Übersicht: Matrix Inputs × Busse in einem Fenster, filterbar | S | – |
| AUD.14 | Pegel-Fades per Aktion (Ziel, Dauer) für Input, Master, Busse | S | ✅ (`fade_volume`, bis 60 s, linear im Pegel; Kurven mit SND.6) |
| AUD.15 | Mehrkanal-Kombinationen für Senken: Master, einzelne Busse, MA, MAB, MABC, alle 16 Kanäle | S | – |
| AUD.16 | Taktabweichung je Quelle: Lücke füllen oder resamplen (wählbar) | M | – |
| AUD.17 | Abtastrate 48 kHz (44,1 kHz wählbar); intern Gleitkomma | M | ✅ (48 kHz, 44,1 kHz wählbar je Show; Inputs werden im Kanalzug umgerechnet; intern Gleitkomma) |
| AUD.18 | -20-dB-Pad je Input; +20 dB je Netzausgang | K | – |
| AUD.19 | Mixer-Fenster abdockbar; Kanalzüge ein-/ausblendbar; nach Kategorie filtern | S | – |
| AUD.20 | Sprachbefehle als Tastenbelegung | X | – |

### 3.8 Ausgänge und Monitoring (OUT)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| OUT.1 | Master-Format: Auflösung (SD bis 4K, Hochkant 1080×1920, frei), Bildrate (23,976–60, 50i/59,94i, optional 100/120), Seitenverhältnis | M | ◐ (`set_master_format`: Größe frei 320–7680 px, auch hochkant, Bildrate 23,976–60; ohne Halbbilder 50i/59,94i, ohne 100/120; Seitenverhältnis folgt der Größe) |
| OUT.2 | Ausgang 1 = Program; Ausgang 2–4 mit Quelle Program/Preview/Multiview 1–2/Mix n/Input n; Overlay-Auswahl je Ausgang; Quelle live umschaltbar | M | ◐ (Ausgänge mit Quelle Program, Preview (seit 19.10., Vorschau-Ausgang des Mischers), Multiview oder Input, Ton aus Aux; ohne Mix, Overlay je Ausgang; Quelle nicht live umschaltbar) |
| OUT.3 | Vollbild 1–2 auf Monitoren (Monitor, Rechteck, Cursor aus, immer oben); Quelle wie OUT.2 oder Wiederholungs-Multiview | M | ◐ (Fenster, Vollbild Wayland, DRM; Quelle wie OUT.2 (Program, Vorschau, Multiview, Input); Monitor wählt der Compositor, kein Rechteck; Prüfung auf Hardware offen, H.20) |
| OUT.4 | Kartenausgang SDI/HDMI (DeckLink, AJA wenn Treiber), eigenes Format, Ton-Kanäle, Ton-Delay | S | ◐ (DeckLink: Anschluss und Modus wählbar, Halbbild-Modi, eingebetteter Ton Stereo (2026-10-27, H.7); ohne AJA, ohne Ton-Kanäle und Ton-Delay; zweiter Start im selben strom-Prozess braucht einen GStreamer-Fix) |
| OUT.5 | Key+Fill-Ausgang (zwei Ports, gerade/vormultipliziert) | K | – |
| OUT.6 | NDI- und OMT-Ausgang je Ausgang 1–4, mit Alpha, Ton-Bus, Skalierung, OMT-Qualität | M | ◐ (NDI je Ausgang mit Aux-Ton; ohne OMT, Alpha, Skalierung) |
| OUT.7 | Alle Kameras, Gäste und Audio-Inputs automatisch als NDI/OMT; Busse als reine Ton-Streams | S | – |
| OUT.8 | SRT-Ausgang je Ausgang: Modus, Latenz, Verschlüsselung, Stream-ID, H.264/HEVC, Bitrate, CBR mit Füllung, Stereo-Paare als eigene Spuren, HW-Encode | M | ◐ (SRT je Ausgang: H.264 mit HW-Encode, Bitrate, Modus/Optionen über die URI; ohne HEVC, CBR-Füllung, eigene Tonspuren) |
| OUT.9 | Virtuelle Kamera und virtuelles Audiogerät für andere Programme (v4l2loopback, PipeWire) | S | – |
| OUT.10 | Multiview 1 und 2, je eigenes Layout: PVW/PGM + 2/8/14 Inputs, nur Inputs, Reihenfolge nach Nummer oder Typ, Titel oben/unten, Pegel, Tally-Rahmen, Klick wählt Preview | M | ◐ (Multiview der Engine als Ausgangsquelle, OUT.2; Program links oder rechts, Pegel an/aus; ohne 2/8/14 Kacheln, nur Inputs, Reihenfolge, Titel oben/unten, Klick wählt Preview, zweite Multiview) |
| OUT.11 | Live-Monitore in der GUI: PVW, PGM, alle Input-Kacheln flüssig (volle Bildrate); Bilder lokal direkt über GPU-Speicher aus der Engine (G.2) | M | ◐ (alle Inputs live über Shared Memory, 30 fps bei 18 Ansichten; DMABuf ohne Kopie und volle Bildrate offen, H.4) |
| OUT.12 | Schutzbereiche: EBU/SMPTE, 80/90 %, Raster, 4:3, 1:1, 9:16 | S | – |
| OUT.13 | Messinstrumente: Waveform Y/RGB, Parade, Vektorskop (75/100 %-Marken), neben Bild | S | – |
| OUT.14 | Hinweise über PGM-Anzeige (nicht in Senken): zu leise, FTB aktiv, keine Aufnahme | K | – |
| OUT.15 | Snapshot des Programs | K | – |
| OUT.16 | Interlaced-Ausgabe (Aufnahme, Karte), Halbbildreihenfolge nach Norm | S | – |
| OUT.17 | Bühnenmonitor: Ausgang für Sprechende mit Uhr, Restzeit (aus Countdown oder Ablaufliste), Nachricht der Regie, optional Bild des Programs (P2) | S | – |
| OUT.18 | Ausgangs-Geometrie: Eckpunkt-Verzerrung und Kantenblende je Vollbild-Ausgang (P7) | K | – |

### 3.9 Aufnahme, Einzelaufnahme, Streaming (REC, STR)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| REC.1 | Aufnahme von Ausgang 1–4 mit Ton-Bus-Wahl; Start/Stopp; Laufzeit + verworfene Frames | M | ◐ (Aufnahme von Program oder Input mit Aux-Ton, Start/Stopp, Laufzeit; ohne verworfene Frames) |
| REC.2 | Formate: MP4, MOV, MKV, MXF, MPG/TS, AVI; Codecs H.264, MPEG-2, VC-3/DNxHD, MJPEG, verlustfrei; HW-Encode; Ton AAC/PCM (Auswahl im Detail lmix) | M | ◐ (MKV, MP4, TS; H.264 HW/SW + AAC) |
| REC.3 | Ausfallsicher: unterbrochene Datei bleibt lesbar (fragmentiertes MP4, MKV, MXF, TS) | M | ✅ (Live-Recorder: fragmentiertes MP4, TS, MKV ab GStreamer 1.26; davor MKV über den älteren Recorder; DECISIONS 18.10. „Aufnahmen über den Live-Recorder“) |
| REC.4 | Neue Datei alle n Minuten, nahtlos | S | – (E: Teilen nach Zeit/Größe im strom-Recorder) |
| REC.5 | Dateinamensmuster mit Datum/Zeit; Standardordner | S | ◐ (Ordner je Ausgang, Datum/Zeit fest im Namen; kein Muster) |
| REC.6 | Zweiter Recorder mit eigenem Format/Quelle, startet mit dem ersten | S | ◐ (beliebig viele Aufnahme-Ausgänge mit eigenem Format und eigener Quelle; Start nicht gekoppelt) |
| REC.7 | WAV-Begleitaufnahme (PCM) | K | – |
| REC.8 | Aufnahmeprotokoll: Zeitmarke per Aktion ins Log schreiben | K | – |
| REC.9 | Dateiname aus Datenquelle, Titelfeld oder Zeitplan-Eintrag (z. B. Vortrag, Kurs) (P2, P6) | S | – |
| MCR.1 | Einzelaufnahme: beliebige Live-Inputs und Ausgänge 1–4 gleichzeitig roh in getrennte Dateien | S | ◐ (Aufnahme je Input als ISO-Ausgang, auch gleichzeitig; Ausgänge 2–4 als Quelle fehlen) |
| MCR.2 | Bereits komprimierte Quellen ohne Neukodierung sichern: SRT als TS, NDI im Original-Format | S | – (E: TS ohne Neukodierung im strom-Recorder) |
| MCR.3 | Ordner je Quelle, Split, Ton je Quelle (eigener oder Bus) | S | ◐ (Ordner und Ton je Ausgang, eigener oder Aux; ohne Teilen) |
| STR.1 | Bis 5 Streamziele, einzeln und gemeinsam startbar; Fehler eines Ziels stoppt nur dieses | M | ✅ (SRT-Ausgänge einzeln oder alle zusammen, `start_outputs`/`stop_outputs`; ein Fehler stoppt nur diesen) |
| STR.2 | Protokolle RTMP; zusätzlich RTMPS, SRT, WHIP (lmix); Ziel = URL + Schlüssel; Profile (benannte Zielsätze) | M | ◐ (SRT, RTMP/RTMPS mit Schlüssel; WHIP E; ohne Profile) |
| STR.3 | Qualitätsprofile: Auflösung, Bitrate, Codec H.264/HEVC/AV1, Profil/Level/Preset, Keyframe-Abstand, CBR, Ton-Bitrate/-Kanäle; Ziele teilen Profil oder eigenes | M | ◐ (Video-Bitrate, Codec H.264/HEVC, CBR/VBR, Keyframe-Abstand je Ausgang; ohne AV1 (strom-Recorder und TS-Muxer), Auflösung, Profil/Preset, Ton-Bitrate, geteilte Profile) |
| STR.4 | Multi-Bitrate: ein Ziel mit mehreren Qualitäten, keyframe-ausgerichtet | K | – |
| STR.5 | Sendeverzögerung n s (Puffer im RAM, Countdown) | S | – |
| STR.6 | Beschnitt auf 1:1 oder 9:16 je Stream | S | – |
| STR.7 | Status: Bitrate, Puffer, verworfene Frames, Laufzeit, Encoder CPU/GPU | M | ◐ (Senderate, Paketverluste und -verwürfe, RTT, Laufzeit; ohne Puffer, Encoder-Last) |
| STR.8 | Lokales Mitschauen im LAN per HLS-Seite (≈ 10 s Verzug, ~100 Zuschauer) | K | – |
| STR.9 | Anmeldung bei Plattformen (YouTube, Twitch …) mit Kanalwahl | X | – |
| STR.10 | Adaptive Bitrate: bei schwankender Leitung Bitrate automatisch senken und wieder anheben, ohne Stream-Abbruch (P3) | S | – |

### 3.10 Wiederholung (RPL)

Zurückgestellt (User 2026-10-08). Anforderungen bleiben als Ziel erhalten.

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| RPL.1 | Daueraufnahme von bis 8 Live-Inputs (oder Program) in Sitzungsordner, Ton bis 4 Kanäle; Split alle 20 min; ältestes Material bei Platzmangel löschen (wählbar); mehrere Laufwerke | L | – (ROADMAP 4.5) |
| RPL.2 | Ereignisse: In/Out, Dauer, Tempo, aktivierte Kameras, Text je Kamera, Tags; 20 Ereignislisten mit Namen; ID mit fester Stellenzahl | L | – |
| RPL.3 | Live- und Rückschau-Modus für Markierungen; Markieren per Taste ("letzte 5/10/20 s"), In/Out einzeln, Abbruch | L | – |
| RPL.4 | Abspielkanäle A und B (gekoppelt oder getrennt), optional C/D als Vierfachansicht; Kamera je Kanal live wechseln | L | – |
| RPL.5 | Tempo 0–100 % (+ schneller), rückwärts, Einzelbild, Jog/Shuttle, Mausrad; Ton bei Zeitlupe gedehnt | L | – |
| RPL.6 | Abspielen: letztes/gewähltes/alle Ereignisse, mit Ein-/Aus-/Zwischenübergang (Tempo-angepasst), Musikbett, automatisch auf Program und zurück | L | – |
| RPL.7 | Ereignisse kopieren/verschieben/löschen, In/Out frameweise nachstellen | L | – |
| RPL.8 | Export eines Ereignisses als MP4 | L | – |
| RPL.9 | Wiederholungs-Multiview: Kanäle + Live-Kameras | L | – |
| RPL.10 | Vor-/Nachlauf, Doppelklick spielt, Sortierung nach Zeit | L | – |

### 3.11 Titel und Grafik (GFX)

Entscheidung 2026-10-08 (User): Titel sind Muss, auf HTML-Basis, nicht
kompatibel zu vMix-Titeln; Animationen per CSS/JavaScript und Lottie. Kein
CasparCG-Server (T.2), aber die Titel-Fähigkeiten orientieren sich an
CasparCG: dessen offene Vorlagen-Konvention, Ebenen-Modell und Steuerbefehle
dienen als Vorbild. strom rendert die Vorlagen selbst (Chromium über
`cefsrc`), also ein Takt und ein GPU-Pfad. CasparCG dient nur als
Funktionsreferenz, kein Code (MIT, `claude.md` Entscheidung 6).

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| GFX.1 | Titel = HTML-Vorlage nach der offenen CasparCG-Konvention: globale Funktionen `play()`, `stop()`, `next()`, `update(data)` und beliebige weitere per Aufruf; bestehende CasparCG-HTML-Vorlagen laufen unverändert. Optional ein lmix-Manifest (Name, Größe, Felder mit Typ Text/Bild/Farbe/Zahl/Schalter, Animationsdauern) | M | ◐ (Konvention, Manifest ohne Größe/Dauern) |
| GFX.2 | Rendering in der Engine (Chromium, `cefsrc`; E.9) mit Alpha in Ausgabeauflösung und -bildrate, ohne Neustart der Pipeline; Rendern pausieren/fortsetzen; ein hängender Titel nimmt Program nicht mit | M | ◐ (Quelle `graphics` über strom `html_input` in Program-Größe und -Rate, eigener Flow; pausieren mit `set_page_running`; mit Chromium ungeprüft) |
| GFX.3 | Daten: `update` mit Datensatz als JSON oder als XML im CasparCG-Format (`templateData` mit `componentData`/`data`); Felder einzeln per Aktion setzen und lesen | M | ✅ |
| GFX.4 | Lebenszyklus wie CasparCG: laden (vorbereitet, unsichtbar), laden und sofort abspielen, abspielen, weiter (mehrstufige Animation), stoppen mit Ausblend-Animation, sofort entfernen, Funktion aufrufen; Vorlage meldet Ende; Overlay-Kanal wartet auf Ende der Ausblendung | M | ◐ (Overlay wartet noch nicht) |
| GFX.5 | Lottie: Lottie-Animationen (JSON, .lottie) in Vorlagen (lottie-web) und später als eigener Input (E.10); Textebenen als Felder; Marker als Abläufe | S | ◐ (in Vorlagen; eigener Input später, E.10) |
| GFX.6 | Beliebige Webseiten (URL) als Input mit Ton, Größe und eigenem CSS; JavaScript-Funktion der Seite per Aktion aufrufen | S | ◐ (Quelle `web` mit Ton und `css`, JS-Aufruf `page_call`; CSS noch nicht in der Console; mit Chromium ungeprüft) |
| GFX.21 | Textfolien: Liste von Texten (Liedtexte, Zitate) aus Datei als Titel, mit Taste weiterschalten; im Programm und auf Beamer (P1) | S | ◐ (Text im Feld oder aus Datei, ein Absatz je Folie; Beamer mit GFX.2) |
| GFX.20 | Webseiten und Vorlagen spielen H.264 und AAC ab, auch über Media Source Extensions (HLS-Player, Streaming-Einbettungen wie Twitch, YouTube); bevorzugt mit Hardware-Dekodierung. DRM-geschützte Inhalte (EME/Widevine) sind nicht gefordert | M | – |
| GFX.7 | Titel-Editor in der GUI: Formular aus Manifest oder Datensatz, Felder live oder per "Übernehmen" | M | ✅ |
| GFX.8 | Datensätze speichern, benennen, abrufen, auflisten (wie CasparCG-Datenspeicher); vor/zurück; CSV ex-/import | S | ✅ |
| GFX.9 | Countdown/Stoppuhr/Uhr als Vorlagenbaustein: Dauer, Ziel-/Startzeit, Richtung, Format, Start/Stopp/Pause/Anpassen per Aktion, Ereignis bei 0 | S | ✅ |
| GFX.10 | Laufschrift: Tempo, Richtung, Anhängen vs. Ersetzen | S | ✅ |
| GFX.11 | Titel als Stinger: Schnittpunkt und Dauer aus Manifest oder Aktion | S | – |
| GFX.12 | Vorlagensatz (eigen gestaltet): Bauchbinde, Name+Funktion, Laufschrift, Uhr, Countdown, Spielstand, Vollbild-Karte; Hochkant-Varianten | M | ✅ |
| GFX.13 | Vorlagen-SDK: kleine JS-Bibliothek für die Konvention, Vorlagen-Gerüst, Vorschau im normalen Browser mit Testdaten; Gestaltung mit üblichen Web-Werkzeugen | S | ✅ |
| GFX.14 | Vorlage neu laden bei Dateiänderung; gesetzte Daten bleiben erhalten | S | ✅ |
| GFX.15 | Leistungsbudget: ≥ 8 aktive animierte Titel 1080p60 gleichzeitig ohne Drop | M | – |
| GFX.16 | Import von vMix-Titeln (.gtzip, XAML) | X | – |
| GFX.17 | Titel-Ebenen: mehrere Vorlagen gleichzeitig in einem Grafik-Input (z. B. Bauchbinde, Logo, Uhr), je Ebene unabhängig steuerbar; Grafik-Input als Overlay nutzbar | S | ◐ (Ebenen; Grafikkanal als Input `graphics`, als Overlay nutzbar; mit Chromium ungeprüft) |
| GFX.18 | Ebenen-Transformation je Titel-Ebene: Position, Größe, Crop, Deckkraft, Drehung, Perspektive, Helligkeit/Kontrast/Sättigung, Blend-Modus; animiert mit Dauer und Easing; mehrere Änderungen gemeinsam auf einen Frame anwenden | S | ✅ (Console: Platz, Größe, Drehung, Deckkraft, Farbe) |
| GFX.19 | Vorlagen- und Medienliste mit Infos (Felder, Größe, Dauer) und Vorschaubild über die API abfragbar | S | ✅ |

### 3.12 Datenquellen (DAT)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| DAT.1 | Typen: CSV, XLSX/ODS, Google Sheets (öffentlich + API-Schlüssel), RSS, Text, XML (XPath), JSON (Array von Objekten) | S | ✅ (XML über Pfad statt XPath) |
| DAT.2 | Lokal, im Netz, im Internet; automatische Aktualisierung bei Änderung | S | ✅ |
| DAT.3 | Optionen: erste Zeile = Spaltennamen, Zeilen zu Spalten | K | ✅ |
| DAT.4 | Zuordnung Titelfeld ← Quelle/Tabelle/Spalte (oder automatisch nach Name/Index)/Zeile (fest oder "gewählt"), Formatmuster `…{0}…`; Bilder per Pfad/URL | S | ✅ |
| DAT.5 | Datenquellen-Fenster: Zeile wählen, Auto-Weiter mit Intervall, Schleife; Aktionen nächste/vorige/bestimmte Zeile | S | ✅ |

### 3.13 Automatisierung (AUT)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| AUT.1 | Tastenbelegung: Tastatur, MIDI (Note, CC, Wert, Jog-Typen), HID-Controller (X-keys, Shuttle), Gamepad (inkl. Druck/Analog), Stream-Deck-Anbindung | S | ◐ (Tastatur, MIDI Note/CC; ohne HID, Gamepad, Stream Deck direkt) |
| AUT.2 | Je Belegung: Aktion, Input (fest oder "Input-Nummer"), Wert, Dauer, Beschriftung für Fernbedienung, Anzeige (Farbe/Bild/Live-Bild) | S | ◐ (Aktionen mit Parametern, Beschriftung, Farbe, Live-Bild des geschalteten Inputs auf der Tasten-Seite; ohne eigenes Bild) |
| AUT.3 | Mehrere Aktionen je Taste in fester Reihenfolge; global (Einstellungen) oder lokal (Show), lokal gewinnt | S | ✅ |
| AUT.4 | Vorlagen für Belegungen ex-/importieren | K | ◐ (nur über die ganze Einstellungsdatei, `export_settings`/`import_settings`) |
| AUT.5 | Dynamische Inputs/Werte 1–4: Taste A wählt Input/Wert, Taste B wirkt darauf | K | ✅ |
| AUT.6 | Rückmeldungen an Controller: Lampen, Farben, Motorfader nach Zustand (Input auf PGM/PVW/spielt, Pegel, Mute, Solo, Bus, FTB, Aufnahme, Stream, …) | S | ◐ (MIDI-Lampen: PGM/PVW, FTB, Ausgang, Sperre, Overlays; ohne Pegel, Motorfader) |
| AUT.7 | Auslöser je Input: beim Ein-/Ausblenden (Take, Overlay), Ende, Wiedergabezeit/Restzeit, Countdown 0, Gast verbunden/getrennt, Wiederholung fertig → Aktionsliste mit Verzögerung (max. 30 s je Schritt) | S | ◐ (PGM/PVW/Overlay, Clip-Ende mit Vorlauf, Wiedergabezeit, Titelmeldungen; ohne Gast, Wiederholung) |
| AUT.8 | Ablaufliste: Inputs mit Startposition, Dauer, Übergang, Anzeigeart (Vollbild/Overlay); Start, Stopp, vor/zurück, Schleife, manuell; mehrere benannte Listen in der Show | S | – |
| AUT.9 | Skripte: benannte Skripte (global/lokal), Start/Stopp per Aktion; Sprache JavaScript (E.6) mit Zugriff auf Aktionen, Zustand, Warten, Netzwerk; einfache Zeilen-Syntax (Aktion + Pause) | K | – |
| AUT.10 | Rückgängig für Input schließen | S | ✅ |
| AUT.11 | Schutzschloss: Schließen, Show laden, Aufnahme-/Stream-/Ausgabe-Stopp sperren | S | ✅ |
| AUT.12 | Globale Tastenkürzel, auch wenn ein anderes Programm im Vordergrund ist (P4) | S | ◐ (über das Portal; Plasma-Prüfung H.32) |
| AUT.13 | OSC als Steuerweg: Aktionen per OSC empfangen, Zustand per OSC senden (P5, P7) | S | ✅ |
| AUT.14 | Cue-Liste: jede Zeile führt mehrere Aktionen aus (Inputs, Ebenen, Overlays, Ton, Übergang); GO, zurück, springen; auslösbar per Taste, MIDI, OSC (P7) | S | ✅ |

### 3.14 Gäste (CAL)

Zurückgestellt (User 2026-10-08).

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| CAL.1 | Gast tritt per Browser bei (Link + Kennwort/Token), Kamera + Mikrofon; ein Input je Gast; bis 8 Gäste | L | – (ROADMAP M6) |
| CAL.2 | Rückweg je Gast: Bild (Ausgang n oder keins), Ton (Master, Abhörweg, Bus A–G) mit automatischem Mix-Minus | L | – |
| CAL.3 | Bandbreite Rückweg wählbar (bis 1080p); Gast-Bandbreite automatisch oder fest | L | – |
| CAL.4 | Verbindungsmanager: Status grün/gelb/rot, Statistik (Auflösung, Bitrate, fps, Jitterpuffer), Text-Chat mit allen Gästen | L | – |
| CAL.5 | Nur Direktverbindung erzwingen; Vermittlungsserver selbst betrieben, kein Herstellerdienst (lmix) | L | – |
| CAL.6 | Gäste automatisch als NDI/OMT | L | – |

### 3.15 Aktionskatalog und APIs (API)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| API.1 | Ein Aktionskatalog für alle Bedienwege (§2.3); jede Aktion: Name, Gruppe, Parameter (Input, Wert, Dauer, Feldname, Index, Mix, Kanal), Beschreibung; maschinenlesbar exportierbar | M | ✅ (JSON-Schema mit Beschreibungen und Gruppen `x-group`) |
| API.2 | Umfang: alle Bedienfunktionen dieses Lastenhefts, mindestens alles, was die vMix-Funktionsreferenz steuern kann; Gruppen wie vMix: Allgemein, Audio, Übergang, Ausgang, Titel, Input, Overlay, Ablaufliste, Skript, Wiederholung, NDI/OMT, PTZ, Show, Datenquellen, Browser (vMix 29: rund 800 Aktionen inkl. nummerierter Varianten; lmix darf sie parametrisieren statt durchnummerieren, z. B. `overlay.in {channel}` statt `OverlayInput1In…8In`) | M | ◐ (147 Aktionen in 13 Gruppen; ohne Ablaufliste, Skript, Wiederholung, Browser, NDI) |
| API.3 | HTTP: Aktion per Aufruf mit Parametern; vollständiger Zustand abrufbar (JSON) | M | ✅ |
| API.4 | Zustand enthält: Version, Inputs (ID, Nummer, Typ, Titel, Zustand, Position, Dauer, Schleife, Ton, Pegel, Busse, Ebenen, Titelfelder, Listeneinträge), Overlays, Preview, Program, FTB, Übergänge, Aufnahme/Stream/Ausgaben/Ablaufliste/Einzelaufnahme, Audio-Busse, dynamische Werte, Mixe | M | ◐ (Inputs mit Abspielstand: Zustand, Position, Dauer, Playlist-Eintrag; ohne Schleife, Ebenen, Titelfelder im Input) |
| API.5 | Ereigniskanal mit Abo: Tally (aus/PGM/PVW je Input), Rückmeldungszustände, Zustandsänderungen; Aktionen über denselben Kanal; leichtgewichtig genug für Mikrocontroller (Textzeilen über TCP oder WebSocket) | M | ✅ |
| API.6 | WebSocket mit Zustandsdiffs für eigene Clients | S | ✅ (auf Wunsch, `?diff=true`) |
| API.7 | Sicherheit: nur LAN (Standard), Passwort, Ausnahmen je Seite, lokale Programme ohne Anmeldung, "erhöhte Sicherheit" sperrt Skripte | M | ✅ (lmix hat keine Skripte; OSC ohne Passwort, nur LAN) |
| API.8 | Optionaler Kompatibilitätsmodus: vMix-Funktionsnamen, XML-Zustand, TCP-Protokoll, Ports 8088/8099 | K | – |
| API.9 | Integrationsziel: alles, was Fremdprogramme an vMix nutzen, geht auch mit der lmix-API: Aktionen mit allen Parametern, Inputs über Nummer, Name oder ID (plus Kurzformen "Preview", "Program"), vollständiger Zustand, Ereignis-Abo (API.5), Pegel live, Titelfelder lesen/schreiben, Live-Vorschaubild je Input für Tasten-Displays, Fehler mit Grund. Referenz: Bitfocus Companion, Stream-Deck-Plugins, Tally-Software (`LASTENHEFT-QUELLEN.md` §2) | M | ◐ |
| API.10 | Schnittstelle dokumentiert und versioniert (maschinenlesbare Beschreibung, Änderungen nur additiv innerhalb einer Hauptversion) | M | ◐ (JSON-Schema, `api_version`) |
| API.11 | Eigenes Companion-Modul für lmix, Beispiel-Clients (Tally auf Mikrocontroller, Skript) | S | ◐ (Beispiele in `examples/`: Python, ESP32-Tally; ohne Companion-Modul) |

### 3.16 Web-Fernbedienung (WEB)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| WEB.1 | Seite "Tasten": alle als sichtbar markierten Belegungen als Raster | S | ✅ |
| WEB.2 | Seite "Mischer": Inputs, PVW/PGM, Übergangstasten | S | ✅ |
| WEB.3 | Seite "Tally": Input wählen, ganzer Bildschirm in PGM-/PVW-Farbe | S | ✅ |
| WEB.4 | Seite "Titel": Felder bearbeiten, Presets anwenden/anlegen | S | ✅ |
| WEB.5 | Seite "Mitschauen" (STR.8) und "Zeichnen" (§3.17) | K | – |

### 3.17 Zeichenebene (TEL)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| TEL.1 | Browserseite mit Hintergrundvideo (wählbarer Ausgang, Latenz normal/niedrig); Zeichnen mit Maus/Finger/Stift erscheint live im Zeichen-Input | K | – |
| TEL.2 | Werkzeuge: Farbe, Stärke, Freihand, Rechteck, Kreis, Bild-Stempel (eigene PNGs), fortlaufende Nummern, Laserpunkt, Rückgängig, Löschen, Zeichnen an/aus | K | – |
| TEL.3 | 10 Belegungstasten auf der Seite; Klick in Multiview wählt Preview | K | – |

### 3.18 Tally, PTZ, Uhren, Statistik (PER)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| PER.1 | Tally an: Web-Seite, TCP-API, Rückmeldungen, TSL UMD 3.1/5.0 (P5), USB-/Netz-Tally-Geräte (generisch, z. B. HTTP/OSC/GPIO), Arduino/Firmata | S | ◐ (Webseite, TCP, OSC, TSL 3.1/5.0 per UDP und TCP, HTTP; ohne GPIO/Firmata) |
| PER.2 | PTZ je Input über Netzwerk (Protokolle VISCA-over-IP, NDI, ONVIF: lmix); Bewegen, Zoom, Fokus, Home, Tempo | S | ◐ (VISCA over IP und in UDP; ohne NDI, ONVIF) |
| PER.3 | PTZ-Positionen als virtuelle Inputs: Preview fährt die Position an, Standbild außerhalb PVW/PGM, aktualisieren | S | ◐ (Positionen per Auslöser; ohne virtuelle Inputs, Standbild) |
| PER.4 | Virtuelles PTZ: digitaler Zoom/Schwenk auf beliebigem Input | K | – |
| PER.5 | 2 Produktionsuhren: Uhrzeit (+Offset, 12/24 h), Ereignis (vor/während/nach in Bernstein/Grün/Rot), Aufnahme-, Streamdauer, Restzeit Program; Genauigkeit wählbar; als Input | S | ◐ (Uhr und Countdown als Titelvorlage; ohne Ereignisfarben, Aufnahme-/Streamdauer, Restzeit Program; Rendern E) |
| PER.6 | Statistik je Live-Input: Quell-Drops, Render-Drops, Resync, Frame-Abstände, Ton-Puffer/Drops/Resample; Encoder (CPU/GPU), SRT (Bandbreite, Verlust, RTT) | M | ◐ (Selftest; `/metrics`: QoS je Flow, SRT-Ausgänge Rate/Verlust/RTT; ohne Drops/Resync je Input, Ton-Puffer) |
| PER.7 | Warnung bei Überlast (Renderzeit, CPU) | S | ◐ |
| PER.8 | Takt-Korrektur gegenüber externer Hardware | K | – |
| PER.9 | Timecode: LTC/MTC einlesen, Systemzeit oder Timecode in Aufnahmen schreiben (P5) | K | – |

### 3.19 Show-Datei (SHW)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| SHW.1 | Show enthält: Inputs mit allen Einstellungen, Overlays, Übergänge, Audio (Inputs + Busse), Ablauflisten, Kategorien, lokale Belegungen/Rückmeldungen/Skripte, Aufnahme, Streaming, Ausgänge, Karte, Datenquellen, Master-Format | M | ◐ (Inputs mit Einstellungen und Farb-Kategorien, Overlays, Übergänge, Audio samt Bussen und Kopfhörer, Layouts, Ablauflisten, Kurzbefehle, Auslöser, Ausgänge, Titel, Datenquellen; ohne Master-Format, Karte) |
| SHW.2 | Offenes, versioniertes, lesbares Format (JSON); Medien per Pfad | M | ✅ (JSON `lmix-show`, Version 3, Medien per Pfad) |
| SHW.3 | Neu (mit Formatwahl), Öffnen (ersetzt), Anhängen (nur gewählte Inputs), Speichern, Speichern unter, zuletzt geöffnet | M | ✅ (Neu mit Programmformat, Öffnen, Anhängen gewählter Inputs, Speichern, Speichern unter, zuletzt geöffnet) |
| SHW.4 | Fehlende Dateien: markieren, suchen lassen, sonst Input offline | M | ✅ (fehlende Mediendateien markiert, in einem Ordner nach Namen gesucht; fehlende Quelle: Input offline, kommt von selbst zurück) |
| SHW.5 | Automatische Sicherung, "letzte Show" wiederherstellen (auch nach Absturz) | M | ✅ (Autosave nach jeder Änderung, Fortsetzen nach Absturz) |
| SHW.6 | Paket: Show + Medien + Titel in einer Datei | S | ✅ |
| SHW.7 | Formatwechsel beim Laden bestätigen | S | ✅ (älteres Format beim Öffnen umgewandelt, Hinweis in der Console, alte Datei beim Speichern als `.v<n>.bak`) |
| SHW.8 | Globale Einstellungen getrennt ex-/importieren, Werkseinstellungen | S | ✅ (`export_settings`, `import_settings`, `reset_settings`; im Menü „Paket“; ohne Geheimnisse) |
| SHW.9 | Import von vMix-Presets (E.3: nie) | X | – |
| SHW.10 | Import von OBS-Szenen (Quellen, Anordnung) als Starthilfe beim Umstieg (P4) | K | – |

### 3.20 Bedienoberfläche (UI)

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| UI.1 | Hauptfenster: Preview links, Program rechts (Farbcodes einstellbar), Übergangsspalte dazwischen, Input-Leiste unten mit Live-Kacheln, Fußleiste mit Senken-Tasten | M | ◐ (Console) |
| UI.2 | Input-Kachel: Titelzeile (Nummer, Name, Kategoriefarbe), Bild, Tasten: Schließen, Cut, Schleife, Overlay 1–8, GO, Ton, Großansicht, Einstellungen; Tally-Farbe | M | ◐ |
| UI.3 | Mehrere Input-Reihen, Kachelgröße einstellbar, Einklappen einzelner Inputs, Bildlauf | S | ✅ (Kachelgröße S/M/L, Kacheln füllen die Zeile, Bildlauf, Einklappen je Console; Monitore im Projektformat, Höhe per Ziehleiste) |
| UI.4 | Kontextmenüs je Input-Typ (Liste-Einträge, Fotos, Titel-Presets, NDI-Quelle, Browser …) | S | ◐ (Wiedergabe, Playlist-Einträge, Senden an WHIP; ohne Fotos, NDI, Browser) |
| UI.5 | Transportleiste unter PVW/PGM für Wiedergabe-Inputs | M | ✅ (Tasten, Position, Länge, Springen per Klick) |
| UI.6 | Andockbare Bereiche (Audio, Wiederholung, Liste) und abdockbare Fenster für zweiten Monitor | S | – |
| UI.7 | Einfacher und erweiterter Modus | S | ✅ |
| UI.8 | Input-Einstellungen in Reitern: Allgemein, Farbe, Key, Farbkorrektur, Effekte, Position, Ebenen, Auslöser, Tally, PTZ, Erweitert | M | ◐ (Allgemein, Farbe, Ton, Auslöser, PTZ, Erweitert; ohne Key, Effekte, Position, Ebenen, Tally) |
| UI.9 | Gestaltung nach dem Kante Design System (`agent.md`, GUI-Regel): Stilwahl Kante (Standard, dunkel; hell „Leinen“ bei hellem Systemthema), Kante Light (nur Formsprache, Farben des Systems), System als Rückfall (KDE Plasma/Breeze, folgt dem Plasma-Farbschema); Bildflächen bleiben dunkel; Sprachen en (Quelle) und de (erweiterbar) | M | ✅ (Stilwahl in den Einstellungen, `KanteStyle.kind`; en/de) |
| UI.10 | Bestätigung vor Stopp von Aufnahme/Stream (abschaltbar) | S | ✅ |
| UI.11 | Hochkant-Produktion: Vorlagen für 9:16, Schutzbereich, Quellrotation | K | – |
| UI.12 | Startoptionen: maximiert, erweiterter Modus, Vollbild/Kartenausgang/SRT starten, letzte Show | S | ◐ (Fenster, Modus, Ausgänge starten, letzte Show; Vollbild-Ausgang über `--start-outputs`) |
| UI.13 | Show-Vorlagen (z. B. Gottesdienst, Konferenz, Hörsaal) und Einrichtungsassistent: Kameras finden, Ton wählen, Stream-Ziel eintragen (P1, P6) | S | – |

---

### 3.21 Betrieb (OPS)

Aus den Persona-Durchläufen (`PERSONAS.md`).

| ID | Anforderung | Prio | Stand |
|---|---|---|---|
| OPS.1 | Headless-Betrieb: `lmix-core` und Engine laufen ohne GUI und ohne Monitor, Show wird beim Start geladen (systemd-Dienst) (P5) | S | ✅ |
| OPS.2 | Mehrere Bedienplätze gleichzeitig, auch über Netz: z. B. Bildregie und Tonregie an getrennten Rechnern; Rechte je Platz (P2, P5) | S | ◐ (mehrere Consoles über `--core`; Rechte je Schlüssel und Adresse: Ansehen, Steuern, Bearbeiten; ohne Rechte je Bereich, z. B. nur Ton) |
| OPS.3 | Monitoring: Gesundheitszustand und Metriken (Drops, Latenz, Encoder, Pegel, Speicher) über HTTP, Prometheus-Format (P5) | S | ✅ |
| OPS.4 | Zeitplan: Show laden, Aufnahme und Stream starten/stoppen zu festen Zeiten, mit Namen für Dateien (REC.9) (P6) | K | – |

## 4. Nichtfunktionale Anforderungen (NFA)

| ID | Anforderung | Prio |
|---|---|---|
| NFA.1 | Plattform: Linux x86-64 (aktuelle Ubuntu LTS, Fedora, Arch); NVIDIA, AMD, Intel gleichrangig, kein Rückfall auf CPU im Hot-Path | M |
| NFA.2 | Kapazität (Kennzahl "max. Inputs @ Budget", `ROADMAP.md`): 16 × 1080p30 Live + PGM + Multiview + 2 Encodes ohne Drop; Ziel 4K30 mit 4 Live-Inputs | M |
| NFA.3 | Latenz: intern ≤ 2 Frames; GUI-Monitor ≤ 3 Frames hinter Program | M |
| NFA.4 | A/V-Synchronität ± ½ Frame über 8 h | M |
| NFA.5 | Robustheit: Quelle fällt aus, Stream bricht ab, Datei fehlt → Program läuft weiter; Wiederverbinden automatisch | M |
| NFA.6 | Crash-Isolation: GUI-Absturz stoppt weder Engine noch Senken noch API (siehe E.4) | M |
| NFA.7 | Dauerlauf: 8 h Soak mit Last (16 Inputs, 2 Streams, Einzelaufnahme) ohne Speicher-/Latenzdrift; zusätzlich 48 h Betrieb ohne Neustart (P2) | M |
| NFA.8 | Bildraten 23,976/24/25/29,97/30/50/59,94/60 p, 50/59,94/60 i; Auflösungen 640×360 bis 3840×2160 | M |
| NFA.9 | Lizenz: Repo MIT-kompatibel, keine GPL-Abhängigkeit im Repo (`claude.md` Entscheidung 6); Laufzeit-Plugins Dritter getrennt (LGPL wie GStreamer und `gstcefsrc` nur dynamisch geladen, nie im Repo; ROADMAP §5.3) | M |
| NFA.10 | Bedienbarkeit unter Last: GUI-Reaktion < 50 ms, Tastenaktion → Program < 1 Frame + interne Latenz | M |
| NFA.11 | Lokalisierung: alle Texte aus Katalogen, de/en | S |
| NFA.12 | Sicherheit: APIs standardmäßig nur LAN/Loopback; keine Telemetrie; keine Herstellerdienste für Gäste | M |
| NFA.13 | Prüfbarkeit: jede M-Anforderung hat einen automatischen Test (Selftest, API-Test) oder einen dokumentierten Abnahmebefehl | M |
| NFA.15 | Persona-Durchläufe (§9): jede Meilenstein-Abnahme spielt die betroffenen Personas aus `PERSONAS.md` durch, mindestens 5; neue oder geänderte Funktionen werden gegen die Personas geprüft, bevor sie als fertig gelten | M |

---

## 5. Bewusst ausgeschlossen (X)

| Was | Grund |
|---|---|
| DVD, Flash/SWF, WMV-Streaming, DirectShow-Filter, FMLE | Windows-/Altformat, ohne Nutzen unter Linux |
| XAML/WPF-Titel, VB.NET-Skripte | Windows-Technik; Ersatz: offenes Titelformat (GFX), eigene Skriptsprache (AUT.9) |
| Zoom-Integration | proprietäres SDK, Lizenz- und Kontobindung |
| Virtuelle 3D-Sets | Aufwand hoch, Nutzen gering; Ersatz: Ebenen + Key |
| Plattform-Logins für Streamziele | Pflege fremder APIs; URL + Schlüssel genügt |
| Sprachbefehle | experimentell, plattformabhängig |
| Hersteller-Hardware-Pult (eingestellt), Windows-Update-Sperre, Lizenzschlüssel/Editionen | entfällt |

---

## 5a. Abweichungen von vMix: Altlasten

vMix trägt Funktionen aus vielen Jahren Windows-Geschichte. lmix baut sie
nicht nach, sondern ersetzt sie durch Heutiges oder lässt sie weg
(Leitlinie). Ausgeschlossenes steht zusätzlich in §5.

| vMix-Funktion | Warum Altlast | lmix stattdessen |
|---|---|---|
| DVD-Input mit Menüs | Optisches Laufwerk, MPEG-2-Decoder-Wahl, Menünavigation | Nicht übernehmen. DVD vorher in eine Datei umwandeln; Datei-Input deckt MPEG-2/VOB ab. |
| Flash (SWF, FLV) | Flash ist seit 2020 tot | Nicht übernehmen. Animationen als Bildsequenz, Video mit Alpha oder HTML-Titel; FLV spielt der Datei-Input. |
| PowerPoint-Input | Braucht installiertes Office, nur statische Folien | Folien als PDF oder Bildordner in den Fotos-Input. Kann-Funktion: Import über LibreOffice. |
| XAML/WPF-Titel, eigene WPF-Steuerelemente | Windows-only, in vMix selbst seit v24 abgekündigt | Nicht übernehmen. Eigenes Titelformat + HTML-Titel. |
| VB.NET-Skripte | Bindet an .NET Framework und Windows | Eingebettete Skriptsprache (Rhai oder Lua) mit Zugriff auf den Aktionskatalog; einfache Zeilen-Syntax „Aktion + Pause“ bleibt. |
| WMV-Aufnahme und WMV-Streaming | Veraltet, im Handbuch schon nicht mehr unterstützt | Nicht übernehmen. MP4/MKV/MOV mit H.264/HEVC/AV1. |
| FMLE als Streaming-Encoder, MP3 im Stream | Externes Adobe-Programm von 2012 | Nur eingebauter Encoder. Ton AAC/Opus. |
| DirectShow-Decoderwahl, Filtersperrliste, Renderer EVR/VMR9, FlipEx, Display-Methode „Legacy“ | Windows-Videostack, Fehlerquelle für Anwender | Keine Einstellung. GStreamer wählt Decoder nach Rang, GPU-Profil wählt Hardwarepfad. |
| VLC-Variante für Netzstreams | Sonderbuild eines Fremdprogramms | Nicht übernehmen. RTSP, RTMP, TS, RTP und HLS nativ über GStreamer. |
| Eigenes AVI-Zwischenformat | Proprietärer Codec, nur mit installiertem vMix schneidbar | Offene Schnittformate: ProRes, DNxHR, FFV1 in MOV/MKV; ausfallsicher als fragmentierte Datei. |
| Virtuelle 3D-Sets (fest eingebaut) | Starre Vorlagen, wenig genutzt | Nicht übernehmen. Ebenen + Chroma-Key + Ebenen-Editor; animierte Ebenen-Wechsel (SetLayerAnimated) decken Kamerafahrten ab. |
| Virtuelles Gerät „vMix Video“ für Skype/FMLE | Gedacht für Programme von 2010 | Moderne Entsprechung behalten: virtuelle Kamera über v4l2loopback/PipeWire, für Browser-Konferenzen. |
| Arduino-Tally mit Firmata, Pin-Reihenfolge „2012“ | Bastellösung, feste Pins | Standard-Tally-Protokolle: TSL UMD 3.1/5.0, HTTP-Webhooks, OSC, Web-Tally; Firmata nur als Kann. |
| Hardware-Pult des Herstellers | Eingestellt | Nicht übernehmen. Generische MIDI/HID-Pulte, Stream Deck, Companion. |
| Manuelle Takt-Korrektur (Formel mit Drop-Zählern) | Anwender rechnet Clock-Drift selbst aus | Automatisch: Drift je Quelle messen und per Resampling/Frame-Wiederholung ausgleichen; nur Anzeige in der Statistik. |
| Schalter 16–235 → 0–255, Preview-Skip, Low-Latency-Capture, High-Input-Performance-Modus | Workarounds für alte GPUs/Treiber | Automatisch aus Farbraum-Metadaten und GPU-Profil; keine Anwender-Schalter. |
| Aufnahme-Puffer „Wert 10“, Anwendungspriorität, Windows-Update-Sperre | Windows-Tuning | Entfällt. Puffer automatisch nach Schreibrate; Echtzeit-Priorität für Engine-Threads per Systemd/rtkit. |
| Sprachen per Google Translate | Maschinell, ungeprüft | Eigene Kataloge de/en, Community-Übersetzungen. |
| Sprachbefehle (Windows-Spracherkennung) | Experimentell | Nicht übernehmen. |
| Datenquellen aus XLS, PDF, XPS | Unzuverlässige Tabellenquellen | CSV, XLSX/ODS, Google Sheets, JSON, XML, RSS, Text. |
| Interlaced-Verarbeitung als Standardpfad, SD/Composite | Broadcast-Altlast | Progressiv als Standard; Interlaced nur an Ein- und Ausgang (Deinterlace, SDI-Ausgabe 50i), Kann. |
| Plattform-Logins für Streamziele | Pflege fremder APIs, bricht regelmäßig | URL + Schlüssel genügen; Profile je Plattform als Vorlage. |
| Editionen und Lizenzschlüssel | Kommerzielle Staffelung | Entfällt; immer der volle Funktionsumfang. |

---

## 6. Entscheidungen

### 6.1 Entschieden (User, 2026-10-08)

| # | Frage | Entscheidung |
|---|---|---|
| E.1 | Ausschlüsse in `claude.md` (Control-Surfaces, 4K, Web-Oberfläche) | Aufgehoben; Haupt-GUI bleibt nativ, Web nur als Fernbedienung |
| E.2 | API | Eigene API, mindestens so mächtig wie die vMix-API für Fremdprogramme (API.9); vMix-Kompatibilität optional (API.8) |
| E.3 | Import von vMix-Presets | Nie (SHW.9 = X) |
| E.4 | Ort von Show-Zustand, Aktionskatalog, APIs | Eigener Prozess `lmix-core`; GUI und APIs sind Clients |
| E.5 | Titelformat | HTML-Vorlagen mit Feld-API, Animation per CSS/JS und Lottie, nicht vMix-kompatibel (§3.11) |
| E.8 | Audio-Plugins | LV2, VST3 und CLAP (AUD.11 = S) |
| E.10 | Lottie | lottie-web in Vorlagen jetzt, nativer Lottie-Input (ThorVG) später |
| G.1 | GUI-Toolkit | Entscheidung nach Prototyp in M3; Kriterium dazu: Kante-Anbindung (Qt Quick/Kirigami hat `qml/Kante` und Breeze-Rückfall, egui bräuchte eine neue Kante-Anbindung) |
| G.2 | Live-Monitore | Bilder lokal über GPU-Speicher (DMABuf) aus der Engine; WHEP nur für entfernte Clients |
| E.6 | Skriptsprache | JavaScript (QuickJS), gleiche Sprache wie die HTML-Titel |
| E.9 | Browser-Engine für HTML-Titel | Wahl nach Benchmark M7.0; Ausgangspunkt `cefsrc` (in strom vorhanden), Vergleich WPE WebKit (`wpesrc`), Testplan `BENCH-HTML.md`. Hartes Kriterium GFX.20 (H.264/AAC): Standard-CEF kann es nicht, nur ein eigener CEF-Build (`proprietary_codecs`, `ffmpeg_branding=Chrome`) mit Patentfragen beim Verteilen; WPE spielt Medien über GStreamer und nutzt die installierten Decoder (wie der Rest von lmix) |
| T.1/T.2 | Titelsystem | Kein CasparCG-Server (nach Analyse: zweite Engine, zweiter Takt, Bild per NDI). Titel-Fähigkeiten nach dem Vorbild CasparCG, gerendert in strom (§3.11) |
| E.12 | Lizenz | lmix bleibt MIT-kompatibel (`claude.md` Entscheidung 6 unverändert) |
| E.11 | Submixe | Wie vMix: bis 15 Mix-Inputs mit Preview/Program, Cut/Übergang, Overlays, Stinger; als Quelle für Ausgänge und Vollbild (SWI.10) |
| — | Wiederholung, Gäste | Später (Priorität L) |
| — | Prioritäten | AUD.11, OUT.16, PER.5, SRC.23 auf S; SHW.9 auf X; aus Persona-Durchlauf UI.7, SRC.28 auf S |

### 6.2 Offen

| # | Frage | Stand |
|---|---|---|
| E.7 | Gäste: Signalisierung/TURN selbst betreiben | Später |
| S-E.1–12 | Show-Modus: Ablauf-Engine, Zonen, Patches, Bühnen, Plugins u. a. | Vorgeschlagen, `LASTENHEFT-SHOW.md` §6 |

---

## 7. Mengengerüst (Referenz vMix 29, höchste Edition)

| Größe | Wert |
|---|---|
| Overlay-Kanäle | 8 |
| Ebenen je Input | 10 |
| Übergangstasten / Stinger | 4 / 8 |
| Mix-Inputs | 15 (+ Hauptmischer) |
| Ausgänge / Vollbild | 4 / 2 |
| Streamziele | 5 |
| Recorder | 2 |
| Audio-Busse | 8 (M, A–G) + Abhörweg |
| Kanäle je Input | 16 (Matrix 16 × 16) |
| Wiederholung | 8 Kameras, 20 Ereignislisten, 2 (4) Kanäle |
| Gäste | 8 |
| Multiviews | 2 (+ Wiederholungs-Multiview) |
| Kategorien | 6 fest + 16 eigen |
| Dynamische Inputs/Werte | je 4 |
| Auslöser-Verzögerung | ≤ 30 s je Schritt |
| Aktionen im Katalog | rund 800 (davon viele nummerierte Varianten) |
| Inputs je Projekt | 1000 |
| SRT-Ausgänge | 4 (je Ausgang 1–4) |

Das Handbuch begrenzt vieles nach Edition (HD, 4K, Pro, Max); lmix kennt
keine Editionen, es gilt immer der höchste Wert.

---

## 9. Testmethode: Persona-Durchläufe

Neben automatischen Tests (NFA.13) wird lmix gegen typische Nutzer geprüft.
Sieben Personas stehen in `PERSONAS.md`: Ehrenamt in der Kirchengemeinde,
Konferenztechnik, Amateur-Sport, Twitch-Streamerin, Broadcast-Ingenieur,
Hochschul-Medienzentrum, Theater-Video.

| Schritt | Inhalt |
|---|---|
| Wann | Am Ende jedes Meilensteins (Teil der Abnahme) und bei jeder neuen oder geänderten Funktion im Rahmen der Entwicklung; mindestens 5 Personas je Durchlauf, ausgewählt nach Betroffenheit |
| Wie | Szenario Schritt für Schritt durchspielen: bis M2b gegen Lastenheft (Schreibtischtest), ab M3 am laufenden Programm mit Testquellen |
| Bewertung | je Schritt ✅ abgedeckt, ⚠ Lücke/Hürde, ✗ fehlt, ⏸ später; dazu Zeit bis zum Ziel und Anzahl der Bedienschritte |
| Ergebnis | Befunde mit Nummer in `PERSONAS.md`; daraus neue Anforderungen hier oder Vorschläge zur Entscheidung |
| Pflege | Personas bei neuen Zielgruppen ergänzen; Erfolgskriterien je Persona bleiben stabil, damit Durchläufe vergleichbar sind |

Erster Durchlauf (2026-10-08, gegen Lastenheft): 21 Befunde (B1–B21),
daraus neu SRC.31, OUT.17–18, REC.9, STR.10, AUT.12–14, PER.9, SHW.10,
UI.13, GFX.21, OPS.1–4; präzisiert PER.1, NFA.7. Prioritätsänderungen bereits
entschiedener Punkte, entschieden 2026-10-08 (User): UI.7 und SRC.28 auf S;
SRC.18 bleibt K (SRC.31 genügt), SRC.19 bleibt L, M5b wie geplant.

## 8. Quellen

- vMix User Guide 29, <https://www.vmix.com/help29/> (abgerufen 2026-10-08,
  210 Seiten; lokal außerhalb des Repos), inkl. Shortcut Function Reference,
  HTTP- und TCP-API.
- Zuordnung Anforderung → Handbuchseite: `docs/LASTENHEFT-QUELLEN.md`
- Show-Modus: QLab-5-Handbuch, Zuordnung in `docs/LASTENHEFT-SHOW-QUELLEN.md`
