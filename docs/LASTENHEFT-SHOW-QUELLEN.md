# Quellen zum Lastenheft Show-Modus

Zuordnung Anforderung → Seite im QLab-5-Handbuch (SCR.3). Basis:
`https://qlab.app/docs/v5/` (abgerufen 2026-10-16, 97 Seiten). Pfade unten
sind relativ zu dieser Basis; in Klammern der Abschnitt der Seite.
**(lmix)** = ohne Vorbild im Handbuch.

## Cue-Modell (CUE)

| ID | Quelle |
|---|---|
| CUE.1 | fundamentals/inspector/ (Number, Name, Notes, Color) · fundamentals/workspace/ (Cue Number) · networking/using-osc/ (Zeichen in Nummern) |
| CUE.2 | fundamentals/inspector/ (Pre Wait, Post Wait, Zeitauflösung) |
| CUE.3 | fundamentals/cue-sequences/ · fundamentals/inspector/ (Continue) |
| CUE.4–6 | fundamentals/group-cues/ |
| CUE.7 | (lmix; heutiger Cue AUT.14) |
| CUE.8 | other-cues/memo-cues/ · fundamentals/inspector/ (Notes) |
| CUE.9 | other-cues/wait-cues/ |
| CUE.10 | other-cues/transport-cues/ (Start) · fundamentals/cue-lists/ |
| CUE.11 | fundamentals/inspector/ (Fade and stop others) |
| CUE.12 | fundamentals/inspector/ (Armed, Skip if disarmed) · other-cues/arm-and-disarm-cues/ |
| CUE.13 | other-cues/goto-cues/ |
| CUE.14 | fundamentals/inspector/ (Second trigger) |
| CUE.15 | other-cues/devamp-cues/ · audio/audio-cues/ (Schleifen) |
| CUE.16 | fundamentals/cue-sequences/ · fundamentals/inspector/ (Triggers) |
| CUE.17 | other-cues/transport-cues/ |
| CUE.18 | fundamentals/inspector/ (Ducking) |
| CUE.19 | tools/workspace-status-window/ (defekte Cues in Folgen) |
| CUE.20 | fundamentals/group-cues/ (Schachtelung); Mengen (lmix) |
| CUE.21 | (lmix) |

## Ablauf (RUN)

| ID | Quelle |
|---|---|
| RUN.1 | fundamentals/cues/ · fundamentals/workspace/ (GO) · fundamentals/cue-sequences/ · fundamentals/group-cues/ |
| RUN.2 | other-cues/goto-cues/ · general/keyboard-shortcuts/ (Abspielkopf bewegen) · fundamentals/workspace-settings/ (Steuerbefehle Standby/Sequenz) |
| RUN.3 | fundamentals/workspace/ (Active Cues) |
| RUN.4 | general/keyboard-shortcuts/ (Auswahl stoppen, doppelt = harter Stopp) · other-cues/transport-cues/ (Stop) |
| RUN.5 | fundamentals/workspace/ (Pause/Resume all) · other-cues/transport-cues/ (Pause) |
| RUN.6 | fundamentals/workspace-settings/ (General: Mindestabstand GO, Taste loslassen) · scripting/osc-dictionary-v5/ (Restzeit Doppel-GO-Fenster); Standardwert 300 ms (lmix) |
| RUN.7 | fundamentals/cue-lists/ |
| RUN.8 | fundamentals/inspector/ (Hinweis zur Zeit) · audio/introduction-to-audio/ (Gleichlauf) |
| RUN.9 | tools/override-controls/ · scripting/osc-dictionary-v5/ (overrides) · networking/network-cues/ (Warnungen) |
| RUN.10 | tools/auditioning-cues/ · fundamentals/workspace-settings/ (Audition) · fundamentals/cue-sequences/ (Vorschau) |
| RUN.11 | fundamentals/workspace/ (Reset) · other-cues/transport-cues/ (Reset) |
| RUN.12 | fundamentals/cue-carts/ |
| RUN.13 | other-cues/transport-cues/ (Load to Time) · tools/tools-menu/ |
| RUN.14 | (lmix) |

## Klang (SND)

