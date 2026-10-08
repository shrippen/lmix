# Quellen zum Lastenheft

Stand 2026-10-08, vMix 29.0.0.49. Nur Fundstellen und eigene
Zusammenfassungen; kein Handbuchtext (Clean-Room-Regel CR.2).
**[unsicher]** = nur Drittquelle oder Forum.

## 1. Primärquellen

| Kürzel | Quelle |
|---|---|
| H23, H29 | vMix User Guide 23 / 29: <https://www.vmix.com/help23/>, <https://www.vmix.com/help29/> |
| SFR | Shortcut Function Reference: <https://www.vmix.com/help29/ShortcutFunctionReference.html> |
| API | HTTP-API: <https://www.vmix.com/help29/DeveloperAPI.html>, TCP-API: <https://www.vmix.com/help29/TCPAPI.html> |
| DL | Versionshinweise v21–29: <https://www.vmix.com/software/download.aspx> |
| CHL | Changelog 29.0.0.x: <https://forums.vmix.com/posts/t33735-vMix-29-Changelog> |
| FEAT | <https://www.vmix.com/software/features.aspx> |
| BUY | Editionsvergleich: <https://www.vmix.com/purchase/> |
| HW | <https://www.vmix.com/software/supported-hardware.aspx> |
| GT | Titel-Designer-Hilfe: <https://help.vmix.com/graphics/6/> |
| KB | Wissensdatenbank: <https://www.vmix.com/knowledgebase/> (Artikel 3 Latenz, 74 Input-Zählung, 144 API-Pegel, 178 Uhrformate, 232 Call direkt) |
| OMT | <https://www.openmediatransport.org/>, <https://github.com/openmediatransport> (MIT) |

Drittquellen für den API-Zustand (nur gelesen, kein Code übernommen):
Bitfocus-Companion-Modul <https://github.com/bitfocus/companion-module-studiocoast-vmix>,
vmix-js-utils <https://github.com/jensstigaard/vmix-js-utils>.

## 2. Editionen (BUY, Okt. 2026)

| | Basic HD | HD | 4K | Pro | Max |
|---|---|---|---|---|---|
| Preis | 60 $ | 350 $ | 700 $ | 1200 $ | 50 $/Monat |
| Inputs / davon Kamera+NDI+OMT | 4 / 3 | 1000 | 1000 | 1000 | 1000 |
| Auflösung max. | 1080p | 1080p | 4096×2160 | 4096×2160 | 4096×2160 |
| Overlay-Kanäle | 1 | 8 | 8 | 8 | 8 |
| Gäste | 0 | 1 | 4 | 8 | 8 |
| Wiederholungskameras | 0 | 0 | 1 | 8 | 8 |
| SRT-Ausgänge | 1 | 1 | 4 | 4 | 4 |

Ab 4K: Einzelaufnahme, Skripte, Mix-Inputs, PTZ, Zeichenebene,
Hochkant-HD, Titelanimationen, 2 Recorder, 4 externe + 2 Vollbild-Ausgänge.
Nur Windows 10/11.

## 3. Entwicklung v24–29 (DL, eigene Kurzfassung)

- **24** (2021): Wiederholung neu (8 Kameras, 240 fps, 20 Listen, A/B
  getrennt, Export); Titel als Stinger; "Input-Multiview" heißt Ebenen;
  dynamische Inputs/Werte; Matrix-Presets; Alarm-Hinweise.
- **25** (2022): LAN-Mitschau (HLS); 16 eigene Kategorien; Show-Pakete;
  Program als Input; Titelfarben per API; APIs standardmäßig nur LAN.
- **26** (2023): Effekte Weichzeichner/LUT; Sendeverzögerung; Hochkant;
  15 Mix-Inputs; SRT-Mehrspurton, CBR, SRT in Wiederholung; 1440p;
  Preview als Input.
- **27** (2024): Zoom; AV1/HEVC-Streaming; Ebenen-Editor; Bus-Übersicht,
  PFL je Input, Bus-Mixer; ~150 neue Aktionen (Ebenen animiert, Effekte,
  Kanalmischer); 4 externe Ausgänge.
- **28** (2025): Zeichenebene im Browser; Multiview 2; Overlays/Stinger auf
  Mix-Inputs; System-/Anwendungston; Auslöser nach Wiedergabezeit; Ebenen
  flach rechnen; 5 Streamziele; Ausgänge im API-Zustand.
- **29** (2025): OMT ein/aus; Wiederholung Vierfachansicht und
  Laufwerk je Kamera; 8 Overlays (vorher 4) und 8 Stinger; GO-Taste je
  Input; neue Bus-Kombinationen; Countdown-Auslöser; Alpha-Blende.

## 4. Fakten für Anforderungen

| Anforderung | Fakt | Quelle |
|---|---|---|
| SWI.4 | Offiziell 13 Effekte + Alpha-Blende (v29). Umgekehrte Varianten, Tür, Rolltor nur in Companion/Loupedeck **[unsicher]** | FEAT, DL, Companion |
| SWI.12, NFA.3 | vMix-Latenz Karte → Bildschirm ca. 2–3 Frames; höhere Bildrate halbiert | KB 3 |
| API.4 | Zustand: Inputs mit Position/Crop/Farbkorrektur/Ebenen/Liste/Titelfeldern/Wiederholung, Ausgänge (v28), Overlays mit Preview-Kennung, Mixe, Busse, dynamische Werte, 5 Streamkanäle | Companion, vmix-js-utils |
| API.8 | Pegel: Zustand = Amplitude × 100, Fader = Amplitude^¼ × 100 | KB 144 |
| GFX.1–2 | Objekttypen, Füll-/Effektarten, Abläufe In/Out/Seite 1–10/Dauer/Datenwechsel, 3 Animationen je Objekt, 6 Easing-Stile | GT |
| GFX.11 | ca. 100–200 MB GPU-Speicher je HD-Titel, ≤ 20 Titel je Show empfohlen | GT |
| CAL | WebRTC, STUN direkt, TURN-Rückfall; Mix-Minus automatisch | KB 232, Produktseite |
| SRC.11, OUT.6 | OMT: TCP, Ports 6400–6600, DNS-SD, Codec VMX 4:2:2(:4), Ton 32 Bit float bis 32 Kanäle, Qualität niedrig/mittel/hoch | OMT |
| NFA.14 | vMix rechnet 4:4:4 float, kein 10 Bit/HDR **[unsicher, Forum]** | FEAT, Forum |
| AUD.15 | MP4 nimmt ≤ 6 Tonkanäle, Rest als WAV | CHL |

## 5. Offen, nur an echter Installation prüfbar

1. XML-`type` für Zoom, OMT, Output, Preview, Farbbalken, System-Ton, Zeichenebene.
2. Existenz der umgekehrten Übergänge, Tür, Rolltor.
3. Vollständige Liste der Titel-Objektanimationen.
4. Auswahlliste der Master-Bildraten und Abtastraten.
5. Änderungen durch "optimiertes API-XML" (v28).
