# Lastenheft: lmix tools (Werkzeuge für NDI und OMT unter Linux)

Stand 2026-10-28, Entwurf 1. Vorbild sind die NDI Tools 6.3 (Windows und
macOS); jedes Werkzeug ist hier für **NDI und OMT** gedacht. Ergänzt
`LASTENHEFT.md` (Mischer) und `LASTENHEFT-SHOW.md` (Show) und ändert sie
nicht. Offene Fragen in §6 zur Entscheidung, nicht stillschweigend.

Dieses Dokument beschreibt **was** die Werkzeuge können müssen, nicht **wie**.

**Leitlinie**: Unter Linux gibt es für NDI nur das SDK, den Discovery
Server und Kommandozeilenwerkzeuge (Ton-Brücke, Aufnahme); für OMT nur die
Bibliotheken aus Quellcode. Es fehlt jede Oberfläche: kein Monitor, kein
Testbild, kein Bildschirm-Sender, keine virtuelle Kamera, keine
Netzübersicht, kein OMT-Betrachter auf irgendeinem System (Recherche
2026-10-28, `LASTENHEFT-TOOLS-QUELLEN.md`). lmix tools schließt diese Lücke.
Die NDI Tools sind die Vorlage, kein Zwang: wo Linux etwas besser kann
(PipeWire, Portale, systemd), nutzt lmix tools das, und jede Abweichung steht
hier mit Grund.

---

## 0. Clean-Room-Regeln

Es gelten CR.1 bis CR.8 aus `LASTENHEFT.md` sinngemäß, mit den NDI Tools
statt vMix:

| # | Regel |
|---|---|
| TCR.1 | Quelle für Vorbild-Verhalten: nur die öffentliche NDI-Dokumentation (<https://docs.ndi.video/>, <https://ndi.video/tools/>) und für OMT die öffentlichen Repos und Seiten (<https://github.com/openmediatransport>, <https://www.openmediatransport.org>). Kein Zerlegen der NDI-Tools-Programme, kein Mitschnitt ihres internen Verkehrs. |
| TCR.2 | Das NDI-Protokoll wird nur über das NDI-SDK angesprochen (zur Laufzeit geladen, nie im Repo, NFA.9). OMT ist offen (MIT); eigene Umsetzung nach den Repos ist erlaubt. |
| TCR.3 | Eigene Worte, eigene Namen. Kein Dokutext, keine Screenshots, keine Icons der Vorlage. Werkzeuge heißen nicht „Studio Monitor“, „Scan Converter“ o. ä. (Namen §1). „NDI“ und „OMT“ erscheinen nur als Name des Protokolls, nach den Markenregeln von NDI (§6, T-E.5). |
| TCR.4 | Anforderungen ohne Vorbild in den NDI Tools sind mit **(lmix)** markiert. Zuordnung Anforderung → Quelle: `LASTENHEFT-TOOLS-QUELLEN.md`. |

---

## 1. Begriffe und Werkzeuge

| lmix tools | Aufgabe | Vorbild (NDI Tools) |
|---|---|---|
| **Monitor** | Eine Quelle ansehen und anhören, mit Tally, Hilfslinien, PTZ, Aufnahme | Studio Monitor / Video Monitor |
| **Testbild** | Testbilder und Testton als Quelle senden | Test Patterns |
| **Bildschirm senden** | Bildschirm, Fenster oder Bereich als Quelle, optional mit Fernbedienung (KVM) | Screen Capture (HX) / Scan Converter |
| **Virtuelle Kamera** | Eine Quelle als Kamera für Videokonferenz-Programme | Webcam Input / Virtual Input |
| **Netzübersicht** | Alle Sender und Empfänger im Netz, Discovery-Server, Empfänger fernsteuern | Discovery |
| **Analyse** | Datenrate, Bildtakt, Aussetzer, Fähigkeiten einer Quelle messen | Analysis |
| **Ton-Brücke** | Soundkarte bzw. PipeWire-Knoten ↔ Netzquelle | Free Audio, Audio Direct |
| **Aufnahme** | Eine oder mehrere Quellen ohne Neukodierung aufnehmen | NDI Record (SDK) |
| **Kreuzschiene** | Virtuelle Quellen auf echte Quellen schalten | Router (später) |
| **Zugang** | Gruppen, Netzwerkkarte, Multicast, Discovery-Server je Rechner | Access Manager (später) |
| **Brücke** | Netzquellen über WAN verbinden | Bridge (später) |
| **Einladung** | Kamera oder Bildschirm eines Gastes per Link als Quelle | Remote, eingestellt (später) |
| Quelle | Ein Sender im Netz (NDI oder OMT), Name `Rechner (Kanal)` | Source |
| Gruppe | NDI: Sendegruppe; OMT kennt keine Gruppen (§2.2) | Group |
| Proxy | Verkleinerter Zweitstrom zum Sparen (NDI: Low Bandwidth, OMT: Vorschaustrom 1/8) | Low Bandwidth |
| Discovery-Server | Zentrales Verzeichnis statt mDNS (NDI: TCP 5959, OMT: TCP 6399) | Discovery Server |

---

## 2. Systemmodell

### 2.1 Aufbau

```
                 ┌──────── lmix tools (Oberfläche, Kante) ─────────┐
                 │ Monitor · Testbild · Bildschirm · Kamera ·      │
                 │ Netzübersicht · Analyse · Ton · Aufnahme        │
                 └───────────────┬─────────────────────────────────┘
                                 │ lmix-tools-API (HTTP/WS, CLI)
                 ┌───────────────▼─────────────────────────────────┐
                 │ lmix-toolsd (Dienst, ohne Oberfläche lauffähig) │
                 │ Sender, Empfänger, Kreuzschiene, Ton-Brücke,    │
                 │ Aufnahme, Discovery-Server                      │
                 └──┬───────────────┬───────────────┬──────────────┘
                    ▼               ▼               ▼
              GStreamer-Elemente  PipeWire      Portale (Wayland)
              NDI: ndisrc/ndisink  Ton, Kamera   ScreenCast, RemoteDesktop
              OMT: omtsrc/omtsink  (Knoten)      (KVM über libei)
              (gemeinsam mit strom/lmix: SRC.10/11, OUT.6)
```

- Die Protokoll-Elemente (NDI und OMT als GStreamer-Elemente) sind dieselben,
  die lmix für SRC.10, SRC.11 und OUT.6 braucht. Das OMT-Element entsteht
  einmal und dient beiden (§6, T-E.3).
- Was dauerhaft laufen soll (Testbild auf einem Signal-Rechner, Ton-Brücke,
  Kreuzschiene, Monitor als Bildschirm an der Wand), läuft ohne Oberfläche
  als Dienst mit systemd. Die Oberfläche ist ein Client wie in lmix (E.4).

### 2.2 NDI und OMT nebeneinander

Jedes Werkzeug zeigt beide Protokolle in einer Liste, mit Kennzeichen am
Namen. Wo ein Protokoll etwas nicht kann, ist die Bedienung dafür aus, mit
Hinweis warum.

| Merkmal | NDI | OMT |
|---|---|---|
| Finden | mDNS oder Discovery-Server, Gruppen | DNS-SD (mDNS) oder OMT-Discovery-Server, keine Gruppen |
| Bild | Full Bandwidth (SpeedHQ), HX (H.264/HEVC) | VMX (4:2:2, Intra), Qualität Niedrig/Mittel/Hoch auf Wunsch des Empfängers |
| Sparen | Low-Bandwidth-Proxy | Vorschaustrom 1/8 |
| Alpha, HDR | ja | ja (16 Bit P216/PA16) |
| Ton | bis viele Kanäle, Float | bis 32 Kanäle, Float, stumme Kanäle entfallen |
| Tally | Vorschau/Program | Vorschau/Program, alle Verbindungen zusammengefasst |
| PTZ | eingebaut | VISCA über IP (URL in Metadaten) oder VISCA in Metadaten |
| KVM | ja | nein (§6, T-E.4) |
| Weiterleiten | Empfänger auf andere Quelle umlenken | `setredirect` |
| Bibliothek | NDI-SDK, proprietär, nur zur Laufzeit | libomt/libvmx, MIT |

---

## 3. Funktionale Anforderungen

Priorität wie `LASTENHEFT.md`: **M** Muss, **S** Soll, **K** Kann, **L**
später (Entscheidung User 2026-10-28: Router, Access Manager, Bridge,
Remote), **X** ausgeschlossen (§5). **MS**: Meilenstein (§7). Stand: alle –
(neu); Bezug auf bestehende IDs in Klammern.

### 3.1 Gemeinsames (TGM)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TGM.1 | Quellenliste für NDI und OMT zusammen: nach Rechner gruppiert, Kennzeichen des Protokolls, Suche, Vorschaubild beim Überfahren, manuell per Adresse hinzufügen (SRC.10) | M | T1 |
| TGM.2 | Discovery: mDNS und Discovery-Server je Protokoll, mehrere Server; Einstellung je Rechner, gemeinsam für alle Werkzeuge | M | T1 |
| TGM.3 | Fehlt die NDI-Laufzeit, sagt jedes Werkzeug das mit Weg zur Installation; OMT und alles andere laufen weiter | M | T1 |
| TGM.4 | Jedes Werkzeug auch ohne Oberfläche: Kommandozeile und Dienst (systemd-Unit), gleiche Optionen wie in der Oberfläche **(lmix)** | M | T1 |
| TGM.5 | API (HTTP/WS) für alles, was die Oberfläche kann; Companion-tauglich (API.9) **(lmix)** | S | T2 |
| TGM.6 | Ein Werkzeug kann mehrfach laufen (zwei Monitore, drei Testbilder), jedes mit eigenem Namen und eigener Einstellung | M | T1 |
| TGM.7 | Oberfläche aus Kante, de/en (NFA.11), Demo-Modus mit Demodaten für Screenshots (nur intern) | M | T1 |
| TGM.8 | Aus lmix heraus: Rechtsklick auf eine NDI/OMT-Quelle → „Im Monitor öffnen“, „Analysieren“ **(lmix)** | K | T2 |

### 3.2 Monitor (TMO)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TMO.1 | Quelle wählen und mit Ton zeigen; Ton-Ausgang wählen, Pegel, stumm | M | T1 |
| TMO.2 | Fenster, Vollbild auf einem oder allen Bildschirmen, immer oben, ohne Rahmen; Fensterposition merken | M | T1 |
| TMO.3 | Alpha auf Schachbrett, Spiegeln; zweite Quelle als Bild im Bild oder mit Alpha darüber | S | T2 |
| TMO.4 | Hilfen: Mittelkreuz, Sicherheitsbereiche, 4:3- und 1:1-Maske, Pegelanzeige, Tally-Rahmen | M | T1 |
| TMO.5 | Modus geringste Latenz (ohne Bildtakt-Angleichung) und Proxy-Modus | S | T1 |
| TMO.6 | HDR (PQ/HLG) zeigen; Deinterlacing wählbar | S | T2 |
| TMO.7 | Die gezeigte Quelle erneut senden (unter eigenem Namen) | K | T2 |
| TMO.8 | PTZ: Schwenken, Neigen, Zoom, Fokus, Presets; Gamepad/Joystick über evdev (PER.2) | S | T2 |
| TMO.9 | Aufnahme der gezeigten Quelle (über TRC) | S | T2 |
| TMO.10 | Als Empfänger im Netz anmelden; die Netzübersicht kann die Quelle umstellen (TDI.4) | S | T2 |
| TMO.11 | Fernbedienung über Browser (Quelle wechseln, Hilfen), Link als QR-Code | K | T2 |
| TMO.12 | Kiosk: startet beim Hochfahren ohne Desktop (DRM/KMS) mit fester Quelle, Rückfallbild bei Ausfall; tauglich für Raspberry Pi 5 **(lmix)** | S | T2 |
| TMO.13 | Werte der Quelle einblenden: Format, Bildrate, Datenrate, Aussetzer **(lmix)** | S | T1 |

### 3.3 Testbild (TTP)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TTP.1 | Sendet beim Start sofort; Name, Auflösung (bis 2160p), Bildrate (NFA.8), Protokoll NDI, OMT oder beides | M | T1 |
| TTP.2 | Eigene Testbilder: Farbbalken (75 %/100 %), Graustufen, Raster, Kreis/Geometrie, Schärfe, Bewegung (Laufbalken, Zähler), Schwarz, Farbe **(eigene Gestaltung)** | M | T1 |
| TTP.3 | Eigenes Bild mit Alpha (PNG) als Testbild oder als Ebene darüber | S | T1 |
| TTP.4 | Testton 1 kHz in wählbarem Pegel (−18/−20 dBFS …), Kanal-Ansage oder Kanal-Pieps je Kanal zur Belegungsprüfung **(Kanal-Ansage: lmix)** | M | T1 |
| TTP.5 | HDR-Testbild (PQ/HLG) | K | T3 |
| TTP.6 | Lippensynchron-Test: Blitz und Klick im selben Bild, Gegenstück zur Messung in TAN.5 **(lmix)** | S | T2 |
| TTP.7 | Name, Uhrzeit und Bildzähler im Bild (erkennt eingefrorene Strecken) **(lmix)** | S | T1 |

### 3.4 Bildschirm senden (TSC)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TSC.1 | Bildschirm, Fenster oder Bereich über das ScreenCast-Portal (Wayland und X11) **(Linux-Weg)** | M | T2 |
| TSC.2 | Bildrate wählbar, Mauszeiger an/aus, Größe nativ oder verkleinert | M | T2 |
| TSC.3 | Ton: Systemton, ein Programm oder ein Eingang (PipeWire-Knoten) mitsenden | S | T2 |
| TSC.4 | Kamera des Rechners zusätzlich als eigene Quelle mit eigenem Ton | K | T2 |
| TSC.5 | Fernbedienung (KVM): ein Empfänger steuert Maus und Tastatur, über das RemoteDesktop-Portal (libei), nur nach Freigabe am Rechner; nur NDI (T-E.4) | K | T3 |
| TSC.6 | Sparmodus: NDI HX (H.264/HEVC per GPU) bzw. OMT-Qualitätsstufe; bis 4K, hohe Bildraten | S | T2 |

### 3.5 Virtuelle Kamera (TVC)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TVC.1 | Eine Quelle als Kamera: als PipeWire-Kamera (Portal) und als v4l2loopback-Gerät, wenn das Modul da ist **(Linux-Weg)** | M | T2 |
| TVC.2 | Quelle aus dem Tray wählen, Vorschau beim Überfahren | S | T2 |
| TVC.3 | Ausgabeformat festlegen (480p/720p/1080p), bei fehlender Quelle ein Standbild | M | T2 |
| TVC.4 | Ton der Quelle als virtuelles Mikrofon (PipeWire-Quelle), Kanäle zusammenfassen, Pegel | S | T2 |

### 3.6 Netzübersicht (TDI)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TDI.1 | Alle Sender mit Rechner, Adresse, Format, Protokoll, Gruppen; alle Empfänger mit verbundener Quelle, Zustand, Empfangsart | M | T1 |
| TDI.2 | Discovery-Server für NDI und OMT als Dienst auf diesem Rechner starten, Adresse und Port wählen; Zustand der bekannten Server | S | T2 |
| TDI.3 | Fehlerbild erklären: Quelle per mDNS sichtbar, aber nicht verbindbar (Firewall, Ports NDI 5959 ff., OMT 6399–6600), mDNS blockiert, falsche Schnittstelle **(lmix)** | S | T2 |
| TDI.4 | Empfänger fernsteuern: verbundene Quelle umstellen (NDI über Discovery-Server, OMT über `setredirect` wo möglich) | K | T3 |
| TDI.5 | Firewall-Hilfe: nötige Regeln für firewalld/ufw anzeigen und auf Wunsch setzen **(lmix)** | K | T2 |

### 3.7 Analyse (TAN)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TAN.1 | Je Quelle: Datenrate, Bildgröße, Bildtakt und Schwankung, verlorene Bilder, Formatwechsel, Fähigkeiten (PTZ, KVM, Aufnahme), Bittiefe | M | T2 |
| TAN.2 | Nur Bild, nur Ton oder beides; ohne Dekodierung, wo das Protokoll es erlaubt | S | T2 |
| TAN.3 | Verlauf als Diagramm (mit Achsen und Legende), Export als CSV | S | T2 |
| TAN.4 | Dauerüberwachung mehrerer Quellen mit Grenzwerten und Meldung; Messwerte für Prometheus **(lmix)** | K | T3 |
| TAN.5 | Laufzeit messen: Testbild TTP.6 senden, über eine Strecke zurück, Versatz Bild/Ton und Latenz anzeigen **(lmix)** | K | T3 |

### 3.8 Ton-Brücke (TAU)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TAU.1 | Eingang → Quelle: ein Gerät oder PipeWire-Knoten als Ton-Quelle senden; Name, Kanäle, Pegel in dB | M | T1 |
| TAU.2 | Quelle → Ausgang: eine Quelle auf einem Gerät oder Knoten abspielen | M | T1 |
| TAU.3 | Jede Netzquelle erscheint auf Wunsch als PipeWire-Knoten, jedes Programm kann sie aufnehmen; eine „Senke“ im Netz erscheint als PipeWire-Ausgang (Ersatz für das Audio-Direct-Plugin, für alle Programme statt nur für VST-Hosts) **(Linux-Weg)** | S | T2 |
| TAU.4 | Pufferlänge und Taktangleichung einstellbar; Aussetzer zählen | S | T2 |

### 3.9 Aufnahme (TRC)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TRC.1 | Eine Quelle ohne Neukodierung aufnehmen (NDI im Original, OMT als VMX), Ton als Float (MCR.2) | M | T2 |
| TRC.2 | Mehrere Quellen gleichzeitig, gemeinsamer Start und Stopp, gemeinsamer Zeitcode | S | T2 |
| TRC.3 | Neue Datei ohne Bildverlust (Schnitt), automatisch bei Formatwechsel | S | T2 |
| TRC.4 | Absturzfest: ein Abbruch hinterlässt eine lesbare Datei (fragmentiertes MP4/MKV statt MOV-Index am Ende) **(lmix)** | M | T2 |
| TRC.5 | Steuerbar über API und Kommandozeile; Zustand mit Bildzahl, Zeitcode, Pegel | M | T2 |
| TRC.6 | Proxy-Datei (klein, H.264) neben der Originalaufnahme **(lmix)** | K | T3 |

### 3.10 Anbindung an andere Programme (TPL)

| ID | Anforderung | Prio | MS |
|---|---|---|---|
| TPL.1 | VLC: Ausgabemodul NDI/OMT (jede VLC-Instanz eine Quelle) | K | T3 |
| TPL.2 | Schnittprogramme unter Linux (Kdenlive/MLT): Zeitleiste als Quelle senden | K | T3 |
| TPL.3 | OBS: NDI über DistroAV (vorhanden); OMT-Plugin für Linux bauen und als Paket anbieten | K | T3 |
| TPL.4 | Ton-Plugins (LV2/VST3/CLAP) für Ein- und Ausgang, wenn TAU.3 nicht reicht | K | T3 |

### 3.11 Später (TLA)

Auf Wunsch des Users (2026-10-28) aufgenommen, aber zurückgestellt.

| ID | Anforderung | Prio |
|---|---|---|
| TLA.1 | **Kreuzschiene**: bis 64 virtuelle Ziele × beliebige Quellen; ein Ziel ist eine Quelle mit festem Namen, die auf eine echte umleitet (kein Bild läuft durch); Tasten mit Namen und Farbe, Presets, Vorschau; steuerbar über API, Companion, Tally zurück | L |
| TLA.2 | **Zugang**: Sende- und Empfangsgruppen je Rechner, externe Quellen per IP, Netzwerkkarte, Multicast-Bereiche, Discovery-Server; schreibt die Konfigurationsdatei des NDI-SDK und `~/.OMT/settings.xml` | L |
| TLA.3 | **Brücke**: zwei Netze über WAN verbinden (Gastgeber, Beitreten, lokal), verschlüsselt, auf Wunsch umkodiert (H.264/HEVC, Alpha, HDR), Puffer, Statistik, Verbindungstest; für OMT über SRT/WebRTC statt eigenem Protokoll **(OMT-Weg: lmix)** | L |
| TLA.4 | **Einladung**: Link für einen Gast, Kamera oder Bildschirm aus dem Browser (WHIP) wird zur Quelle; ersetzt auch die Handy-Apps (§5) | L |

### 3.12 Vorschläge: weitere Werkzeuge (TVS) (lmix)

Nebenprodukt der Analyse; jedes ohne Vorbild in den NDI Tools. Zur
Entscheidung (§6, T-E.6).

| ID | Werkzeug | Was | Vorschlag |
|---|---|---|---|
| TVS.1 | **Multiview** | Viele Quellen auf einem Bildschirm, Raster frei, Tally-Rahmen, Pegel, Namen, Uhr; Fenster, Vollbild oder als neue Quelle senden | S, T2 |
| TVS.2 | **Wandler** | NDI ↔ OMT, und beide ↔ SRT, RTMP, WHIP/WHEP, HLS; DeckLink-SDI/HDMI ↔ Netzquelle (Hardware-Wandler in Software); V4L2-Kamera und Capture-Karten als Quelle | S, T2 |
| TVS.3 | **Abspieler** | Datei, Playlist, Schleife als Quelle; ersetzt das VLC-Plugin für den häufigsten Fall | S, T2 |
| TVS.4 | **Web-Ansicht** | Netzquellen im Browser ansehen (WHEP), mit Tally; Vorschau von unterwegs ohne Programm | S, T2 |
| TVS.5 | **Tally** | Tally jeder Quelle als Seite für Handy/Tablet (Vollbild-Farbe), USB-Lampen, GPIO am Raspberry Pi; Quelle von lmix oder aus dem Protokoll | K, T3 |
| TVS.6 | **PTZ-Pult** | Mehrere PTZ-Kameras (NDI, VISCA, OMT-Metadaten) mit Gamepad, Presets mit Vorschaubild | K, T3 |
| TVS.7 | **Grafik** | HTML-Seite oder Titel als Quelle mit Alpha (Bauchbinden, Uhr, Countdown, Zeitcode) | K, T3 |
| TVS.8 | **Messgeräte** | Waveform, Vektorskop, Histogramm, Lautheit (EBU R128), Phasenkorrelation einer Quelle | K, T3 |
| TVS.9 | **Verzögerung** | Quelle um einstellbare Zeit verzögert neu senden (Lippensynchronität, Zeitversatz) | K, T3 |
| TVS.10 | **Diagnose-Paket** | Ein Befehl sammelt Netz, Quellen, Versionen, Logs für eine Fehlermeldung | K, T2 |

---

## 4. Nichtfunktionale Anforderungen (TNF)

| ID | Anforderung | Prio |
|---|---|---|
| TNF.1 | Plattform wie NFA.1; zusätzlich ARM64 (Raspberry Pi 5) für Monitor, Testbild, Ton-Brücke und Discovery-Server | M |
| TNF.2 | Leicht: ein Werkzeug startet in unter 1 s und braucht kein lmix-core und kein strom | M |
| TNF.3 | Lizenz NFA.9: NDI-SDK nur zur Laufzeit, nie im Repo oder Paket; OMT (MIT) darf mit | M |
| TNF.4 | Dauerlauf: Monitor, Testbild und Ton-Brücke 48 h ohne Drift und Speicherzuwachs | M |
| TNF.5 | Latenz: Monitor ≤ 2 Bilder über der Protokoll-Latenz; Ton-Brücke ≤ 20 ms Puffer einstellbar | S |
| TNF.6 | Robust: Quelle fällt aus → Rückfallbild und Wiederverbinden; Netzwechsel ohne Neustart | M |
| TNF.7 | Prüfbarkeit wie NFA.13: Testbild → Monitor/Analyse als automatischer Kreistest für beide Protokolle | M |
| TNF.8 | Pakete: Flatpak und Distro-Pakete; Dienste als systemd-User- und System-Units | S |

---

## 5. Bewusst ausgeschlossen (TX)

| Was | Grund |
|---|---|
| Windows und macOS | Dort gibt es die NDI Tools; Fokus Linux (S-E.7) |
| Plugins für Premiere Pro, After Effects, Final Cut Pro | Nicht unter Linux; Ersatz TPL.2 |
| Eigene Handy-Apps (Kamera, Bildschirm) | Ersatz über den Browser (TLA.4, TVS.2 mit WHIP) |
| Eigene Umsetzung des NDI-Protokolls | Proprietär; nur über das SDK (TCR.2) |
| KVM für OMT über eigene Metadaten | Kein Standard; erst wenn OMT es festlegt (T-E.4) |

---

## 6. Entscheidungen

### 6.1 Entschieden (User 2026-10-28)

| # | Frage | Entscheidung |
|---|---|---|
| T-E.0 | Umfang | Alle NDI Tools für NDI und OMT; Router, Access Manager, Bridge, Remote als „später“ (TLA); Name des Pakets „lmix tools“ |

### 6.2 Offen

| # | Frage | Vorschlag |
|---|---|---|
| T-E.1 | Ein Programm mit Ansichten oder getrennte Programme | **Beides auf einer Codebasis**: ein Programm `lmix-tools` mit Seitenleiste wie lmix (Übersicht aller laufenden Werkzeuge), jede Ansicht auch einzeln startbar (`lmix-tools monitor --source … --fullscreen`) und mehrfach (TGM.6); Desktop-Einträge je Werkzeug. Grund: Monitore und Testbilder laufen oft einzeln und mehrfach, an Wand und Kiosk; die Übersicht hilft am Arbeitsplatz. Getrennte Programme allein verlieren die Übersicht, ein Programm allein passt nicht zum Kiosk |
| T-E.2 | Eigenes App-Gerüst oder Seite in lmix | **Eigenes Programm**, das Bausteine der Console teilt (Monitor, Pegel, Quellenliste aus Kante). lmix ist schwer (lmix-core, strom) und für Produktionen; die Werkzeuge müssen auf einem Pi ohne beides laufen (TNF.2). Aus lmix heraus aufrufbar (TGM.8) |
| T-E.3 | Engine | **GStreamer direkt** (kein strom): NDI über `ndisrc`/`ndisink` aus gst-plugins-rs, OMT über ein eigenes Element `omtsrc`/`omtsink` (Rust, gegen libomt oder eigene Umsetzung mit libvmx). Dieses Element braucht lmix ohnehin für SRC.11/OUT.6 (strom-Block); es entsteht einmal und dient beiden |
| T-E.4 | KVM über OMT | Ausgeschlossen, bis OMT einen Metadaten-Standard dafür hat; ggf. dort vorschlagen |
| T-E.5 | Markenregeln NDI | Vor dem ersten Release prüfen: Nennung „NDI®“, Hinweis auf Vizrt, ob „für NDI“ im Namen erlaubt ist, ob das SDK-Lizenzabkommen Weitergabe von Programmen erlaubt, die die Laufzeit nur laden |
| T-E.6 | Welche Vorschläge (TVS) übernehmen | Vorschlag: Multiview, Wandler, Abspieler, Web-Ansicht als S in T2; Rest K |
| T-E.7 | Eigenes Repo oder im lmix-Repo | Vorschlag: eigenes Repo `lmix-tools`, gemeinsame Teile (OMT-Element, QML-Bausteine) als eigene Crates bzw. Kante-Module |

---

## 7. Meilensteine

### T1 — Sehen und Senden

Netzübersicht (Liste), Monitor mit Hilfen und Kennwerten, Testbild mit
Testton, Ton-Brücke, alles für NDI und OMT; das OMT-Element. Abnahme:
Testbild auf Rechner A (NDI und OMT) → Monitor auf Rechner B zeigt beide
mit Ton, Analyse zeigt keine Aussetzer über 1 h; derselbe Test als
automatischer Kreistest (TNF.7) und gegen vMix 29 (OMT) bzw. ein
NDI-Gerät (Hardware-Abnahme).

### T2 — Arbeitsplatz

Bildschirm senden, virtuelle Kamera, Aufnahme, Analyse, Monitor mit PTZ,
Kiosk und Fernbedienung, Discovery-Server, API. Mit den gewählten
Vorschlägen (T-E.6).

### T3 — Erweiterungen

KVM, HDR, Anbindungen (TPL), übrige Vorschläge.

### Später

Kreuzschiene, Zugang, Brücke, Einladung (§3.11).

---

## 8. Quellen

`LASTENHEFT-TOOLS-QUELLEN.md`.