| ID | Quelle |
|---|---|
| SND.1 | audio/audio-cues/ (Datei, Formate, Startverzögerung komprimierter Formate) · audio/introduction-to-audio/ |
| SND.2 | audio/audio-cues/ (Pegel) · audio/fading-audio/; Ein-/Ausblendzeit im Cue (lmix) |
| SND.3 | audio/audio-cues/ (Wiederholungen) |
| SND.4 | (lmix) |
| SND.5 | audio/audio-cues/ (Cue-Matrix, Kreuzungspegel) · audio/introduction-to-audio/ |
| SND.6 | audio/fading-audio/ (Ziele Cue/Patch, Ziel stoppen, nur gesetzte Parameter) · fundamentals/cues/ |
| SND.7 | audio/fading-audio/ (absolut/relativ) |
| SND.8, SND.9 | audio/fading-audio/ (Kurven, Pegelbereich) |
| SND.10 | audio/audio-output-patch-editor/ · audio/introduction-to-audio/ · fundamentals/workspace-settings/ (Audio Outputs); Zuordnung pro Rechner (lmix) |
| SND.11 | audio/fading-audio/ (Fade auf Patch) · audio/audio-output-patch-editor/ (Gesamtpegel) |
| SND.12 | audio/audio-cues/ (Startverzögerung) · other-cues/transport-cues/ (Laden) |
| SND.13 | fundamentals/workspace-settings/ (Volume Limits) |
| SND.14 | (lmix) |
| SND.15 | audio/introduction-to-audio/ (Gleichlauf, Drift zwischen Geräten) · audio/mic-cues/ |
| SND.16 | audio/audio-output-patch-editor/ (fehlendes Gerät) · audio/audio-cues/ (Warnungen) |
| SND.17 | (lmix) |
| SND.18 | audio/audio-cues/ (Start-/Endzeit) |
| SND.19 | audio/fading-audio/ (Kreuzungspegel faden) |
| SND.20 | audio/audio-cues/ (Trim) |
| SND.21 | audio/audio-cues/ (Abschnitte) |
| SND.22 | audio/audio-cues/ (Pegelkurve) |
| SND.23 | audio/mic-cues/ |

## Bild und Bühne (VIS)

| ID | Quelle |
|---|---|
| VIS.1 | video/introduction-to-video/ · video/video-output/ (mehrere Bühnen, Routen auf Bildschirme) · fundamentals/workspace-settings/ (Video); Zuordnung pro Rechner (lmix) |
| VIS.1a | video/video-output/ (NDI als Ausgabegerät) · video/using-ndi/ |
| VIS.2 | video/video-cues/ (Standbild, Zeit, Schleifen, am Ende halten) |
| VIS.3 | video/video-cues/ (Ebenen) |
| VIS.4 | video/video-cues/ (Deckkraft) · video/fading-video/ |
| VIS.5 | video/video-cues/ (Geometrie, Füllmodus) |
| VIS.6 | video/video-cues/ · video/blend-modes/ |
| VIS.7 | (lmix); sinngemäß video/fading-video/ (relative Deckkraft) |
| VIS.8 | video/video-output/ (Schwarz zwischen Cues) · tools/tools-menu/ (Desktop schwarz) |
| VIS.9 | video/video-cues/ (Geometrie) · video/fading-video/ |
| VIS.10 | video/video-cues/ (Videoton) |
| VIS.11 | video/video-output/ (Bühnen, Bereiche, Routen) |
| VIS.12 | (lmix) |
| VIS.13 | video/video-cues/ (Effekte) · video/fading-video/ |
| VIS.14 | video/text-cues/ |
| VIS.15 | video/camera-cues/ · video/using-ndi/ |
| VIS.16 | video/video-output/ (Kantenüberblendung, Verzerrung, Masken) |
| VIS.17 | (lmix) |

## Externe Ziele (EXT)

| ID | Quelle |
|---|---|
| EXT.1 | fundamentals/workspace-settings/ (Network Outputs) · networking/network-cues/ (Patches, mehrere Ziele) |
| EXT.2 | (lmix) |
| EXT.3 | (lmix; QLab hat kein HTTP); Analogie networking/network-cues/ |
| EXT.4 | (lmix); Analogie networking/network-cues/ (Gerätebeschreibungen) |
| EXT.5 | networking/network-cues/ (Testsenden) |
| EXT.6 | networking/network-cues/ (OSC) · scripting/osc-dictionary-v5/ |
| EXT.7 | scripting/osc-queries/ |
| EXT.8 | networking/network-cues/ (Dauer, wiederholt senden, blenden) |
| EXT.9 | networking/midi-cues/ · networking/using-midi-and-msc/ |
| EXT.10 | lighting/introduction-to-lighting/ |

