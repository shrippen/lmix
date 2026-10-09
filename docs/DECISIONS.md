# Entscheidungen

Format: Datum, Kontext, Optionen, Entscheidung.

## 2026-09-15 — M1: `app-desktop` als eigenständiges Crate statt Fork/Lib-Nutzung von strom-frontend

**Kontext**: `claude.md` sagt für `app-desktop`: "Fork/Neuaufbau auf Basis
strom-frontend". Vor dem Bauen zwei Explore-Agents auf `workspace/strom`
angesetzt, um zu prüfen, was von `strom-frontend` für eine vMix-Style
Schalter-Console wiederverwendbar ist.

**Befund**: Es existiert keine PGM/PVW-Switcher-UI in Rust/egui — das
einzige Vision-Mixer-Bedienelement ist eine eigenständige Browser-Seite
(`backend/static/vision-mixer.html`, Vanilla-JS), komplett losgelöst vom
egui-Prozess. `StromApp` ist ein ~100-Felder-Monolith mit zentralem
Event-Pump; `AppMode` wird nicht mal aus der Lib exportiert. Als
Bibliothek für ein zusätzliches Panel taugt das ohne Upstream-Patch nicht.

**Entscheidung**: `app-desktop` ist ein neues, unabhängiges egui/eframe-
Binary-Crate (kein Fork, keine Lib-Abhängigkeit auf `strom-frontend`),
das direkt gegen `strom_types` + die dokumentierte REST/WS-API arbeitet,
orientiert an den *Mustern* aus `strom-frontend` (Thumbnail-Polling→
`TextureHandle`, Hotkey-Idiom) und am Tastenschema/API-Contract von
`vision-mixer.html`. Hält den Diff gegen `workspace/strom` bei exakt
Null für M1 (verifiziert: `git log main..dev` im Submodule zeigt nur die
zwei M0-Fixes, keine neuen Commits).

**Zwei Implementierungs-Gotchas dabei gefunden** (Details in
`docs/STROM-ANATOMY.md`): `strom_types::vision_mixer::Source` ist
extern-tagged lowercase auf der Leitung (`{"input": N}`, nicht aus dem
Rust-Typ ersichtlich ohne einen echten Request mitzuschneiden) und
`PropertyValue` ist `#[serde(untagged)]` mit `Int` vor `UInt` — ein
JSON-Integer wird nie als `UInt` erkannt.

## 2026-09-15 — Korrekturen an den Repo-Struktur-Annahmen in `claude.md`

**Kontext**: `claude.md` nennt Crates `strom`, `strom-frontend`, `strom-types`,
`strom-mcp-server`. Beim tatsächlichen Klonen von `Eyevinn/strom` (main)
stimmten drei Annahmen nicht mit dem Code überein.

**Befund**:
1. Kein `strom-mcp-server`-Crate. Workspace-Mitglieder sind `types`
   (Paket `strom-types`), `backend` (Paket `strom`), `frontend` (Paket
   `strom-frontend`). MCP ist ein Modul im Backend (`backend/src/mcp/`,
   Endpunkt `/api/mcp`).
2. Frontend ist WASM-first (Trunk-Build, `docs/DEVELOPMENT.md` beschreibt nur
   den Browser-Weg), aber `frontend/src/main.rs` hat einen vollwertigen
   nativen `eframe::run_native`-Pfad (`#[cfg(not(target_arch = "wasm32"))]`).
   Trunk/wasm32-Target werden für uns nicht gebraucht: `backend/build.rs`
   fällt ohne `trunk` automatisch auf eine Platzhalterseite zurück.
3. Server bindet hart auf `0.0.0.0`, kein `--host`-Schalter existierte.

**Entscheidung**: Punkt 1-2 sind reine Doku-Korrekturen, keine Neubewertung
der Entscheidung "Basis = STROM-Workspace, GUI nativ" — die bleibt bestehen,
ist jetzt nur genauer begründet. Punkt 3 siehe eigener Eintrag unten.

## 2026-09-15 — `--host`/`STROM_HOST`-Patch statt Firewall

**Kontext**: Entscheidung 4 in `claude.md` verlangt Server nur auf
`127.0.0.1`. Upstream-Code bindet hart auf `0.0.0.0`
(`backend/src/main.rs`, zwei Stellen). Optionen: (a) kleiner Code-Patch mit
konfigurierbarem Host, (b) nur OS-Firewall/Netzwerk-Namespace, kein Code-Diff,
(c) vorerst ignorieren, nur dokumentieren.

**Entscheidung** (User, per Nachfrage): (a). Neues `host`-Feld in
`ServerConfig`/`Config`, CLI `--host` + Env `STROM_HOST`, gleiche
Priorisierungskette wie `--port` (figment). Default bleibt `0.0.0.0` —
keine Verhaltensänderung für bestehende Deployments, nur wer explizit
`--host 127.0.0.1` setzt, bekommt Loopback-only. Umgesetzt als zwei saubere
Commits auf `workspace/strom` (`dev`-Branch, Diff gegen `main`):
`4bce876` (Config-Feld + main.rs-Verdrahtung), `779cc60` (siehe nächster
Eintrag — anderes Thema, gleicher Anlass).

**Begründung für (a) statt (b)**: robuster (kein Firewall-Regel-Vergessen-
Risiko), minimal genug (~70 Zeilen, rein additiv), und potenziell
upstream-tauglich als PR.

**Verifiziert**: `curl http://127.0.0.1:8080/health` antwortet, `ss -tlnp`
zeigt den API-Server nur auf `127.0.0.1:8099`/`:8080` gebunden (getestet mit
beiden Ports).

**Offene Lücke**: der RTSP-Discovery-Server (`backend/src/rtsp_server.rs`)
bindet weiterhin hart auf `0.0.0.0:8554`, unabhängig vom `--host`-Patch. Als
Media-Plane-Dienst (kein Control-API-Endpunkt) vorerst nicht angefasst —
Bewertung, ob das in Entscheidung 4 mitgemeint war, zurückgestellt auf
M3 (Robustheit/Netzwerk-Review).

## 2026-09-15 — Tokio-Runtime-Fix im eigenständigen nativen Frontend

**Kontext**: `cargo run -p strom-frontend` (Prozess-Trennung laut
Entscheidung 4) paniert sofort: "there is no reactor running, must be
called from the context of a Tokio 1.x runtime" (`frontend/src/ws.rs:68`).
Ursache: der eingebettete native-GUI-Pfad (`backend/src/gui.rs` →
`strom_frontend::run_native_gui`) läuft im Tokio-Kontext des Backends; das
eigenständige Binary (`frontend/src/main.rs`) hatte nie einen eigenen.

**Entscheidung**: `tokio::runtime::Runtime::new()` + `.enter()` vor
`eframe::run_native` im nativen `main()`, Guard für die Prozesslaufzeit
gehalten. Kleinstmöglicher Fix (7 Zeilen), reiner Bugfix ohne
Verhaltensänderung für den eingebetteten Pfad. Commit `779cc60` auf `dev`.

