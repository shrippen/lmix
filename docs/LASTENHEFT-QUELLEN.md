# Quellen zum Lastenheft

Einzige Grundlage: vMix User Guide 29, <https://www.vmix.com/help29/>
(abgerufen 2026-10-08, 210 Seiten). Seitennamen = `<Name>.html` dort.
Kein Handbuchtext im Repo (CR.2).

| Bereich | Handbuchseiten |
|---|---|
| Überblick, Mengen | Introduction, GettingStarted |
| INP | Inputs, InputButtons, InputSettings, InputSettingsGeneral, Categories, LivePause, GOButton, Undo, Snapshot |
| SRC | AddNewInput, Video, VideoList, Capture, NDIDesktopCapture, NDIandOMT, LocalDesktopCapture, Stream, SRTInput, ImageSequence, VideoDelay, Image, Photos, PowerPoint, Colour, Audio1, AudioInput, Title, VirtualSet, WebBrowser, Mix, OutputInput, PreviewInput, DVD, ZoomInput, TelestratorInput |
| PRC, LAY | InputSettingsColourAdjust, InputSettingsColourKey, ColourCorrection, InputEffects, InputSettingsPosition, InputSettingsMultiView, LayerDesigner, CopyFrom, InputSettingsAdvanced |
| SWI, OVL | TransitionsMenu, QuickPlay, Cut, Transitions, MergeEffect, StingerTransitions, FadeToBlack, FadeBar, Overlay, Overlay2 |
| AUD | Mixer, AudioSettings, AudioPlugins, AudioMeters, OutputAudioChannels, AudioBusManager, AudioOutputs, Audio (Settings), RecordingAudio |
| OUT | Display, SettingsOutputs, Fullscreen, ExternalOutput, ExternalOutput1, KeyFill, NDIandOMT1, NDIAlphaChannel, SRT, SafeAreas, WaveformMonitor, Alerts, Interlaced, VerticalVideoProduction |
| REC, MCR | Recording, Setup, RecordingStart, SecondRecorder, FaultTolerantRecordings, vMixVideoCodec, WAVFileRecord, Recording1, MultiCorder |
| STR | Streaming, StreamingSettings, StreamingQuality, StreamingMultiBitrate, StreamingMultipleDestinations, IndependentStreamControl, StreamingAdvanced, LiveLan |
| RPL | InstantReplay1, SystemRequirements, GettingStarted1, WorkflowOverview, LiveandRecordedmodes, ReplayEvents, UserInterfaceOverview, ControlOptions, ShortcutReference, ReplayMultiView, ReplaySettings, ReplayQuadView |
| GFX, DAT | Title, GTTitleDesigner, DataSources, DataSourcesTypes, DataSourcesSetup, DataSourcesTitleMapping, DataSourcesControl, DataSourcesImages |
| AUT | KeyboardShortcuts, ControllerOptions, AddShortcut, ShortcutTemplates, LocalGlobalShortcuts, MultipleShortcuts, AdvancedMIDIShortcuts, DynamicShortcuts, Activators, ActivatorsEdit, Triggers, Playlist, ScriptingandAutomation, VBNetScripting, WebScripting, PadLock |
| CAL | VideoCall, VideoCallSetup, CallManagerChat, vMixCallAudio, vMixCallMenu |
| API | ShortcutFunctionReference, DeveloperAPI, TCPAPI |
| WEB, TEL | WebController, WebControllerShortcuts, WebControllerSwitcher, WebControllerTallyLights, WebControllerTitleEditor, Telestrator |
| PER | TallyLights, TallyLightsSettings, PTZPanTiltZoom, PTZControlVirtualInputs, PTZControlShortcuts, ProductionClocks, Statistics, Performance |
| SHW | PresetsMenu, NewPreset, OpenPreset, SavePreset, LastPreset, PresetBundle, ImportExport |
| UI | GettingStarted, VideoControls, RightClickMenus, CogIconMenu, Options, Display |

## 2. Integrationsreferenzen (API.9)

Fremdprogramme, die heute an vMix andocken. Sie zeigen, was die lmix-API
mindestens leisten muss. Nur als Anforderungsreferenz gelesen, kein Code
übernommen, keine Quelle für vMix-Verhalten (CR.1).

| Programm | Nutzt | Folgt für lmix |
|---|---|---|
| Bitfocus Companion, vMix-Modul <https://github.com/bitfocus/companion-module-studiocoast-vmix> | Aktionen per HTTP/TCP mit Input, Wert, Dauer, Mix; Zustand zyklisch als Ganzes; Abo von Tally und Rückmeldungen per TCP; Feedback auf Tasten (PGM/PVW, Pegel, Mute, Aufnahme) | Vollständiger Aktionskatalog, Zustand + Ereignis-Abo, Pegel live; eigenes Companion-Modul (API.11) |
| Stream-Deck-Plugin (Handbuch 29: ControllerOptions) | Aktionen je Taste, Live-Vorschaubild eines Inputs etwa 4×/s | Vorschaubild je Input über die API |
| Tally-Software, z. B. Tally Arbiter <https://github.com/josephdadams/TallyArbiter> | Tally-Abo, Inputnummer/-name | Schlanker Ereigniskanal, auch für Mikrocontroller |
| Skript-Bibliotheken, z. B. vmix-js-utils <https://github.com/jensstigaard/vmix-js-utils> | Zustand lesen, Aktionen senden | Dokumentiertes, versioniertes Schema (API.10) |

## 3. CasparCG (Vorbild für Titel, §3.11)

Nur Funktionsreferenz; kein Code übernommen.

| Quelle | Inhalt |
|---|---|
| <https://github.com/CasparCG/help/wiki/AMCP-Protocol> | CG-Befehle (ADD, PLAY, STOP, NEXT, UPDATE, INVOKE, REMOVE, INFO), MIXER-Befehle mit Tween, Datenspeicher, TLS/CLS |
| <https://github.com/CasparCG/server> | GPL-3.0, daher kein Code in lmix |
| <https://casparcg.com/docs/wiki/server/consumers/ndi-consumer> | NDI-Consumer seit 2.3 |
| <https://casparcg.com/docs/wiki/server/producers/> | Producer, u. a. HTML (CEF) |
| <https://github.com/CasparCG/server/releases> | 2.4.0 stabil; 2.5 in Arbeit |

## 4. Nicht im Handbuch 29, an echter Installation zu klären

1. Vollständiges aktuelles XML-Zustandsschema (nur für den optionalen Kompatibilitätsmodus API.8).
2. Vollständige Liste der Übergangseffekte (Handbuch nennt sie nur im Überblick).
3. Titelformat und Animationsmodell im Detail (Verweis auf separate Designer-Hilfe).
4. Auswahlliste Master-Bildraten und Abtastraten.