## Variablen (VAR)

| ID | Quelle |
|---|---|
| VAR.1–3 | (lmix); Analogie scripting/osc-queries/ und scripting/osc-dictionary-v5/ (Werte ändern mit +/−) |

## Drehbuch-Import (IMP)

| ID | Quelle |
|---|---|
| IMP.1–6, 8, 9 | (lmix); Analogie: Cues über die Fernsteuerung anlegen, scripting/osc-dictionary-v5/ |
| IMP.4 | zusätzlich tools/workspace-status-window/ · tools/file-target-search/ |
| IMP.7 | fundamentals/workspace-settings/ (Cue Templates) · fundamentals/workspace-templates/ |

## Bedienung und Fernsteuerung (CTL)

| ID | Quelle |
|---|---|
| CTL.1 | networking/using-osc/ · scripting/osc-dictionary-v5/ (Grundlagen, Show-Befehle) · networking/using-midi-and-msc/ · fundamentals/workspace-settings/ (Controls) · general/keyboard-shortcuts/ |
| CTL.2 | scripting/osc-dictionary-v5/ (Statusmeldungen, Antworten, Laufzustand); Restzeit fertig gerechnet und Taktbegrenzung (lmix) |
| CTL.3 | networking/show-control-broadcast/ · scripting/osc-dictionary-v5/ (Lebenszeichen, Anmeldung festhalten) |
| CTL.4 | networking/show-control-broadcast/ (Nachrichtenform je Abonnent; feste Ziele nur für MSC); feste OSC-Abonnenten (lmix) |
| CTL.5 | fundamentals/workspace-settings/ (OSC Access) · scripting/osc-dictionary-v5/ (connect) · networking/stream-deck/ (kein Passcode möglich); Trennung localhost/LAN (lmix) |
| CTL.6 | general/keyboard-shortcuts/ (Escape fest) · fundamentals/workspace-settings/ (Keyboard) |
| CTL.7 | scripting/osc-dictionary-v5/ (Antworten, Lebenszeichen) |
| CTL.8 | networking/stream-deck/ · networking/network-cues/ (Companion als Gerätebeschreibung) |
| CTL.9 | networking/qlab-remote/ |
| CTL.10 | fundamentals/workspace-settings/ (OSC- und MIDI-Lernen) · networking/using-midi-and-msc/ |
| CTL.11 | tools/workspace-status-window/ (Auslöse-Protokoll) |
| CTL.12 | networking/using-midi-and-msc/ · networking/show-control-broadcast/ |
| CTL.13 | fundamentals/workspace-settings/ (General: Cue beim Öffnen) · general/qlab-preferences/ |

## Sicherheit (SAF)

| ID | Quelle |
|---|---|
| SAF.1, SAF.2 | fundamentals/workspace-settings/ (Panikdauer) · fundamentals/workspace/ (Panik) · scripting/osc-dictionary-v5/ (panic, panicInTime) |
| SAF.3 | general/keyboard-shortcuts/ · scripting/osc-dictionary-v5/ (hardStop) |
| SAF.4 | (lmix) |
| SAF.5 | fundamentals/workspace-settings/ (Keyboard, Controls) |
| SAF.6 | (lmix); Gegenpol fundamentals/workspace-settings/ (Cue beim Öffnen) |
| SAF.7 | (lmix) |
| SAF.8 | fundamentals/workspace/ (Show-Modus) · general/qlab-preferences/ (Systemfunktionen im Show-Modus) |

## Oberfläche (SUI)

| ID | Quelle |
|---|---|
| SUI.1 | (lmix) |
| SUI.2–4 | fundamentals/workspace/ (Spalten, GO, Standby-Anzeige, Notizen, Active Cues) |
| SUI.5 | tools/workspace-status-window/ |
| SUI.6 | (lmix) |
| SUI.7 | tools/monitor-windows/ |
| SUI.8 | (lmix) |
| SUI.9 | (lmix; Kante) |
| SUI.10 | tools/tools-menu/ (Umnummerieren) · tools/paste-cue-properties/ · tools/find/ · fundamentals/workspace-settings/ (Auto-Nummer) |
| SUI.11 | fundamentals/workspace-settings/ (Abspielkopf an Auswahl koppeln) |
| SUI.12 | fundamentals/inspector/ (Markierung, Zweitfarbe) |
| SUI.13 | (lmix) |

