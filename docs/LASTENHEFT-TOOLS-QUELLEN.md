# Quellen zum Lastenheft lmix tools

Zuordnung Anforderung → öffentliche Quelle (TCR.4), abgerufen 2026-10-28.
NDI-Doku: Basis `https://docs.ndi.video/all/`; Pfade unten relativ dazu.
NDI Tools 6.3.2 (2026-04-13). **(lmix)** = ohne Vorbild.

## Bestand unter Linux (Leitlinie)

| Aussage | Quelle |
|---|---|
| Keine NDI Tools für Linux; SDK, Discovery Server, Free Audio | <https://ndi.video/platform/linux> · using-ndi/utilities/discovery-service · developing-with-ndi/utilities/free-audio |
| NDI Record und Benchmark im SDK | <https://docs.ndi.video/docs/sdk/command-line-tools> |
| GStreamer `ndisrc`/`ndisink` (lädt das SDK zur Laufzeit) | <https://docs.rs/crate/gst-plugin-ndi/> |
| OBS-Plugin DistroAV (Linux als Flatpak/apt) | <https://github.com/DistroAV/DistroAV> |
| FFmpeg ohne NDI seit 2019 | <https://ffmpeg.org/pipermail/ffmpeg-devel/2019-March/240883.html> |
| OMT: Linux nur aus Quellcode, kein Betrachter | <https://github.com/openmediatransport> · `libomt` · `omtplugin` |

## Werkzeuge

| ID | Quelle |
|---|---|
| TGM.1–2 | using-ndi/ndi-tools/ndi-tools-for-windows/studio-monitor (Quellenwahl) · using-ndi/utilities/discovery-service · OMT: `OMTDiscoveryServer`, `libomt.h` (DNS-SD) |
| TGM.3–8 | (lmix) |
| TMO.1–11 | using-ndi/ndi-tools/ndi-tools-for-windows/studio-monitor · ndi-tools-for-mac/video-monitor · using-ndi/ndi-tools/release-notes (HDR 6.0, Deinterlace 6.2.1, Empfänger-Anmeldung 6.2) |
| TMO.12–13 | (lmix); Kiosk-Bedarf vgl. github.com/lplassman/NDI-Monitor |
| TTP.1–5 | using-ndi/ndi-tools/ndi-tools-for-windows/test-patterns; Muster selbst gestaltet (TCR.3) |
| TTP.6–7 | (lmix) |
| TSC.1–6 | ndi-tools-for-windows/screen-capture · screen-capture-hx · ndi-tools-for-mac/scan-converter; Linux-Weg: xdg-desktop-portal ScreenCast/RemoteDesktop |
| TVC.1–4 | ndi-tools-for-windows/webcam-input · ndi-tools-for-mac/virtual-input; Linux-Weg: PipeWire, v4l2loopback |
| TDI.1–2, TDI.4 | ndi-tools-for-mac/discovery · discovery-server-additional-information · release-notes (6.2, 6.3.0); OMT: `setredirect` in `libomt.h` |
| TDI.3, TDI.5 | (lmix) |
| TAN.1–3 | using-ndi/utilities/analysis |
| TAN.4–5 | (lmix) |
| TAU.1–2, TAU.4 | developing-with-ndi/utilities/free-audio |
| TAU.3 | using-ndi/ndi-tools/plugins (Audio Direct); Linux-Weg: PipeWire |
| TRC.1–3, TRC.5 | docs.ndi.video/docs/sdk/command-line-tools (NDI Record) |
| TRC.4, TRC.6 | (lmix) |
| TPL.1–4 | using-ndi/ndi-tools/plugins (VLC, Premiere Pro, After Effects, Final Cut Pro, Audio Direct) · github.com/openmediatransport/omtplugin |
| TLA.1 | ndi-tools-for-windows/router · OMT Matrix Router (openmediatransport.org) |
| TLA.2 | ndi-tools-for-windows/access-manager |
| TLA.3 | ndi-tools-for-windows/bridge · developing-with-ndi/utilities/bridge-service-for-windows |
| TLA.4 | ndi-tools-for-windows/remote-discontinued |
| TVS.1–10 | (lmix) |

## Protokolle (§2.2)

| Merkmal | Quelle |
|---|---|
| OMT: TCP, Ports 6400–6600, VMX, Qualitätsstufen, Vorschaustrom, Alpha, 16 Bit, Ton 32 Kanäle Float, Tally | <https://github.com/openmediatransport/libomtnet> · `libomt.h` · <https://www.openmediatransport.org> |
| OMT: Metadaten (Web-URL, PTZ über VISCA) | <https://github.com/openmediatransport/Metadata> |
| OMT: Discovery-Server TCP 6399, `~/.OMT/settings.xml` | <https://github.com/openmediatransport/OMTDiscoveryServer> |
| NDI: Discovery-Server TCP 5959, Konfiguration | using-ndi/utilities/discovery-service |