**Verifiziert**: natives Fenster verbindet per WebSocket + REST gegen
`127.0.0.1:8080`, lädt 1537 GStreamer-Elemente und 32 Blocks, kein Absturz.

## 2026-09-15 — GUI/Server-Prozesstrennung: `no-gui`-Feature statt nur `--headless`

**Kontext**: Das Backend kann per Default (ohne `--headless`) selbst ein
natives GUI-Fenster im selben Prozess starten (`backend` hängt direkt von
`strom-frontend` ab). Das würde Entscheidung 4 ("GUI-Crash darf Engine nie
mitnehmen") bei jedem versehentlichen Start ohne `--headless` verletzen.

**Entscheidung**: Backend immer mit `--features no-gui` bauen (compile-time
erzwungen laut `backend/src/main.rs`: "Always headless when no-gui feature is
enabled"), nicht nur den Laufzeit-Flag `--headless` verwenden. Kein Diff
gegen strom nötig — das Feature existiert bereits upstream, wird nur von uns
konsequent genutzt.

## 2026-09-15 — libomt-Build verifiziert + omt-rs-Spike erfolgreich → Option (a)

**Kontext**: `docs/OMT-PLAN.md` nannte zwei offene Vorbedingungen, bevor
zwischen Option (a) (OmtBlock in strom) und (b) (omtpw-bridge) entschieden
werden kann: (1) `libomt` selbst aus Quelle bauen und per `ldd` verifizieren,
dass keine .NET-Runtime-Reste im AOT-Kompilat stecken, (2) `omt-rs` gegen
dieses Build spiken.

**Durchgeführt** (alles im Scratchpad, nicht Teil des Repos — reiner
Machbarkeits-Nachweis, kein Vendoring):
1. `dotnet-sdk-8.0` via pacman installiert (User hat das manuell erledigt,
   `sudo` hatte in der Agent-Session kein Passwort-Terminal).
2. `libomtnet` (netstandard2.0) und danach `libomt` (`buildlinuxx64.sh`,
   AOT-Publish für `linux-x64`) sauber gebaut — Ergebnis: `libomt.so`,
   8,2 MB, Version 1.0.0.19.
3. `ldd libomt.so`: nur `libm.so.6`, `libc.so.6`, `ld-linux-x86-64.so.2` —
   keine .NET-Runtime-Abhängigkeit (kein `libcoreclr.so` o. ä.). Der in
   `OMT-PLAN.md` offen gelassene Schritt ist damit verifiziert, nicht mehr
   nur angenommen.
4. `udoschneider/omt-rs` geklont, mit `OMT_LIB_DIR` auf das eigene Build
   gezeigt (trotz Versions-Offset: omt-rs ist auf libomt v1.0.0.16 gepinnt,
   unser Build ist v1.0.0.19) — `cargo build --workspace` kompiliert ohne
   Fehler, `bindgen` läuft gegen den vendorten Header durch.
5. `cargo run --example discovery` (gegen `LD_LIBRARY_PATH` auf unser Build
   gesetzt) läuft, startet einen echten mDNS-Scan, meldet korrekt "No
   sources found" (kein OMT-Sender im Netz vorhanden) — kein Crash, kein
   Linker-/ABI-Fehler trotz Versions-Offset.

**Nicht verifiziert** (nächster Schritt, nicht mehr Vorbedingung für die
Options-Entscheidung): tatsächlicher Frame-Empfang über `Receiver` gegen
einen echten Sender (`omtsendtest` o.ä.) — dafür fehlte eine Gegenstelle im
Netz. Blockiert die OmtBlock-Implementierung nicht, nur den späteren
Latenz-Messschritt aus dem M2-Akzeptanzkriterium.

**Entscheidung**: Option (a) — OmtBlock in der strom-Block-Registry, wie in
`docs/OMT-PLAN.md` skizziert (`appsrc`-Bridge, SPSC-Queue mit Frame-Drop).
Die ursprüngliche Sorge bei (a) (eigene C-FFI-Schicht) entfällt durch
`omt-rs`; der Latenzvorteil eines direkten Blocks (kein Zwischen-Prozess wie
bei (b)) macht damit den Ausschlag, wie in `OMT-PLAN.md`s Empfehlung bereits
vermutet — jetzt durch den Spike bestätigt statt nur angenommen.

## 2026-09-15 — Repo-Hosting und Push-Verhalten

**Kontext**: Wohin soll das lmix-Repo, wann pushen?

**Entscheidung** (User): Remote `https://git.arianw.de/shrippen/lmix`. Push
nur nach Abschluss eines Meilensteins oder auf explizite Anweisung — nie
automatisch nach einzelnen Commits während der Arbeit an einer Meile.

## 2026-09-19 — Quellen-Builder, Perf-Overlay und Selftest in der Console

**Kontext**: Zum Testen braucht die Console eigene Quellen und messbare
Performance (Latenz, Jitter), ohne Node-Editor.

**Umsetzung** (nur `workspace/app-desktop/`, keine Server-Änderung):
- `sources.rs`/`source_ui.rs`: Quellenliste (Testbilder: Bars, Farbtafel,
  Schwarz/Weiss/Rot/Grün/Blau, Ramp, Checkers, Zone-Plate, Ball, Rauschen …;
  V4L2-Geräte per `/api/discovery/devices`; Dateien; URLs) → ein Flow
  `Quelle → Thumbnail-Tap → Vision Mixer`. 2–16 Inputs.
- `perf.rs`: Overlay (F3 / `--debug-perf`), CSV aller Samples (`--perf-log`).
  Metriken: UI-Frame/-Kosten, Thumbnail-RTT/-Abstand, WS-Abstand,
  Take-RTT, Take→WS-Event, State-RTT, Pipeline-Latenz. Jitter = RFC-3550-
  Mean-Deviation.
- `selftest.rs`: `--selftest` (headless): Flow bauen, alle Inputs pollen,
  alle 1,5 s PVW+Take (cut/fade/dip/wipe), Budgets prüfen, Exit-Code.
  Beispiel: `app-desktop --server URL --selftest --selftest-patterns 8
  --selftest-file x.mp4 --selftest-url http://… --selftest-devices`.

**Befunde**:
1. `autovideoconvert` (gewählt, weil CUDA-GL-Probe besteht) liefert auf
   GStreamer 1.28.7 `not-negotiated` an jedem Mixer-Input. Fix: upstream-
   tauglicher strom-Commit `bd86930` (`STROM_FORCE_SOFTWARE_CONVERT=1`),
   in `run.sh` gesetzt. Default-Verhalten unverändert.
2. `builtin.media_player` liefert ohne `decode=true` nur das encodierte
   Signal (Passthrough) → not-negotiated. `builtin.local_input` (pipewire)
   braucht explizit Auflösung + Framerate.
3. **Thumbnails sind serverseitig fest auf 1 fps / 320x180** begrenzt
   (`ThumbnailTapConfig::default`, `update_interval = 1 s`, nicht per API
   änderbar). Die Console-Monitore aktualisieren deshalb höchstens 1×/s,
   egal wie schnell gepollt wird. Der Multiview-Ausgang des Mixers
   (`multiview_out`) ist nur per WHEP erreichbar, nicht per REST.
   → egui-/API-Grenze für "echte" Live-Monitore. Optionen: (a) upstream-PR
   für konfigurierbares Thumbnail-Intervall, (b) WHEP-Empfang in der
   Console, (c) Qt/Tauri-Shell (Entscheidung spätestens M3). Offen.
4. Kein GL auf dem Host → CPU-Compositor (PGM 1080p30 + Multiview 720p30).

**Messwerte** (strom Release-Build, dieser Host, 20 s, Selftest):

| Inputs | Backend-CPU | RSS | Pipeline-Latenz | thumb_rtt p95 | take→event p95 | Start |
|-------:|------------:|----:|----------------:|--------------:|---------------:|------:|
| 8      | ~700 %      | 0,7 GB | 233 ms max   | 3 ms          | 11 ms          | 10 s  |
| 16     | ~1700 %     | 2,4 GB | **2,2 s**    | 10 ms         | 26 ms          | 6 s   |

8 Inputs bestehen alle Budgets; 16 scheitern an der Pipeline-Latenz
(CPU-Compositor). Debug-Build war noch schlechter (8 Inputs: 3–4 s
Latenz im GUI). Take-Latenz und REST sind unkritisch, der Engpass ist der
Compositor.

## 2026-09-27 — Korrektur: GPU vorhanden; GPU-Pipeline zuerst, herstellerneutral

**Kontext**: Eintrag 2026-09-19, Befund 4 ("Kein GL auf dem Host →
CPU-Compositor") ist falsch. Host: RTX 3090, OpenGL 4.6 (NVIDIA-EGL),
NVDEC/NVENC. `glvideomixerelement` und `cudacompositor` laufen per
`gst-launch-1.0` headless (ohne `DISPLAY`) fehlerfrei. Der CPU-Fallback
kam aus stroms Auto-Auswahl (`has_hardware_gl()`) oder dem
`STROM_FORCE_SOFTWARE_CONVERT`-Workaround; Ursache offen (M1b G.1). Die
Messwerte vom 2026-09-19 zeigen damit den CPU-Pfad, nicht die
Hardwaregrenze.

**Entscheidung** (User):
1. Performance hat Vorrang: GPU-Pipeline (M1b) kommt vor allen weiteren
   Features. Ziel: möglichst viele parallele Inputs/Outputs live.
2. NVIDIA, AMD und Intel gleichrangig: herstellerneutraler GL-Kern,
   herstellerspezifische Decode/Encode-Pfade hinter einer Abstraktion.
3. AMD-/Intel-Hardware derzeit nicht verfügbar: Pfade jetzt bauen und
   optimieren, nur per Unit-/Snapshot-Test absichern, als `unverified`
   markieren; Abnahme auf echter Hardware später (M5b).

Details: `docs/ROADMAP.md`.

## 2026-09-30 — M1b: Ursachen des CPU-Pfads, CUDA-Mixer auf NVIDIA

**Kontext**: M1b G.1 (Diagnose), G.7 (GL vs. herstellereigener
Compositor), G.3 (Backendwahl). Host: RTX 3090, GStreamer 1.28.7,
Treiber 615.71.

**Befunde G.1** (Ursachen für "kein GL"/CPU-Compositor):
1. stroms GL-Probe (`gpu.rs`) endete in einem unbeschränkten `appsink`.
   `glupload` verhandelt in 1.28 dann DMABuf statt GLMemory, der Puffer
   trägt keinen GL-Kontext → `has_hardware_gl()` = false → "auto" wählt
   still den CPU-Compositor. Fix: Caps-Filter `memory:GLMemory`
   (strom-Commit, upstream-tauglich). Allein das: 8 Testbilder 700 → 92 %
   CPU, 16 → 1700 → 160 %.
2. GStreamer wählt auf diesem Host GLES (EGL). GLES-Kontexte sind nicht
   CUDA-kompatibel (`OpenGL context could not support PBO download`):
   NVDEC fällt dann still auf Systemspeicher zurück. Mit
   `GST_GL_API=opengl3` gibt NVDEC GLMemory aus.
3. GL-Speicher aus einer anderen Pipeline (Media-Player dekodiert in einer
   eigenen) scheitert in `glcolorconvert`, wenn das `GstGLDisplay` nicht
   geteilt ist.
4. `autovideoconvert` `not-negotiated` (2026-09-19) betraf nur den
   CPU-Builder; mit GPU-Backend tritt es nicht auf.
   `STROM_FORCE_SOFTWARE_CONVERT` bleibt in strom, `run.sh` braucht es
   nicht mehr. Nachtrag: Das CPU-Backend startete ohne Override gar nicht
   und lieferte sonst schwarzes PGM; seit strom `b38ace4` nutzt es
   `videoconvert` + Speicherbrücke und läuft ohne Override.

**Messung G.7** (gst-launch, 16 × 1080p30 H.264 aus Datei, NVDEC,
4×4-Raster 1080p30, `bench.sh` im Scratchpad):

| Pfad | fps | CPU | GPU | VRAM |
|---|---:|---:|---:|---:|
| NVDEC → GLMemory → `glvideomixer` | 25 | 108 % | 17 % | 5,7 GB |
| NVDEC → System → `glupload` → `glvideomixer` | 30 | 89 % | 24 % | 5,5 GB |
| NVDEC → CUDA → `cudacompositor` | 30 | 6 % | 1 % | 4,7 GB |
| NVDEC → System → `compositor` (CPU) | 30 | 240 % | 12 % | 4,1 GB |

32 Inputs: NVDEC bei 100 % (Hardwaregrenze dieser Karte für diesen
Inhalt); `cudacompositor` 28 fps/28 % CPU, GL 27 fps/186 %.

**Entscheidung** (nach Leitprinzip "herstellereigener Pfad nur, wenn
messbar schneller"):
1. NVIDIA: Vision Mixer mit neuem Backend `cuda` (`cudacompositor`),
   Frames bleiben von NVDEC bis NVENC im CUDA-Speicher. strom-Patch,
   upstream-tauglich: gleiche Topologie wie der CPU-Builder, `cudaupload`
   statt `videoconvert`, Crop über `videocrop`-Meta.
   Einschränkung: keine Shader-FX (Wipes fallen wie auf CPU auf Fade
   zurück, Looks/Master-FX fehlen). Wer FX braucht, wählt `gl`.
2. AMD/Intel: GL-Mixer (`gpu`), unverifiziert (M5b).
3. Backendwahl in der Console (`gpu_profile.rs`): Hersteller aus stroms
   GL-Renderer (WS `SystemStats`), `--mixer-backend auto|cuda|gl|cpu`.
   `auto` ohne Hardware-GPU bricht ab; CPU nur explizit.
4. strom teilt einen CUDA-Kontext über alle Pipelines
   (`gpu::share_gpu_contexts`). Ohne: pro Media-Player-Pipeline ein
   Kontext; 16 Inputs 9,6 GB VRAM/4,1 GB RSS, mit: 4,6 GB/1,0 GB.
5. Thumbnail-Tap skaliert CUDA-Frames auf der GPU (`cudaconvertscale`),
   lädt nur 320×180 herunter.

Messwerte der Abnahme: `docs/LATENCY.md`.

## 2026-09-30 — M1b: weitere Befunde und Fixes (strom)

1. **Thumbnail-Tap pausierte die Pipeline**: Der `appsink` des Taps war
   async; jedes Einhängen warf die laufende Pipeline auf PAUSED, bis er
   prerollte (~1 s). Andere Taps warteten → Inputs erschienen seriell
   (16 Inputs: 15 s). Fix `async=false`: 2,3 s.
2. **Buffer-Age-Probes hinter SRT stumm**: Puffer vor der Uhr wurden
   verworfen (auch aus der Zählung). Jetzt Alter 0.
3. **GL-Mixer mit Hardware-Decodern startete nicht**: `glupload` nimmt kein
   CUDA/VA. Brücke (`cudadownload` bzw. `vapostproc`) per Caps-Probe; betrifft
   AMD/Intel (VA ist dort Standard) und NVIDIA mit `--mixer-backend gl`.
4. **Mixer-Kappung 16 → 32 Inputs** (weiche Grenze laut strom-Kommentar).
5. **NVENC `rate-control` → `rc-mode`** in 1.28 (Panic bei
   `nvautogpuh264enc`).
6. **GL-Mixer schwarz bei direkt verlinkten Quellen (NVIDIA)**:
   `glupload` (1.28) schlägt der Quelle udmabuf-Speicher vor; der NVIDIA-
   Treiber importiert den DMABuf als leere Textur. Auch reines gst-launch
   zeigt das. Fix nur für NVIDIA: Allocation-Query am Mixer-Eingang nicht
   weiterreichen. Damit ist der vorbestehend rote
   `vision_mixer_fx_test::wipe_between_letterboxed_sources_animates` grün
   und die strom-Testsuite auf diesem Host erstmals vollständig grün.
   Console-Flows waren nicht betroffen (Thumbnail-Tee vor dem Mixer).
7. **Selftest prüft jetzt das Bild**: Thumbnail-Taps auf PGM und
   Multiview; ohne sie waren schwarze Ausgänge unsichtbar, weil nur
   fps/Alter gemessen wurden. Alle drei Backends (cuda, gl, cpu) zeigen
   Bildinhalt.

**Offen (M1b-Rest / M5b)**:
- VA-Encode aus GL-Speicher (DMABuf-Export) — ohne AMD/Intel nicht
  prüfbar. Heute scheitert GL-Mixer → `videoenc` auf Hosts ohne NVENC laut
  (`videoconvert` nimmt kein GLMemory) statt still über die CPU zu kopieren.
- V4L2-MJPEG auf GPU (`nvjpegdec`/`vajpegdec`): strom-`devicesrc` nimmt nur
  Rohformate an (dokumentierte Einschränkung dort).
- 32 × 720p: frühere Aussetzer waren ein Probe-Messartefakt (behoben).
  Offen bleibt ein sporadischer Startfehler (`not-negotiated` an einem
  Media-Player-appsrc) bei 32 Inputs, siehe `LATENCY.md`.

## 2026-09-30 — Messdaten zur offenen Entscheidung "Live-Monitore" (M1-Rest 1.1)

Keine Entscheidung, nur Daten (Entscheidung beim User, `ROADMAP.md` §5.1).
Option (a) "Thumbnail-Intervall konfigurierbar" gemessen mit temporärem
Patch (nicht committet), 16 × 1080p30 NVDEC, CUDA-Mixer, Console pollt
im selben Takt:

| Thumbnail-Rate | JPEG | Backend-CPU | geliefert |
|---|---|---:|---|
| 1 fps (heute) | CPU | 17 % | 1 fps |
| 1 fps | NVJPEG | 15 % | 1 fps |
| 10 fps | CPU | 39 % | 10 fps |
| 10 fps | NVJPEG | 18 % | 10 fps |
| 25 fps angefragt | NVJPEG | 21 % | ~15 fps (Ratenbegrenzung gegen 30-fps-Quelle) |

NVJPEG ist seit heute Standard im CUDA-Zweig des Thumbnail-Taps (G.6).
Damit wäre (a) für 16 Monitore à 320×180 bei 10–15 fps billig; offen
bleiben REST-Polling-Last pro Bild und egui-Texturupload in der Console.
(b) WHEP bleibt die Variante für einen großen PGM-Monitor in voller
Framerate.

## 2026-09-30 — M1-Rest (Branch `m1-rest`): Audio, AFV, Monitore

Autonom umgesetzt, wartet auf Freigabe (M1b zuerst).

1. **Audio im Console-Flow**: ein `builtin.mixer`, Kanal `i + 1` = Input
   `i`. Testbilder bekommen einen Testton (Quinte je Input), Dateien den
   Player-Ton; SRT/Geräte vorerst ohne Ton. Mixer-Summe → `vm:pgm_audio_in`
   (Multiview-Meter). Kein Audio-Ausgang zu Encodern (M3).
2. **AFV** clientseitig: AFV-Kanäle sind nur live, solange ihr Input auf
   PGM ist; folgt dem PGM aus den WS-Events (wirkt auch bei Takes über API
   oder andere Clients). Harter Schnitt ohne Audio-Blende; vMix blendet mit.
   Geprüft über die Mixer-Properties der Engine, weil die Kanal-Meter vor
   Fader/Mute messen.
3. **PROGRAM-Monitor** = Thumbnail-Tap am PGM-Ausgang (1 fps). Vorher
   zeigte er das Input-Thumbnail und damit weder FTB noch Blenden.
4. strom-Takes nehmen immer den aktuellen PVW, unabhängig von `to_input`
   im Request.

## 2026-09-30 — M1b freigegeben; GPU-Deinterlacing bleibt

**Entscheidung** (User):
1. M1b (GPU-Pipeline) ist abgenommen. Branch `m1-rest` wird in `main`
   übernommen.
2. Interlaced Quellen auf NVIDIA werden auf der GPU deinterlaced
   (`gldeinterlace`, Bildrate bleibt: 1080i50 → 25p), nicht per CPU-yadif
   (50p). Grund: Performance zuerst.

## 2026-09-30 — Live-Monitore: Eingänge immer flüssig (umsetzen später)

**Entscheidung** (User): Alle Eingänge müssen in der Console immer
flüssig dargestellt werden, wie bei vMix — nicht nur PGM/PVW. Umsetzung
später, nicht jetzt.

**Folge für die Wahl (M1-Rest 1.1)**: Das Thumbnail-Polling (auch mit
konfigurierbarer Rate, gemessen max. ~15 fps) reicht für die Eingänge
nicht. Kandidaten, wenn es umgesetzt wird:
- WHEP-Empfang des Multiview-Ausgangs (alle Eingänge + PGM/PVW in einem
  NVENC-Stream, eine GPU-Dekodierung in der Console).
- Oder je Eingang ein kleiner Vorschau-Stream in Videorate.
Vorher messen: Kosten pro Eingang (Encode/Decode, Latenz) gegen das
Leitprinzip "max. Inputs @ Budget".

## 2026-10-08 — Lastenheft: Clean-Room-Ziel, CasparCG, GPL

**Kontext**: Ziel ist eine Clean-Room-Reimplementierung von vMix
(`docs/LASTENHEFT.md`, Grundlage vMix-Handbuch 29).
**Entscheidungen** (User, über die Entscheidungsseite, Details
`LASTENHEFT.md` §6): eigene API mindestens auf vMix-Niveau, Kompatibilität
optional; `lmix-core` als eigener Prozess; Titel HTML über komplette
CasparCG-Integration; Wiederholung und Gäste später; Ausschlüsse
Control-Surfaces/4K aufgehoben; Skripte in JavaScript; Submixe wie vMix.
**Lizenz**: GPL ist okay. `claude.md` Entscheidung 6 geändert:
GPL-3.0-or-later-verträgliche Abhängigkeiten erlaubt; lmix wird
GPL-3.0-or-later, sobald GPL-Code ins Repo kommt; NDI nur zur Laufzeit
laden. CasparCG bleibt eigener Prozess (Crash-Isolation, NFA.6).

## 2026-10-08 — Korrektur: kein CasparCG, MIT bleibt

**Kontext**: Analyse der CasparCG-Integration (Entscheidungsseite T.2):
zweite vollständige Engine neben strom, zweiter Bildtakt, Bild nur per NDI
(Kodierung, CPU-Kopien, 1–2 Frames), Nutzen liegt fast ganz in den
HTML-Vorlagen; strom rendert HTML bereits (`cefsrc`).
**Entscheidung** (User): CasparCG gestrichen, lmix bleibt MIT
(`claude.md` Entscheidung 6 wieder wie vorher). Titel-Fähigkeiten
orientieren sich an CasparCG (Vorlagen-Konvention, Ebenen, Datensätze),
Rendering in strom.

## 2026-10-08 — M2b Schritt 1: `lmix-core` und `lmix-types`

**Kontext**: E.4 (eigener Steuerprozess), API.1 (ein Aktionskatalog).
**Umsetzung**: zwei neue Crates neben `app-desktop`, außerhalb des
strom-Workspace (strom-Diff bleibt null):
- `lmix-types`: API-Typen (Action, Input, ShowState, Event), geteilt mit
  GUI und Fremdprogrammen. Katalog = JSON-Schema von `Action` (schemars).
- `lmix-core`: Prozess mit Switcher (reine Zustandsmaschine → Engine-
  Kommandos), Engine-Trait (`strom`, `none`), Show-Datei (JSON, Version 1,
  atomisch geschrieben), HTTP/WS-API auf `127.0.0.1:8200`.
**Entscheidungen im Detail**:
- Inputs belegen feste Mischer-Slots (Standard 16). Ein Slot wird ein
  strom-Flow mit fester ID, damit der `inter`-Kanalname gleich bleibt und
  der Mischer beim Quellentausch nicht neu startet.
- Aktionen laufen nacheinander (ein Lock über Switcher und Engine), damit
  alle Clients dieselbe Reihenfolge sehen.
- Engine-Fehler machen den Input offline, der Rest der Aktion läuft weiter;
  die API meldet 502 mit Grund.
- Fehlt strom beim Start, versucht `lmix-core` alle 2 s neu und spielt dann
  die Show ein.

## 2026-10-08 — GUI-Gestaltung: Kante, Kante Light, Rückfall Breeze

**Entscheidung** (User): Die lmix-GUI folgt dem Kante Design System
(`../Kante`, Regel aus `AGENT-RULE.md` in `agent.md`). Stilwahl Kante
(Standard), Kante Light, System (KDE Plasma/Breeze) als Rückfall.
**Folge**: Kante hat Anbindungen für Web (CSS) und Qt Quick/Kirigami
(`qml/Kante`, mit System-Modus = Breeze). Für egui gibt es keine; sie
müsste zuerst in Kante entstehen. Das ist ein Kriterium für G.1
(GUI-Toolkit, Entscheidung nach Prototyp in M3).

## 2026-10-08 — M2b Schritt 4: Console als Client, Zeitbasis der Kopplung

**Kontext**: Persona-Befund B22 (Console muss Client von `lmix-core`
werden). Beim Bau fiel auf: PGM blieb schwarz, sobald die Quellen über
`inter` kamen. Der VM-Test von Schritt 2 hatte das nicht bemerkt, er prüfte
nur die JPEG-Größe (ein schwarzes Bild ist ebenso groß).
**Ursache**: `inter` reicht Puffer mit der Laufzeit des erzeugenden Flows
weiter. Jeder strom-Flow hat eine eigene Basiszeit, also passen die Zeiten
von Slot und Mischer nicht zusammen; der Mischer verwirft alles als zu alt
oder wartet auf "Zukunft".
**Entscheidung**:
- Alle lmix-Flows laufen mit Monotonic-Uhr und Basiszeit 0
  (`direct_media_timing`), dann ist die Laufzeit überall die Uhrzeit.
- Der Mischer startet seine Ausgabe beim ersten Puffer statt bei null.
  Dafür ein kleiner strom-Patch (Branch `lmix-start-time`, 69 Zeilen):
  neue Block-Eigenschaft `start_time_selection` (`zero` Standard wie
  bisher, `first`). Kein Umbau von strom, Verhalten anderer Flows gleich.
- Bilder für GUI und Fremdprogramme liefert `lmix-core` selbst
  (`/api/pictures/program`, `/api/pictures/inputs/{input}`), jeder Slot hat
  dafür einen Thumbnail-Abgriff. Clients brauchen strom nicht zu kennen.
- Hinter dem PGM-Abgriff hängt eine `fakesink` (sync=false), sonst bleibt
  der Zweig nach dem ersten Bild stehen. Echte Ausgänge (OUT) ersetzen sie.
- Der VM-Test prüft Bildinhalt (Helligkeit) und Bewegung (zwei Bilder
  unterscheiden sich), nie mehr Bytes. Er schlägt gegen den alten Stand
  fehl und läuft mit dem Fix durch.
- Console: `--core URL` startet den Client-Modus (Input-Leiste, Monitore,
  Übergänge, Show-Datei). Der direkte strom-Modus (M1b: Ton, DSK,
  Selbsttest) bleibt, bis `lmix-core` ihn abdeckt.
**Nachtrag**: Die CRITICALs kamen von der `fakesink` hinter dem PGM-Abgriff
(QoS-Events, die eine strom-Probe verwirft, siehe ROADMAP §5 Punkt 6). Die
Senke läuft jetzt mit `qos=false`; der VM-Test verlangt null CRITICALs.

## 2026-10-08 — M2b Schritt 5: Ton über die Slots (B23)

**Umsetzung**:
- Jeder Slot-Flow veröffentlicht seinen Ton auf einem zweiten
  `inter`-Kanal (`out_audio`). Testbilder bringen einen leisen Ton (Tonhöhe
  je Slot), Dateien/URLs ihre Tonspur, SRT die erste Tonspur. Geräte sind
  Bild ohne Ton; Ton einer Kamera ist ein eigenes Audiogerät (später,
  Audio-Inputs).
- Der Mischer-Flow hat einen strom-Audio-Mischer (`builtin.mixer`) mit
  einem Kanal je Slot; seine Summe geht als Programmton in den
  Vision-Mixer. Der strom-Patch `start_time_selection` gilt auch für ihn
  (gleiches Zeitproblem wie beim Bild).
- Je Input: Ton an/aus, Audio-folgt-Bild (AFV), Lautstärke 0–100. Der
  Switcher berechnet daraus Fader und Mute je Kanal und schickt nur
  Änderungen. AFV blendet so lange wie der Übergang, sonst 20 ms gegen
  Knacken.
- Lautstärke wird quadratisch auf den Fader abgebildet (50 ≈ −12 dB), wie
  ein Pegelsteller sich anfühlt.
- Pegel: `lmix-core` liest die Meter-Events von strom, ordnet sie den
  Inputs zu und sendet `levels` (10/s) über den WebSocket, dazu
  `GET /api/audio/levels`. Kanalpegel vor Fader/Mute, Programm danach.
**strom-Eigenheit**: Fader und Mute eines Kanals stellen dasselbe
`volume`-Element; strom merkt sich beim Stummschalten die Lautstärke und
stellt sie beim Aufheben wieder her. Beides in einem PATCH verliert den
Fader. lmix schickt deshalb getrennt: Aufheben vor Fader, Fader vor
Stummschalten. Kandidat für einen Fehlerbericht an strom.

## 2026-10-08 — Abhören: PFL und AFL je Input

**Anforderung** (User): Jede Audiospur hat Pre-Fader-Listen und
After-Fader-Listen.
**Umsetzung**: strom bringt beides je Mischerkanal mit (`chN_pfl`,
`chN_afl`) samt Solo-Bus und Monitor-Bus; der Monitor folgt dem Programm
und schaltet auf die Solos, sobald eines aktiv ist (strom schaltet selbst).
lmix setzt je Input `listen: off | pfl | afl` (Aktionen `set_listen`,
`clear_listen`), mehrere Solos summieren sich wie am Pult.
- PFL hört den Input vor Fader und Mute (auch stummgeschaltet), AFL so,
  wie er ins Programm geht.
- Solos sind Laufzeitzustand, nicht in der Show-Datei (wie strom und
  übliche Pulte). Nach einem Neustart von `lmix-core` werden sie im
  Mischer zurückgesetzt, ein entfernter Input gibt sein Solo frei.
- Monitor-Bus hörbar über `--monitor-out auto` (Standard-Audioausgang der
  Engine-Maschine); Standard `none`, weil `lmix-core`/strom oft headless
  laufen. Pegel des Monitor-Busses in `levels.monitor_peak_db`.
**Offen**: Monitor-Ausgang wählbar (Gerät, NDI, an den Bedienplatz
streamen) mit den Ausgängen (OUT, M3).

## 2026-10-08 — M2b Schritt 6: Quellen, Transport, Signalverlust

**Quellentypen (2b.3, B29)**: Farbe (`#rrggbb`), Standbild (decodebin →
imagefreeze, live), Playlist (Media-Player, Schleife), NDI (`ndi_input`),
WHIP (`whip_input`, strom nimmt unter `/whip/<endpoint>` an), Audio-Gerät
(nur Ton). Audio-Inputs haben kein Bild; Vorschau und Programm lehnen sie
ab (`no_video`). Ton jedes Slots wird vor dem `inter`-Ausgang auf ein
Format gebracht (F32, 48 kHz, Stereo): der Audio-Mischer nimmt das Format
seines ersten Eingangs, ein abweichender Slot scheiterte (not-negotiated).
**Transport**: Play, Pause, Neustart (Seek 0), Weiter, Zurück für Datei,
URL, Playlist (`transport_input`).
**Signalverlust je Input** (User): letztes Bild halten (Standard) oder
Schwarz. Umsetzung über `max-last-buffer-repeat` der Compositor-Pads im
Vision-Mixer (PGM und Multiview): unendlich = halten, 400 ms = danach
Schwarz. Gespeichert in der Show-Datei. Die Kachelbilder von `lmix-core`
folgen dem (halten = letztes Bild weiterreichen). Eine Umstellung wirkt ab
dem nächsten Frame der Quelle; bei bereits stehender Quelle erst, wenn sie
wieder läuft.
**Fehler aus Schritt 2 behoben**: strom übernimmt bei `transition` immer
seine Vorschau ins Programm und ignoriert `to_input`. Ein `cut_direct` auf
einen Input außerhalb der Vorschau und das Schalten auf Schwarz trafen
den falschen Eingang. Der Treiber setzt das Ziel jetzt zuerst in die
Vorschau und gibt die alte Vorschau danach zurück; der VM-Test prüft das
Programm direkt im Mischer.
**strom-Patch** (Branch `lmix-start-time`, PR #1): Der Media-Player rief
`add_signal_watch` nicht auf, der Abbau entfernte einen Watch zu viel
(CRITICAL je gestopptem Player).

## 2026-10-08 — Arbeitsweise: ohne Zwischenabnahme, Zwischenstände als PR

**Entscheidung** (User): Abnahmen sind nur auf anderer Hardware möglich.
In dieser Session geht die Arbeit ohne Zwischenabnahme weiter (über das
Meilenstein-Ende hinaus). Jeder Zwischenstand wird automatisch gepusht und
als PR hinterlegt: ein Branch je Meilenstein, auf dem vorigen aufgesetzt
(`lastenheft` ← `m2b` ← `m3` …), PR mit Basis auf den Vorgänger-Branch.
Abnahme später gesammelt auf der Zielhardware (`HARDWARE-TODO.md`).

## 2026-10-08 — M3 Schritt 1: Ausgänge (3.1)

**Aufbau**: Der Mischer-Flow veröffentlicht Programmbild und -ton auf
`inter` (`pgm_out`, `pgm_audio_out`). Jeder Ausgang ist ein eigener
strom-Flow (feste ID je Ausgang): ein abstürzender oder neu gestarteter
Ausgang berührt den Mischer nicht (Crash-Isolation, 3.3-Prinzip).
- SRT: H.264 (`videoenc`, Hardware zuerst, sonst Software) + AAC in
  MPEG-TS, Wiederverbinden an.
- Aufnahme: H.264 + AAC in MKV (Standard, übersteht Absturz), MP4 oder TS,
  im Medienverzeichnis der Engine.
- NDI: unkomprimiert (NDI-SDK nötig).
**Bedienung**: Ausgänge stehen in der Show-Datei, laufen aber erst auf
Start (wie vMix). Status je Ausgang: gestoppt, läuft (seit), Fehler (mit
Meldung aus den Pipeline-Fehlern von strom).
**Robustheit**: Ein Neustart von strom startet laufende Ausgänge neu; ein
Neustart von `lmix-core` übernimmt Ausgänge, die in strom weiterlaufen,
ohne sie neu zu starten (keine Lücke in einer Aufnahme).
**strom-Patch** `eos_on_stop` (Flow-Eigenschaft, Standard aus): Stoppen
schickt EOS und wartet höchstens 5 s, damit Muxer ihre Datei abschließen.
Vorher war eine MP4-Aufnahme nach dem Stopp unlesbar.
**Mischer-Layout**: Der Flow heißt jetzt "lmix mixer 2"; ein laufender
Mischer eines älteren Layouts wird beim Start neu gebaut statt
übernommen.

## 2026-10-08 — M3 Schritt 2: MultiCorder und ISO-Ausgänge (3.2)

Statt eines eigenen MultiCorders hat jeder Ausgang eine Quelle: Programm
(Standard) oder ein Input (`source`). Ein ISO-Ausgang abonniert den
`inter`-Kanal des Slots direkt, vor dem Mischer. Damit gibt es neben der
Aufnahme je Input auch ISO-Streams (SRT) und ISO-NDI ohne Zusatzaufwand.
- Ein Input ohne Ton bekommt keine Tonspur: der Recorder (splitmuxsink)
  würde sonst ewig auf eine Spur warten.
- Audio-Inputs (ohne Bild) können keine ISO-Quelle sein.
- Wird der Input entfernt, stoppt sein ISO-Ausgang und steht auf Fehler.

## 2026-10-08 — M3 Schritt 3: Bildschirm-Ausgang (3.3)

Ausgangsart "Display", Sink im Engine-Prozess (Absturz der GUI lässt den
Beamer weiterlaufen). Drei Wege zum Bildschirm:
- Fenster (`glimagesink`): Kontrollmonitor, auch in der VM prüfbar.
- Vollbild (`waylandsink fullscreen`): welcher Bildschirm, entscheidet der
  Compositor (KDE: Fensterregel). Ein Wayland-Client kann den Ausgang nicht
  selbst wählen.
- DRM-Anschluss (`kmssink connector-id`): ohne Desktop, z. B. Beamer an
  einer Engine-Maschine ohne Sitzung.
Nur Bild; Ton über HDMI folgt mit Audio-Ausgängen. Die Sink läuft mit
`sync=false` (Frames kommen schon um die Mischer-Latenz gealtert, eine
synchronisierende Sink verwarf sie alle) und `qos=false` (CRITICAL-Probe).
Quelle wie alle Ausgänge: Programm oder ein Input (ISO-Kontrollbild).

## 2026-10-08 — M3 Schritt 4: Failover und Latenz-CI (3.4)

**Ausfälle erkennen**: `lmix-core` folgt den strom-Ereignissen
`PipelineError` und `FlowStopped`. Eigene Umbauten stoppen Flows auch; ein
Stopp bis 1 s nach einem eigenen Kommando auf denselben Flow zählt nicht
als Ausfall.
- **Input**: Slot-Flow fällt aus → Input „offline“, der Mischer hält das
  letzte Bild (Signalverlust „halten“, jetzt mit `repeat-after-eos`, sonst
  verwarf der Compositor das Bild beim Stream-Ende) oder zeigt schwarz.
  Failback: Slot neu bauen nach 2 s, dann mit Verdopplung bis 10 s, bis er
  läuft oder der Input entfernt ist. Kein Umschalten auf ein Ersatzbild:
  der Operator sieht „offline“ und entscheidet.
- **Ausgang**: Fehler oder unerwarteter Stopp → Status „Fehler“, dann
  automatischer Neustart.
**Latenz**: `POST /api/latency/measure` setzt eine strom-Buffer-Age-Probe
auf den Programm-Ausgang des Mischers, `GET /api/latency` liefert Median,
p95, Maximum. Der VM-Test prüft ein Budget (`LATENCY_BUDGET_MS`, 150 ms
CPU); der Gitea-Workflow `vm-check` fährt den ganzen Test bei jeder
Änderung an lmix-core, Typen, Console oder strom (Messwerte: LATENCY.md).
**SRT-Input**: Der strom-Block `srt_input` mit `decode` liefert in der VM
aus `srt-live-transmit`/ffmpeg kein Bild: mit `ignore_pcr` (Standard)
„No valid frames before EOS“, ohne legt decodebin keine Pads an, und der
Schutz wird bei erneutem Plug nicht zurückgesetzt (Failback unmöglich).
Befund für strom, nicht in lmix umgangen; lmix setzt nur
`wait_for_connection` und `keep_listening`. Prüfung mit echter Quelle: H.23.
**MP4-Aufnahme**: strom reserviert je Datei ~93 MB für den moov-Kopf
(Absturzsicherheit); MKV bleibt Standard.

## 2026-10-08 — Verbindungszustand der Ausgänge (B32)

Für SRT-Ausgänge liest `lmix-core` jede Sekunde die Statistik des
`srtsink` aus strom (`/flows/{id}/srt-stats`) und schickt sie als Ereignis
`outputs` an alle Clients (auch `GET /api/outputs/stats`):
- Empfänger verbunden ja/nein, Senderate, seit Start verlorene Pakete
  (vom Empfänger gemeldet), lokal verworfene Pakete (Leitung zu langsam
  für die Bitrate), RTT.
- Ein Listener mit mehreren Empfängern: Raten und Zähler summiert, RTT
  der langsamste.
Die Console zeigt am laufenden Ausgang die Rate oder „no receiver“,
Details im Tooltip. Die Werte stehen nicht im Show-Zustand (kein
Speichern, kein Zustandsereignis je Sekunde). Aufnahme und NDI haben keine
Leitung; Füllstand der Platte folgt mit den Aufnahme-Einstellungen.

## 2026-10-08 — GUI-Shell (3.5): Vorschlag, offen

Bewertung egui gegen Qt Quick/Kirigami (Tauri scheidet aus: keine
Web-GUI) in `GUI-SHELL.md`. **Empfehlung**: Qt Quick/Kirigami über cxx-qt,
weil Video ohne Kopie (`qml6glsink`) und die Kante/Breeze-Gestaltung
(`qml/Kante`) dort fertig sind, in egui Eigenentwicklung. **Nicht
entschieden**: `claude.md` legt egui fest, ein Wechsel ist Sache des
Users. Bis dahin bleibt die egui-Console die Oberfläche.

## 2026-10-09 — GUI-Shell (3.5): Qt Quick/Kirigami

**Entscheidung** (User): Die Console wechselt auf Qt Quick/Kirigami über
cxx-qt, wie in `GUI-SHELL.md` empfohlen. `claude.md` Entscheidung 3
angepasst. Vorgehen: Prototyp (QML-Fenster im Kante-Stil, PGM-Monitor über
`qml6glsink`, Bild aus strom über `unixfdsink`/`unixfdsrc`), messen
(H.11), dann Funktionen übertragen. Die egui-Console bleibt bis zur
Gleichwertigkeit für Selftest und VM-Screenshot.

## 2026-10-09 — Lizenz: MIT, auch für Kante

**Entscheidung** (User): lmix steht unter MIT (`LICENSE` im Repo, bisher
fehlte die Datei; Crates von `MIT OR Apache-2.0` auf `MIT`). Kante kommt
ebenfalls unter MIT (Kante-PR #35; `kimai/` bleibt GPL-3.0-or-later,
Schriften OFL-1.1), damit lmix die QML-Module ausliefern darf.
Abhängigkeiten wie bisher (`claude.md` Entscheidung 6): Qt, Kirigami,
GStreamer und gstcefsrc (LGPL) nur dynamisch, kein GPL-Code im Repo oder
Paket (z. B. kein `x264enc` mitliefern), NDI-SDK zur Laufzeit.

## 2026-10-09 — Ton der Slots: Mischer fest auf 48 kHz

**Befund** (lokal, Arch, GStreamer 1.28.7): Gesamttest „no sound from the
slots“. Der Audiomischer in strom läuft mit `force-live` und mischt nach
Timeout auch ohne Eingang; startet er vor dem ersten Slot-Ton, legt er
sich auf 44,1 kHz fest. Die Slots liefern 48 kHz (`AUDIO_CAPS`), der
Kanalzug wandelt nur das Format, nicht die Rate: `not-negotiated` an
`ain*`. In der Cloud kam der Ton offenbar vor dem Timeout an.
**Entscheidung**: strom-Mischer bekommt die Eigenschaft `sample_rate`
(strom `lmix-sample-rate`, auf `lmix-start-time`): Capsfilter hinter jedem
Bus, Resampler je Kanal; Standard 0 lässt alles wie bisher. lmix-core
setzt 48 kHz. Dazu auf `lmix-start-time` der fehlende OpenAPI-Snapshot
für `eos_on_stop` (sonst rot in strom-CI).

## 2026-10-09 — Qt-Console: Aufbau des Prototyps

**Kontext**: GUI-SHELL Schritt 1. **Entscheidung**: eigenes Crate
`console-qt` neben `app-desktop`. Ein Rust-QObject (`Core`, cxx-qt 0.10)
hält Zustand und Aktionen; die Bilder baut Rust mit gstreamer-rs und
übergibt `qml6glsink` das QML-Item als `widget` (die QML-Seite ruft
`core.showLive(item, source)`, sobald `itemInitialized`). Kein eigener
C++-Code. Kante liegt als Kopie im Crate (README von Kante: kopieren oder
qrc) und geht über eine generierte `.qrc` in die Ressourcen.
**Verworfen**: C++-QObject für die Pipeline (mehr Build-Aufwand, kein
Vorteil); Kante über Importpfad zur Laufzeit (Paket hinge an einem Pfad).
**Offen**: die egui-Console wird erst nach Gleichwertigkeit entfernt.

## 2026-10-09 — CUDA-Programm für CPU-Abnehmer herunterladen

**Befund** (RTX 3090): Mit `--mixer cuda` trägt der `inter`-Kanal des
Programms `memory:CUDAMemory`. Aufnahme und SRT (NVENC) liefen, Bildschirm-
Ausgang und Live-Socket nicht (`not-negotiated`). **Entscheidung**:
`cudadownload` plus Capsfilter `video/x-raw` nur in Flows, die das
Programm auf der CPU lesen (Bildschirm, NDI, Live-Socket). Ein Download
schon im Mischer-Flow wäre einfacher, kostete aber den Encodern den
Weg ohne Kopie. **Offen**: Mit `--mixer gl` ist noch ungeprüft, ob der
Kanal `GLMemory` trägt (H.22, AMD/Intel).

## 2026-10-09 — Neustart der Engine erkennen

**Befund**: lmix-core speiste die Show nur beim eigenen Start ein. Stürzte
strom ab und startete neu, blieb Programm leer, bis auch lmix-core neu
startete. **Entscheidung**: lmix-core fragt alle 2 s `/api/version` und
vergleicht `process_started_at`; ein neuer Wert heißt neue Engine: Mischer
aufbauen, Show samt laufender Ausgänge einspielen. Aktionen bis dahin
ändern nur die Show (wie beim Start). Geprüft mit `kill -9` auf strom:
Flows, SRT-Ausgang und Programmbild kamen zurück. **Verworfen**:
Neustart an Flow-Fehlern erkennen (ein Absturz meldet sich nicht über den
eigenen Bus); den Mischer-Flow abfragen (ein leerer Mischer nach Neustart
wäre nicht von einem gerade gebauten zu unterscheiden). **Offen**: eine
Aktion in den bis zu 2 s bis zur Erkennung antwortet noch 502.

## 2026-10-09 — Qt-Console: Stil, Reihenfolge, Vorschau

**Entscheidung**: Stilwahl Kante / Kante Light / System in der
Werkzeugleiste, gespeichert mit QSettings (`agent.md`: Stilwahl in den
Einstellungen); `--style` gilt für einen Lauf. Inputs per Drag & Drop
umsortieren (`move_input`). Der Vorschau-Monitor liest den Input in
voller Größe (`?size=full`, eigener Flow je Slot, nur auf Abruf), die
Kacheln weiter 640×360.

## 2026-10-09 — Hardware-Läufe auf RTX 3090 (H.10, H.18, H.22)

**Messung** (`scripts/soak-hardware.sh`, 60 min, CUDA-Mischer, 8 Inputs,
Programmwechsel alle 30 s, MKV-Aufnahme und SRT an lokalen Empfänger):
Latenz Quelle → PGM 56–66 ms (einmal 78 ms), kein Anstieg; Aufnahme
108 800 Bilder in 3624 s (lückenlos), SRT 108 737 Bilder empfangen, kein
Paketverlust, strom 90–125 % eines Kerns, GPU 4–7 %, keine CRITICALs.
CPU-Mischer zum Vergleich: 88 ms.

**Befund GL-Mischer** (`--mixer gl`, auf derselben NVIDIA-Karte): mit
`gl_download` laufen Aufnahme (MKV und TS), Bildschirm und Live-Socket
(RGBA), Latenz 56–67 ms. Der **SRT-Ausgang sendet aber kein Video**: nur
Ton (~200 kbit/s), Audio-Queue im Ausgangs-Flow voll (2 s), der Muxer
wartet auf Video. Mit CUDA derselbe Flow fehlerfrei. Nachstellen: Stack mit
`--mixer gl`, Input `ball`, SRT-Ausgang starten, `srtsrc ! filesink`, dann
`ffmpeg -i x.ts -map 0:v -f null -` → 0 Bilder. Vermutlich im strom-Block
`mpegtssrt_output` (dynamischer Parser) bei RGBA-Quelle; offen.