## Show-Datei (SSH)

| ID | Quelle |
|---|---|
| SSH.1, SSH.2 | (lmix) |
| SSH.3 | (lmix; QLab hält Patches im Dokument, fundamentals/workspace-settings/) |
| SSH.4 | fundamentals/managing-workspaces/ (Projektordner) |
| SSH.5 | (lmix) |

## Licht (LGT)

| ID | Quelle |
|---|---|
| LGT.1, LGT.2 | lighting/introduction-to-lighting/ (Art-Net, USB-DMX); sACN (lmix) |
| LGT.3 | lighting/light-patch-editor/ (Patch-Tabelle); Adressen pro Rechner (lmix) |
| LGT.4 | lighting/light-patch-editor/ (Definitionen, virtuelle Parameter) · lighting/introduction-to-lighting/ |
| LGT.5 | lighting/light-library/; Import offener Bibliotheken (lmix) |
| LGT.6 | lighting/introduction-to-lighting/ (Gruppen) · lighting/lighting-command-language/ |
| LGT.7 | lighting/light-cues/ (Befehle, Kurve) |
| LGT.8 | lighting/introduction-to-lighting/ (nicht genannte Werte bleiben) · lighting/light-cues/ (vorherige einrechnen) |
| LGT.9 | lighting/lighting-command-language/ |
| LGT.10, LGT.11, LGT.12 | lighting/light-dashboard/ (Befehlszeile, über Zeit, Farbwähler, Aufnehmen, Ursprung, DMX-Status, Parken, Submaster) |
| LGT.13 | (lmix) |
| LGT.14 | lighting/light-cues/ (defekte Licht-Cues) · lighting/light-patch-editor/ |

## Objekt-Audio (OBJ)

| ID | Quelle |
|---|---|
| OBJ.1, OBJ.2 | audio/audio-map-editor/ (Marken, Gewicht, Schatten, Filter) · audio/object-audio/ |
| OBJ.3 | audio/object-audio/ (Cue- und Karten-Objekte) · audio/audio-map-editor/ (Ausdehnung) |
| OBJ.4 | audio/audio-cues/ (Objekte-Reiter, Objekt-Matrix) |
| OBJ.5 | audio/fading-audio/ (Pfad) |
| OBJ.6, OBJ.7 | audio/audio-map-editor/ (Wärmebild, Testobjekt) |

## Effekt-Plugins (FX)

| ID | Quelle |
|---|---|
| FX.1 | audio/audio-cues/ (Effekte je Cue, Reihenfolge, Umgehen, Editor) |
| FX.2 | audio/audio-output-patch-editor/ (Effekte je Cue-Ausgang, Kanäle koppeln) |
| FX.3 | audio/fading-audio/ (Effektparameter faden) |
| FX.4 | audio/audio-cues/ (Nachklangzeit) · general/keyboard-shortcuts/ (harter Stopp) |
| FX.5 | audio/audio-cues/ (fehlendes Effekt-Plugin, Kanalzahl) |
| FX.6 | (lmix) |
| FX.7 | audio/audio-cues/ (nur Effekte, keine Erzeuger) |

## Timecode (TC)

| ID | Quelle |
|---|---|
| TC.1, TC.3, TC.4 | networking/using-timecode/ (Liste synchronisieren, Start/Stopp-Verhalten, Freilauf) |
| TC.2 | fundamentals/inspector/ (Timecode-Auslöser, Übernahme) |
| TC.5 | networking/timecode-cues/ |
| TC.6 | networking/timecode-cues/ (Kanal 0 als Schutz, Warnung vor Lautsprechern); Zonen-Warnung (lmix) |
| TC.7 | tools/timecode-status-window/ |
| TC.8 | (lmix; `LASTENHEFT.md` PER.9) |

## Ausschlüsse (SX)

| Was | Quelle |
|---|---|
| Tonhöhe, Rate | audio/audio-cues/ |
| Skript-Cue | scripting/script-cues/ |
| Mehrplatz-Bearbeitung | networking/collaboration/ |
| MIDI-Datei-Cue | networking/midi-file-cues/ |
