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
CPU); der CI-Job `vm-check` (GitHub) fährt den ganzen Test bei jeder
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
Einstellungen); `--style` (seit 19.10. `--theme`) gilt für einen Lauf. Inputs per Drag & Drop
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
`ffmpeg -i x.ts -map 0:v -f null -` → 0 Bilder. `GST_DEBUG=basetsmux:6`:
am Video-Pad des Muxers (`srt:mpegtsmux:sink_66`) kommen genau zwei Bilder
an, dann steht der Zweig; der Encoder davor ist derselbe wie in der
funktionierenden Aufnahme. Verdacht: die dynamische Parser-Einfügung im
strom-Block `mpegtssrt_output` (Pad-Probe) bleibt beim GL-Timing hängen.
Offen, Fix gehört nach strom.

## 2026-10-09 — Dauerlauf NFA.7 (Teil): Speicherwachstum in strom

**Messung** (`scripts/soak-hardware.sh`, 295 min, CUDA-Mischer, 16 Inputs,
MKV- und ISO-Aufnahme, 2 × SRT lokal, Werte in `LATENCY.md`): Latenz
56–63 ms ohne Anstieg, keine Bilder verloren, keine CRITICALs. Das Skript
schlägt fehl, weil der Speicher von strom gleichmäßig wächst: 763 MB nach
10 min, 998 MB am Ende (~32 MB/h, rund 0,8 GB am Tag). Für 8 h unkritisch,
für den 48-h-Betrieb (NFA.7) nicht. **Offen**: Ursache eingrenzen (Läufe
ohne Ausgänge, ohne Programmwechsel, mit `heaptrack`), der Fix gehört
wahrscheinlich nach strom. lmix-core blieb ohne Fehler.

## 2026-10-09 — Hardware-Decoder: GPU-Speicher im Slot-Kanal

**Befund** (beim Demo-Modus): Auf einem NVIDIA-Rechner dekodiert der
Media Player Dateien mit NVDEC und legt CUDA-Speicher in den Slot-Kanal
(`intersink` nimmt alles). Kacheln und Vorschau von Datei-Inputs blieben
leer (`videoscale`: not-negotiated); bisher unbemerkt, weil die
Hardware-Tests nur Testbilder nutzten. Ein Capsfilter im Slot-Flow hilft
nicht: der Player dekodiert in einer eigenen Pipeline hinter `appsrc`.
**Entscheidung**: strom-Player bekommt `system_memory` (strom-Branch
`lmix-player-sysmem`): mit CPU-/GL-Mischer übergibt er Systemspeicher, mit
CUDA-Mischer bleibt CUDA-Speicher (ohne Kopie in den Mischer), und die
Live-Flows der Slots laden dann wie Programm per `cudadownload` herunter.
**Offen**: andere dekodierende Quellen (SRT, NDI, Gerät) auf demselben Weg
prüfen; VA-API (AMD/Intel) ungetestet.

## 2026-10-09 — Demo-Modus

**Entscheidung**: Die Demo-Show ist der Sendehaus-Stream von „Graufeld nach
Mitternacht“ (Studio-Weber-Welt, `graufeld.stream`). `demo/start.sh [de|en]`
rendert je Input einen synthetischen Clip (videotestsrc-Muster, Name, Ton;
VP8/Opus), startet strom, lmix-core und die Qt-Console mit eigener
Konfiguration und baut die Show über die API. Screenshots über
`shrippen.github.io/demo/tools/screenshots.py lmix` (`demo/shots.json`,
Xvfb, dunkles Farbschema). Der Demo-Modus liegt nur in `demo/`, in keinem
Crate; ein Release-Build muss `demo/` ausschließen und das prüfen.

## 2026-10-09 — Aktion während eines Engine-Neustarts

**Befund**: Zwischen Neustart von strom und der nächsten Abfrage (bis
2 s) scheiterte eine Aktion mit 502, weil der neuen Engine der Mischer
fehlt. **Entscheidung**: Scheitert ein Engine-Kommando, fragt lmix-core
sofort die Prozess-Kennung ab. Ist sie neu, gilt die Aktion wie beim
Start nur für die Show (Antwort OK), und der Watcher spielt die Show
sofort ein statt nach der nächsten Abfrage. Eine Engine, die gar nicht
antwortet, bleibt ein Fehler (502): dann weiß lmix-core nicht, ob sie
wiederkommt.

## 2026-10-09 — GL-Mischer und SRT: in der VM nicht nachstellbar

**Versuch** (Cloud-VM, Xvfb, Mesa llvmpipe): Stack mit `--mixer gl`,
Input `ball`, SRT-Ausgang, `srtsrc ! filesink` 8 s. Ergebnis: gar keine
Daten (auch kein Ton), aber der Mischer selbst läuft hier 9 s hinter der
Zeit (Bildalter am Programm-Ausgang Median 9,3 s, `vm:queue_0` voll),
während ein nackter `glvideomixer` mit llvmpipe 1080p mit ~46 fps schafft.
Die VM zeigt also ein eigenes Problem (Software-GL im Mischer mit Looks und
Multiview zu langsam) und sagt über den Hardware-Befund nichts. Der Fix
bleibt an echter Hardware zu suchen (AMD/Intel oder NVIDIA mit `gl`).
**Nebenbefund** (CPU-Mischer, VM): der SRT-Ausgang trägt Video, aber nur
~42 Pakete in 8 s, weil `x264enc` in der VM mit QoS Bilder verwirft; der
VM-Test prüfte bisher nur die Dateigröße.

## 2026-10-09 — Selbsttest als eigenes Binary

**Kontext**: Die egui-Console soll gehen (GUI-SHELL, „Parität“), ihr
kopfloser Selbsttest (`--selftest`, M1b-Abnahme) bleibt gebraucht.
**Entscheidung**: neues Crate `workspace/selftest`, Binary
`lmix-selftest`, ohne GUI-Abhängigkeit. Es nimmt den Selbsttest und eine
Kopie der strom-Client-Module (`api`, `sources`, `afv`, `perf`,
`gpu_profile`, `ws`) mit, ohne die egui-Teile (Overlay, Repaint). Schalter
ohne Präfix (`--patterns`, `--file`, `--encode`, `--secs`, …);
`scripts/acceptance-m1b.sh` ruft es auf. `app-desktop` verliert
`--selftest` und den nur dafür genutzten Code. **Verworfen**: die Module
als gemeinsame Bibliothek beider Crates (Aufwand für Code, der mit der
egui-Console verschwindet; danach gibt es nur noch eine Kopie).

## 2026-10-09 — strom-Speicher (H.24): Eingrenzung in der VM

**Läufe** (Cloud-VM, CPU-Mischer, 4 Testbild-Inputs, Programmwechsel
alle 30 s, RSS von strom jede Minute, je 40 min):

| Lauf | Ausgänge | RSS nach 2 min → 40 min |
|---|---|---|
| A | keine | 389 → 394 MB, ab Minute 7 flach |
| B | MKV-Aufnahme + SRT mit Empfänger | 375 → 392 MB, in Stufen |
| C | nur MKV-Aufnahme | 422 → 422 MB (schwankt 418–433) |
| D | nur SRT | 420 → 434 MB (schwankt bis 442) |
| F | nur SRT | 382 → 424 MB, Stufen bis Minute 12, dann flach |
| G | nur SRT, `MALLOC_ARENA_MAX=2` | 332 → 388 MB, Stufen bis Minute 12, dann flach |

`/srt-stats` 50-mal je Sekunde über 5 min (≈12 000 Abfragen) ließ den
Speicher nicht stetig steigen (325 → 327 MB, ein einzelner Sprung am
Ende): die Leitungsanzeige (B32) leckt nicht.

**Befund**: In der VM wächst strom in den ersten ~12 min in Stufen (in B,
F und G jeweils um Minute 8) und bleibt danach flach; das sieht nach
Puffern und Verläufen aus, die sich bis zu ihrer Grenze füllen, nicht nach
einem Leck. `MALLOC_ARENA_MAX=2` senkt das Niveau um ~35 MB, ändert den
Verlauf nicht. Das stetige Wachstum der Hardware-Läufe (~32 MB/h über 5 h)
ist hier nicht nachstellbar. heaptrack taugt nicht: strom stürzt darunter
beim Start eines Ausgangs-Flows ab (Stack-Überlauf in einem Tokio-Worker,
einmal Segfault).

**Nächste Schritte auf Hardware** (`scripts/soak-hardware.sh`, je 60 min):
ohne ISO-Aufnahme; mit 8 statt 16 Inputs; mit `--mixer gl` statt `cuda`;
einmal mit `MALLOC_ARENA_MAX=2`. Wächst nur der CUDA-Lauf, liegt die
Ursache im CUDA-Pfad (Speicherpools, `cudadownload`).

## 2026-10-09 — egui-Console entfernt, Qt-Console in der Cloud-VM

**Entscheidung**: `app-desktop` (egui) und `run.sh` sind weg. Der
Selbsttest lebt als `lmix-selftest` weiter, VM-Test und Screenshot nutzen
die Qt-Console. Mit ihr gingen der M1b-Modus direkt an strom und das
Leistungs-Overlay; beides hatte keinen Nutzer mehr.
**Qt in der Cloud-VM** (Ubuntu 24.04 hat weder Qt ≥ 6.5 noch Kirigami 6):
KDE neon als zusätzliche apt-Quelle, gepinnt auf Qt-, KF6- und qml6-Pakete
plus die neueren Grundbibliotheken, die sie verlangen (libwayland 1.26,
libinput 1.29, libpcre2 10.45); GStreamer bleibt das von Ubuntu.
`scripts/cloud-setup.sh` richtet das ein. **Verworfen**: Arch-chroot (mehr
Platz, root-Mounts), Qt und Kirigami aus Quellen (Stunden Bauzeit).
**Befund dabei**: `unixfdsink` sendet vor GStreamer 1.28 nur Bilder in
fd-Speicher. Programm und volle Vorschau eines CPU-/GL-Mischers kamen als
gewöhnlicher Systemspeicher, der Live-Flow brach ab, der Programm-Monitor
blieb leer (auf Arch mit 1.28 unbemerkt). lmix-core liest die
GStreamer-Version von strom und setzt unter 1.28 einen `compositor` mit
einem Eingang (Start beim ersten Bild) vor den Sink; Kacheln brauchen das
nicht, `videoscale` schreibt schon in den Speicher des Sinks. Außerdem
sendet der Live-Sink kein QoS mehr (strom-QoS-Probe: CRITICAL je Bild).

## 2026-10-09 — M4 Schritt 1: Overlays (4.1)

**Entscheidung**: Ein Overlay-Kanal ist eine DSK-Ebene des strom-Mischers
(über Programm und seinen Übergängen). Der Mischer-Flow bekommt vier
DSK-Eingänge, jeder über einen eigenen `inter`-Eingang; geht ein Overlay
auf Sendung, stellt lmix-core dessen `producer-name` auf den Kanal des
Slots um und blendet die Ebene ein. Damit braucht ein Overlay keinen
zweiten Slot-Flow und keinen Mischer-Umbau.
- **4 statt 8 Kanäle**: strom erlaubt höchstens 4 DSK-Ebenen, vMix hat
  ebenfalls 4. Mehr wären ein strom-Umbau ohne Bedarf.
- **Blende**: strom-Patch `lmix-dsk-fade` (DSK-Toggle mit `duration_ms`,
  Alpha mit denselben Keyframes wie Übergänge und FTB). Standard 500 ms.
- **Zustand**: Overlays stehen in der Show (wie Programm), Neustart und
  Wiederherstellen setzen sie ohne Blende. Entfernter Input → sein Overlay
  geht ab. Tally: ein Overlay auf Sendung zählt als Programm (rot).
- **Console**: Knöpfe 1–4 je Kachel (an/aus für diesen Input), Leiste
  „Overlays“ unter FTB (frei: Vorschau-Input drauf; belegt: ab).
- **Offen**: Position und Größe (bildfüllend wie ein Vollbild-Input; PiP
  mit 4.6); Multiview-Anzeige der Overlays prüfen.

## 2026-10-09 — M4 Schritt 2: T-Bar (4.3)

**Entscheidung**: ohne strom-Patch. Während die T-Bar gezogen wird, setzt
lmix-core am Programm-Compositor (`vm:mixer`) das Pad des Vorschau-Inputs
nach vorn (strom-Konstante `TRANSITION_FOREGROUND_ZORDER`, unter den
Overlay-Ebenen) und seine Deckkraft auf die Position. Bei 1 folgt ein Take
als Cut: strom setzt dabei alle Pads zurück (Alpha, Lage, Ebene), so dass
kein Rest der Handmischung bleibt. Zurück auf 0 nimmt das Pad wieder weg.
Aktion `t_bar { position }` (0–1), Position im Zustand (`t_bar`, nicht in
der Show-Datei). Die Console schickt Bewegungen höchstens ~30-mal je
Sekunde; ab 98 % gilt das Loslassen als Ende.
**Verworfen**: strom-Patch „Übergangsfortschritt setzbar“ (ROADMAP 3b):
die Übergänge sind dort zeitgesteuerte Keyframes; für eine Blende reicht
die direkte Deckkraft, und sie kostet keine Änderung an strom.
**Offen**: andere Effekte als Blende von Hand (Wipe, Slide); Ton während
des Ziehens (AFV-Inputs kommen erst mit dem Take); Richtungswechsel wie bei
vMix (Hebel bleibt am Ende stehen).


## 2026-10-09 — M4 Schritt 3: Kanalzug (4.7, ohne Busse)

**Entscheidung**: Kanalzug je Input mit Ton: Gain (±20 dB), Pan,
Hochpass, 4-Band-EQ (80 Hz, 400 Hz, 2 kHz, 8 kHz), Gate, Kompressor,
Verzögerung (0–500 ms). Gain bis Kompressor sind die Kanaleigenschaften
des strom-Audiomischers (`ch{N}_…`, Block-PATCH); lmix stellt nur die
nötigen Regler bereit (EQ-Frequenzen fest wie strom-Standard, Gate und
Kompressor mit Schwelle und Verhältnis). Aktion
`set_channel_strip { input, strip }` mit dem ganzen Zug; er steht in der
Show-Datei und folgt dem Input über Slot-Wechsel und Neustarts (beim
Zuweisen und Wiederherstellen gesetzt). Inputs ohne Ton lehnt lmix-core
ab („has no sound“).
**Verzögerung**: ein `identity` je Slot vor dem Mischer-Eingang, `ts-offset`
verschiebt die Zeitstempel, der Mischer spielt den Ton entsprechend später.
Der inter-Puffer der Slot-Tonspur wächst dafür auf 1 s. strom lehnt
Eigenschaften ohne `MUTABLE_PLAYING` im laufenden Betrieb ab, `ts-offset`
hat das Flag nicht, ist aber live unkritisch: strom-Patch
`lmix-live-offset` (Liste live sicherer Eigenschaften, ein Eintrag).
**Verworfen**: Verzögerung nur beim Aufbau des Mischers (Regler wirkt
nicht live, Lippensynchronität wird während der Probe eingestellt);
`audioecho` (live änderbar, aber mischt das trockene Signal immer bei).
**Console**: Menü „Ton“ der Kachel → „Kanalzug…“, Dialog mit Schiebern,
höchstens alle 100 ms eine Aktion.
**VM-Befund nebenbei**: der Qt-Console-Test wackelte, weil Software-GL der
Console (1,4 Kerne) und der CPU-Mischer (2,2 Kerne) die 4 Kerne der VM
ausschöpfen; das Programmbild kam dann nur alle 5–50 s, Klicks wirkten
verspätet. Die Engine lieferte dabei auf allen Live-Sockets 30 fps. Der
Test nimmt jetzt 2 Slots und wartet auf Bewegung und Zustand statt fester
Pausen.
**Offen**: Aux-Busse (Sends je Kanal, eigene Ausgänge), Pegelanzeige für
Gate/Kompressor-Wirkung, Hardware-Prüfung der Verzögerung (H.25).

## 2026-10-09 — M4 Schritt 4: Aux-Busse (4.7)

**Entscheidung**: zwei Aux-Busse im strom-Audiomischer (`num_aux_buses`),
mit festen Rollen: **Aux 1 vor dem Fader** (Monitormix, In-Ear: hört ein
Input auch, wenn er stumm oder nicht auf Sendung ist), **Aux 2 nach dem
Fader** (zweiter Sendemix, folgt Fader und An/Aus). Je Input ein Send-Pegel
pro Bus (0–100, 0 = aus, gleiche Kurve wie die Lautstärke), je Bus Master
mit Lautstärke und Stumm. Aktionen `set_aux_send { input, bus, level }`,
`set_aux_bus { bus, volume, mute }`; Sends und Master stehen in der
Show-Datei und werden beim Wiederherstellen gesetzt. Ein Ausgang kann
seinen Ton von einem Bus nehmen (`add_output … aux: 1|2`, z. B.
Mix-Minus für eine Schalte); das Bild bleibt Programm oder der Input.
Pegel je Bus in `GET /api/audio/levels` (`aux_peak_db`).
**Warum feste Rollen**: strom setzt vor/nach dem Fader (`chX_auxY_pre`)
nur beim Aufbau des Mischers. Umschalten hieße, den laufenden Mischer
(Bild und Ton) neu aufzubauen. Pegel, Master und Stumm sind live.
**Mischer**: Flow heißt jetzt `lmix mixer 4`, damit ein laufender Mischer
ohne Busse neu gebaut wird. Jeder Bus geht auf einen eigenen
inter-Kanal (`aux1_out`, `aux2_out`), Ausgänge abonnieren ihn statt des
Programmtons.
**Console**: Kanalzug-Dialog mit Abschnitt „Aux-Sends“, Knopf „Aux…“ über
den Inputs (Pegel, Lautstärke, Stumm je Bus), im Ausgangsdialog „Ton“
(wie die Quelle, Aux 1, Aux 2). Der Ausgangsdialog übergibt sein Formular
jetzt als ein JSON-Objekt (cxx-qt erlaubt an der Bridge keine Attribute,
acht Einzelargumente verletzten den Clippy-Standard).
**Offen**: mehr Busse und umschaltbares vor/nach (strom-Patch, Neuverknüpfen
im Betrieb); Aux als Kopfhörerquelle (Monitorbus hört heute Programm oder
Solos); Hardware-Prüfung mit echtem Rückweg (H.26).

## 2026-10-09 — M4 Schritt 5: Stinger (4.2)

**Entscheidung**: Stinger in lmix-core, ohne die Übergangs-Engine von strom
anzufassen. Eine Übergangstaste mit Effekt `stinger` nennt einen Clip-Input
(Datei, URL oder Playlist; WebM/VP9 mit Alpha lässt das Programm
drumherum sehen), ihre Länge und einen Cut-Punkt. Beim Drücken:
1. Clip von vorn starten und auf eine eigene DSK-Ebene unter den Overlays legen,
2. am Cut-Punkt Cut von Vorschau auf Programm darunter (Ton mit
   Audio-follows-Video schaltet hier),
3. nach der Länge Ebene weg und Clip pausieren.
Die Wartezeiten laufen im Service in einer eigenen Task (`Command::Wait`),
der Switcher bleibt frei (Zustand, Pegel). Cut, Übergänge, Direkt-Cut und
T-Bar lehnt er ab, solange ein Stinger läuft (`stinger_playing`).
Programm und Tally springen beim Drücken, das Bild erst am Cut-Punkt.
**Ebene**: strom erlaubte 4 DSK-Ebenen, alle für Overlays. strom-Patch
`lmix-stinger-layer`: bis zu 8 Ebenen (nur die Grenze). Ebene 0 ist der
Stinger, 1–4 die Overlays (Mischer-Flow `lmix mixer 5`, inter-Eingänge
`dsk0`–`dsk4`).
**Clip im Leerlauf**: pausiert, sobald er einer Taste zugewiesen ist und
nach jedem Neustart; sonst liefe er unsichtbar in Schleife. Entfällt sein
Input, wird die Taste ein Cut.
**Geprüft**: Alpha kommt auf dem CPU-Mischer bis ins Programm. Eine erste
Testdatei war überall durchsichtig (`drawbox` schreibt keine Alpha-Ebene);
mit `overlay` gebaut stimmt sie.
**Console**: Effekt „Stinger“ im Menü der Übergangstaste öffnet einen
Dialog (Clip, Länge, Cut-Punkt). Nebenbei behoben: das Menü ging bei leerer
Vorschau nicht auf, weil die gesperrte Taste auch den Rechtsklick schluckte;
er liegt jetzt auf einer Fläche um die Taste.
**Offen**: Länge aus der Clip-Dauer übernehmen; Stinger auf CUDA/GL mit
Alpha prüfen (H.27); Ton des Clips (spielt nach seinen Ton-Einstellungen);
die Kachel eines gleich nach dem Anlegen pausierten Clips bleibt leer.

## 2026-10-09 — M4 Schritt 6a: Looks (4.6, Farbkorrektur)

**Entscheidung**: Farbkorrektur je Input über den Shader-Look von strom
(`POST /effect`, `color_correct`): Helligkeit, Kontrast, Sättigung, Farbton,
Gamma, Farbtemperatur, Tönung. Der Look sitzt vor dem Verteiler des Inputs,
gilt also für Programm, Vorschau, Kacheln und Layouts. Aktion
`set_look { input, look }`; er steht in der Show-Datei, wird beim
Wiederherstellen gesetzt, und ein neuer Input auf einem Slot räumt den Look
des Vorgängers ab. Neutraler Look nimmt den Shader weg (`none`).
**Nur GL-Mischer**: strom hat Shader-Looks nur im GL-Backend; CUDA und CPU
haben keine Farbkorrektur. Der Zustand meldet `looks_available`; ohne GL
lehnt lmix-core einen Look mit Grund ab („looks need the GL mixer“), die
Console sperrt den Menüeintrag mit diesem Hinweis.
**Geprüft**: `scripts/test-looks-gl.sh` (Software-GL in der VM): Grau 128,
Helligkeit +0,4 → 230, neutral → 128. Unterwegs: Das Vorschaubild schläft
nach einer Pause, lmix-core liefert dann das letzte Bild; Tests wecken den
Abgriff vor dem Messen.
**Offen**: Looks auf CUDA (strom-Patch mit `cudaconvert`/Shader oder
GL-Umweg; H.28); Looks als Vorlagen speichern und auf mehrere Inputs legen.

## 2026-10-09 — M4 Schritt 6b: Layouts (4.6, Bild im Bild)

**Entscheidung**: Ein Layout ist ein Hintergrund-Input (oder Schwarz) und
bis zu acht Boxen darüber, jede ein Input in einem Rechteck (Bruchteile des
Bildes) mit optionalem weißem Rahmen. Es läuft als PiP-Komposition des
strom-Mischers (`PUT …/blocks/vm/pip/{n}`), der Mischer-Flow hat dafür vier
(`num_pips`, Flow `lmix mixer 6`). Die Id eines Layouts steht in Vorschau
und Programm wie die eines Inputs: Cut, Übergänge und Stinger nehmen es
(`take_source`), die T-Bar lehnt ein Layout in der Vorschau ab. Die Inputs
eines Layouts auf Programm haben rotes Tally; ein entfernter Input
verschwindet aus den Layouts. Aktionen `add_layout`, `set_layout`,
`remove_layout`, `preview_layout`; Layouts stehen in der Show-Datei.
**Fehler in strom** (strom-PR #7, `lmix-live-underlays`): Nach dem Start
zeigte ein Layout minutenlang nur seine Rahmen auf Schwarz, auch ein
einfacher Take war in der Zeit schwarz. Die Rahmen kommen aus kleinen
`videotestsrc` (nicht live), die ihre Puffer ab 0 stempeln und schon beim
Vorrollen schieben. Mit `start_time_selection=first` (Basiszeit 0, Inputs
mit Uhrzeit) nahm der Compositor deshalb 0 als Start seiner Ausgabe und
lief der Uhr um die Laufzeit der Maschine hinterher; die Bilder der Inputs
galten als „Zukunft“ und wurden nicht gezeichnet, bis er aufgeholt hatte
(50 s bis mehrere Minuten). Patch: mit Startzeit „first“ laufen die
Rahmenquellen live in der Bildrate des Programms (höchstens ein Bild mehr
Latenz, gemessen: unverändert 126,67 ms); andere Flows unverändert.
Gefunden mit `GST_DEBUG=aggregator:5` („Selecting start time 0“ beim
Bildmischer, Uhrzeit bei den Tonmischern).
**Console**: Leiste „Layouts“ unter den Inputs (Klick: Vorschau,
Rechtsklick: bearbeiten, entfernen) und ein Dialog mit Hintergrund, vier
Boxen an festen Plätzen (Viertel, Hälften, klein in einer Ecke) und Rahmen.
**Geprüft**: VM-Test, Abschnitt Layouts: Orange als Hintergrund, Ball oben
rechts, beide mit rotem Tally, wieder entfernt.
**Offen**: Boxen frei ziehen statt fester Plätze, Zuschnitt einer Box,
Übergänge zwischen Layouts als Bewegung (strom kann morphen); Layouts auf
CUDA/GL prüfen (H.29).

## 2026-10-09 — M8 Schritt 1: Browser-Seiten (WEB.1–3)

**Entscheidung**: lmix-core liefert kleine Seiten selbst aus (in der
Binärdatei, `web.rs`), im Kante-Aussehen (Farb-Tokens, abgeschrägte
Knöpfe). Sie nutzen dieselbe API wie jeder Client: Ereignisstrom und
Aktionen, kein eigener Weg. `/mixer`: Inputs mit Bild (alle 1,5 s), Klick
Vorschau, Doppelklick Programm, Cut, Übergangstasten, FTB, Layouts.
`/tally?input=N`: ganzer Bildschirm rot/grün, verbindet sich selbst neu.
`/keys`: für das Web freigegebene Kurzbefehle als große Tasten.
**Geprüft**: `scripts/test-web.sh` (Chromium/Playwright gegen lmix-core
ohne Engine).
**Offen**: Anmeldung/Rechte (WEB.4), Seiten über das LAN nur mit
`--listen 0.0.0.0`; Bilder als Live-Strom statt JPEG.

## 2026-10-09 — M6: Steuerung (AUT.1–3, 7, 10, 11, 13, 14)

**Kurzbefehle**: Beschriftung, optionale Taste, bis zu 16 Aktionen der
Reihe nach; die erste fehlerhafte bricht ab. Sie gehören zur Show und
laufen über Tasten der Console, `/keys` und `run_shortcut`. Der Dienst
führt jede Aktion aus, als käme sie allein (Datei-Aktionen, Stinger).
Kurzbefehle können keine Kurzbefehle auslösen oder ändern (keine
Schleifen); Tasten sind eindeutig, eine Show-Taste gewinnt gegen 1–9.
**Wiederherstellen und Sperre**: `restore_input` holt den zuletzt
geschlossenen Input mit Id, Einstellungen, Platz und Slot zurück (die
letzten zehn, solange die Show offen ist; seit 2026-10-29 `restore` für
Inputs, Layouts, Busse und Ausgänge, siehe dort). `set_lock` sperrt Schließen von
Inputs, Show öffnen/leeren und Ausgänge stoppen/entfernen; Umschalten geht
weiter. Die Sperre gilt nur zur Laufzeit.
**Auslöser**: je Input bis zu 8: bei Programm an/aus, Vorschau, Overlay
ein/aus, nach einer Verzögerung (bis 30 s) Aktionen ausführen. Sie feuern
nur für Aktionen des Bedieners (und von Cues), nicht für Aktionen anderer
Auslöser: Automation kann sich nicht aufschaukeln.
**Cue-Listen**: nummerierte Cues mit je 1–16 Aktionen; GO führt den
nächsten aus, Zurück den davor, ein Sprung setzt den nächsten. Bearbeiten
während der Show behält die Stelle. Beides steht in der Show-Datei.
**OSC** (`--osc`, UDP): `/lmix/action` mit JSON und Kurzadressen (`cut`,
`preview`, `program`, `transition`, `ftb`, `tbar`, `overlay/N/in|out`,
`cue/go|back`); Fehler gehen als `/lmix/error` an den Absender zurück;
`/lmix/subscribe` liefert Tally als `/lmix/tally s:Ziffern`. Eigener
kleiner Parser (Nachrichten, Bundles, Typen i/f/s/T/F), keine neue
Abhängigkeit; Zeitmarken von Bundles werden ignoriert.
**Console**: „Kurzbefehle…“, „Cues…“ (GO, Zurück, Nächster je Cue), im
Kachelmenü „Auslöser…“ und „Zurückholen“, Knopf „Sperre“. Die
Aktionszeilen sind ein gemeinsamer Baustein (Vorlagen oder JSON).
**Geprüft**: Switcher-Tests; VM-Test Abschnitt „automation“ (Cue-Liste,
deren Cues Auslöser feuern, Zurück feuert erneut); OSC im Headless-Test.
**Offen**: MIDI (AUT.4), globale Tasten außerhalb des Fensters (AUT.12),
Auslöser bei Countdown/Clip-Ende, Zustand per OSC über Tally hinaus.

## 2026-10-09 — M8 Schritt 2: Textprotokoll, Metriken, Headless (API.5, OPS.1, OPS.3)

**Textprotokoll** (`--tcp`): Zeilen über TCP, Begrüßung `LMIX 1`. Eine
JSON-Zeile ist eine Aktion, Antwort `OK` oder `ER <code> <grund>`;
`TALLY`, `STATE`, `SUB/UNSUB TALLY|STATE|LEVELS|OUTPUTS|ALL`, `QUIT`.
Tally als Ziffernfolge passt auf jeden Mikrocontroller. Wer zurückfällt,
bekommt danach den aktuellen Stand statt verlorener Zwischenstände.
Zeilen werden in einer eigenen Task gelesen (abbruchsicher im `select!`),
höchstens 64 KiB je Zeile.
**Metriken** (`GET /metrics`, Prometheus-Text): Inputs online, Tally,
Pegel je Input und Programm, Ausgänge (läuft/gestoppt/Fehler), Rate,
verlorene und verworfene Pakete, Latenz der letzten Messung, Laufzeit,
Speicher von lmix-core. Speicher von strom fehlt noch (eigener Prozess).
**Headless**: `LMIX_SHOW` öffnet eine Show beim Start, SIGTERM beendet
sauber (Engine und Ausgänge laufen weiter), `packaging/systemd` hat
Benutzer-Units für strom und lmix-core (Einstellungen in
`~/.config/lmix/core.env`).
**Geprüft**: `scripts/test-headless.sh` (ohne Engine): SIGTERM, Show beim
Start, Textprotokoll, OSC, Metriken.
**Offen**: Metriken von strom (Drops, Encoder, Speicher) durchreichen;
Units auf einer echten Maschine ohne Anmeldung (H.30).

## 2026-10-10 — Offene Punkte ohne Hardware: Layouts, Metriken, MIDI, globale Tasten, Clip-Ende, Stinger-Länge, OSC

**Layout-Boxen frei** (4.6): Der Layout-Dialog zeigt eine 16:9-Fläche.
Boxen zieht man an ihren Platz, die Ecke ändert die Größe (16:9, mit
Umschalt frei), Pfeiltasten verschieben (mit Umschalt: Größe), Raster 1 %.
Beim Ziehen bewegt sich nur das Element; die Box ändert sich beim
Loslassen, denn ein neues `boxes` baut die Delegates neu und bräche das
Ziehen ab. Die festen Plätze bleiben als Schnellwahl, bis zu 8 Boxen.
**Zuschnitt** je Box (`crop`: Anteil links/oben/rechts/unten, mindestens
5 % bleiben sichtbar) geht an strom als `transforms` der Quelle. strom
passt den Ausschnitt mit seinem Seitenverhältnis in die Box ein
(Ränder schwarz, wenn die Formen nicht passen). VM-Test: rechte Hälfte
von Orange in einer Vollbild-Box, linker Rand schwarz.
**Metriken der Engine** (OPS.3): lmix-core liest `SystemStats` und
`QoSStats` aus dem Ereignisstrom von strom mit: CPU, Speicher, GPU-Last,
-Speicher, Encoder/Decoder der Maschine; QoS je Flow (späte Puffer,
Meldungen „fällt zurück“, letzter Anteil), beschriftet als Mischer, Input
oder Ausgang. strom-Patch `lmix-process-memory` (strom-PR #8):
`SystemStats.process_memory`, der eigene Speicher von strom, damit ein
Leck (H.24) von anderem Verbrauch zu trennen ist. Geprüft: 263 MB in der VM.
**MIDI** (AUT.4): lmix-core liest rohe Bytes aus Gerätedateien
(`--midi /dev/snd/midiC1D0`, mehrere mit Komma), ohne ALSA-Bibliothek;
fehlt das Gerät, versucht es alle 2 s neu. Der Parser kennt Running
Status und überspringt SysEx und Echtzeit-Bytes. Ein Kurzbefehl kann eine
Note oder einen Regler tragen (Kanal fest oder beliebig); er läuft bei
Note an bzw. wenn ein Regler über 64 steigt. `set_midi_tbar` legt einen
Regler auf die T-Bar (0–127 → 0–1). Bindungen sind eindeutig. Jede
Nachricht geht als `Event::Midi` an die Clients (Textprotokoll: Thema
`MIDI`), so lernt die Console: Knopf „No MIDI“ im Kurzbefehl-Editor und
„T-bar from MIDI“ in der Liste. Geprüft mit einer FIFO statt Gerät
(Headless-Test, Console unter Xvfb). Rückmeldung an Controller (LEDs)
fehlt.
**Globale Tasten** (AUT.12): Die Console bindet die Tasten der
Kurzbefehle über das Portal `org.freedesktop.portal.GlobalShortcuts`
(KDE Plasma, GNOME 48+, Hyprland; unter X11 ohne Portal nicht). Der
Desktop fragt einmal und meldet Drücke, auch wenn ein Spiel vorne ist.
Einstellung der Console („Keys also work when the Console is not in
front“), der Dialog zeigt den Stand. Bei jeder Änderung wird die Sitzung
geschlossen und neu gebunden. Geprüft gegen ein nachgebautes Portal
(`workspace/tools/fake-portal`, `scripts/test-global-keys.sh`).
**Clip-Ende** (AUT.7): Auslöser `clip_end` für Datei-, URL- und
Playlist-Inputs feuert, wenn das letzte Stück endet (der Player beginnt
von vorn), oder mit `before_ms` (bis 60 s) so lange vor dem Ende
(Countdown, um rechtzeitig weiterzuschalten). lmix-core liest dazu die
Positionsmeldungen der Mediaplayer von strom (alle 200 ms) und erkennt
Ende und Schwelle aus zwei aufeinanderfolgenden Meldungen; ein
Zurückspringen um mehr als 1 s zählt bei einer Einzeldatei als Ende.
Die Aktionen laufen wie andere Auslöser und lösen keine weiteren aus.
Console: „Clip ends“ mit „ms before“ im Auslöser-Dialog eines Clips.
VM-Test: Countdown 2 s vor dem Ende und Ende einer Playlist.
**Stinger-Länge**: Inputs mit Player melden `clip_ms`, die Länge des
laufenden Stücks (aus den Positionsmeldungen). Der Stinger-Dialog nimmt
sie für einen neuen Stinger und bietet „Clip length (2.0 s)“ an.
**OSC mit Zustand** (AUT.13): Abonnenten bekommen außer der Tally
`/lmix/program` und `/lmix/preview` (Name, Nummer; 0 für ein Layout),
`/lmix/tbar`, `/lmix/ftb` und `/lmix/cue/next` je Liste, gleich nach dem
Abo und danach nur Änderungen. `/lmix/state` beantwortet den ganzen
Zustand als JSON.
**Offen**: Bewegung zwischen Layouts; MIDI-Rückmeldung; globale Tasten
am echten KDE prüfen (H.32), MIDI mit echtem Controller (H.31).

## 2026-10-11 — M7 Teil A: Titel und Datenquellen ohne Render-Engine

**Kontext**: Titel sind Muss (§3.11, GFX). Die Engine-Wahl (E.9, CEF
gegen WPE) hängt am Benchmark M7.0, der eine freie GPU braucht; auf dem
Entwicklungsrechner gibt es zudem weder `cefsrc` noch `wpesrc`. Alles,
was nicht von der Engine abhängt, sollte trotzdem fertig werden.
**Optionen**: (a) Vorlagen über eine Engine-Schnittstelle direkt
ansteuern (cefsrc: `listen-for-js-signals`, WPE: `run-javascript`), je
Engine eigener Code; (b) eine Seite je Grafikkanal, die lmix-core selbst
ausliefert und die dem Zustand über `/api/events` folgt; die Engine
rendert nur eine URL.
**Entscheidung**: (b). Ein **Grafikkanal** (`add_graphics`, bis 8) ist
die Seite `/graphics/<n>`: bis zu 8 Ebenen, je ein `iframe` mit einer
Vorlage, transparent, 1920×1080 (oder `?size=1080x1920`). Die Seite
gleicht Zustand und Vorlage ab: neue Vorlage oder neue Revision → laden,
`update()`; andere Werte → `update()`; Phase/`played`/`step` → `play()`,
`next()`, `stop()`; neue `call.seq` → Funktion aufrufen. Damit gilt die
CasparCG-Konvention unverändert (GFX.1), CasparCG-XML wählt das Manifest
(`data_format: xml`, GFX.3). Dieselbe Seite ist im Browser die Vorschau
(GFX.13) und läuft in CEF wie in WPE: E.9 bleibt offen, GFX.2 braucht
nur noch „URL als Input“ in strom.
**Lebenszyklus** (GFX.4): `title_load` (mit `play`), `title_play`,
`title_next`, `title_stop`, `title_clear`, `title_update` (Felder
mischen, leer entfernt), `title_call`. Die Vorlage meldet über das SDK
(`lmixTitle.report`), die Seite schickt `title_report`, lmix-core gibt
es als `Event::Title` weiter (Textprotokoll `TITLE`). Nur die
Renderer-Seite meldet, `?preview`-Seiten nicht (sonst doppelt).
**Vorlagen**: eingebaut (Kante-Satz, im Binary über `build.rs`, Kante-
Tokens und Schriften per `scripts/sync-kante.sh`, offline) und ein
Ordner (`--templates`), je Vorlage `index.html` + `manifest.json` oder
eine einzelne `.html` (+ `.json`). Das Manifest beschreibt Felder (Text,
mehrzeilig, Zahl, Farbe, Bild, Schalter, Auswahl), `steps`,
`functions`. Ordner-Vorlagen bekommen eine Revision aus den Dateizeiten;
ändert sich eine Datei, lädt die Seite die Vorlage neu und gibt ihr die
gleichen Daten (GFX.14). Pfade bleiben im Ordner (auch über Links).
**Vorlagensatz** (GFX.12): Bauchbinde, Name+Funktion, Laufschrift
(Tempo, Richtung, `append`), Uhr, Countdown (Dauer oder Uhrzeit, ab-
oder aufwärts, `start`/`pause`/`reset`/`adjust`, meldet `zero`, GFX.9),
Spielstand, Vollbild-Karte (zweiter Schritt). Hochkant über
`orientation: portrait`. Kante: Schnittecke, Akzentleiste, Rajdhani,
JetBrains Mono, Bewegung aus den Kante-Dauern; die Wischblende ist eine
Maske, damit die Schnittecke (clip-path) bleibt.
**Datensätze** (GFX.8): `save_dataset`, `apply_dataset`,
`remove_dataset`, in der Show.
**Datenquellen** (DAT.1 teils, DAT.2–5): CSV (Trennzeichen geraten,
Anführungszeichen, Kopfzeile wahlweise) und JSON (Array von Objekten,
Spalten in Dateireihenfolge); Pfade relativ zur Show-Datei. lmix-core
liest neu, wenn sich die Datei ändert. `title_bind` legt ein Feld auf
eine Spalte (Name oder Nummer, Muster `{0}`); `data_row`, `data_next`,
`data_previous`, `set_data_auto` (ab 500 ms, Schleife). Wird eine Quelle
entfernt, behalten gebundene Felder ihren letzten Wert.
**Console**: „Titles…“ (Kanäle, Ebenen 1–8 mit Stand, Vorlage laden,
Formular aus dem Manifest, Live oder „Apply“, Funktionen, Datensätze,
Feld an Spalte binden, Vorschau im Browser) und „Data…“ (Tabelle, Zeile
per Klick, vor/zurück, Weiterschalten mit Schleife).
**Demo**: die Sendehaus-Show bekommt einen Grafikkanal mit Sendung,
Gast und Uhr aus der Studio-Weber-Welt.
**Geprüft**: Rust-Tests (Switcher, Show-Datei, API, Vorlagen, CSV/JSON);
`scripts/test-titles.sh` in Chromium (ohne GPU): laden unsichtbar,
spielen, Feld ändern, stoppen mit `done`, zweiter Schritt, entfernen,
Countdown meldet `zero`, Ordner-Vorlage mit XML-Daten und Funktions-
aufruf, Datei geändert → neu geladen mit Daten, CSV-Feld folgt Zeile und
Datei, Vorschau meldet nichts, Vorlage allein mit Vorschauleiste. Console
unter Xvfb per Klick (Abspielen, Feld live, Zeile wählen).
**Keine neue Abhängigkeit** (CSV und geordnetes JSON von Hand).
**Auslöser auf Meldungen** (GFX.9, B36): ein Grafikkanal hat eigene
Auslöser (`set_title_triggers`): Meldung (`zero`, `done`, leer = jede),
wahlweise nur von einer Ebene, Verzögerung, 1–16 Aktionen; dieselben
Grenzen wie bei Input-Auslösern (höchstens 8, keine Automatik in den
Aktionen). Console: „Triggers…“ im Titel-Dialog (der Auslöser-Dialog der
Inputs mit Meldung und Ebene statt Ereignis). Browser-Test: Countdown bei
0 lädt einen Titel.
**Zahl hochzählen** (B35): `title_add` addiert auf ein Feld (Standard
+1; leer oder keine Zahl zählt als 0), in der Console „−1“/„+1“ an
Zahlenfeldern, als Kurzbefehl auf jede Taste oder Stream-Deck-Taste.
**Textfolien** (GFX.21): Vorlage `lmix/text-slides`, Folien durch
Leerzeilen getrennt; `next` zeigt die nächste, Funktion `back` die
vorige, die letzte meldet `last`; Layout unten (Stream) oder ganzes Bild
(Beamer).
**Quellen im Netz und Text** (DAT.1, DAT.2): eine Quelle darf eine
`http(s)://`-Adresse sein (öffentliches Google Sheet über den CSV-Export
`…/export?format=csv`); lmix-core holt sie alle 10 s (höchstens 16 MB,
10 s Zeitlimit) und meldet nur Änderungen. Typ `text`: jede nicht leere
Zeile eine Zeile mit der Spalte `text` (Laufschrift, Folien). Dateien
und Adressen laufen in einer eigenen Schleife, damit ein langsamer
Abruf das Weiterschalten nicht aufhält.
**Sprachen der Vorlagen**: ein Manifest kann `i18n` je Sprache haben
(Name, Feldbezeichnungen, Anzeige der Auswahlwerte); die Console nimmt
ihre Sprache, die Vorschauleiste die des Browsers. Der eingebaute Satz
hat Deutsch (ein Test prüft, dass kein Feld fehlt); die Werte selbst
bleiben die englischen Schlüssel (`left`, `down`).
**Ebenen-Transformation** (GFX.18): `title_transform` setzt alle Werte
einer Ebene zugleich (Verschiebung, Größe, Drehung, Neigung mit
Perspektive, Zuschnitt, Deckkraft, Helligkeit/Kontrast/Sättigung,
Blend-Modus) und bewegt sie in `duration_ms` mit Easing dorthin. Die
Grafikseite setzt es per CSS auf das iframe der Ebene, also für jede
Vorlage und jede Engine gleich; die Transformation gehört zur Ebene und
bleibt beim Laden einer anderen Vorlage (wie CasparCGs MIXER). Console:
Bereich „Place“ mit den häufigsten Werten.
**Lottie** (GFX.5, E.10 erster Teil): lottie-web 5.12.2 (MIT, nur der
SVG-Build, 240 KB, npm-Integrität geprüft) liegt im SDK
(`sdk/lottie/`), `sdk/lmix-lottie.js` macht aus einer Bodymovin-
Animation eine Vorlage: Textebenen sind Felder (Ebenenname = Feld),
Marker `in` und `out` die Ein- und Ausblendung, übrige Marker die
Schritte für `next`. Ebenen `kante:<token>` nehmen Füllung oder
Textfarbe aus den Kante-Tokens, also keine festen Farben in der
Animation. Die Schriften der Animation werden vor dem Messen geladen
(sonst falsche Abstände nach einem Textwechsel). Eingebaut:
`lmix/lottie-lower-third` (erzeugt von
`scripts/make-lottie-lower-third.py`). Nativer Lottie-Input (ThorVG)
bleibt später.
**Datensätze vor/zurück, CSV** (GFX.8): `dataset_next` /
`dataset_previous` gehen durch die Sätze der Vorlage einer Ebene (und die
ohne Vorlage), nach dem letzten wieder zum ersten; `import_datasets`
liest eine CSV-Tabelle (Spalte `name`, übrige Spalten Felder; gleiche
Namen ersetzt), `export_datasets` schreibt sie zurück (atomar, Pfade
relativ zur Show-Datei). Console: ◀/▶ und Import/Export im Titel-Dialog.
**Spät geladene Seite** (B37): der Zustand trägt `played_at_ms`; lädt
eine Grafikseite, während ein Titel läuft, gibt sie der Vorlage mit, wie
lange er schon spielt (`lmixTitle.resumedMs()`), der Countdown zählt von
dort weiter. Eine Pause vor dem Neuladen kennt die Seite nicht.
**Feste Zeile** (DAT.4): eine Bindung kann `row` tragen und bleibt dann
bei dieser Zeile, gleich welche gewählt ist (etwa Heim- und Gastteam aus
Zeile 1 und 2 derselben Tabelle).
**Vorschaubild** (GFX.19): liegt `thumbnail.png` neben einer Vorlage
(bei einzelnen `.html`: `<name>.png`), nennt `GET /api/templates` die
Adresse. Die eingebauten erzeugt `scripts/make-title-thumbnails.cjs`
(Chromium, Feldbezeichnungen als Text, transparent, 480×270); die
Console zeigt das Bild neben der Vorlagenwahl. Der Titel-Dialog scrollt,
wenn er höher als das Fenster wird.
**RSS/Atom und XML** (DAT.1): Typ `feed` macht aus jedem `item`/`entry`
eine Zeile (`title`, `text`, `link`, `date`, `author`), Typ `xml` aus
den Elementen eines Pfads (`scores/game`, mit `//` irgendwo) eine Zeile
mit Attributen (`@id`) und Kindelementen als Spalten. Parser `roxmltree`
0.21 (MIT/Apache-2.0, ohne eigene Abhängigkeiten außer `memchr`), kein
volles XPath. Console: Art der Quelle wählbar, aus dem Namen vorbelegt.
**Zeilen nicht im Zustand**: der Zustand geht bei jeder Änderung (auch
jeder T-Bar-Bewegung) als Ganzes an alle Clients; mit bis zu 10 000
Zeilen je Datenquelle wären das Megabytes je Ereignis. Er trägt deshalb
nur Spalten, Zeilenzahl, die gewählte Zeile und eine Revision; die
Tabelle liefert `GET /api/data/<quelle>`, die Console holt sie, wenn sich
die Revision ändert.
**Webseite „Titel“** (WEB.4): `/titles` zeigt die Ebenen eines Kanals
mit Play/Next/Stop, das Formular der gewählten Ebene (gebundene Felder
gesperrt) mit „Apply“, die Datensätze der Vorlage und „Save as set“; Werte
nur über DOM-Knoten, nie als HTML (sie können aus fremden Quellen kommen).
**Review**: ein unabhängiger Durchgang über den Branch fand sechs Fehler
(geleertes Feld blieb sichtbar, Laden auf eine laufende Ebene ging auf
Sendung, Netzabruf unter der Switcher-Sperre, Auslöser-Schleife über
`title_report`, Datensatz-Index nach Löschen, Ebenenprüfung der Auslöser),
alle behoben und mit Tests abgedeckt.
**Offen**: Rendern in strom (GFX.2, nach M7.0), Grafikkanal als Input
und Overlay (GFX.17), Overlay wartet auf das Ende der Ausblendung
(GFX.4), Titel als Stinger (GFX.11), XLSX/ODS, Google Sheets mit API-Schlüssel (DAT.1), Zeilen zu Spalten
(DAT.3), Bilder aus lokalen Pfaden außerhalb des Vorlagenordners.

## 2026-10-11 — M9 Anfang: Console ohne GPU (UI.5, UI.10)

**Kontext**: M7 wartet für den Rest auf die Render-Engine (M7.0, GPU);
der User hat neue Meilensteine freigegeben. Aus M9 zuerst Console-Punkte,
die ohne GPU prüfbar sind.
**Transportleiste** (UI.5): unter Vorschau und Programm, sobald dort ein
Datei-, URL- oder Playlist-Input steht: Neustart, Abspielen, Pause, bei
Playlists Voriges/Nächstes, daneben die Länge des laufenden Stücks
(`clip_ms`). Eine Positionsanzeige fehlt: der Zustand kennt nur die Länge.
**Stopp-Rückfrage** (UI.10): ein laufender Ausgang (Aufnahme, Stream)
stoppt erst nach „OK“; „Ask before stopping“ im Dialog schaltet die Frage
ab (Console-Einstellung `safety/confirmStop`). Die Sperre (AUT.11) bleibt
der harte Schutz.
**Geprüft**: Console unter Xvfb gegen lmix-core ohne Engine: Leiste mit
Playlist auf Programm; Abbrechen lässt laufen, OK stoppt, abgeschaltet
stoppt ohne Frage; keine QML-Warnungen.
**TSL-Tally** (PER.1): `--tsl <host:port>` (mehrere mit Komma) schickt
per UDP TSL UMD 5.0 (Standard) oder mit `--tsl-version 3` 3.1 an
Multiviewer und Tally-Systeme: je Input mit Bild Name und Tally
(Programm rot = Tally 1, Vorschau grün = Tally 2), Adresse/Index = Nummer
in der Leiste. Gesendet bei jeder Änderung und alle 5 s (Empfänger
vergessen stumme Quellen). TSL 3.1 hat 127 Adressen und 16 ASCII-Zeichen;
5.0 bekommt alle Inputs in einem Paket (bis 32 je Paket). TCP-Rahmen
(DLE/STX) fehlt noch. Geprüft mit Paket-Tests nach der Spezifikation und
einem UDP-Empfänger im Test; an echter Hardware nicht.
**PTZ** (PER.2, PER.3 teils): ein Input kann eine PTZ-Adresse haben
(`set_ptz`, in der Show): VISCA over IP nach Sony (UDP, 8-Byte-Kopf,
Port 52381) oder VISCA direkt in UDP (PTZOptics u. a.). Aktionen
`ptz_move` (Schwenken/Neigen mit Tempo −1…1, 0 0 hält an), `ptz_zoom`,
`ptz_focus` (auto, manuell, nah, fern, stopp), `ptz_home`, `ptz_preset`
(anfahren oder speichern, 0–127). Der Service schickt sie, die Show
ändert sich nicht; Antworten der Kamera wertet lmix nicht aus.
Kameraadresse 1. Positionen als virtuelle Inputs (PER.3) noch nicht:
eine Position als Auslöser „geht in die Vorschau“ fährt die Kamera an.
Console: „PTZ…“ im Kachelmenü (Adresse, Steuerkreuz beim Halten, Tempo,
Zoom, Fokus, Home, Positionen 1–8 anfahren oder speichern). Geprüft mit
Byte-Tests nach der VISCA-Spezifikation, einem UDP-Empfänger im Test
und der Console gegen eine nachgebaute Kamera; an einer echten Kamera
nicht.
**Startoptionen** (UI.12): Console `--window normal|maximized|fullscreen`
und `--mode simple|advanced`; lmix-core `--start-outputs` startet
genannte Ausgänge (Nummer oder Name), sobald die Engine bereit ist, einen
schon laufenden (übernommenen) nicht neu, also keine Lücke in einer
Aufnahme. Die letzte Show öffnet lmix-core ohnehin (Autosave, `--show`).
**Ansicht** (UI.3, UI.7): Menü „View“ neben „Inputs“: Kachelgröße klein,
mittel, groß; sichtbare Reihen 1–4, darüber Bildlauf; „Simple mode“
blendet Kurzbefehle, Cues, Aux, Titel, Daten, Layouts und die
Latenzmessung aus (Umschalten, Overlays, FTB, Ausgänge, Sperre bleiben).
Gespeichert unter `view/…`. Einzelne Inputs einklappen fehlt noch.
**Dynamische Inputs und Werte** (AUT.5): `set_dynamic_input` (Platz
1–4) lässt `"dynamic1"`…`"dynamic4"` in jeder späteren Aktion einen Input
bezeichnen (wie `"preview"`, `"program"`): eine Taste wählt, eine andere
wirkt. `set_dynamic_value` setzt Werte 1–4, die in Titeltexten und
Funktionsargumenten für `{value1}`… stehen. Beides gilt zur Laufzeit,
nicht in der Show; eine neue Show leert die Inputs. Console: „Dynamic
input“ im Kachelmenü, Kennzeichen „D1“… auf der Kachel.
**Review M9**: ein unabhängiger Durchgang fand vier Fehler, alle behoben:
ein kurzer Tipp auf eine PTZ-Taste konnte den Stopp vor der Bewegung
ankommen lassen (die Console schickte Aktionen parallel; jetzt der Reihe
nach, eine IP-Adresse braucht keine DNS-Abfrage, der Stopp geht nach
150 ms ein zweites Mal raus); Zoom und Fokus hielten beim Abbrechen nicht
an (jetzt auch beim Schließen des Dialogs); TSL ließ verschwundene
Anzeigen in ihrem letzten Tally stehen und schickte vor der ersten
Änderung nichts; der Stopp-Dialog nannte den Ausgang über seine Nummer
(jetzt über die Id).
**Show-Paket** (SHW.6): `export_package` schreibt ein Tar (`.lmixpkg`,
unkomprimiert: Medien sind es schon) mit der Show, den Dateien der
Inputs (Datei, Standbild, Playlist), den Dateien der Datenquellen und
den eigenen Titelvorlagen; die Pfade in der Show zeigen ins Paket.
Fehlt eine Datei, nennt der Fehler sie. `import_package` entpackt in
einen neuen oder leeren Ordner (nur Dateien und Ordner, nichts außerhalb),
setzt die Inputs auf absolute Pfade dort, übernimmt Vorlagen, die der
Vorlagenordner nicht hat (vorhandene bleiben), und öffnet die Show. Beides
läuft ohne Switcher-Sperre (Medien können Gigabytes sein); die Sperre
(AUT.11) verhindert den Import. Abhängigkeit `tar` 0.4 (MIT OR
Apache-2.0, ohne xattr). Console: „Package“ → Export…/Import….
**Input-Einstellungen in Reitern** (UI.8): „Settings…“ oben im
Kachelmenü öffnet einen Dialog mit „General“ (Name, Farbmarke, Signal-
verlust, Quelle mit „Replace…“), „Colour“ (Look), „Audio“ (Kanalzug und
Aux-Sends), „Triggers“, „PTZ“ (Adresse, „Controls…“) und „Advanced“ (Id,
Slot, Zustand, WHIP-Adresse). Farbe und Ton sind die Inhalte der
bisherigen Dialoge, herausgelöst als `LookPanel` und `ChannelStripPanel`;
die Dialoge gibt es weiter. Key, Effekte, Position, Ebenen und Tally
fehlen, solange die Engine sie nicht hat.
**Position im Clip** (B39, UI.5): die Positionsmeldungen der Player
(alle 200 ms) gehen als `Event::Position` an die Clients (Textprotokoll
`POSITION`), nicht als Zustandsänderung (kein Autosave, kein ganzer
Zustand je Meldung). Die Transportleiste zeigt Fortschritt und
„Position / Länge“. Springen im Clip fehlt (strom hat dafür keine Aktion
in lmix).
**Review der späteren M9-Teile**: ein dritter Durchgang fand: Import
packte vor der Sperrprüfung aus (jetzt vorher geprüft); Vorlagen, die nur
in Aktionen (Kurzbefehle, Cues, Auslöser) geladen werden, fehlten im
Paket; Pfade mit `..` in einem fremden Paket zeigten aus dem Ordner
(jetzt abgelehnt); eine halbe Paketdatei blieb nach einem Fehler liegen;
dynamische Werte wurden zweimal ersetzt (jetzt in einem Durchgang);
Reiterleiste und Auswahlfelder der Input-Einstellungen zeigten nach dem
Wiederöffnen den alten Stand. Alle behoben.
**TSL über TCP** (B40): `--tsl-tcp <host:port>` schickt dasselbe über
TCP, TSL 5.0 mit DLE/STX-Rahmen (0xFE im Paket verdoppelt), TSL 3.1
unverändert. Die Verbindung entsteht beim Senden und nach einem Fehler
neu; ein nicht erreichbarer Empfänger wird 2 s lang übersprungen, damit
er die übrigen Pakete nicht aufhält.
**Layout-Editor** (LAY.6): eine gezogene Box rastet mit Anfang, Ende
oder Mitte an Bildkanten, Bildmitte, dem Schutzbereich (5 %) und den
Kanten und Mitten anderer Boxen ein (näher als 1,5 % des Bildes), sonst
ins 1-%-Raster; mit Strg frei (Alt verschiebt unter KWin das Fenster).
Rückgängig/Wiederholen (Knöpfe, Strg+Z, Strg+Umschalt+Z) über bis zu 100
Schritte samt Hintergrund, solange der Dialog offen ist; Pixelwerte der
gewählten Box für 1920 × 1080. Mehrfachauswahl und Sperre fehlen.
**Lampen an MIDI-Controllern** (AUT.6): mit `--midi-feedback` schreibt
lmix-core auf die `--midi`-Geräte: die Note oder der Regler eines
Kurzbefehls zeigt den Stand seiner ersten Aktion (Input auf Programm, in
der Vorschau, aus; FTB; Ausgang läuft; Sperre). Nur Änderungen, kurz
gesammelt (eine Blende ergibt einen Schwung). Werte für Programm,
Vorschau und aus mit `--midi-lamps` (Farben je Controller, APC mini
3,1,0). Pegel und Motorfader fehlen. Geprüft mit einer Datei statt
Gerät; an einem echten Controller nicht.
**Review, vierter Durchgang**: TSL über TCP konnte bei einem Empfänger,
der nicht liest, beim Schreiben hängen (jetzt 1 s Frist, dann neu
verbinden); MIDI-Lampen blieben nach dem Entfernen eines Kurzbefehls an,
kannten `dynamicN` nicht und merkten sich ungeschriebene Stände (ein
wieder angesteckter Controller blieb dunkel); im Layout-Editor fehlte
ohne Einrasten das Raster, Alt kollidierte mit KWin, und Rückgängig
übersah den Hintergrund und speicherte leere Schritte. Alle behoben.

## 2026-10-11 — strom-Speicher (H.24): Vergleichsläufe auf Hardware

**Läufe** (`scripts/soak-hardware.sh`, RTX 3090, je 60 min, RSS von strom
jede Minute in `rss.csv`; Grundlage wie am 2026-10-09: CUDA, 16 Inputs,
ISO-Aufnahme, 2 × SRT; je Lauf eine Änderung):

| Lauf | RSS 15 → 60 min | Verlauf ab Minute 25 | Latenz |
|---|---|---|---|
| Grundlage | 999 → 1004 MB | 985 → 1004 MB, stetig (~33 MB/h) | 106–107 ms |
| ohne ISO | 773 → 759 MB | flach (760–775 MB) | 106–107 ms |
| 8 Inputs | 679 → 729 MB | 709 → 729 MB, stetig (~30 MB/h) | 106–108 ms |
| `MALLOC_ARENA_MAX=2` | 809 → 947 MB | Sprung um Minute 37 (818 → 940), danach ~20 MB/h | 86–89 ms |
| `--mixer gl` | 1531 → 1945 MB | ~580 MB/h | 112–892 ms |

**Störung**: Ab ~13:45 lief ein fremder Prozess auf der GPU (bis 22 GB,
oft 95–100 %). Die Läufe „8 Inputs“, GL und `MALLOC_ARENA_MAX=2` sind
dadurch nur eingeschränkt vergleichbar.

**Befund**: Nur der Lauf ohne ISO-Aufnahme bleibt flach, alle Läufe mit
ISO wachsen gleichmäßig um 20–33 MB/h, unabhängig von der Zahl der
Inputs. Verdacht: der Ausgangs-Flow mit Input-Quelle (ISO). 60 min sind
für diese Rate knapp; bestätigen mit einem längeren Paar (Grundlage gegen
ohne ISO, je ≥ 3 h, GPU sonst frei).
**GL**: Mit dem GL-Mischer kommt über SRT weiter kein Video an (Befund
vom 2026-10-09), und strom wächst dabei um ~580 MB/h; der hängende
SRT-Zweig staut offenbar nicht nur die 2-s-Audio-Queue. Erst nach dem Fix
in strom erneut messen.
**Latenz**: Mit CUDA jetzt 106–107 ms statt 56–63 ms am 2026-10-09
(seitdem M4: Overlays, T-Bar, Kanalzug, Aux-Busse, Stinger). Der Lauf mit
`MALLOC_ARENA_MAX=2` lag bei 87 ms; Ursache für beides offen.

## 2026-10-12 — Tabellen, Folien aus Datei, eingeklappte Inputs (DAT.1, DAT.3, GFX.21, UI.3)

**Kontext**: Rest von M7 und M9, der weder GPU noch Engine braucht.

**Entscheidung**:
- **Tabellenkalkulationen** (DAT.1): neue Quellenart `sheet` für XLSX,
  XLSM, XLS und ODS über `calamine` (ohne Standard-Features, kein
  `chrono`), ein Blatt (`sheet`, sonst das erste), sonst wie CSV
  (Kopfzeile). Datumszellen werden ISO-Daten (`2026-10-01`, mit Uhrzeit,
  wenn eine dabei ist, Zeiten unter einem Tag als `18:30:00`, aus ODS
  ebenso). Grenzen: Dateien mit dem 1904-Kalender (altes Excel für Mac)
  lägen vier Jahre daneben und XLSX-Dauern ab 24 h (`[h]:mm`) erscheinen
  als Datum, beides verrät `calamine` ohne `dates` nicht; verbundene
  Zellen haben den Wert nur oben links; Zahlenformate (Prozent, Währung)
  gehen verloren, es kommt die Zahl. Tabellen werden im Hintergrund und
  außerhalb der Laufzeit gelesen (Entpacken dauert), wie Adressen. Ein
  Google Sheet als XLSX-Export geht auch (`format=xlsx`).
- **Zeilen zu Spalten** (DAT.3): `transpose` bei CSV und Tabellen dreht
  die Tabelle vor der Kopfzeile: Namen in der ersten Spalte, je weitere
  Spalte ein Eintrag (Formulare, Spielberichte). Ganz leere Zeilen zählen
  bei Tabellen und beim Drehen nicht (Tabellen haben sie oft im benutzten
  Bereich); in CSV bleibt eine Zeile `;;;` eine Zeile wie bisher, damit
  eine gespeicherte Zeilennummer auf denselben Eintrag zeigt.
- **Textfolien aus Datei** (GFX.21): `text` mit `paragraphs` macht jeden
  Block zwischen Leerzeilen zu einem Eintrag, Zeilenumbrüche bleiben.
  An das Feld `text` der Vorlage „Textfolien“ gebunden zeigt sie eine
  Folie, die Quelle schaltet weiter (`data_next` auf einer Taste); der
  Wechsel blendet wie `next()`. Kein eigener Folienzähler in der
  Vorlage: Vor/zurück, Auto-Weiter und Zeilenwahl gibt es bei Quellen
  schon. Beamer erst mit GFX.2.
- **Einklappen** (UI.3): ein Input lässt sich im Kachelmenü einklappen;
  er steht dann als Knopf in der Zeile „Eingeklappt“ (Klick Vorschau,
  Doppelklick Schnitt, Rechtsklick ausklappen), rot/hervorgehoben wie die
  Layout-Knöpfe. Gespeichert je Console (Einstellung `view/collapsed`,
  Input-Ids), nicht in der Show: es ist die Ansicht eines Bedieners. Die
  Kachelliste bleibt ein `GridView` (nur sichtbare Kacheln leben,
  Performance), eingeklappte Inputs fehlen in ihrem Modell; Ziehen zum
  Umsortieren rechnet über die Ids, nicht über Positionen.

**Geprüft**: Tests für Tabellen (XLSX und ODS aus LibreOffice in
`lmix-core/testdata`), Datum, Drehen, Absätze; `scripts/test-titles.sh`
mit Folien aus Datei; Datenquellen-Dialog und Einklappen auf Xvfb.

**Review**: ein Durchgang fand: schnelles Vor und Zurück in den Folien
endete auf der falschen (eine laufende Blende wurde nicht ersetzt; jetzt
mit Test in `test-titles.sh`); CSV verlor Leerzeilen (siehe oben);
ODS-Zeiten kamen roh (`PT18H30M00S`), Excel-Daten vor dem 1. März 1900
lagen einen Tag daneben; große Tabellen hielten den Mischer auf; die
eingeklappten Knöpfe wurden bei jeder Zustandsmeldung neu gebaut (ein
offenes Menü ging zu; jetzt ein Modell mit Schlüsseln wie bei den
Kacheln). Alle behoben.

## 2026-10-12 — Layout-Editor: Mehrfachauswahl und Sperre (LAY.6)

**Entscheidung**: Umschalt+Klick nimmt eine Box zur Auswahl hinzu (oder
heraus); gewählte Boxen tragen einen Fokusrahmen. Ziehen einer gewählten
Box zieht die anderen sichtbar mit, gefangen wird an der gezogenen Box
(gegen Bild, Schutzbereich und nicht gewählte Boxen), die anderen
verschieben sich um gleich viel; Pfeiltasten verschieben alle gewählten,
zusammen im Bild gehalten, als ein Schritt fürs Rückgängig. Umschalt
gehört damit beim Klick der Auswahl, Strg dem freien Platzieren, Umschalt
am Eckgriff dem freien Seitenverhältnis wie bisher.
**Sperre** je Box (Häkchen „Sperren“): die Box zieht und skaliert nicht,
die Ortswahl ist aus, Pfeiltasten lassen sie stehen; Eingang, Rahmen und
Zuschnitt bleiben änderbar. Die Sperre wird mit dem Layout gespeichert
(`locked` an `BoxSpec`/`LayoutBox`, fehlt sie, ist sie aus); die Engine
sieht sie nicht.
**Geprüft**: Switcher-Test (Sperre bleibt am Layout); auf Xvfb zwei
Boxen gewählt und gezogen (gesperrte bleibt), Pfeiltasten, Strg+Z,
gespeichert.
**Review**: gemeinsam gezogene Boxen rasteten danach jede für sich ins
Raster (das Fangen ging verloren, Abstände änderten sich; jetzt bewegen
sich alle um denselben Betrag, ohne Raster je Box); die ganze Gruppe
bleibt beim Ziehen im Bild; eine mit Umschalt herausgenommene Box zog
die anderen scheinbar mit; ein abgebrochenes Ziehen ließ Boxen versetzt;
ein einfacher Klick auf eine von mehreren gewählten wählt jetzt nur sie.

## 2026-10-12 — Google Sheets über die API (DAT.1)

**Entscheidung**: Quellenart `google_sheet` mit Adresse oder Id des
Sheets und optional Blatt/Bereich (`Gäste!A1:D50`, sonst das erste Blatt,
`A:ZZ`); gelesen über die Sheets API v4 (`values`, formatierte Werte), dann
wie CSV (Kopfzeile, Drehen). Der **API-Schlüssel** steht nicht in der Show
(Shows wandern als Paket weiter), sondern ist Startoption von lmix-core:
`--google-api-key` bzw. `LMIX_GOOGLE_API_KEY` (in `core.env`), in `--help`
verdeckt; Fehlermeldungen nennen die Adresse ohne Schlüssel. Ohne
Schlüssel sagt die Quelle das und verweist auf den CSV-Export, der für
öffentliche Sheets weiter ohne Schlüssel geht. Die Console schlägt die Art
bei einer Sheet-Adresse ohne `format=`/`output=` vor. Pakete nehmen das
Sheet nicht mit (keine Datei). OAuth (private Sheets eines Kontos ohne
Freigabe) fehlt; ein Schlüssel liest öffentliche und per Link geteilte
Sheets. Verweist die Adresse auf einen Reiter (`#gid=`) und ist kein
Bereich angegeben, wird sein Titel über die Sheet-Eigenschaften
nachgeschlagen; ein „im Web veröffentlicht“-Link (`/d/e/…`) hat keine Id
und wird mit Hinweis abgelehnt.
**Geprüft**: Adresse aus URL/Id und Bereich, Antwort mit leeren Zeilen
und Zahlen (Test); gegen die echte API nicht (kein Schlüssel hier).

## 2026-10-12 — Kurzbefehle für alle Shows (AUT.3)

**Entscheidung**: Ein Kurzbefehl gehört der Show oder, mit „Alle Shows“
(`all_shows`), den Einstellungen von lmix-core: Datei `--shortcuts`
(`LMIX_SHORTCUTS`, Standard `~/.config/lmix/shortcuts.json`), gelesen beim
Start, nach jeder Änderung atomar geschrieben; eine kaputte Datei wird
als `.json.broken` beiseitegelegt statt überschrieben. Sie gelten in
jeder Show (auch nach „Neu“ und „Öffnen“) und stehen im Zustand nach den
Kurzbefehlen der Show (Nummern danach). **Die Show gewinnt**: belegt einer
der Show dieselbe Taste, läuft auf der Taste der der Show
(`key_shadowed`, in der Liste durchgestrichen); ebenso je MIDI-Note oder
-Regler und gegen den T-Bar der Show (`midi_shadowed`, auch keine Lampe).
Verdeckt wird je Belegung, nicht der ganze Kurzbefehl: Knopf, Nummer und
die andere Belegung gehen weiter. Innerhalb derselben Ebene bleiben
Tasten und Noten eindeutig. Ändern verschiebt einen Kurzbefehl zwischen
Show und Einstellungen (ans Ende, mit neuer Id: eine ältere Kopie der
Show kann die alte noch haben; trifft eine Id der Einstellungen doch eine
der Show, bekommt sie eine neue). Ein MIDI-Kanal „beliebig“ überdeckt
jeden Kanal: eine Note der Show auf Kanal 1 verdeckt dieselbe Note eines
Kurzbefehls für alle Shows auf beliebigem Kanal. „Global“ heißt in lmix
schon „Taste auch, wenn die Console nicht vorn ist“ (AUT.12), daher der
Name „Alle Shows“. Testskripte und die Demo geben eine eigene Datei im
Arbeitsordner an, damit sie nie die des Bedieners lesen oder schreiben.
Schlägt das Schreiben fehl, wird es bei der nächsten Änderung wieder
versucht; lässt sich eine kaputte Datei nicht beiseitelegen, wird sie
nie überschrieben (Änderungen bleiben dann bis zum Ende im Speicher).
Webseite „Tasten“, Ausführen und Entfernen in der Console rufen
Kurzbefehle über die Id auf, nicht über die Nummer (die verschiebt sich
mit den Kurzbefehlen der Show).
**Geprüft**: Switcher-Test (Vorrang, Noten, Verschieben, neue Show),
Service-Test (Datei geschrieben, neu gelesen, kaputte beiseitegelegt);
Console auf Xvfb: Liste mit „Alle Shows“ und durchgestrichener Taste, F2
führt den der Show aus, Häkchen im Bearbeiten-Dialog.

## 2026-10-12 — Zustand als Diff (API.6)

**Entscheidung**: `/api/events?diff=true` schickt nach dem ersten ganzen
Zustand jede Änderung als `{"event":"state_diff","ops":[…]}`: JSON Patch
(RFC 6902, nur `add`, `remove`, `replace`), Objekte je Schlüssel,
Listen je Eintrag (hinten angefügt oder entfernt), sonst ersetzt. Ist der
Patch länger als der ganze Zustand, kommt der ganze; nach einem
Rückstand (Lagged) auch. Ohne `diff` bleibt alles wie bisher (Console,
eigene Clients, TCP-Protokoll unverändert, nur additiv). Die Webseiten
nutzen es über `lmixEvents` in `web/lmix.js`, das die Diffs anwendet und
den Seiten ganze Zustände gibt: kopiert wird nur entlang geänderter
Pfade, ältere Zustände, die eine Seite hält (Titel-Seite), ändern sich
nicht. Passt ein Diff nicht, verbindet die Seite neu und beginnt mit dem
ganzen Zustand. Nutzen: Handy-Tally und Fernbedienung im WLAN bekommen
bei einem Statuswechsel einige hundert Byte statt des ganzen Zustands.
Die Console bekommt weiter den ganzen Zustand (lokal, ihr Modell hängt
an ganzen Zuständen); das ist ein eigener Schritt, wenn es sich messbar
lohnt.
**Geprüft**: Diff und Anwenden (Rust-Test, auch Escapes `~0`/`~1`);
`test-web.sh` zählt Diffs, `test-titles.sh` läuft mit Diffs.
**Review**: ein Fehler in einer Seite trennte die Verbindung (jetzt nur
eine Warnung); der ganze Zustand wurde für jeden Diff serialisiert, nur
um die Länge zu vergleichen (jetzt nur, wenn der Diff an die Länge des
letzten ganzen herankommt); `?diff=1` und `?diff` gingen nicht; ein
Schlüssel `__proto__` wäre in der Seite verloren gegangen. Kosten je
Client und Änderung bleiben ein `serde_json::Value` des Zustands und ein
Vergleich; den Wert einmal je Änderung statt je Client zu bauen, lohnt
erst bei vielen Diff-Clients.

## 2026-10-12 — Tally über HTTP (PER.1)

**Entscheidung**: `--tally-http <Adresse>` (mehrfach, `LMIX_TALLY_HTTP`)
ruft bei jeder Tally-Änderung und alle 30 s (ein neu gestartetes Gerät
holt auf) jede Adresse auf: ohne Platzhalter POST mit JSON
`{"tally":[…],"program":[…],"preview":[…]}` (Tally je Input wie die
TCP-API: 0 aus, 1 Programm samt Overlays und Layout-Boxen, 2 Vorschau;
dann die Nummern), mit Platzhaltern `{program}`, `{preview}`, `{tally}`
ein GET mit eingesetzten Werten, für einfache Geräte (ESP32-Tally,
Shelly, Hausautomation), die nur eine Adresse aufrufen können. Jedes
Gerät hat eine eigene Aufgabe mit 1 s Frist: ein langsames oder fehlendes
hält nur sich selbst auf; Änderungen während eines Aufrufs enden in einem
Aufruf mit dem letzten Stand; ein nicht erreichbares Gerät wird einmal
gemeldet, bis es wieder antwortet. In `LMIX_TALLY_HTTP` stehen mehrere
Adressen durch Leerzeichen getrennt (Kommas kommen in Adressen vor). GPIO und Firmata bleiben
offen (Hardware).
**Geprüft**: Test mit einem kleinen HTTP-Empfänger (erster Stand, Schnitt
auf Programm), Platzhalter und JSON; an einem echten Tally-Gerät nicht.

## 2026-10-12 — Auslöser auf die Wiedergabezeit (AUT.7)

**Entscheidung**: neben „Clip endet“ (mit Vorlauf = Restzeit) gibt es
`play_time` mit `at_ms`: feuert, wenn ein Eintrag die Zeit erreicht (aus
den Positionsmeldungen der Player, alle 200 ms), bei jedem Durchlauf und
für jeden Eintrag einer Playlist; ein Sprung zurück (Schleife, Neustart)
feuert nicht, der nächste Durchlauf wieder. Nur für Datei, URL und
Playlist, `at_ms` Pflicht und nur hier erlaubt. In der Console „Clip ist
gelaufen … ms“. Ein neuer Eintrag oder Durchlauf zählt ab 0: Zeiten
vor der ersten Meldung (alle 200 ms) feuern mit ihr; `at_ms` mindestens
1 (für den Anfang gibt es „geht auf Programm“). Nach dem Review: so
feuern auch Zeiten am Anfang eines Eintrags.
**Geprüft**: Tests für Durchlauf, Schleife, nächsten Eintrag und die
Prüfung der Felder; Dialog auf Xvfb (Wert bleibt erhalten).

## 2026-10-12 — Aktionsgruppen und Beispiel-Clients (API.1, API.11)

**Entscheidung**: Jede Aktion gehört einer Gruppe (`switching`, `input`,
`overlay`, `layout`, `audio`, `output`, `title`, `data`, `automation`,
`ptz`, `show`, `safety`), angelehnt an die Funktionsgruppen von vMix;
`GET /api/actions` markiert jede Variante mit `x-group` und listet die
Gruppen in `x-groups` (additiv, Schema sonst unverändert). Die Tabelle
steht in `lmix-types` (`ACTION_GROUPS`); ein Test verlangt, dass jede
Aktion in genau einer Gruppe steht, eine neue Aktion ohne Gruppe fällt
also auf.
`examples/`: `python/lmix_client.py` (Aktion, Tally lesen und folgen,
nur Standardbibliothek) und `tally-micropython/main.py` (Tally-Licht für
einen Input auf einem ESP32, ohne `machine` auf dem Rechner zum
Ausprobieren). Beide gegen lmix-core mit `--tcp` geprüft (Schnitt,
Vorschau, Fehler), der ESP32 selbst nicht. Ein Companion-Modul bleibt
offen (eigenes Repo, eigene Sprache).

## 2026-10-12 — Inputs aus einer anderen Show (SHW.3)

**Entscheidung**: `append_show { path, inputs }` nimmt Inputs einer
anderen Show-Datei (nach ihrer Nummer dort, leer = alle) mit ihren
Einstellungen (Quelle, Name, Farbe, Ton, Kanalzug, Sends, Look, Signal-
verlust, PTZ) ans Ende, mit neuen Ids und in freien Slots; Auslöser
bleiben zurück, ihre Aktionen nennen Inputs der anderen Show. Alles
oder nichts: reichen die Slots nicht, kommt keiner. Geht auch bei
Sperre (nichts wird geschlossen). `GET /api/shows/inputs?path=` listet
die Inputs einer Show-Datei (Nummer, Name, Art); die Console fragt so
nach dem Dateidialog „Aus Show…“, welche übernommen werden.
**Geprüft**: Service-Test (Auswahl, Einstellungen, neue Id, zu wenig
Slots, falsche Nummer); Console auf Xvfb (Datei gewählt, einen Input
abgewählt, nur der andere kam).
**Review**: angehängte Inputs bekamen keinen Tonpegel (jetzt wie bei
jeder Änderung aus dem Mix); gleiche Namen bekommen „ (2)“, weil Aktionen
Inputs auch über den Namen finden; Show-Dateien werden nur als normale
Datei bis 64 MB gelesen (die API sollte nicht `/dev/zero` lesen). Auf der
Tasten-Seite: `dynamic1`–`4` zeigen ihr Bild, Tasten auf „preview“ bauen
die Seite nicht mehr bei jedem Wechsel neu, ein Bild wird erst neu
geladen, wenn das vorige da ist. Das ESP32-Beispiel fragt alle 5 s nach
dem Tally und geht nach 12 s Stille dunkel (ein lmix ohne Strom ließ
das Licht sonst an).

## 2026-10-12 — Fehlende Mediendateien (SHW.4)

**Entscheidung**: lmix-core sieht alle 3 s nach, ob die Dateien der
Datei- und Playlist-Inputs da sind (ohne den Mischer zu sperren); fehlende
stehen je Input in `missing`. Die Console zeigt „Fehlt“ auf der Kachel
(Tooltip mit den Pfaden) und, solange etwas fehlt, „Fehlende Dateien
suchen…“: ein Ordner, in dem `relocate_media` nach den Dateinamen sucht
und gefundene einsetzt (Quelle ersetzt, wie „Quelle ersetzen“). Findet
es keine, meldet es das. Adressen (URL) prüft es nicht.
**Geprüft**: Service-Test (Datei und Playlist markiert, falscher Ordner
abgelehnt, richtiger eingesetzt, danach nichts mehr fehlend); Console
auf Xvfb (Markierung, Knopf).
**Review**: das Suchen im Ordner lief blockierend unter der Sperre des
Mischers (ein hängendes Netzlaufwerk hätte das Schalten angehalten; jetzt
vorher, asynchron, 2 s je Datei); ein Input auf Programm würde durch die
neue Quelle neu starten (jetzt bleibt er, die Meldung sagt es);
Adressen in Datei-Inputs galten als fehlend; nach dem Einsetzen stand
„Fehlt“ noch bis zur nächsten Prüfung, und eine laufende Prüfung konnte
alte Pfade zurückschreiben (jetzt nur Pfade, die der Input noch hat).
Zwei fehlende Dateien gleichen Namens in verschiedenen Ordnern finden
dieselbe Datei; nicht lesbare Dateien gelten als fehlend.

## 2026-10-12 — Zugriff: LAN, Passwort, offene Seiten (API.7)

**Kontext**: Die Webseiten und die API hatten keine Anmeldung; mit
`--listen 0.0.0.0` konnte jeder im Netz schalten.
**Entscheidung** (`access.rs`):
- **Dieser Rechner** (loopback) braucht nie ein Passwort: Console,
  Skripte, Companion auf derselben Maschine.
- **Nur LAN als Standard**: Anfragen von Adressen außerhalb privater,
  link-lokaler, unique-local Netze und des VPN-Bereichs 100.64.0.0/10
  (Tailscale) bekommen 403, auch über das Textprotokoll und OSC;
  `--allow-internet` hebt das auf (Reverse-Proxy, VPN).
- **Passwort** `--password` (`LMIX_PASSWORD`, in `--help` verdeckt): aus
  dem Netz HTTP Basic (Benutzername egal, der Browser fragt einmal; auch
  der WebSocket geht so) oder `Authorization: Bearer`; Textprotokoll
  erste Zeile `AUTH <passwort>` innerhalb von 10 s, sonst Fehler und
  Ende. Vergleich in konstanter Zeit. Ohne Passwort bleibt es im LAN wie bisher.
- **Offene Seiten** `--open-page tally,keys,mixer,titles`: die Seite,
  ihre Dateien und was Seiten lesen (Zustand, Ereignisse, Tally, Bilder)
  ohne Passwort, und nur die Aktionen, die die Seite sendet, einzeln
  aufgezählt: `tally` keine, `keys` Kurzbefehle mit Häkchen „Webseite“
  (auf ihre Id festgelegt), `mixer` Vorschau, Schnitt, Übergang, FTB,
  Layout-Vorschau, `titles` Abspielen, Weiter, Stopp, Felder und
  Datensätze (nie Dateien, Datenquellen, Adressen). Eine offene Seite
  macht damit den Zustand lesbar (Namen der Inputs), nicht mehr.
- **Browser**: eine Anfrage mit einem `Origin`, der nicht die Adresse
  ist, an die sie geht, wird abgewiesen (eine fremde Webseite kann nicht
  über den Browser des Bedieners schalten); auf diesem Rechner zählt nur
  `localhost` oder eine IP-Adresse als Adresse (DNS-Rebinding).
- **Reverse-Proxy**: trägt eine Anfrage `Forwarded`, `X-Forwarded-For`
  oder `X-Real-IP`, gilt sie als aus dem Netz, auch wenn der Proxy auf
  diesem Rechner läuft (sonst wäre hinter ihm jeder „lokal“). Der Proxy
  muss diese Kopfzeilen setzen (nginx, Caddy tun es üblicherweise).
- **OSC** hat kein Passwort (UDP, Controller): es gilt die LAN-Regel, und
  mit Passwort nimmt OSC nur noch Nachrichten von diesem Rechner an.
- „Erhöhte Sicherheit sperrt Skripte“: lmix hat keine Skripte.
**Geprüft**: Tests für Adressen, Passwörter, offene Seiten, den Wächter
mit simulierter Adresse und `AUTH` im Textprotokoll; echt über die
LAN-Adresse: ohne Passwort 401, mit 200, offene Tally-Seite ohne,
Mixer-Seite im Browser mit Basic-Anmeldung samt WebSocket.
**Review** (Sicherheit): die offene Titel-Seite ließ alle Titel- und
Datenaktionen zu, darunter Datensätze in beliebige Dateien schreiben und
Dateien als Datenquelle lesen (jetzt nur die Aktionen der Seite, einzeln);
OSC umging das Passwort; ein Proxy auf demselben Rechner machte jeden
lokal; fremde Seiten im Browser des Bedieners hätten über loopback
schalten können; `AUTH` ohne Frist. Alle behoben, mit Tests.

## 2026-10-12 — Springen im Clip, Playlist-Eintrag (UI.5, UI.4)

**Entscheidung**: strom hat am Player `seek` und `goto`; lmix gibt sie
als Aktionen `seek_input {input, position_ms}` (Datei, URL, Playlist:
im laufenden Eintrag) und `play_item {input, item}` (Playlist, 1-basiert,
danach Play) weiter. Die Transportleiste springt beim Klick in den
Fortschrittsbalken; das Kachelmenü einer Playlist hat „Eintrag
abspielen“ mit den Dateinamen. (Die Notiz „strom hat dafür keine
Aktion“ in „M9 Anfang“ war falsch: lmix hatte keine.)
**Geprüft**: Switcher-Test (Befehle, Bereich, nur Playlist); Menü auf
Xvfb. Gegen ein laufendes strom nicht geprüft (Pfad und Felder aus strom-types `SeekRequest`, `GotoRequest`).
**Review**: ein Sprung zurück (oder zu einem früheren Eintrag, Neustart,
Zurück) sah für die Clip-Überwachung wie ein Ende aus und löste
„Clip endet“ und Wiedergabezeit-Auslöser aus. Jetzt gelten die
Positionsmeldungen eines Players bis 600 ms nach so einem Befehl als
neuer Anfang (Meldungen kommen alle 200 ms und können den alten Stand
noch zeigen); Test dazu.

## 2026-10-12 — Ohne NVENC fallen alle Programm-Ausgänge aus

**Befund** (Start des 8-h-Soaks): Aufnahme und beide SRT-Ausgänge sofort
`failed`, alle 7 s neu gestartet, nur die ISO-Aufnahme lief. strom:
`NVENC not available`, `Using software fallback encoder: x264enc`, dann
`not-negotiated` am `pgm`-intersrc (`enc:videoconvert` kann
`memory:CUDAMemory` nicht wandeln). Ursache 1, am Rechner: Das
Treiber-Update 615.71 → 615.78 lief um 08:09 mit dem alten Kernelmodul;
GStreamer speicherte dabei eine Registry ohne NVENC-Encoder und prüfte sie
nach dem Neustart nicht neu (Abhilfe in `BUILD.md`). Ursache 2, in strom:
Im Software-Modus setzt der Encoder-Block ein `videoconvert` vor den
Encoder; der CUDA-Mischer liefert das Programm aber im GPU-Speicher (die
Ausgänge holen es bewusst nicht herunter, damit NVENC es direkt nimmt).
**Fix** (strom-Branch `lmix-encoder-cuda-download`, auf `dev`): Im
Software-Modus ist der Wandler ein Bin `cudadownload ! videoconvert`, wenn
`cudadownload` existiert (Systemspeicher läuft unverändert durch).
Nachgestellt mit einer leeren `libnvidia-encode.so.1` vor der echten
(CUDA da, NVENC nicht): vorher Aufnahme und SRT `failed`, danach laufen
Aufnahme, ISO und SRT mit x264 (480 Bilder in 16 s). Test
`test_software_convert_accepts_cuda_memory` (ignoriert ohne CUDA).
Betrifft jeden Rechner mit CUDA-Mischer, aber ohne NVENC in der Registry
(Treiber ohne `libnvidia-encode`, Rechenkarten ohne NVENC wie A100/H100).

## 2026-10-12 — CUDA-Latenz 106 ms: leere DSK-Ebenen im Mischer

**Bisect** (RTX 3090, CUDA-Mischer, 4 Testbild-Inputs, Aufnahme läuft,
viermal `POST /api/latency/measure?seconds=5`; jede lmix-core-Version mit
ihrem strom-Pin, parallel zum 8-h-Soak):

| Stand | strom | Median |
|---|---|---|
| `70458ac` (Soak 2026-10-09) | `ed647f2` | 56 ms |
| `e49e4cd`, `f370d79` | `ed647f2`, `1dbe229` | 56 ms |
| `f370d79` (lmix-core alt) | `401053c` (strom neu) | 56 ms |
| `dde8237` Overlays und T-Bar | `d47a50a` | 107 ms |
| `f851798`, `60fb92d`, `619760d`, `ad3657f`, `6ce0d2b` | … `9274bbd` | 86–107 ms |

**Ursache**: Seit `dde8237` hängen am Bildmischer vier DSK-Ebenen für
die Overlays (später fünf mit dem Stinger), gespeist über `inter` von
einem Kanal, den niemand füttert. Ein Live-Aggregator wartet bei jedem
Bild bis zu seiner Frist, solange ein Pad keine Daten hat; das Programm
kommt dadurch um das ganze Latenzbudget (`latency` + `min-upstream-latency`
je 20 ms, plus Vorlauf) später. Die Werte springen zwischen 86 und 107 ms
je nach Lage der Frist zum Bildtakt.
**Fix** (strom-Branch `lmix-ignore-inactive-pads`, auf `dev`):
`ignore-inactive-pads=true` an den Compositoren des Bildmischers. Pads ohne
je ein Bild zählen nicht mehr, ab ihrem ersten Bild wieder. Gemessen mit
dem heutigen lmix-core: **56 ms** statt 87–107 ms, auch mit Overlay an und
wieder aus (der Kanal zeigt danach weiter auf den Slot, das Pad bekommt
Bilder).
**Rest**: Nach dem ersten Stinger 87 ms. Ursache ist nicht die
Stinger-Ebene, sondern der Input des Clips selbst: Schon das Hinzufügen
des Stinger-Clips (ohne Übergang) ergibt 86 ms. Ein pausierter Player
liefert keine Bilder mehr, sein Pad am Mischer hatte aber schon eins und
gilt als aktiv. Gelöst am 2026-10-13 (Eintrag „strom-Fixes“).

## 2026-10-12 — NFA.7: 8-h-Soak und strom-Speicher (H.24)

**Lauf** (`MINUTES=480 INPUTS=16 STREAMS=2 ISO=1 scripts/soak-hardware.sh`,
RTX 3090, CUDA-Mischer, lmix `511460a`, strom `9274bbd`, 08:42–16:42):
kein Ausfall. Aufnahme 864 919 Bilder in 28 828 s (30,0 fps), ISO 864 617,
beide SRT-Empfänger je 864 809, 0 verlorene und 0 verworfene Pakete.
Latenz durchgehend 106–107 ms (p95 einmal 111 ms; Ursache der 107 ms:
Eintrag „CUDA-Latenz 106 ms“). Zwei FAILs im Bericht:
- **strom-Speicher**: 1027 MB nach 10 min, 1387 MB am Ende. In den ersten
  2 h liefen parallel Builds und die Läufe unten; ab Stunde 3 ruhig und
  gleichmäßig: 1232 → 1387 MB in 5 h, **~31 MB/h** (wie am 2026-10-09).
- **CRITICALs**: 28 Meldungen `gst_value_collect_int_range` beim Start
  (ein Zeitpunkt, danach keine). Sie kommen aus dem PipeWire-Geräteanbieter
  von GStreamer (`libgstpipewire`, Backtrace über `gst_caps_set_simple`),
  den strom für die Geräteerkennung startet; `gst-device-monitor-1.0`
  zeigt dieselben 28 ohne strom. Umgebung des Rechners, nicht lmix.

**Eingrenzung des Wachstums** (CPU-Mischer, 1 Testbild-Input, nur eine
ISO-Aufnahme als MKV, RSS von strom jede Minute):

| Lauf | Wandler vor dem Encoder | Verlauf |
|---|---|---|
| ISO + NVENC (2 ×, 60 und 90 min) | `autovideoconvert` | stetig ~21–36 MB/h |
| wie oben, `MALLOC_ARENA_MAX=2` | `autovideoconvert` | stetig ~28 MB/h |
| ISO + NVENC, `STROM_FORCE_SOFTWARE_CONVERT=1` (40 min) | `videoconvert` | flach (544 MB von Minute 10 bis 30, dann 486) |
| ISO + x264 (90 min) | `videoconvert` | flach |
| ohne ISO (90 min) | – | flach |

Reines GStreamer (je 60 min, ohne strom) wächst mit keiner Variante:
`nvh264enc` direkt aus Systemspeicher, mit `cudaupload`, mit
`autovideoconvert` und mit den Encoder-Einstellungen von strom.
**Befund**: Das Wachstum hängt am Weg Systemspeicher → `autovideoconvert`
→ NVENC in strom. `autovideoconvert` lädt dort jedes Bild über OpenGL
hoch (`glupload ! glcolorconvert`, RGBA in GL-Speicher), NVENC nimmt es
per CUDA-GL-Interop. Die Programm-Ausgänge kommen beim CUDA-Mischer schon
im GPU-Speicher und wachsen nicht. In strom selbst nicht weiter verfolgt
(was in der GL-Kette leckt, ist offen; ohne strom nicht nachstellbar).
**Vorschlag** (strom, ungeprüft): Für NVENC-Encoder mit Systemspeicher
`videoconvert` (oder `cudaupload ! cudaconvert`) statt `autovideoconvert`;
den GL-Umweg braucht NVENC nicht. Danach ISO-Lauf ≥ 3 h, dann 48 h (H.24).

## 2026-10-13 — strom-Fixes: Encoder, pausierter Player, Speicher

**Stand** (Submodul `strom` auf `8e8da6f`, Branch `lmix-pin-1013`: Pin
vom 2026-10-12 plus strom-PRs #9–#12):
- **#9** Encoder im Software-Modus nimmt GPU-Speicher
  (`cudadownload ! videoconvert`), Programm-Ausgänge laufen ohne NVENC.
- **#10** `ignore-inactive-pads` am Bildmischer: 107 → 56 ms.
- **#11** Vor NVENC `cudaupload ! cudaconvert` statt `autovideoconvert`
  (kein Umweg über OpenGL). Erster Versuch mit `videoconvert`: Aufnahme
  und SRT `not-negotiated`, NVENC nimmt I420 nicht aus CUDA-Speicher, und
  `videoconvert` kann CUDA-Speicher nicht wandeln.
- **#12** Pausierter Player wiederholt sein letztes Bild in der Bildrate
  (auch am Ende der Playlist), mit der Laufzeit des Slot-Flows. Ein
  pausierter Clip kostet das Programm keine 30 ms mehr.

**Stinger** (Entscheidung „b“ vom 2026-10-12, verworfen): Die Stinger-Ebene
beim Ausschalten zu spülen, wirkt im Test (Pad zählt danach wieder als
inaktiv), lässt die Latenz aber bei 86 ms: Wartet hat das Input-Pad des
pausierten Clips. Das Standbild (#12) ist Weg „c“ in strom statt in lmix
und deckt jeden pausierten Clip ab, nicht nur Stinger.
**Gemessen** (CUDA, 4 Inputs, Aufnahme): 56 ms vor dem Stinger, mit
hinzugefügtem Clip, nach dem ersten und zweiten Stinger, Overlay an und
aus.
**Soak** (3 h, CUDA, 16 Inputs, Aufnahme + ISO + 2 × SRT, mit #9–#11):
strom-RSS 1091 → 1101 MB von Minute 90 bis 180 (vorher ~31 MB/h), Latenz
56 ms (einmal p95 85 ms), alle Bilder angekommen, 0 Pakete verloren. In
der Zeit lief ComfyUI auf der GPU (bis 16 GB, zeitweise 100 %).
**`soak-hardware.sh`** zählt die PipeWire-CRITICALs vom Start (Eintrag
„NFA.7: 8-h-Soak“) nicht mehr als Fehler.
**Ton**: Ein pausierter Clip mit Ton ändert das Programm-Audio nicht
(Pufferalter an `pgm_audio_out` 80 ms vorher, beim Abspielen und
pausiert, mit altem und neuem strom; CPU-Mischer). Das Bild lag im selben
Lauf mit altem strom bei 112 ms, mit den Fixes bei 56 ms.
**8-h-Lauf** (2026-10-13, 01:57–09:59, `d4a88e7`, ohne Nebenlast; Rest
wie oben): Latenz durchgehend 56 ms, alle Bilder in Aufnahme, ISO und
beiden SRT-Empfängern (864 557 bzw. 864 475/864 461 von 864 557), keine
CRITICALs außer PipeWire. strom-RSS ab Stunde 1 von 847 auf 903 MB,
~7,7 MB/h statt ~31, in Stufen (größte +29 MB nach 6,3 h), nicht
gleichmäßig.
**Offen**: 48-h-Lauf (H.24).

## 2026-10-13 — Nachtläufe auf Hardware: GL, Stinger mit Alpha, H-Punkte

**Stand** (Submodul `strom` auf `d4a88e7`, Branch `lmix-pin-1013`, dazu
strom #13; seit dem Rahmen-Fix `b8ea416` mit #14). Läufe auf der RTX 3090, je eigener strom und lmix-core.
- **GL-Latenz** (H.22): GL lag bei 88 oder 108 ms (je nach Start), CUDA bei
  56. `glvideomixerelement` hat keine Eigenschaft `ignore-inactive-pads`,
  #10 griff dort nicht. Jetzt über `gst_aggregator_set_ignore_inactive_pads()`
  an jedem Mischer: GL 56–64 ms, auch nach Stingern und mit Overlay.
  Messweg: Pufferalter je Element im Mischer (Eingänge 0–1 ms, nach dem
  Compositor 88 ms).
- **GL und SRT** (Befund vom 2026-10-09): mit #11 (`cudaupload ! cudaconvert`
  statt `autovideoconvert` vor NVENC) trägt der SRT-Ausgang am GL-Mischer
  Video, 107 996 Bilder in 60 min (30,0 fps); mit altem strom 0 Bilder.
  Speicher in der Stunde 725 → 779 MB (Minute 15 bis 55), parallel zu
  Builds gemessen. Ruhig nachgemessen (90 min, 1 Input, `b8ea416`):
  672 → 813 MB ab Minute 10, ~110 MB/h, 162 053 Bilder (30 fps). Der
  GL-Weg mit SRT wächst also weiter; CUDA im 8-h-Lauf nicht. Offen.
- **Stinger mit Alpha** (H.27): ProRes 4444 (`A444_10LE`) und VP9 mit Alpha
  (`A420`) liefen auf CUDA nicht: `cudaupload` hat diese Formate nicht,
  Slot-Eingang und DSK-Ebene standen mit not-negotiated, kein Stinger zu
  sehen. strom #13: `videoconvert ! cudaupload` an Eingängen und
  DSK-Ebenen des CUDA-Mischers (Durchreichen für alles andere, auch
  CUDA-Speicher). Danach auf CUDA wie auf GL: Balken über dem Programm,
  Umgebung sichtbar, Schnitt in der Mitte.
- **Quelle tauschen** (H.3): Testbild → Testbild bildgenau ohne Schwarz;
  Testbild → Datei hält 1–3 Bilder (Decoderstart), kein Schwarz.
- **Warmlauf** (H.9): erster Schnitt nach 2,4 s (CUDA) bzw. 3,1 s (GL).
- **Signalverlust** (H.17, H.21 teilweise): SRT-Sender 6 s weg: „halten“
  ohne Schwarzbild, „schwarz“ nach ~0,4 s, Rückkehr nach ~0,2 s; CUDA und
  GL gleich.
- **Layouts** (H.29): Plätze, Inhalt und Latenz stimmen. Auf GL fehlten
  die weißen Rahmen, auch mit altem strom: Die Unterlage lag richtig und
  bekam Puffer, `glupload` machte daraus aber leere Texturen. Ursache ist
  der udmabuf-Weg von `glupload` (GStreamer 1.28) auf NVIDIA 615.78, der
  für RGBA, BGRA und I420 leere Texturen liefert (NV12 geht); nachgestellt
  mit `gst-launch` und mit ausgeblendetem `/dev/udmabuf`. strom umgeht das
  schon hinter den Eingangs-Queues (`gpu::udmabuf_upload_broken`), die
  Unterlage hängt aber ohne Queue an `glupload`. strom #14 setzt dieselbe
  Sonde an die Unterlage: Rahmen da, Latenz 56 ms. Mit GUI gebauter strom
  (ohne `no-gui`) beendet sich nach diesem Test auf SIGTERM nicht (hängt
  beim Schließen der GUI); lmix baut mit `no-gui`, dort kein Hänger.

**Messfehler unterwegs**: Clip-Pfade relativ zum Skript (`./clip.mp4`)
löst strom relativ zu seinem Arbeitsverzeichnis auf (leere Box); ein mit
`drawbox` auf transparentem Grund erzeugter Alpha-Clip war ganz
durchsichtig. Beides lag am Test, nicht an lmix.
**Soak**: Der 8-h-Lauf ab 00:30 wurde nach 65 min abgebrochen: parallele
Builds ließen strom von 900 auf 1082 MB springen (Warteschlangen unter
CPU-Last), der Speicherwert wäre wertlos gewesen. Neuer Lauf ohne
Nebenlast (Ergebnis im Eintrag „strom-Fixes“).

## 2026-10-16 — GL-Mischer: Speicherwachstum ohne SRT

**Gegenprobe** (`MIXER=gl INPUTS=1 STREAMS=0 MINUTES=60
scripts/soak-hardware.sh`, strom `1ec76e2`, RTX 3090, Treiber 615.78,
GStreamer 1.28.8; nur die Aufnahme, kein SRT): strom-RSS 753 MB nach
10 min, 865 MB nach 60 min. Gleichmäßig ~50 MB/h, dazu ein Sprung von
+42 MB um Minute 43, als eine fremde GPU-Last (100 %, 22 GB) endete.
Latenz durchgehend 56 ms, Aufnahme lückenlos (109 014 Bilder in 3632 s).
Mit Aufnahme und SRT waren es am 2026-10-13 ~110 MB/h: das Wachstum
hängt an jedem Encoder des GL-Mischers, nicht am SRT-Zweig.
**Isoliert** (je 12 min, `gst-launch-1.0`, 1080p30 live, NVENC):
`glupload ! glcolorconvert ! GLMemory RGBA ! cudaupload ! cudaconvert !
nvh264enc`, dasselbe mit `gldownload` davor und `cudaupload` aus dem
Systemspeicher: alle drei flach (304 → 249 bzw. 284 → 224 MB). Der Weg
GL → CUDA → NVENC leckt für sich nicht.
**Aber**: Beim GL-Mischer verlässt Program den Mischer schon im
Systemspeicher (`gl_download` am Bildmischer, `flows.rs`); der Ausgangs-Flow
sieht also genau den dritten, flachen Weg. Damit steht der Mischer selbst
unter Verdacht, nicht der Encoder.
**Ohne Ausgänge** (`RECORD=0 STREAMS=0`, neu in `soak-hardware.sh`,
45 min): 517 MB nach 10 min, 635 MB nach 25 min, danach fast flach (653 MB
nach 45 min). Der Anstieg fiel in die Zeit, in der nebenan gebaut und ein
VM-Test lief (bekannter Effekt, Eintrag „Nachtläufe“), ab Minute 35 dazu
eine fremde GPU-Last (100 %, 21 GB). Der Lauf trennt Leck und Last nicht.
**Ohne Ausgänge, 2 h** (ohne Builds nebenan; die GPU lief die ganze
Zeit fremd voll, 100 %, 21–23 GB): 577 MB nach 10 min, 603 MB nach 45 min,
ein Sprung auf 713 MB in einer Minute (Minute 48), danach 813 MB nach
120 min; ohne den Sprung ~70 MB/h. Latenz durchgehend 56 ms.
**Befund**: Der GL-Mischer wächst auch ganz ohne Encoder. Die Ursache
liegt im Mischer-Flow (GL-Bildmischer, `gl_download` von Program, Live-
und Kachelbilder), nicht im Ausgang. Unter voller GPU-Last kommen Sprünge
dazu (Warteschlangen).
**Nächster Schritt**: in strom den Mischer-Flow zerlegen (ohne
`gl_download`, ohne Kachel-Taps) und je 1 h messen, mit freier GPU.
**Stand**: CUDA bleibt der Standard-Mischer (~7,7 MB/h im 8-h-Lauf); GL
erst nach der Ursache für lange Läufe empfehlen.

## 2026-10-16 — Show-Modus als zweiter Bedienmodus

**Kontext**: lmix soll neben dem Bildmischer (vMix-Vorbild) auch Abläufe
nach Drehbuch fahren können (QLab-Vorbild): Theater, Ausstellungen,
Spieltisch. Technisch teilen beide Engine, Inputs, Ausgänge, Ton,
Aktionskatalog, Auslöser, APIs und Show-Datei; verschieden ist das
Bedienkonzept (im Moment entscheiden vs. vorbereitete Cue-Folge mit GO).

**Optionen**: (a) eigenes Programm, z. B. auf Linux Show Player (GPL,
Python) aufbauend; (b) Plugin neben lmix; (c) zweiter Modus über
demselben `lmix-core`.

**Entscheidung** (User): (c). Eigenes Lastenheft `LASTENHEFT-SHOW.md`
mit Quellenzuordnung `LASTENHEFT-SHOW-QUELLEN.md` (Clean Room gegen das
QLab-5-Handbuch), Meilensteine S1–S3, Personas P8–P10. Licht in S1 nur
über Home Assistant, eigene Lichtsteuerung ab S3 (User). (a) scheidet
auch an der Lizenz (GPL, `claude.md` 6).

**Offen**: S-E.1–12 in `LASTENHEFT-SHOW.md` §6, vor Beginn von S1 zu
entscheiden. `claude.md` nennt beide Modi (Entscheidungen 7, 8).

## 2026-10-16 — Show-Modus: S-E.1–12

**Kontext**: Offene Fragen aus `LASTENHEFT-SHOW.md` §6, vor S1 zu
entscheiden. Vorschläge mit Alternativen auf einer Entscheidungsseite.

**Entscheidung** (User): Vorschlag angenommen für S-E.1, 2, 4, 6–11.
Abweichend: S-E.5 Cue ohne Rückwärtsverträglichkeit umbauen und Shows
migrieren (vor dem ersten Release erlaubt); S-E.12 ein gemeinsamer
Plugin-Host statt eines Prozesses je Plugin, bei Bedarf einer je Zone.
Zurückgestellt: S-E.3, Folgen noch unklar; vor S1 Stufe 2 mit Messungen
neu vorlegen.

**Neu aus den Anmerkungen**: SUI.13 (Felder je Modus, „Erweitert“),
SSH.5 (Show samt Rechner-Einstellungen ex-/importieren, ohne
Geheimnisse), VIS.17 (Bühne als Input im Mischer für die gemischte
Theatershow, S2-Abnahme mit P7). CUE.21 und SSH.1: migrieren statt
alte Formate weiterlesen.

## 2026-10-16 — Clips auf Sendung, alle Overlays aus (INP.11, OVL.8)

**Entscheidung INP.11**: Datei-, URL- und Playlist-Inputs haben drei
Schalter `auto_play {play, restart, pause}` (Aktion `set_auto_play`, im
Zustand und in der Show-Datei, Standard alle aus; für andere Quellen
abgelehnt, außer alles aus). „Auf Sendung“ ist, was Tally Programm
nennt: Programm, ein Layout auf Programm (Hintergrund und Boxen), ein
Overlay-Kanal; dazu die Vorschau, solange der T-Bar sie einblendet. Der
Switcher vergleicht das vor und nach jeder Aktion, damit zählen Cut,
Cut direkt, Übergang, T-Bar, Overlays, geänderte Layouts, Kurzbefehle
und Auslöser gleich, ohne Sonderfälle je Aktion.
- Geht ein Input auf Sendung: `play` → Play, `restart` → Neustart des
  Eintrags (mit `play`) oder Sprung auf 0 (ohne, bleibt pausiert). Die
  Befehle stehen vor dem Umschalten: ein neu gestarteter Clip zeigt kein
  altes Bild, und er läuft schon, während er eingeblendet wird.
- Geht er von Sendung: `pause` erst nach dem Übergang (Dauer des
  Übergangs oder der Overlay-Blende als `Wait`), sonst stünde ein
  Standbild in der Blende. Beim Stinger beides am Schnittpunkt (der
  Clip deckt ab). Kommt der Input während der Blende zurück (Cut
  zurück), entfällt die Pause (`still_due` beim Ausführen der späten
  Befehle). Steht er noch anderswo auf Sendung (Programm verlassen, im
  Overlay geblieben), läuft er weiter.
- FTB schaltet nichts um und berührt die Player nicht. Der Stinger-Clip
  selbst bleibt beim Stinger. Vorschau allein zählt nicht (der Input
  kann nicht zugleich in Vorschau und Programm stehen, ein Layout in der
  Vorschau mit dem Programm-Input ändert nichts).
- Eine Show öffnen oder die Engine neu aufbauen startet keine Player
  (kein Wechsel, der Zustand wird nur wiederhergestellt).

**Entscheidung OVL.8**: `overlay_out_all {duration_ms}` nimmt jeden
belegten Kanal heraus wie `overlay_out`. `overlay_last {channel}` zeigt
den Input wieder, den der Kanal zuletzt zeigte (gemerkt bei
`overlay_in`, im Zustand `overlay_last`); zeigt der Kanal etwas, tut es
nichts, ohne Merkwert Fehler `no_overlay_last`. Nur zur Laufzeit wie
die Cue-Stelle: die Overlays selbst stehen in der Show-Datei, „zuletzt“
gehört zur Sendung, nicht zur Show. Ein geschlossener Input fällt
heraus, bis er wiederhergestellt ist. OSC: `/lmix/overlay/out`,
`/lmix/overlay/{n}/last`.
**Console**: Reiter Allgemein eines Clips „Auf Sendung: Abspielen, Von
Anfang, Danach pausieren“; „Alle Overlays aus“ unter den Overlay-Tasten,
Rechtsklick auf einen leeren Kanal „… wieder zeigen“; Vorlage „Alle
Overlays aus“ für Kurzbefehle, Auslöser und Cues.
**Geprüft**: Switcher-Tests (Take mit Blende, Cut, Neustart mit und ohne
Abspielen, Overlay und Layout auf Sendung, T-Bar, Stinger, späte Pause,
alle aus, letzter Input), Service-Test mit Null-Engine (Pause nach der
Blende, keine nach Cut zurück), `test-headless.sh` (OSC), Console auf
Xvfb (Schalter setzen den Zustand, Menü und Knopf). Mit echten Playern
in strom nicht geprüft. VM-Check (GitHub): Overlay wieder an, während
es noch ausblendet; fand einen strom-Fehler (das Ende der Blende nahm
den DSK wieder weg), behoben in strom 04b87d1.
**Offen**: Ein Cut, während der T-Bar halb steht, lässt `t_bar` stehen
(war schon so); die neue Vorschau gilt dann als eingeblendet und
pausiert nicht.

## 2026-10-16 — Schnellstart, Klick und GO je Input (SWI.8, INP.8, INP.9)

**Entscheidung SWI.8**: Der Schnellstart hat einen eigenen Effekt in der
Show (`quick_play {effect, duration_ms}`, Aktion `set_quick_play`,
Standard Blende 500 ms, im Zustand und in der Show-Datei), neben den
vier Übergangstasten und nicht als fünfte: er ist ein Weg auf Programm,
keine Taste für den T-Bar. Kein Stinger (der bräuchte den Clip-Ablauf
der Tasten; Fehler `bad_quick_play`). `quick_play {input}`:
- Mit Input wie `cut_direct`, nur mit Effekt: die Vorschau bleibt, außer
  der Input stand dort (dann Tausch wie beim Take). Ohne Input die
  Vorschau (ein Layout dort wird nur übernommen).
- Ein Clip (Datei, URL, Playlist) bekommt Play vor dem Umschalten, wie
  bei der Automatik (INP.11); hat er dort „Abspielen“, startet ihn die
  Automatik (kein doppeltes Play). Steht er schon auf Programm, spielt
  er nur. Kein Neustart: wer von vorn will, setzt „Von Anfang“.
- Ton folgt über die Dauer des Effekts; die Automatik pausiert den alten
  Clip nach der Blende wie beim Übergang.
- „Per Klick ins Preview-Fenster“: Doppelklick auf den Vorschau-Monitor
  (ein einfacher Klick löst zu leicht aus). Dazu „Schnellstart“ unter den
  Übergangstasten, Rechtsklick für Effekt und Dauer.

**Entscheidung INP.8, INP.9**: Je Input zwei Felder `click` und `go` mit
je einem von vier Werten, benannt wie die Aktion, die sie ausführen:
`preview_input` (Standard Klick), `cut_direct`, `quick_play` (Standard
GO), `none`. Keine freie Aktionsliste je Input, dafür gibt es
Kurzbefehle; vier Werte bleiben in Menü und Show-Datei einfach
(Standardwerte werden nicht geschrieben). Gesetzt mit
`set_input_press {input, click?, go?}`, ausgeführt mit
`click_input {input}` und `go_input {input}`: der Switcher macht daraus
zuerst die genannte Aktion, Ton, Automatik und Fehler gelten wie dort.
So gehen sie in Kurzbefehlen, Auslösern, Cues, über OSC (`/lmix/go`,
`/lmix/quickplay`) und TCP.
- GO ist der Doppelklick auf Kachel und eingeklappten Input und „GO: …“
  im Kachelmenü, im Web-Mischpult ebenso. Bisher schnitt der Doppelklick
  direkt auf Programm; jetzt ist es standardmäßig der Schnellstart (½ s
  Blende, Clip spielt). Wer den harten Schnitt will, stellt GO auf
  „Direkt auf Programm“. Eine eigene GO-Taste im Übergangsfeld gibt es
  nicht: für die Vorschau ist das der Schnellstart-Knopf.
- Ein Doppelklick löst vorher den Klick aus (Qt und Browser melden
  beide); mit den Standardwerten geht der Input erst in die Vorschau,
  dann nimmt der Schnellstart die Vorschau und der alte Programm-Input
  kommt in die Vorschau, wie bisher beim Doppelklick.
- Bricht (noch kein Release): der Doppelklick schneidet nicht mehr hart;
  alte Show-Dateien laden mit den Standardwerten.

**Nachtrag OVL.8** (offen aus PR #31): MIDI-Lampen (AUT.6) von
`overlay_out` und `overlay_out_all` leuchten in Programmfarbe, solange
der Kanal (irgendein Kanal) etwas zeigt; `overlay_last` in
Programmfarbe, solange der Kanal etwas zeigt, in Vorschaufarbe, solange
er seinen letzten Input wieder zeigen kann. `quick_play`, `click_input`
und `go_input` leuchten wie `cut_direct` nach ihrem Input. Die Seite
`/keys` zeigt dasselbe für Tasten ohne Input als Balken in Tally-Farbe
und Wort („on“, „ready“), dabei auch FTB, Sperre und Ausgänge, die
bisher nur die Lampen kannten.
**Geprüft**: Switcher-Tests (Effekt und Vorschau, nur Abspielen auf
Programm, kein Stinger, Automatik ohne doppeltes Play, Klick und GO),
Lampen-Test, OSC-Zuordnung, `test-headless.sh` (OSC), `test-web.sh`
(Doppelklick im Mischpult, Taste „Overlays aus“), Console auf Xvfb mit
Null-Engine (Knopf und Menü, Doppelklick auf Vorschau und Kachel,
Einstellungen, keine QML-Warnungen), `test-m2b-vm.sh` (pausierter Clip
per Schnellstart auf Programm, spielt).
**Offen**: Klick und GO kennen keine Overlay-Kanäle (vMix bietet mehr
Ziele); ein Schnellstart, während der T-Bar halb steht, lässt ihn
stehen wie Cut (siehe INP.11).

## 2026-10-16 — Overlay-Kanäle, Overlay mit dem Take, Layout-Vorlagen (OVL.2, OVL.3, LAY.5)

**Entscheidung OVL.2**: Jeder Overlay-Kanal hat Einstellungen in der Show
(`overlay_channels`, Aktion `set_overlay {channel, effect, duration_ms,
auto_off_ms?, rect?}`, im Zustand und in der Show-Datei; Standardwerte
werden nicht geschrieben). Gesetzt wird alles auf einmal wie beim
Schnellstart.
- **Effekt**: nur `cut` und `fade`. strom blendet DSK-Ebenen nur über
  Alpha (`lmix-dsk-fade`); Schieben, Wischen usw. bräuchten Keyframes auf
  der Position des DSK-Pads in strom. Ausblenden ist dieselbe Blende
  rückwärts. Andere Effekte: Fehler `bad_overlay_settings`.
- **Dauer in den Aktionen**: `overlay_in`, `overlay_out`,
  `overlay_out_all` und `overlay_last` haben `duration_ms` jetzt als
  Option. Ohne sie gilt der Kanal (Schnitt: 0), mit ihr eine Blende
  dieser Länge (0 schneidet), wie bisher. So bleiben Kurzbefehle mit
  eigener Dauer gleich, und ohne Dauer folgen sie dem Kanal. Bricht
  (noch kein Release): ohne Dauer war es fest 500 ms, jetzt der Kanal
  (Standard ebenfalls 500 ms Blende). Bei „alle aus“ wartet die
  Automatik (INP.11) auf die längste Blende.
- **Auto-Aus** nach `auto_off_ms` (1 ms–1 h): der Service nimmt den Kanal
  nach der Zeit selbst heraus (`overlay_out` ohne Dauer, also mit dem
  Kanaleffekt). Jedes Einblenden zählt einen Zähler hoch, der Kanal merkt
  sich seinen Stand; ist der Kanal bis dahin aus- und wieder eingeblendet
  (auch mit demselben Input), entfällt das alte Auto-Aus und das neue
  läuft (`overlay_still_on`, geprüft unter der Sperre des Switchers wie
  `still_due`). Nur zur Laufzeit: Show öffnen oder Neustart stellen
  Overlays ohne Auto-Aus wieder her (wie die Automatik keine Player
  startet). Auto-Aus löst die `overlay_out`-Auslöser des Inputs aus wie
  ein Druck des Bedieners (Auslöser-Aktionen selbst weiterhin nicht).
- **Typ**: `rect` leer = Vollbild, sonst Bild im Bild in diesem Rechteck
  (Bruchteile wie Layout-Boxen). Ohne strom-Änderung möglich: der
  DSK-Pad auf dem Programm-Compositor (`vm:mixer`, `sink_{Schwarz+1+Ebene}`)
  bekommt `xpos/ypos/width/height` (Pixel von 1920 × 1080, strom-Standard
  `pgm_resolution`); strom setzt sie nur beim Aufbau und lässt sie bei
  Takes, PiP und FTB in Ruhe (nur Alpha). `sizing-policy` steht dort auf
  `keep-aspect-ratio`: der Input behält sein Seitenverhältnis in der Box.
  Neuaufbau der Engine und Show öffnen setzen die Plätze aller Kanäle neu
  (`OverlayPlace` in `restore_commands`).
- **Offen (Engine)**: Rand um ein BiB-Overlay (die Rahmen der Layouts
  sind Unterlagen für PiP-Zonen, DSK-Pads haben keine), Schiebe-/
  Wischeffekte, OVL.4 (vergrößern per Aktion: ginge mit `set_overlay`,
  aber ohne Bewegung).

**Entscheidung OVL.3**: `overlay_preview {channel, input}` legt einen
Input auf einen Kanal „für den nächsten Take“ (`null` nimmt ihn weg;
im Zustand und in der Show-Datei `overlay_preview`, wie die Vorschau
gespeichert). Er hat Vorschau-Tally (grün), solange er wartet. Der
nächste Take der Vorschau nimmt ihn mit: Cut, Übergangstaste, T-Bar am
Ende, Schnellstart der Vorschau, Stinger am Schnittpunkt (hinter dem
Clip). Er geht mit Effekt und Länge des Takes ein (Schnitt: sofort),
nicht mit dem Kanaleffekt: er gehört zum Bildwechsel. Danach ist er
verbraucht. `cut_direct` und Schnellstart eines anderen Inputs sind kein
Take der Vorschau und nehmen nichts mit. Zeigt der Kanal schon diesen
Input, passiert nichts; ein anderer wird ersetzt. Ein entfernter Input
fällt heraus. Auto-Aus (OVL.2) läuft ab dem Take. Beim T-Bar geht das
Overlay erst am Ende ein, nicht mit dem Ziehen.
- **Nicht im Vorschau-Bild**: strom zeigt DSK-Ebenen nur auf Programm;
  das Vorschau-Bild im Multiview kommt direkt vom Input. Ein Overlay auf
  der Vorschau bräuchte DSK-Pads auf dem Vorschau-Zweig in strom. Darum
  zeigen Tally, Console und Lampen den vorbereiteten Zustand.
- OSC `/lmix/overlay/{n}/preview [input]` (ohne Input: wegnehmen). MIDI-
  Lampe einer `overlay_preview`-Taste: Vorschaufarbe, solange ihr Input
  dort wartet, Programmfarbe, sobald der Kanal ihn zeigt; `/keys` zeigt
  Bild und Tally des Inputs.

**Entscheidung LAY.5**: Layout-Vorlagen gehören zu den Einstellungen von
lmix-core, nicht zur Show: Datei `--layout-templates`
(`LMIX_LAYOUT_TEMPLATES`, Standard `~/.config/lmix/layout-templates.json`),
gelesen beim Start, nach jeder Änderung ganz geschrieben, kaputt: als
`.json.broken` beiseitegelegt, wie die Kurzbefehle für alle Shows (der
Lese- und Schreibweg ist jetzt für beide derselbe). Export- und
Settings-Datei haben dieselbe Form (`{"format": "lmix-layouts",
"version": 1, "templates": […]}`); eine neuere Version wird abgelehnt.
Höchstens 64 Vorlagen, Name eindeutig (gleicher Name ersetzt).
- Eine Vorlage hält Plätze, Rahmen, Zuschnitt, Sperre und für jeden
  Platz den Input als Nummer und Name von damals, keine Ids (die gelten
  nur in einer Show). Laden (`load_layout_template`) legt ein neues
  Layout an: zuerst der Input gleichen Namens, sonst der an der Nummer,
  wenn kein anderer Platz ihn hat, sonst der nächste übrige Input mit
  Bild; reicht das nicht, Fehler. So passt eine Vorlage in eine Show mit
  anderen Inputs, ohne Dialog.
- Aktionen `save_layout_template {layout, name?}`,
  `load_layout_template {template, name?}`, `remove_layout_template`,
  `export_layout_templates {path, templates}` (leer: alle),
  `import_layout_templates {path}`; `layout_templates` im Zustand.
  Testskripte und Demo geben eine eigene Datei im Arbeitsordner an.

**Console**: Rechtsklick auf einen Overlay-Kanal unter den Übergängen:
„Einstellungen…“ (Effekt, Dauer, Automatisch aus, Bild im Bild mit Platz
in Prozent oder Vorgabe), „Nicht mit dem nächsten Take“. Rechtsklick auf
eine Overlay-Taste einer Kachel bereitet den Input dort vor (noch einmal:
nicht mehr); Taste „1→“, Kachel „bereit OVL 1“ in Vorschaufarbe, Kanal
„→ 3“ mit Vorschau-Betonung. Layout-Leiste: „Als Vorlage speichern“ im
Layout-Menü, „Vorlagen“ (neues Layout aus, entfernen, Import, Export).
Vorlage „Overlay 1 mit dem nächsten Take“ für Kurzbefehle, Auslöser und
Cues; eine Overlay-Vorlage ohne Dauer folgt dem Kanal.
**Geprüft**: Switcher-Tests (Kanaleffekt und eigene Dauer, Platz, Fehler,
Auto-Aus fällig je Einblenden, Overlay mit Übergang, T-Bar und Cut,
verbraucht, entfernter Input, Vorlagen speichern/laden nach Name und
Nummer), Service-Tests (Auto-Aus nach der Zeit, nicht nach Aus und
wieder Ein; Vorlagen-Datei, Export, Import, kaputte Datei), Lampen-Test,
OSC-Zuordnung, `test-headless.sh` (OSC: vorbereitet, Take, Auto-Aus;
Vorlage in eigener Datei), `test-web.sh`, Console auf Xvfb mit
Null-Engine (Dialog setzt den Zustand, Kachel-Rechtsklick, Cut nimmt das
Overlay mit, Vorlage laden, keine QML-Warnungen), `test-m2b-vm.sh`
(strom, CPU-Mischer: vorbereitetes Overlay mit Vorschau-Tally, mit dem Cut
im oberen rechten Viertel auf Programm, nach 6 s von selbst aus).
**Offen**: siehe oben (Rand, Effekte außer Blende, Overlay im
Vorschau-Bild: strom); BiB-Overlays auf CUDA/GL nicht geprüft (H.29).

## 2026-10-16 — Show-Modus S1 Stufe 1: Cue-Modell und Ablauf

**Kontext**: S1 beginnt mit Cue-Modell und Ablauf-Engine in `lmix-core`
(`LASTENHEFT-SHOW.md` §7, S-E.1, S-E.5).

**Entscheidungen**:
- **Ein flacher `Cue`** (`lmix-types/src/cue.rs`) mit `kind` (`actions`,
  `note`, `wait`, `start`, `group`) und den Feldern aller Arten, alle
  optional außer `kind`; `check_cue` lehnt Felder einer anderen Art ab.
  Ein Drehbuch oder Skript schreibt so nur, was es braucht
  (`{"kind":"wait","duration_ms":2000}`), Tippfehler fallen trotzdem auf.
  Verworfen: getaggtes Enum je Art (lange JSON-Pfade, Standardwerte je
  Variante doppelt).
- **Ablauf als reine Logik** (`lmix-core/src/cues.rs`, `Runner`): bekommt
  die Zeit und gibt zurück, was zu tun ist (Aktionen, Ereignisse). Jeder
  Schritt (Aktion, Ende, Folge) zählt von seiner eigenen Fälligkeit, nicht
  vom Zeitpunkt des Weckens: 100 Warte-Cues à 20 ms enden genau 2 s nach
  GO, auch wenn jedes Wecken 7 ms zu spät kommt (Test). Der Dienst schläft
  bis zur nächsten Fälligkeit (`watch_cues`) und wird bei jeder
  Cue-Aktion geweckt.
- **Was bei GO sofort fällig ist, läuft vor der Antwort** (wie ein Cut);
  Späteres läuft im Hintergrund. Eine fehlschlagende Aktion beendet nur
  ihren Cue und kommt als Ereignis `cue`/`failed`; Zeiten und Folgen
  laufen weiter (CUE.19).
- **Standby ist die oberste Ebene**: GO startet den Standby-Cue samt
  Folgekette, Standby springt hinter die Kette. Ein Cue in einer Gruppe als
  Standby-Ziel heißt seine Gruppe (Gruppen „hinein“ erst S2).
- **GO-Sperre** 300 ms je Show (`set_go_guard`, 0–5000 ms), gilt für jedes
  GO über alle Wege (Taste, OSC, API, Kurzbefehl), nicht für `cue_start`
  und Folgen; ein verworfenes GO ist das Ereignis `go_ignored`.
- **Start-Cue mit Liste ohne Cue** = GO in dieser Liste (bewegt deren
  Standby, ohne GO-Sperre); mit Cue: startet ihn samt Kette, Standby bleibt.
- **Stopp und „andere stoppen“ halten sofort an**: Ausblenden gibt es erst
  mit Klang und Bild (Stufe 2); `fade_ms` und `mode: fade` werden schon
  angenommen und gespeichert.

**Bricht** (vor dem ersten Release erlaubt): `cue_back` und `cue_jump`
entfallen, dafür `cue_standby` (`previous`, `{"index": n}`, …); Zurück
startet nichts mehr, es bewegt nur Standby (QLab-Verhalten). Im Zustand
heißt `next` jetzt `standby`; ein Cue hat `name` statt `label`. OSC
`/lmix/cue/next` heißt `/lmix/cue/standby` und trägt den Namen des
Standby-Cues mit. Show-Dateien der Version 1 werden beim Laden migriert
(`label` → `name`), Console-Dialog und VM-Check sind angepasst.

**Geprüft**: 14 Tests der Ablauf-Engine und der Migration, ein
Dienst-Test mit pausierter Zeit (Folge nach 300 ms, kaputter Cue
gemeldet, nächster läuft), Switcher-Tests der Prüfregeln; VM-Check mit
echtem strom (GO in der aktiven Liste, Folge nach 1,5 s, Standby zurück
an den Kettenanfang). **Offen**: 4-h-Genauigkeit im Selftest (SNF.4),
„erst nach Loslassen wieder scharf“ (RUN.6), Anzeige in der Console (SUI).

## 2026-10-16 — Drehbuch-Import (S1 Stufe 6, IMP)

**Kontext**: Spielleitung P8 schreibt Cues ins Drehbuch (Obsidian) neben
den Szenentext. S-E.2: eigenes Werkzeug über die API.

**Entscheidungen**:
- **`lmix-script`** (`workspace/lmix-script`), vorgezogen vor die Stufen
  2–5: hängt nur am Cue-Modell (Stufe 1) und lässt sich ohne GPU und
  Hardware prüfen.
- **Cue-Blöcke in TOML** (```` ```lmix-cue ````), die Felder des API-Cues.
  Verworfen: YAML (das übliche Rust-Crate `serde_yaml` ist archiviert,
  die Nachfolger jung oder umstritten), JSON (in Obsidian mühsam zu
  schreiben). TOML-Crate `toml` (MIT/Apache-2.0), Dateibeobachtung
  `notify` (CC0/MIT/Apache-2.0), HTTP `reqwest` (MIT/Apache-2.0).
- **Überschriften werden Notiz-Cues mit Folge „nach Ende“**: GO auf einer
  Szenenmarke startet die Szene, statt einen leeren Schritt zu machen.
- **Ids aus Liste und Nummer** (UUID v5), ohne Nummer aus dem Namen:
  dasselbe Drehbuch ergibt bei jedem Laden dieselben Ids, `lmix-core`
  hält damit Standby und laufende Cues (`set_cue_list` hält Standby jetzt
  über die Cue-Id statt die Stelle).
- **Liste mit Quelle** (`source: "script:<ordner>/<datei>"`): `lmix-script`
  ersetzt nur eigene Listen, entfernt eigene ohne Datei, lässt eine Liste
  gleichen Namens aus der Console stehen. Die Console bearbeitet Listen
  mit Quelle nicht (IMP.9).
- **Doppelte Nummer**: Fehler an jeder Stelle, keine beteiligte Liste
  lädt. Welche Stelle „die zweite“ ist, hinge sonst an Dateinamen.
- **Fehlende Mediendatei = Warnung**, die Liste lädt: die Abnahme von S1
  verlangt, dass eine fehlende Datei den Szenenwechsel nicht abbricht; der
  Cue meldet den Fehler beim Lauf (CUE.19).

**Geprüft**: 6 Tests im Werkzeug, `scripts/test-script.sh` gegen
lmix-core mit Null-Engine (Fehler mit Datei und Zeile, Laden, GO auf der
Szenenmarke, Neuladen mit gehaltenem Standby, `--watch`, fremde Liste
bleibt, Liste mit Datei weg); beides in der CI (Job `core`).
**Offen**: SSH.2 (die Show speichert nur den Ordner; heute lädt man nach
dem Start neu), Warnung bei Nummern mit Leerzeichen (CUE.1).

## 2026-10-16 — Ausgabeziele (S1 Stufe 4, EXT.1–5)

**Kontext**: P8 schaltet Licht und Duft über Home Assistant; Tokens
dürfen weder in die Show noch ins Drehbuch (EXT.2, S-E.6).

**Entscheidungen**:
- **Ziele sind Rechner-Einstellungen** (`--targets`, Standard
  `~/.config/lmix/targets.json`) wie die Kurzbefehle für alle Shows; die
  Datei entsteht mit Rechten 600. Im Zustand stehen Ziele ohne Geheimnis
  (`has_secret`); `secret: null` beim Setzen behält es, `""` löscht es.
- **HTTP-Cue = Aktions-Cue mit `http_request` oder `ha_service`**, keine
  eigene Cue-Art: dieselben Aktionen gehen auch aus Kurzbefehlen und
  Auslösern, und das Cue-Modell bleibt klein. Abweichung vom Wortlaut von
  EXT.3 („HTTP-Cue“); der Anzeigename eines solchen Cues ist der Name der
  Aktion, bis die Show-Ansicht (SUI) eigene Symbole bekommt.
- **Senden blockiert nie**: die Aktion kehrt sofort zurück, die Anfrage
  läuft im Hintergrund mit dem Zeitlimit des Ziels; das Ergebnis ist das
  Ereignis `target` (`ok` oder `failed` mit Grund), auch per TCP (`SUB
  TARGET`). Ein nicht erreichbares Ziel hält weder GO noch Folgen auf.
- **Mehrere Adressen = gleichzeitig senden**, erreicht ist das Ziel, wenn
  eine mit Erfolg antwortet (Haupt/Reserve).
- **Geheimnis als `Authorization: Bearer`**: so will es Home Assistant
  (langlebiges Zugangs-Token); andere Verfahren bei Bedarf.
- **`test_target`** wartet auf die Antwort (`GET` der ersten Adresse)
  und lehnt mit dem Grund ab (EXT.5).

**Geprüft**: 5 Tests in `targets.rs` (Geheimnis bleibt, Prüfregeln,
Home-Assistant-Pfad und -Rumpf, Token und Rumpf auf der Leitung gegen
einen lokalen Server, Reserve-Adresse, 401 als Fehler), ein Dienst-Test
(Cue → Home Assistant, Ereignis, Dateirechte, kein Geheimnis im Zustand).
**Offen**: OSC senden (EXT.6, S2), Ziele ex- und importieren mit der Show
(SSH.5), Platzhalter (EXT.7, S2).

## 2026-10-16 — Panik, harter Stopp, Notfall-Liste (S1 Stufe 5, SAF)

**Entscheidungen**:
- **Panik hält jetzt sofort an**: Es gibt noch keine Klänge und Bilder zum
  Ausblenden (Stufe 2/3). Die Panikzeit (`set_panic_time`, Standard 1 s,
  0–10 s) zählt trotzdem schon: eine zweite Panik darin ist ein harter
  Stopp (SAF.1), danach ist die nächste wieder eine Panik.
- **Notfall-Liste startet ganz**: bei Panik und hartem Stopp starten alle
  Cues der Liste zugleich, wie eine Zeitleiste (Folgen gelten nicht).
  Wer eine Reihenfolge braucht, setzt Vorwartezeiten. Verworfen: nur den
  ersten Cue starten (eine vergessene Folge ließe das Raumlicht aus).
- **Standby steht in der Show** (`standby` als Cue-Id je Liste) und wird
  bei jeder Bewegung mit der Show gesichert: nach einem Absturz oder
  Neustart startet nichts, Standby steht, wo er war (SAF.6). Auch eine
  gespeicherte Show öffnet mit ihrem Standby (wie beim Vorbild). Am Ende
  einer Liste wird nichts gespeichert, sie beginnt dann oben.
- **Jeder Weg**: Aktionen `cue_panic`, `cue_hard_stop`; OSC `/lmix/panic`,
  `/lmix/stop/hard`; Kurzbefehle tragen sie auf Taste, globale Taste und
  MIDI (SAF.5). Ereignis `panic` (`fade` oder `hard`), per TCP `PANIC`
  im Thema `CUE`. Cues dürfen keine Panik auslösen (keine Schleifen).

**Geprüft**: Runner-Tests (alle Cues und Vorwartezeiten weg, Notfall-Liste
ganz, zweite Panik hart, danach wieder Panik; Standby aus der Show), ein
Dienst-Test (Panik über den Dienst, Notfall-Cue schneidet, Ereignis;
neuer Dienst über dieselbe Sicherung: Standby gleich, nichts läuft), OSC.
**Offen**: Ausblenden und SAF.7 mit Stufe 2, Panik ohne Schlüssel (CTL.5),
Escape als Panik in der Show-Ansicht (CTL.6, SUI).

## 2026-10-16 — Feste OSC-Abonnenten und Restzeit (S1 Stufe 5, CTL.2, CTL.4)

**Kontext**: Abnahme S1: Standby-Name und Restzeit erscheinen auf
Companion-Tasten über einen festen OSC-Abonnenten, ohne eigenes Modul.

**Entscheidungen**:
- **Abonnenten neben den Zielen** in derselben Rechner-Datei
  (`targets.json` ist jetzt `{"targets": […], "feeds": […]}`; das Format
  war erst seit heute da, keine Migration). Ein Abonnent nennt ein
  `osc_udp`-Ziel und Nachrichten je Anlass (`standby`, `running`,
  `panic`) mit Platzhaltern (`{name}`, `{number}`, `{remaining}` …).
  Ein Argument, das nur aus einer Zeit besteht, geht als Float (Companion
  kann damit rechnen), alles andere als Text.
- **Ein Takt für beides** (`--cue-events-hz`, Standard 4, 1–20): das
  Ereignis `running` (laufende Cues mit fertig gerechneter Restzeit) und
  die `running`-Nachrichten gehen in diesem Takt, solange etwas läuft, und
  einmal danach mit leeren Werten, damit Tasten nicht stehen bleiben.
  Standby geht nur bei Änderung, Panik sofort. Ohne Abonnenten und ohne
  laufende Cues arbeitet der Takt nicht.
- **Neu gesetzter Abonnent bekommt alles neu** (Standby jeder Liste).

**Geprüft**: 3 Tests in `feeds.rs` (Standby nur bei Änderung, Restzeit je
Takt und einmal danach, Float bei reiner Zeit, Panik), ein Dienst-Test
mit echtem UDP-Empfänger (Standby, Restzeit, Ereignis `running`, Panik),
zehnmal hintereinander grün.
**Offen**: OSC über TCP (SLIP) für Abonnenten, Ablauf nach 60 s ohne
Lebenszeichen für angemeldete Abos (CTL.3).

## 2026-10-16 — Show-Ansicht der Console (S1 Stufe 7, SUI)

**Entscheidungen**:
- **Umschalter Mischer/Show** in der Werkzeugleiste (gemerkt in den
  Einstellungen der Console). Der Mischer-Bereich und die Show-Ansicht
  teilen sich die Seite; Datei-Knöpfe und Stil bleiben.
- **Die gezeigte Liste ist die aktive**: wer eine Liste wählt, setzt sie
  aktiv (`set_active_cue_list`). GO hier, die Leertaste und GO von außen
  (Companion, OSC) meinen damit dieselbe Liste.
- **Tasten in der Ansicht, nicht fensterweit**: Leertaste GO, Escape
  Panik (zweimal: hart, über die Panikzeit im Core). Die Ansicht hält den
  Fokus, ihre Knöpfe nehmen keinen: so schließt Escape in einem Dialog den
  Dialog, und die Leertaste drückt nie einen fokussierten Panik-Knopf.
- **Zeiten laufen in der Console weiter**: der Core sendet einen Zustand
  bei jedem fälligen Schritt (jetzt auch, wenn eine Vorwartezeit ohne
  Ausgabe endet), dazwischen zählt die Ansicht ab dem Eintreffen weiter.
  Kein eigenes Ereignis nötig.
- **„Erweitert“** zeigt Vor- und Nachwartezeit und Folge (SUI.13);
  Bearbeiten bleibt im Cue-Dialog (SUI.10, S2).

**Geprüft**: Console auf Xvfb gegen lmix-core mit Null-Engine und einer
Drehbuch-Liste (Szenenmarken, Zeitleisten-Gruppe, Warte-Cues): Ansicht,
zweimal Leertaste (GO, Kette, Standby auf der nächsten Szene), laufende
Cues mit Fortschritt und Restzeit, Escape als Panik. Console-Tests,
lmix-core 248 Tests.
**Offen**: dunkle Raumansicht (SUI.6), Bühnenvorschau und Zonen-Leiste
(SUI.7, SUI.8, mit Stufe 2/3), Warnliste (SUI.5).

## 2026-10-16 — Zugangsschlüssel mit Rechten (S1 Stufe 5, CTL.5)

**Kontext**: Companion soll GO und Panik schicken dürfen, aber keine Show
öffnen; ein Taster an der Bühne soll Panik ohne Schlüssel auslösen
(Abnahme S1). Bisher (API.7): ein Passwort für alles, offene Seiten.

**Entscheidungen**:
- **Erweitern statt ersetzen**: `--access-keys` (JSON-Liste `{name, key,
  right}`) neben `--password`; das Passwort zählt als Schlüssel mit
  `edit`. Ohne Passwort und Schlüssel bleibt das lokale Netz offen wie
  bisher; der eigene Rechner braucht nie einen Schlüssel.
- **Drei Rechte**: `view` (lesen), `control` (die Show fahren: GO, Cuts,
  Pegel, Titel, Panik, Ziele ansprechen), `edit` (Vorbereitetes ändern:
  Inputs, Cue-Listen, Ziele, Abonnenten, Dateien, Sperre). Die Zuordnung
  steht in `right_of` als `match` ohne Platzhalter: eine neue Aktion baut
  erst, wenn sie ihr Recht hat.
- **`--open-panic`**: Panik und harter Stopp gehen aus dem lokalen Netz
  ohne Schlüssel, über HTTP, TCP (vor `AUTH`) und OSC (das sonst mit
  Passwort nur den eigenen Rechner hört).
- **Falsche Schlüssel bremsen**: je Adresse 100 ms, dann doppelt, höchstens
  5 s; ein richtiger Schlüssel setzt zurück; jede Abweisung steht im Log.
  Schlüssel unter 12 Zeichen lehnt `lmix-core` beim Start ab.
- **Abweichung**: CTL.5 nennt eine Stufe „ohne Schlüssel“ mit eigener
  LAN-Adressliste. Die gibt es nicht: der Rechner selbst bleibt frei, das
  Netz braucht Passwort oder Schlüssel, sobald einer gesetzt ist. Eine
  Adressliste kommt, wenn eine Persona sie braucht.

**Geprüft**: Tests für die Einordnung, für Rechte über HTTP (Steuern darf
schneiden, nicht `add_input`; Ansehen liest, schaltet nicht; Panik ohne
Schlüssel nur mit `--open-panic`), für die wachsende Wartezeit (pausierte
Zeit) und über TCP (Panik vor `AUTH`, Steuern ja, Bearbeiten nein).
**Offen**: Rechte für die Web-Seiten jenseits der offenen Seiten, eigene
Adressliste.

## 2026-10-16 — Warnliste, Rechner-Einstellungen im Paket (SUI.5, SSH.5, CUE.1)

- **Warnliste** (`cue_warnings` im Zustand): `lmix-core` prüft nach jeder
  Änderung der Show, der Ziele oder der fehlenden Medien, was beim Lauf
  schiefginge: Start-Cue ohne Ziel, Aktion an einen Input oder ein
  Ausgabeziel, das es nicht gibt, Input ohne Mediendatei. Zwischengespeichert,
  nicht je Zustand gerechnet. Jede Warnung hat eine Art (`problem`) mit
  Werten, damit die Console sie übersetzt, und den englischen Text.
- **Rechner-Einstellungen im Paket**: `export_package` mit `machine: true`
  legt `machine.json` (Ziele, Abonnenten, ohne Geheimnisse) ins Paket;
  `import_package` mit `machine: true` übernimmt sie, gleichnamige Ziele
  behalten ihr Geheimnis. Dabei gefunden: Vorlagen, die Cues in Gruppen
  laden, kamen nicht ins Paket (nur oberste Ebene durchsucht); behoben.
- **Nummern mit Leerzeichen** warnen im Drehbuch (CUE.1), laden aber.

**Geprüft**: Test der Prüfungen (fehlendes Start-Ziel, Ziel, Input in einer
Gruppe, fehlende Mediendatei), Paket-Test mit `machine.json`, Test der
Übernahme (Geheimnis bleibt), Drehbuch-Test; Console auf Xvfb zeigt drei
Warnungen auf Deutsch.

## 2026-10-16 — Drehbuch-Ordner der Show (SSH.2, Befund B44)

**Entscheidung**: Die Show merkt sich ihren Drehbuch-Ordner
(`set_script_folder`, Knopf „Drehbuch…“ in der Show-Ansicht);
`lmix-core` startet dafür `lmix-script load <ordner> --watch` als
Kindprozess, beim Öffnen der Show, nach einem Neustart und bei jeder
Änderung des Ordners, und beendet ihn ohne Ordner. `lmix-core` kennt so
weiter kein Markdown (S-E.2); das Werkzeug spricht die API wie jeder Client
(über Loopback, ohne Schlüssel).
**Abweichung von SSH.2**: Die Cues der Drehbuch-Listen bleiben zusätzlich
in der Show gespeichert. Ohne Werkzeug hat die Show dann trotzdem ihre
Cues, und Standby übersteht einen Neustart (SAF.6). Das Werkzeug ersetzt sie
beim nächsten Laden.
**Geprüft**: Test des Kindprozesses (einmal je Ordner, Wechsel, Stopp) mit
einem Ersatzwerkzeug; `scripts/test-script.sh` mit echtem `lmix-script`:
Ordner setzen lädt die Liste, eine Änderung wird übernommen, ohne Ordner
nicht mehr.

## 2026-10-16 — Ziele und Abonnenten in der Console (Befund B43)

Dialog „Ziele…“ in der Show-Ansicht: Ausgabeziele anlegen, bearbeiten,
testen (HTTP), entfernen; OSC-Abonnenten mit Nachrichten je Anlass
(Standby, laufender Cue, Panik), Adresse und Werten mit Platzhaltern. Das
Geheimnis zeigt die Console nie; leer gelassen bleibt es, wie es ist. Ein
Ziel lässt sich nicht umbenennen (das Geheimnis ginge verloren): entfernen
und neu anlegen. Geprüft auf Xvfb gegen lmix-core mit zwei Zielen und
einem Abonnenten.

## 2026-10-16 — Kleinere S1-Punkte: Dauer von Aktions-Cues, dunkler Raum, OSC-Abo-Ablauf

- **Aktions-Cue mit Dauer** (B45): `duration_ms` ist jetzt auch an
  Aktions-Cues erlaubt; so lange gilt der Cue nach seinen Aktionen als
  laufend (z. B. die Länge des Clips, den er startet), „Folge nach Ende“
  wartet darauf. Die Länge aus dem Clip selbst kommt mit den Bild-Cues.
- **Dunkler Raum** (SUI.6): Knopf „Dunkel“ in der Show-Ansicht; Kante
  bekommt `preferDark` (Gruvbox dunkel, kein Weiß), darüber liegt der
  Schleier von Kante mit 60 %; Klicks gehen durch. Nur der Schleier dimmte
  im hellen Leinen-Schema zu wenig (auf Xvfb geprüft).
- **OSC-Abos laufen ab** (CTL.3): 60 s nach der letzten Nachricht des
  Abonnenten; `/lmix/ping` (Antwort `/lmix/pong`) oder ein neues
  `/lmix/subscribe` hält es. **Bricht** für Abonnenten ohne Lebenszeichen;
  für Tasten ohne Keepalive gibt es die festen Abonnenten (CTL.4). Dabei die
  Doku `/lmix/cue/next` → `/lmix/cue/standby` nachgezogen.

## 2026-10-16 — Älteres Show-Format, Einstellungen als Datei (SHW.7, SHW.8)

- **SHW.7 ohne Rückfrage, mit Sicherung**: Eine Show-Datei älterer Version
  wird beim Öffnen umgewandelt (seit heute Version 1 → 2). Statt vorher zu
  fragen, sagt die Console es an, und das erste Speichern über die Datei
  legt die alte als `<datei>.v<n>.bak` daneben (nur einmal). So bleibt die
  alte Fassung, ohne dass jedes Öffnen eine Frage stellt.
- **SHW.8**: `export_settings` schreibt die Einstellungen dieses Rechners
  (Kurzbefehle für alle Shows, Layout-Vorlagen, Ausgabeziele, Abonnenten)
  ohne Geheimnisse in eine Datei (`lmix-settings`, Version 1);
  `import_settings` nimmt sie statt der aktuellen, ganz oder gar nicht,
  gleichnamige Ziele behalten ihr Geheimnis; `reset_settings` sind die
  Werkseinstellungen (in der Console mit Rückfrage). Im Menü „Paket“.
**Geprüft**: Dienst-Tests (Version-1-Datei öffnen, speichern, Sicherung
gleich der alten Datei, neue mit Version 2; Einstellungen exportieren ohne
Geheimnis, zurücksetzen, importieren, Geheimnis bleibt, kaputte Datei ändert
nichts); Console auf Xvfb (Menü, Rückfrage).

## 2026-10-17 — Zonen ohne Engine (S1 Stufe 2, Teil SND.10)

**Kontext**: S-E.4 ist entschieden (Zone = logischer Ausgang mit
Zuordnung je Rechner), S-E.3 (wie Klang-Cues spielen) noch nicht. Die
Zonen selbst hängen nicht an S-E.3.

**Entscheidungen**:
- **In der Show**: Zonen mit Namen und 1–8 Kanälen (bis 16 Zonen).
- **In den Rechner-Einstellungen** (`targets.json`, mit Export, Import
  und `machine.json` im Paket): je Zone 1–4 Ausgänge mit Gerät (leer =
  Standardausgang), Gerätekanal je Zonenkanal und Pegel (−60 bis +12 dB).
  Eine Zuordnung darf für Zonen bestehen, die die aktuelle Show nicht hat
  (der Rechner bleibt für die nächste Show eingerichtet).
- **Gerät als Text**: Es gibt noch keine Erkennung von Ausgabegeräten
  (nur Eingänge); das Format wird mit dem Abspielen (Stufe 2) an die
  Engine angepasst, falls nötig (vor dem ersten Release erlaubt).
- **Ausgänge mit falscher Kanalzahl zählen nicht** (sie würden falsch
  spielen); eine Zone ohne passenden Ausgang steht in der Warnliste.

**Geprüft**: Tests der Prüfregeln und der Zusammenführung, Dienst-Test
(Zuordnung kommt mit Export/Import der Einstellungen zurück); Console auf
Xvfb (Dialog „Zonen…“, Warnung „Zone hinten spielt auf diesem Rechner
nirgends“).

## 2026-10-17 — Cue-Zeitgenauigkeit über 4 h nur ohne Last (RUN.8)

**Messung** (`scripts/test-cue-timing.sh`, Null-Engine, Kette aus 50
Wartezeiten zu 20 ms, Grenze 10 ms):

| Lauf | Rechner | Schritte | spätestens | über 10 ms |
|---|---|---|---|---|
| 4 h (16.10. 22:47–02:47) | nebenbei Rust-/Qt-Builds und Tests | 1 468 803 | 145,6 ms | 694 (0,05 %) |
| 1 h (17.10. 02:48–03:48) | sonst frei (Last ~1) | 367 201 | 5,2 ms | 0 |
| 10 min | alle 24 Kerne mit `yes` belegt | 61 203 | 65,3 ms | 214 |

Die Zeiten driften nicht. Verspätet sind nur Schritte, bei denen der
Rechner ausgelastet ist: Der Zeitgeber läuft als normaler Prozess
(nice 0) in der tokio-Laufzeit und wartet auf einen freien Kern. Das
Autosave beim Weiterrücken von Standby ist nicht die Ursache: Im Test
rückt Standby nicht, die Datei wurde nur beim Start geschrieben.

**Folgerung**: RUN.8 hält auf einem freien Show-Rechner; die 4-h-Messung
ohne Last steht noch aus. Unter Last hält es nicht.

**Vorschlag (offen, nicht gebaut)**: Den Zeitgeber der Cue-Listen auf
einen eigenen Thread mit Echtzeit-Priorität legen und diese wie PipeWire
über rtkit (D-Bus) anfordern, ohne Root. Geht das nicht, warnt der
Selbsttest: In SNF.4 mitmessen, wie spät die Schritte sind, und die
Last des Rechners anzeigen.

## 2026-10-17 — GO erst nach Loslassen (RUN.6)

**Kontext**: RUN.6 verlangt die Option „erst nach Loslassen wieder
scharf“: Ein festgehaltener oder prellender Taster gibt kein zweites GO,
auch nicht nach der GO-Sperre.

**Optionen**: (a) ein Merker für alle Wege: Jedes GO wartet, bis
irgendein Taster losgelassen wird; (b) je Taster: Nur der Taster, der das
GO gab, wartet auf sein Loslassen.

**Entscheidung**: (b). Mit (a) sperrt ein verlorenes Loslassen (UDP) jedes
GO der Show; mit (b) nur diesen einen Weg, die Leertaste und Companion
bleiben bedienbar.

- `set_go_rearm` mit `time` (Standard, nur GO-Sperre) oder `release`;
  gespeichert in der Show (`go_rearm`, fehlt bei `time`).
- Loslassen melden: MIDI (Note aus, Controller unter 64; je Kanal, Art,
  Nummer), OSC `/lmix/cue/release` (je Absender: Companion schickt alle
  Tasten über einen Socket, jede GO-Taste dort zählt als dieselbe). Die
  Leertaste der Show-Ansicht nimmt ihre Wiederholungen ohnehin nicht an.
- HTTP, TCP und Kurzbefehle ohne MIDI melden kein Loslassen: für sie gilt
  nur die GO-Sperre.
- Wechsel des Modus vergisst alle gehaltenen Taster.

**Geprüft**: Tests im Ablauf (gehalten, anderer Weg, Loslassen, Modus
`time`), OSC-Adresse, Speichern und Laden der Show.

## 2026-10-17 — Erreichbarkeit der Ziele in der Warnliste (SUI.5)

**Kontext**: Die Warnliste nannte Ziele, die der Rechner nicht hat, aber
nicht solche, die nicht antworten. SUI.5 verlangt beides vor der Show.

**Entscheidung**:
- `check_targets` (Console „Ziele prüfen“) fragt alle Ziele, an die Cues
  senden, gleichzeitig ab (wie `test_target`: `GET` der ersten Adresse).
  Wer nicht antwortet, steht mit Grund als `target_unreached` an jedem
  Cue, der dorthin sendet.
- Jedes Senden während der Show und jeder `test_target` schreibt das
  Ergebnis fort: Ein Ziel, das ausfällt, erscheint ohne neue Prüfung in
  der Liste, eines, das wieder antwortet, verschwindet.
- Kein Prüfen im Hintergrund: Abfragen im Takt fluten Home Assistant und
  zeigen nur, was das nächste Senden ohnehin zeigt.
- Neue Einstellungen oder Entfernen eines Ziels vergessen sein Ergebnis.
- Zonen-Geräte: strom nennt seine Ton-Ausgänge noch nicht; mit Stufe 2.

**Geprüft**: Regel-Test (Gruppe, doppelte Nennung), Dienst-Test mit
Ziel ohne und mit Gegenstelle.

## 2026-10-17 — Vertraute Adressen ohne Schlüssel (CTL.5)

**Kontext**: CTL.5 verlangt eine Stufe „ohne Schlüssel“ getrennt für den
Rechner und eine LAN-Adressliste, Standard nur der Rechner. Bisher: mit
Schlüssel oder Passwort braucht jedes Gerät im LAN einen Schlüssel; OSC
hat keinen und nahm dann nur den Rechner (und `--open-panic`). Ein
Stream Deck über Companion per OSC ging so nur ohne jeden Schlüssel.

**Entscheidung**: `--trust ADRESSE[/PRÄFIX][=RECHT]` (mehrfach oder mit
Komma, `LMIX_TRUST`), Recht `view`, `control` (Standard) oder `edit`.
- Gilt nur, wenn Schlüssel oder Passwort gesetzt sind: Sonst darf das LAN
  ohnehin alles. Der Standard „nur der Rechner“ aus CTL.5 heißt in lmix:
  Schlüssel setzen; ohne Schlüssel bleibt das Verhalten von API.7, damit
  bestehende Aufbauten (Webseiten, Companion ohne Schlüssel) weiterlaufen.
- HTTP: ohne `Authorization` hat die Adresse ihr Recht; ein falscher
  Schlüssel bleibt falsch (Wartezeit). Textprotokoll: kein `AUTH` nötig,
  das Recht gilt sofort. OSC: jede Nachricht bis zu diesem Recht
  (`cue/go` ist `control`).
- Mehrere passende Einträge: das höchste Recht. IPv4 in IPv6 (`::ffff:`)
  zählt als IPv4.
- Nur Adressen, keine Namen: Ein Name, der auf eine andere Adresse zeigt,
  wäre ein Schlüssel ohne Schloss.

**Geprüft**: Lesen der Einträge (Präfix, Recht, Fehler), Netze v4/v6,
HTTP mit und ohne Liste, Textprotokoll ohne `AUTH`.

## 2026-10-17 — OSC-Abo je Ereignis und Feld (CTL.3)

**Kontext**: Ein OSC-Abonnent bekam bisher alles vom Mischer und den
Standby, aber keine Cue-Ereignisse, keine Restzeit und keine Panik; die
gab es nur über feste Abonnenten (CTL.4) und TCP. CTL.3 verlangt An- und
Abmelden je Ereignis und Feld.

**Entscheidung** (`osc_subs.rs`):
- `/lmix/subscribe` ohne Argumente wie bisher: Mischer und Standby.
- `/lmix/subscribe THEMA [FELD …]`: fügt ein Thema hinzu, nur mit den
  genannten Feldern in dieser Reihenfolge (ohne Felder: alle). Themen
  `mixer`, `standby`, `running` (neuester laufender Cue je Liste im
  Takt der Cue-Ereignisse, am Ende einmal leer, wie bei CTL.4), `cue`
  (`started`, `ended`, `stopped`, `go_ignored`, `failed` mit Grund),
  `panic`, `target`.
- `/lmix/unsubscribe THEMA` nimmt eines weg, ohne Argument alle.
- Zeiten als Float in Sekunden, die Position als Int, sonst Text; ein
  Feld, das das Thema nicht hat, wird abgelehnt (`/lmix/error`).
- Feste Abonnenten (CTL.4) bleiben, wie sie sind: sie brauchen kein
  Lebenszeichen und haben Vorlagen mit Platzhaltern.

**Geprüft**: Tests der Felder und Nachrichten, Headless-Test mit echtem
OSC-Client (Name und Restzeit des laufenden Cues, Cue-Ereignisse, keine
Mischer-Nachrichten).

## 2026-10-17 — S-E.13: Bühnen intern, gemessen (offen)

**Kontext**: S1 Stufe 3 (VIS) braucht Bühnen: mindestens 4 je Show, jede
eine eigene Bildfläche mit Ebenen, auf Vollbild oder Fenster (VIS.1,
S-E.9), Schwarz ohne Cues (VIS.8). Wie bei S-E.3 gibt es zwei Wege:
(A) eine Bühne ist ein Mischer-Flow wie heute, jeder Bild-Cue ein
verdeckter Input (eigener Flow, Bild über `inter`, belegt einen Slot);
(B) ein Bühnen-Block in strom, ein Flow mit `compositor`, ein Zweig je
laufendem Bild-Cue, zur Laufzeit an- und abgehängt.

**Messung** (`scripts/bench-stage/run.sh`, reines GStreamer 1.24 auf der
CPU, `compositor` 1080p30 in `fakesink sync`, ohne strom und GPU; 4 Kerne
Xeon 2,8 GHz; 1080p30-H.264-Clips, je Cue eine Kachel 480 × 270; Zeit von
der Anfrage bis zum ersten Ausgangsbild, das den Cue zeigt):

| | A: Pipeline je Cue + intervideo | B: Zweig je Cue | B vorgeladen |
|---|---|---|---|
| Anfrage → sichtbar (8 Cues nacheinander) | 122–133 ms (kalt 209) | 53–91 ms (kalt 118) | 28–64 ms |
| Speicher je Cue | ~60 MB | ~58 MB | ~53 MB |
| Threads je Cue | 9 | 8 | 8 |
| CPU bei 8 Cues (4 Kerne) | 98 % | 93 % | 88 % |
| Ausgang | 30 fps, nichts verworfen | 30 fps | 30 fps |

Bei 30 fps ist ein Bild 33 ms: B vorgeladen erscheint mit dem nächsten
oder übernächsten Bild, A zwei bis drei Bilder später. A wartet auf die
Periode der Inter-Quelle; in lmix kämen dazu der Flow über REST, der Slot
(16 Standard, geteilt mit Kameras) und für 4 Bühnen 4 Mischer.

**Befunde unabhängig vom Weg**:
- Ein neu angehängter Zweig braucht einen Pad-Offset auf die laufende Zeit
  der Bühne, sonst überspringt der `compositor` den Anfang des Clips (im
  ersten Lauf bis 2,4 s); strom macht das für den Mischer schon
  (`lmix-live-offset`).
- Mischmodi (VIS.6): Kein GStreamer-Compositor kann „aufhellen“
  (Maximum). Der CPU-`compositor` kennt nur über und addieren,
  `glvideomixer` multiplizieren nur über Blendfunktionen, „aufhellen“ gar
  nicht. VIS.6 braucht in beiden Wegen eigenes Blending in strom (z. B.
  ein Shader je Ebene).

**Folgerung (Vorschlag)**: B, und zusammen mit S-E.3 B als **ein**
strom-Block „Show-Wiedergabe“ in einem Flow je Rechner: Zonen
(`audiomixer`) und Bühnen (`compositor`) teilen eine Uhr, ein Szenenwechsel
mit Kulisse und Atmo startet auf denselben Zeitpunkt, und Ton aus Videos
(VIS.10, S2) kommt ohne Umweg in die Zone. A bliebe für Live-Bild als
Bild-Cue (VIS.15, S3), das schon ein Input ist.
**Entscheidung** (User 2026-10-17): B, zusammen mit S-E.3 B als ein strom-Block „Show-Wiedergabe“.

**Bauplan B** (Schätzung, nicht gebaut):
- strom: Bühne = `compositor` (GL bzw. CUDA wie der Mischer, CPU als
  Rückfall) mit schwarzem Grund (VIS.8), Ausgang je Rechner auf Vollbild
  oder Fenster (VIS.1, wie OUT.3). Bild-Cue = Zweig Datei → Dekoder →
  Pad mit `zorder` (Ebene, VIS.3), `alpha` (Deckkraft, Fade über
  Controller wie die DSK-Blende, VIS.4), `sizing-policy` (Füllmodus,
  VIS.5); vorladen vor GO (pausiert, am Pad gehalten). Standbild über
  `imagefreeze`. Bühnen-Dimmer (VIS.7) als letzte Ebene oder Pegel am
  Ausgang. Eigenes Blending für VIS.6.
- lmix-core: Bühnen in der Show (Namen) und Zuordnung je Rechner (wie
  Zonen, SSH.3); Cue-Art `picture` und Fade-Ziel Bild/Bühne; Panik
  blendet Bühnen schwarz.
- Console: Bühnen-Dialog, kleine Vorschau (SUI.7).
- Aufwand grob: mit dem Ton-Block zusammen 1–2 Tage mehr als S-E.3 B
  allein, sonst 3–4 Tage.

**Grenzen der Messung**: nur CPU und ein Rechner mit 4 Kernen; GPU-Pfade
(GL, CUDA) und 4 Bühnen gleichzeitig sind ungemessen, beides gehört in die
Abnahme auf echter Hardware.

## 2026-10-17 — Pegel-Fades und Master (AUD.14, AUD.2)

**Kontext**: AUD.14 (Pegel-Fades per Aktion für Input, Master, Busse)
fehlte; es ist Voraussetzung des Show-Modus (S1, SND.6). Einen
Master-Regler hatte lmix nicht, obwohl strom ihn hat (`main_fader`).

**Entscheidung**:
- `fade_volume {target, volume, duration_ms}` mit Ziel `{"input": …}`,
  `{"bus": 1|2}` oder `"master"`, Dauer 0–60 s. strom rampt die
  Eigenschaft selbst (`ramp_ms` der Eigenschafts-API, Interpolation am
  Element); lmix-core sendet einen Befehl, keine Zwischenschritte. Der
  Zustand zeigt sofort den Endwert; ein neuer Wert auf demselben Ziel
  setzt dort fort, wo der Fade steht.
- Mute bleibt, wie es ist; kürzer als die Klickschutz-Rampe (20 ms) wird
  nicht gefadet.
- Verlauf linear im Verstärkungsfaktor (die Regler sind quadratisch,
  `fader_gain`). S-Kurven und „still“ als Ziel kommen mit SND.6 im
  Show-Modus.
- `set_master {volume, mute}`, gespeichert in der Show (`master`, fehlt
  in alten Dateien = 100, unverändert laut: strom-Standard 1,0). Console:
  Dialog „Busse…“ mit Master, Aux 1 und 2.

**Befund dabei** (VM-Test): Der erste Fade des Masters blieb still. strom
nimmt jedes `mute=false` als Aufheben der Stummschaltung: Es blendet von
Stille auf den Pegel vor dem Stummschalten (ohne den: 1,0) ein, auch wenn
der Kanal gar nicht stumm war (`apply_mute`); eine Regler-Rampe danach
startet bei dieser Stille. lmix schickte bei jeder Pegeländerung
`mute=false` mit, also hatte jede Reglerbewegung eines Kanals einen
Einbruch von etwa 40 ms, und lange Rampen begannen bei 0.
**Behoben in lmix** (`level_writes` in `engine/strom.rs`): Stumm geht nur,
wenn es sich gegenüber dem zuletzt Gesendeten ändert, für Kanäle, Aux-Busse
und Master; nach jedem Start der Engine wird alles einmal gesendet.
strom bleibt unverändert; ein „Aufheben ohne Stumm = nichts tun“ in
`apply_mute` wäre ein passender Fehlerbericht an strom.

**Geprüft**: Tests im Schalter (Input, Bus, Master, Grenzen, JSON-Form),
`level_writes`, VM-Test mit strom: Master fadet über 2 s auf still und
kommt zurück.

## 2026-10-17 — Standbilder mit Alpha (SRC.4)

**Kontext**: Ein Standbild wurde nach NV12 in 1280 × 720 gewandelt: ein
PNG-Logo mit transparentem Grund deckte als Overlay das ganze Program
schwarz zu (VM-Test: rechte, klare Hälfte 0/0 statt Orange), und Bilder
verloren Auflösung gegenüber dem 1080p-Program.

**Entscheidung**: Standbilder in Programmgröße (1920 × 1080). Dateiarten,
die Alpha tragen können (PNG, WebP, TIFF), als BGRA, die übrigen (JPEG,
BMP) als NV12. Nach Dateiart, nicht durch Aushandeln: `videoconvert`
wählt aus `{NV12, BGRA}` auch für ein JPEG BGRA. Ein BGRA-Bild sind
8,3 MB je Bild gegen 3,1 MB in NV12; da `imagefreeze` dasselbe Bild
wiederholt, kostet es nichts zu dekodieren, nur den Weg in den Mischer.
Die Mischer nehmen Alpha schon (Stinger mit Alpha).

**Geprüft**: Test der Formate je Dateiart; VM-Test (CPU-Mischer): ein PNG,
links rot, rechts klar, als Overlay über Orange zeigt rechts Orange.
Offen: CUDA und GL auf echter Hardware (H.33).

## 2026-10-17 — Pegel links und rechts (AUD.1)

strom meldet die Spitzenwerte je Kanal; lmix-core behielt nur den
lautesten. Jetzt tragen die Pegel je Input zusätzlich `channels_db`
(links, rechts; leer, solange keine frische Messung da ist), `peak_db`
bleibt für bestehende Clients. Die Kachel zeigt zwei schmale
`KanteAudioMeter` übereinander, eine Haarlinie auseinander (bei gleichem
Pegel sonst nicht als zwei zu erkennen); ohne Kanalwerte ein Balken wie
bisher. Kein neues Kante-Bauteil nötig.

## 2026-10-17 — Abhörlautstärke, AFL als Standard, Standard-Delay (AUD.4, AUD.6)

- **Abhörlautstärke** (`set_monitor_volume`, 0–100, in der Show): der
  Regler des Abhörbusses in strom (`monitor_fader`), im Dialog „Busse…“
  mit Pegel. strom hat für den Abhörbus kein eigenes Stumm; Lautstärke je
  Input auf dem Abhörbus fehlt weiter (bräuchte einen eigenen Send je
  Kanal in strom).
- **Solo-Taste hört nach dem Fader (AFL)**, wie AUD.4 es als Standard
  verlangt; PFL über das Menü der Kachel. Vorher war PFL der Standard.
- **Standard-Delay** (`set_default_delay`, 0–500 ms, in der Show): neue
  Inputs mit Ton bekommen ihn im Kanalzug, vorhandene behalten ihren.
  Console: „Delay für neue Inputs“ im Kanalzug übernimmt den Wert des
  Inputs.

**Geprüft**: Schalter-Test (Delay neuer Inputs, Grenze, Abhör-Befehl und
Wiederherstellen), VM-Test mit strom (Abhören auf 0 still, Program
bleibt), Console auf Xvfb.

## 2026-10-17 — Multiview als Ausgang (OUT.2)

Ausgänge konnten nur Program oder einen Input zeigen. Der strom-Mischer
rendert die Multiview ohnehin (ein unverbundener Ausgang endet in einem
`fakesink`); lmix gibt `multiview_out` jetzt wie Program über `inter`
heraus, und ein Ausgang kann `{"from": "multiview"}` zeigen, mit
Program-Ton (oder einem Aux-Bus). Console: „Multiview“ als Quelle im
Ausgangs-Dialog.

- Der Mischer-Flow heißt jetzt „lmix mixer 7“: ein laufender alter
  Mischer ohne diesen Ausgang wird beim ersten Start einmal neu gebaut.
- Preview als Quelle bleibt offen: der Mischer-Block hat keinen
  Preview-Ausgang (nur `pgm_out`, `multiview_out`); das bräuchte einen
  strom-Patch.

**Geprüft**: Flow-Tests (Mischer gibt die Multiview heraus, Ausgang liest
sie), VM-Test mit strom: Aufnahme der Multiview, H.264 und AAC.
Nebenbefund: Die VM-Prüfung „pausierter Clip auf Sendung gehalten“ fiel
einmal, als nebenher ein Rust-Build lief; ohne Last grün.

## 2026-10-17 — Programmformat wählbar (OUT.1)

**Kontext**: Program lief fest in 1920 × 1080 bei 30 fps; OUT.1 verlangt
Auflösung von SD bis 4K, Hochkant 1080 × 1920 und Bildraten 23,976–60.

**Entscheidung**:
- `set_master_format {format: {width, height, framerate}}`, gespeichert in
  der Show. Größe frei, gerade, 320–7680 px je Seite (strom nimmt jedes
  `WxH`), Bildrate aus der Liste des strom-Mischers (23,976, 24, 25,
  29,97, 30, 50, 59,94, 60). Halbbilder (50i/59,94i) und 100/120 bietet
  der Mischer nicht.
- Die Engine baut den Mischer nur neu, wenn das Format sich ändert
  (`Command::SetFormat`, erstes Kommando jeder Wiederherstellung); danach
  wird der Mischer wie nach einem Engine-Neustart eingerichtet, laufende
  Ausgänge starten in der neuen Größe neu. Die Quellen laufen weiter (kein
  `SetSlot`), Clips fangen nicht von vorn an. Program steht dabei kurz.
- Startet lmix-core neu, übernimmt es einen laufenden Mischer in seinem
  Format; die Wiederherstellung baut nur neu, wenn die Show ein anderes
  will (kein Bildaussetzer beim Neustart wie bisher).
- Overlay-Rechtecke rechnen mit dem aktuellen Format statt fest 1920 × 1080.
  Standbilder bleiben 1920 × 1080 (der Mischer skaliert).
- Console: „Programmformat…“ im Paket-Menü, Größen als Vorlagen (4K bis
  SD, zwei Hochkant), Hinweis auf die Unterbrechung.

**Geprüft**: Tests (Format im Mischer-Flow und zurückgelesen, Grenzen,
gleiches Format ändert nichts, Quellen bleiben), VM-Test mit strom:
1080 × 1920 bei 25 fps gebaut, Program läuft, zurück auf 1920 × 1080,
Quelle online; Console auf Xvfb.

## 2026-10-17 — Acht Overlay-Kanäle (OVL.1)

**Kontext**: OVL.1 fordert acht Overlay-Kanäle über Program, lmix hatte
vier. Der strom-Mischer erlaubte acht DSK-Ebenen, eine davon braucht der
Stinger (unterste Ebene).

**Entscheidung**:
- `OVERLAY_CHANNELS` = 8. Der Mischer bekommt neun DSK-Ebenen (Stinger +
  8), dafür hebt strom #16 (`lmix-nine-dsk`) die Grenze wie dbaf113 von 8
  auf 9; nur Grenze und Auswahlwerte, keine neue Logik. Mischer-Flow heißt
  jetzt „lmix mixer 8“, ein laufender alter Mischer wird neu gebaut.
- Shows mit vier Kanälen laden weiter: fehlende Kanäle bekommen ihren
  Standard (leer, halbe Sekunde Blende), mehr als acht sind ein Fehler.
- API: Kanal 1–8 in Aktionen, OSC und Zustand (Arrays mit acht Einträgen).
  Die Console richtet sich nach der Länge der Arrays.

**Geprüft**: Tests (alte Show mit vier Kanälen, Kanal 9 abgelehnt, neun
DSK-Ebenen im Mischer), VM-Test mit strom: Kanal 8 über Program an und
aus; Console auf Xvfb.

## 2026-10-17 — Alle Streams zusammen starten (STR.1)

**Entscheidung**: Aktionen `start_outputs` / `stop_outputs` mit einer
Gruppe (`streams` = SRT, `recordings`, `all`) statt eines Schalters.
Start nimmt nur Ausgänge, die nicht laufen; kann einer nicht starten
(Quelle fehlt), starten die anderen trotzdem, der Fehler kommt nur
zurück, wenn keiner startet. Stopp nimmt nur laufende. Console: ab zwei
Streams „Alle Streams starten“ bzw. „… stoppen“ (fragt vorher wie ein
einzelner Ausgang), beides auch als Vorgabe für Kurzbefehle und Auslöser.

**Geprüft**: Test (nur gestoppte starten, Gruppen trennen Streams und
Aufnahmen), VM-Test mit strom: zweiter Stream startet zum laufenden,
Aufnahme bleibt aus, beide stoppen; Console auf Xvfb mit dem Knopf.

## 2026-10-17 — Abspielstand im Zustand (API.4)

**Kontext**: API.4 verlangt Position und Dauer je Input im Zustand.
lmix schickte sie nur als Ereignis `position` (5 × je Sekunde).

**Entscheidung**: `Input.playback` (`state` playing/paused/stopped,
`position_ms`, `duration_ms`, `item` 1-basiert, `items`) aus der letzten
Meldung des Players, eingesetzt beim Lesen des Zustands (`GET /api/state`,
TCP/OSC-Vollzustand). Der gepushte Zustand (WebSocket) ändert sich dadurch
nicht, sonst ginge fünfmal je Sekunde der ganze Zustand hinaus; live
bleibt das Ereignis `position`. Play/Pause kommt aus strom
`MediaPlayerStateChanged` (bisher nicht gelesen); Meldungen älter als 2 s
gelten als weg.

**Geprüft**: Test (Meldung erscheint im Zustand), VM-Test mit strom:
Playlist pausiert → `paused`, 2 Einträge, Dauer > 0; nach Play/Next →
`playing`.

## 2026-10-17 — Schleife oder Stehenbleiben am Ende (INP.10)

**Kontext**: Clips und Playlisten liefen immer in der Schleife.

**Entscheidung**:
- Je Input `at_end`: `loop` (Standard, wie bisher) oder `hold` (bleibt
  auf dem letzten Bild stehen; strom hält es mit dem Standbild-Thread).
  Enum statt Schalter, Aktion `set_at_end`.
- Umschalten live ohne Neustart: strom #17 nimmt `loop_playlist` in die
  Playlist-Anfrage auf (die Eigenschaft ist nicht live), ein gestoppter
  Player fängt nur bei anderen Dateien neu an. Der Slot startet weiter
  mit Schleife; `hold` wird nach jedem Neuaufbau (Engine-Neustart,
  Quelle ersetzt, Slot verschoben) als `SetAtEnd` nachgeschickt.
- Clip-Ende-Auslöser (AUT.7) feuern auch, wenn ein gehaltener Clip
  stoppt (Zustand `stopped` aus strom, seit API.4 gelesen).
- Console: „Am Ende“ im Kachelmenü.

**Geprüft**: Tests (gehaltenes Ende zählt als Ende, zuerst rot;
Aktion, Wiederherstellung), VM-Test mit strom: Playlist bleibt stehen,
Zustand `stopped`, Bild steht, Auslöser feuert; zurück auf Schleife
startet nichts, Neustart spielt wieder.

## 2026-10-17 — Sieben Aux-Busse A–G (AUD.2)

**Kontext**: AUD.2 fordert neben dem Master die Busse A–G; lmix hatte
zwei Aux-Busse. Der strom-Audiomischer kann bis 32.

**Entscheidung**:
- `AUX_BUSES` = 7, in Aktionen 1–7, in der Console A–G. A nimmt die Sends
  vor den Fadern (Monitormix, wie bisher Bus 1), B–G danach. Vor/nach ist
  weiter fest je Bus; wählbar wäre es in strom (`chN_auxM_pre`), gefordert
  ist es nicht.
- Shows mit zwei Bussen laden weiter (fehlende Busse und Sends mit
  Standard), wie bei den Overlay-Kanälen. Mischer-Flow „lmix mixer 9“.
- Offen: Bus optional in den Master (strom kann Aux nicht in den Main
  routen), Ausgang eines Busses auf ein Gerät ohne Ausgang in lmix.
- Console: Bus-Dialog scrollt (sieben Busse sind höher als kleine
  Fenster), Kanalzug und Ton-Auswahl eines Ausgangs mit A–G.

**Geprüft**: Tests (alte Show mit zwei Bussen, Bus 8 abgelehnt), VM-Test
mit strom: G still ohne Send, hörbar mit Send; Console auf Xvfb mit
geöffnetem Bus-Dialog.

## 2026-10-17 — Codec, Rate Control, Keyframes je Ausgang (STR.3)

**Entscheidung**: SRT-Ausgänge und Aufnahmen bekommen `encoding`:
`codec` (`h264` Standard, `h265`), `rate` (`vbr` Standard, `cbr`),
`keyframe_ms` (Standard 2000, 0 = Encoder entscheidet, höchstens 10 s).
Millisekunden statt Frames, weil sie beim Wechsel des Programmformats
(OUT.1) gleich bleiben; die Engine rechnet mit der Bildrate um. Ohne
Angabe bleibt alles wie bisher (Show-Dateien unverändert).

AV1 fehlt: der strom-Recorder (nur `h264parse`/`h265parse`) und
`mpegtsmux` von GStreamer 1.24 nehmen es nicht. Auflösung je Ausgang,
Profil/Preset, Ton-Bitrate und geteilte Profile bleiben offen.

**Geprüft**: Tests (Encoder-Eigenschaften im Flow, 2 s bei 25 und 59,94
fps, Grenze 10 s), VM-Test mit strom: H.265-Aufnahme mit CBR ist HEVC +
AAC; Console-Dialog „Ausgang hinzufügen“ mit Codec, Bitratenart,
Keyframe-Abstand.

## 2026-10-17 — Ton folgt der T-Bar (SWI.5)

**Entscheidung**: Inputs mit „Ton folgt Bild“ blenden mit der T-Bar:
Program mit `1 − p`, Vorschau mit `p` (linear in der Verstärkung, wie
das Bild), zurück auf 0 wie vorher, bei 1 der Take. Stummgeschaltete
Kanäle behalten ihren eigenen Pegel. Inputs ohne „folgt Bild“ bleiben,
wie sie sind.

**Geprüft**: Test (zuerst rot: bei halber T-Bar kam kein Ton-Befehl),
VM-Test mit strom: Ball auf Program bei halber T-Bar ≈ −6 dB, zurück
auf 0 wieder voll.

## 2026-10-17 — Multiview-Layout (OUT.10)

**Entscheidung**: Die Show hat `multiview`: `program` (`left`/`right`,
Standard rechts wie strom) und `meters` (`shown`/`hidden`). Beides sind
strom-Eigenschaften des Mischers, die nur beim Bau gelten
(`swap_pvw_pgm`, `show_vu_meters`); deshalb trägt `Command::SetFormat`
Format und Layout gemeinsam, die Engine baut nur neu, wenn eins davon
sich ändert, und übernimmt beim Neustart von lmix-core einen laufenden
Mischer samt Layout. Kachelzahl (2/8/14), Reihenfolge, Titelposition und
eine zweite Multiview kann der strom-Mischer nicht.

**Geprüft**: Tests (Layout im Flow und zurückgelesen, Neubau ohne
Quellen-Neustart, gleiches Layout ändert nichts), VM-Test mit strom:
Program links ohne Pegel und zurück, Quellen bleiben.

## 2026-10-17 — Layout-Box ein- und ausblenden (LAY.2)

**Entscheidung**: Eine Box hat `hidden`. Ausgeblendet bleibt sie mit
Platz, Zuschnitt und Rahmen im Layout, die Engine zeichnet sie nicht,
ihr Input bekommt kein Tally (nicht auf Sendung). `show_box {layout,
number, shown: shown|hidden|toggle}` schaltet eine Box für Tasten und
Kurzbefehle; der Layout-Editor hat je Box „Sichtbar“ und zeigt
ausgeblendete Boxen blass. Vorlagen (LAY.5) merken sich das nicht.

**Geprüft**: Test (Box aus dem Engine-Befehl, Tally weg, Umschalten,
Box 2 gibt es nicht), VM-Test mit strom: ausgeblendet scheint der
Hintergrund durch, Ball ohne Tally, wieder eingeblendet.

## 2026-10-17 — Neue Show mit Formatwahl (SHW.3)

**Entscheidung**: `new_show` nimmt ein optionales `format` (wie
`set_master_format`, geprüft); ohne bleibt es 1920 × 1080 bei 30 fps.
Ältere Aufrufe ohne Feld gehen unverändert. Der Mischer wird nur neu
gebaut, wenn das Format sich ändert (`SetFormat` der Wiederherstellung).
Console: „Neu“ fragt im Format-Dialog nach Größe und Bildrate, mit dem
Format der offenen Show vorgewählt.

**Geprüft**: Test (neue Show im Format, ungültiges abgelehnt, ohne Feld
der Standard), Console auf Xvfb, Headless-Test.

## 2026-10-17 — RTMP-Ausgang (STR.2, B31) und strom-Upstream

**Kontext**: Eyevinn/strom `main` hat seit unserem Abzweig (14.09.) 177
Commits, darunter `builtin.rtmp_output` (#827). Ein voller Abgleich
ergab 22 Konfliktdateien, vor allem in den GPU-Pfaden: upstream hat
`gl_bridge.rs` durch `video_adapt.rs`/`gl_input_front.rs` ersetzt, auf
denen unsere CUDA-/GL-Mischer-Patches aufsetzen. Ohne GPU in der VM
nicht prüfbar, daher nicht allein entschieden.

**Entscheidung**: Nur den RTMP-Block übernommen (cherry-pick #827, #869,
#871 auf den lmix-Stapel, strom #18 `lmix-rtmp-output`), dazu #850: der
VM-Test zeigte, dass x264 H.264 High 4:4:4 lieferte, das der RTMP-Block
und die Plattformen ablehnen (betraf mit Software-Encoder auch SRT und
Aufnahmen); seitdem 8-bit 4:2:0. Der RTMP-Sink läuft mit `sync=false`,
weil Ausgangs-Flows auf der Laufzeit des Mischers laufen: mit `sync`
kam beim Empfänger nur der FLV-Kopf an. lmix:
Ausgang `rtmp` mit `url` (rtmp:// oder rtmps://) und `key` getrennt,
Encoder-Einstellungen wie SRT, nur H.264 (FLV); zählt als Stream
(STR.1). Der Schlüssel steht wie die URL im Zustand und in der Show;
die Console zeigt ihn verdeckt.

**Upstream-Abgleich** (offen, mit Abnahme auf der RTX 3090): neben RTMP
u. a. HTML-Input-Block (#879, Weg zu GFX.2), Player für Live-Streams und
URLs (#945), Abtastrate wählbar (#946, AUD.17), Stinger-Übergänge
(#1010), begrenztes `set_state(NULL)` beim Stoppen (#874), Player-
Deadlock-Fix (#986).

**Geprüft**: strom-Tests `rtmp_output_test`, `rtmp_output_pipeline_test`,
`output_refusal_test` (51 grün); lmix-Tests (Flow, Ablehnung von
Nicht-RTMP-Adressen und H.265); VM-Test: ffmpeg als RTMP-Server bekommt
H.264 + AAC.

## 2026-10-17 — Abtastrate 44,1 kHz wählbar (AUD.17)

**Entscheidung**: Die Show hat `sample_rate` (48000 Standard, 44100),
Aktion `set_sample_rate`. Der Audiomischer legt seine Busse darauf fest
(strom-Eigenschaft `sample_rate`, lmix-Patch ed647f2); die Slots
liefern weiter 48 kHz, die Kanalzüge rechnen um. Wie Format und
Multiview gilt die Rate beim Bau des Mischers: `SetFormat` trägt sie
mit, die Engine baut nur bei Änderung neu und übernimmt beim Neustart
von lmix-core die Rate eines laufenden Mischers. Ausgänge erben sie
(AAC bei 44,1 kHz).

**Geprüft**: Tests (Rate im Flow und zurückgelesen, Neubau ohne
Quellen-Neustart, 96 kHz abgelehnt), VM-Test mit strom: Mischer bei
44,1 kHz, Program hörbar, zurück auf 48 kHz.

## 2026-10-17 — strom mit ganzem upstream main (0.7.1)

**Entscheidung**: Der Fork übernimmt upstream Eyevinn/strom `main`
(8fce2ee) als Merge auf die lmix-Patches (strom-PR „lmix-upstream-sync").
Wo upstream jetzt löst, was ein Patch tat, gilt upstream:

- Abtastrate des Audiomischers: upstreams `sample_rate` (#946) ersetzt
  ed647f2. Gleicher Name, gleiche Werte (48000, 44100), AUD.17 läuft
  unverändert.
- CUDA-Bilder in den GL-Mischer: upstreams `gl_input_front` ersetzt
  unseren `gl_bridge`. VA-Bilder (AMD/Intel) werden dort nicht mehr
  überbrückt; ungetestete Hardware vorher wie nachher. Der CPU-Mischer
  behält das Herunterladen als kleines Modul `memory_bridge`.
- Rahmen-Unterlagen schieben ein Bild und dann EOS (upstream); bei
  Startzeit „first" ist die Quelle live, damit das Bild Laufzeit trägt.

Behalten und angepasst: Startzeit „first" für beide Mischer (upstreams
neuer Schutz gegen das Zurückspringen beim ersten Puffer schaltet sie
sonst ab), Standbild des pausierten Players (erste Videospur; nicht für
Stinger-Quellen), Systemspeicher und Loop-Steuerung des Players, DSK-
Blenden, CUDA-Compositor (dort keine Software-Alpha-Mischung),
GPU-Deinterlace und -Thumbnails, geteilter CUDA-Kontext, NVENC-Wege,
udmabuf-Umgehung, `--host`.

**lmix folgt**: Übergänge nennen die Eingänge als Option (Stinger-Clip
leer). Der Player hält jeden Puffer eine Ausspielverzögerung lang
(Standard 500 ms gegen Netz-Aussetzer); bei lokalen Dateien ist das nur
die Zeit vom Cue bis zum Bild, deshalb 100 ms für Datei und Playlist,
URL-Streams behalten den Standard.

**Neu verfügbar, nicht genutzt**: HTML-Input (#879, Weg zu GFX.2),
Stinger-Übergänge in strom (#1010; lmix hat eine eigene Stinger-Ebene),
Live-Recorder (#1034), Multiview-Tally (#1009), Eingangsnamen zur
Laufzeit (#1014).

**Geprüft**: strom: 967 Unit-Tests, Integrationstests (OpenAPI,
Pipeline-Lebenszyklus, Mediaplayer, Mischer-Abtastrate, Vision Mixer
CPU/CUDA-Eingang/Formate/Unterlagen, Stinger, RTMP, Ablehnungen),
clippy `-D warnings` (no-gui und efp). lmix-core 297 Tests, VM-Test mit
strom. **Offen**: GPU-Abnahme auf der RTX 3090 (CUDA-Mischer,
GL-Mischer, NVDEC/NVENC, Thumbnails).
## 2026-10-16 — S-E.3: Klang-Cues intern, gemessen

**Kontext**: S-E.3 zurückgestellt (User), vor S1 Stufe 2 mit Zahlen neu
vorlegen. Gefragt: Klang-Cue als verdeckter strom-Input (A, wie ein
Clip-Input heute: eigener Flow, Ton über `inter` in den Audio-Mischer)
oder als Zweig eines Abspielers je Zone (B, ein Flow je Zone, ein
`audiomixer`, ein Zweig je laufendem Klang).

**Messung** (`scripts/bench-sound-cues/run.sh`, reines GStreamer 1.28,
ohne strom und Gerät, 24 Kerne, GPU belegt, nicht genutzt; 30-s-FLAC,
Stereo):

| | A: Pipeline je Klang + interaudio | B: Zweig je Klang |
|---|---|---|
| Anfrage → erstes hörbares Sample am Zonen-Ausgang | 51–102 ms (kalt 192) | 31–32 ms |
| Speicher je Klang | ~1 MB | ~0,6 MB |
| Threads je Klang | 3 | 2 |
| CPU bei 10 Klängen | 2,2–3,3 % | 3,8–4,4 % |

A schwankt mit der Periode der Inter-Quelle. In lmix kommt dazu: der
Slot-Ton läuft über strom-`inter` mit bis zu 1000 ms Puffer
(`AUDIO_INTER_BUFFER_MS`, für die Kanalverzögerung), jeder Klang belegt
einen der Mischer-Slots (Standard 16, höchstens 32, beim Start
fest) und der Flow entsteht über REST.
SND.12 verlangt GO → hörbar ≤ 50 ms, vor der Latenz des Geräts.

**Folgerung (Vorschlag)**: B. Nur B hält SND.12; Speicher und CPU sind
in beiden Fällen klein. Preis: ein neuer strom-Block „Zonen-Abspieler“
(Zweige zur Laufzeit an- und abhängen, Pegel-Fade je Zweig), als eigener,
upstream-tauglicher Commit; die Zone selbst (Gerät, Kanäle, SND.10) ist
ohnehin neu. A bliebe nur für Videoton (VIS.10, S2), der schon ein Input ist.
**Entscheidung** (User 2026-10-17): B, Abspieler je Zone, im strom-Block „Show-Wiedergabe“ zusammen mit den Bühnen (S-E.13).

**Bauplan, damit die Folgen greifbar sind** (Schätzung, nicht gebaut):

*B, Abspieler je Zone* (Vorschlag):
- strom: ein neuer Block „Show-Ton“ (eigener, upstream-tauglicher Commit)
  in einem Flow je Rechner. Je Zone ein `audiomixer` → Pegel (`volume`,
  SND.11) → Begrenzer (SND.14) → Ausgang je Gerät mit Kanalzuordnung
  (SND.10). Klänge kommen zur Laufzeit dazu: je Klang eine eigene
  Dekodier-Pipeline wie im `mediaplayer`-Block (Datei → `appsink` →
  `appsrc` im Flow), dann Kreuzungspunkte Datei-Kanal → Zone mit Pegel wie
  im `liveaudiorouter` (`volume` je Kreuzung, Rampen klickfrei; SND.5,
  SND.6, SND.8 mit S-Kurve). Vorladen vor GO (pausiert, dekodiert) hält
  SND.12. Alle Zonen in einem Flow teilen eine Uhr; Geräte gleichen über
  die Resampling-Nachführung der Sinks ab (SND.15, im Selftest messen).
- lmix-core: Cue-Arten `sound` (Datei, Zonen-Zuordnung, Pegel,
  Wiederholungen, Ein-/Ausblendung) und `fade` (Ziel Cue oder Zone,
  Parameter, Dauer, Kurve, „danach stoppen“); Dauer aus der Datei;
  Panik blendet über die Panikzeit aus (SAF.1/7).
- Console: Zonen-Leiste mit Pegeln (SUI.8).
- Aufwand grob: strom-Block 2–3 Tage, lmix-core 2 Tage, Console 1 Tag.

*A, verdeckte Inputs*:
- Jeder Klang belegt einen Mischer-Slot (16 Standard, geteilt mit
  Kameras), sein Ton läuft über die Inter-Strecke mit bis zu 1000 ms
  Puffer in den Audio-Mischer. Zonen wären Busse des Mischers (heute zwei
  Aux-Busse), Geräte pro Zone bräuchten eigene Ausgänge des Mischers;
  das widerspricht S-E.4 (Zone mit Rechner-Zuordnung, nicht Aux-Bus).
- Weniger neuer strom-Code, aber SND.12 (≤ 50 ms) verfehlt, Slots werden
  knapp, Zonen über Busse sind ein Umweg.

## 2026-10-17 — Entscheidungen des Users nach dem strom-Abgleich

Abgefragt über eine Entscheidungsseite (Optionen mit Vorschlag).

- **S-E.3 und S-E.13**: beide B, ein strom-Block „Show-Wiedergabe“ mit
  Zonen (`audiomixer`) und Bühnen (`compositor`) in einem Flow je Rechner.
- **Merge**: alles sofort, auch der Upstream-Abgleich (strom #15–#20 auf
  `dev`, lmix #38–#90, #39, #45 auf `main`).
- **GPU-Abnahme** auf der RTX 3090: später, mit der nächsten
  Hardware-Runde (HARDWARE-TODO).
- **GFX.2** auf upstreams HTML-Input bauen; was gegenüber dem eigenen Weg
  fehlt, wird ergänzt.
- **Stinger**: eigene Ebene gegen upstreams Stinger vergleichen (Matte,
  Timing, Latenz), dann neu vorlegen.
- **Live-Recorder** (#1034): prüfen, ob er MultiCorder verbessert.
- **Player-Verzögerung** 100 ms für lokale Dateien: bestätigt.
- **AMD/Intel (VA)** weiter unterstützen: die Brücke für VA-Bilder in den
  GL-Mischer konzeptionell wieder anlegen, Test auf Hardware später.

## 2026-10-17 — GFX.2 auf upstreams HTML-Input

**Kontext**: User-Entscheidung: GFX.2 auf strom `builtin.html_input`
(upstream #879, Chromium über `cefsrc`) bauen und ergänzen, was dem
eigenen Weg gegenüber fehlt. Der eigene Weg war seit M7 Teil A die
Grafikseite von lmix-core (`/graphics/<n>`), für die nur „URL als
Input" in strom fehlte.

**Entscheidung**: zwei Quellen.
- `web` (GFX.6): beliebige http(s)-Seite mit Ton (`audio_video`),
  `strict_network` an (keine Seiten aus dem eigenen Netz).
- `graphics` (GFX.2, GFX.17): der Grafikkanal als Input, Nummer, Name
  oder ID. Die Engine rendert `http://<lmix-core>/graphics/<kanal>`
  transparent und ohne Ton; `strict_network` aus, denn die Seite kommt
  von diesem Rechner. lmix-core gibt strom seine Adresse aus `--listen`
  (Platzhalter-Bind 0.0.0.0 wird zu 127.0.0.1). Als Overlay-Kanal
  gekeyt wie jedes Input mit Alpha.
Beide rendern in Program-Größe und -Rate (29,97 → 30, Chromium rendert
ganze Bilder je Sekunde), je in einem eigenen Slot-Flow: eine hängende
Seite nimmt Program nicht mit. Unbekannte Kanäle und Adressen ohne
http(s) werden beim Anlegen abgelehnt (`bad_web_url`,
`unknown_graphics`).

**Vergleich: was upstreams Block gegenüber dem Lastenheft nicht kann**:

| Anforderung | upstream `html_input` | Stand |
|---|---|---|
| Alpha in Ausgabegröße und -rate (GFX.2) | BGRA, Größe/Rate als Eigenschaften | genutzt |
| ohne Pipeline-Neustart (GFX.2) | `url` live | genutzt; Inhalte wechseln über die Grafikseite |
| hängender Titel nimmt Program nicht mit (GFX.2) | – | eigener Flow je Slot |
| Rendern pausieren/fortsetzen (GFX.2) | fehlt | ergänzt: strom #22 (`freeze`/`resume`), Aktion `set_page_running` |
| Ton (GFX.6) | `stream_mode` | genutzt |
| eigenes CSS (GFX.6) | fehlt | ergänzt: strom #22 (`set_css`), Quelle `web` mit `css`, die Engine setzt es, sobald die Seite antwortet |
| JS-Funktion der Seite aufrufen (GFX.6) | fehlt (Fernsteuerung nur als DevTools-Link) | ergänzt: strom #22 (`evaluate`), Aktion `page_call` |
| H.264/AAC in Seiten (GFX.20) | hängt am CEF-Build von strom | ungeprüft |
| 8 Titel 1080p60 (GFX.15) | – | mit GPU messen |
| neue Program-Größe | Größe beim Bau | Seiten-Slots werden bei neuem Format neu gebaut |

**Nicht geprüft**: Ein echter Lauf mit Chromium. In der VM gibt es kein
`cefsrc`; ein Plugin aus dem strom-full-Image auszuführen hat die
Sicherheitsprüfung der Sitzung abgelehnt (fremder Code). Geprüft sind die
Flows (Größe, Rate, Ton, Netz, Adresse) und die Ablehnungen in Tests.
Abnahme mit strom-full oder auf der RTX 3090 (HARDWARE-TODO H.36).


## 2026-10-17 — Live-Recorder für MultiCorder (Prüfung, Vorschlag)

**Kontext**: User: prüfen, ob upstreams Live-Recorder (#1034,
`builtin.liverecorder`, fragmentiertes MP4) MultiCorder verbessert.
Heute nimmt lmix Program und ISO-Spuren mit `builtin.recorder` auf
(`splitmuxsink` + `mp4mux`).

**Messung** (`scripts/bench-recorders/run.sh`, VM ohne GPU, H.264
640×360@25, 7 s, dann strom mit `kill -9`):

| | Recorder | Live-Recorder |
|---|---|---|
| während der Aufnahme lesbar | ja, 6,0 s (151 Bilder) | ja, 7,0 s (175 Bilder) |
| nach `kill -9` lesbar | ja, 6,0 s | ja, 7,0 s |
| Dateigröße nach 7 s | 47,6 MB (Index-Platz vorab reserviert) | 130 KB |

Beide überstehen einen Absturz; der Live-Recorder ist eine Sekunde näher
an „jetzt" und belegt keinen reservierten Platz. Laut upstream hält eine
stumme Spur die anderen nicht auf (GAP-Ereignisse) und kommt in derselben
Datei zurück; der Recorder schreibt eine Spur, die nie Daten bekam, gar
nicht erst an. Dazu teilt er nach Zeit oder Größe und stoppt auf Wunsch
selbst. Upstream nennt ihn „experimentell"; fragmentiertes MP4 öffnen
gängige Schnittprogramme, ältere Werkzeuge nicht immer.

**Vorschlag**: ISO-Spuren (MultiCorder, H24-Vergleich) auf den
Live-Recorder umstellen, Program-Aufnahme vorerst beim Recorder lassen;
nach einer langen Aufnahme (H.24) über beides entscheiden.
**Offen**: Entscheidung des Users.

## 2026-10-17 — Stinger: eigene Ebene gegen upstream (Vergleich, Vorschlag)

**Kontext**: User: eigene Stinger-Ebene (seit M4, DECISIONS 09.10.)
gegen upstreams Stinger-Übergänge (#1010) vergleichen (Matte, Timing,
Latenz), dann neu vorlegen.

| | lmix-Ebene | upstream |
|---|---|---|
| Timing | lmix-core zählt vom Start des Clips (Warte-Befehle, HTTP-Aufrufe); seit dem Abgleich wartet die Engine die Ausspielverzögerung des Clips ab, sonst kam der Cut 100 ms zu früh | bildgenau: der erste Frame wartet geparkt, jeder Schritt ist ein Keyframe auf einen Clip-Zeitstempel; verspätete Cuts werden gemeldet |
| Track-Matte, Nur-Maske | nein | ja auf GL (Mischmodus über Blendfunktionen); CPU spielt klassisch; auf unserem CUDA-Mischer klassisch, ungeprüft |
| Cut-Punkt | von Hand | von Hand oder aus der Clip-Analyse (wo die Grafik deckt) |
| Clips | ein Clip-Input je Taste (4 Tasten), beliebige Player-Quelle | Clip-Liste einer Stinger-Quelle je Mischer, Clip je Übergang wählbar |
| Ziel | Input oder Layout (PiP) | nur Inputs: PGM-Input zu PVW-Input |
| Lage | unter den Overlays | Grafik über dem Programm, unter den DSK-Ebenen (korrigiert 18.10.) |
| Alpha | ProRes 4444, VP9 mit Alpha (auf Hardware geprüft, H.27) | dieselben Formate, Analyse erkennt Alpha |
| Ton | Ton des Clip-Inputs nach seinen Einstellungen | Ton des Clips über die Stinger-Quelle |
| Prüfung | VM-Test (Clip mit Alpha, Cut darunter) | eigene strom-Tests, `stinger_test` grün in der VM |

**Folgerung**: upstream ist bildgenau und kann Track-Matte, aber nur
zwischen Inputs und über den Overlays. Ganz umstellen hieße, Layouts als
Stinger-Ziel und „unter den Overlays" aufzugeben oder in strom
nachzubauen.
**Vorschlag**: beides anbieten: Stinger mit Matte oder Clip-Analyse
laufen über upstream (nur Inputs, GL), alles andere bleibt bei der
eigenen Ebene; die Taste wählt den Weg nach Clip und Ziel. Zuerst im
VM-Test messen, wie weit der eigene Cut vom geplanten Bild abweicht.
**Offen**: Entscheidung des Users.

## 2026-10-17 — Klang-Cues gebaut (S1 Stufe 2, S-E.3 B)

**Kontext**: S-E.3 B entschieden (User 17.10.): Klang über einen strom-
Block mit einem `audiomixer` je Zone, ein Flow je Rechner.

**Gebaut**:
- strom (#23, #24, #25 auf `dev`): Block `builtin.show_sound`. Je Zone ein
  Live-`audiomixer` → Pegel → Ausgang `zone_out_N`; je Klang ein
  Dekodierzweig zur Laufzeit, Kanalzuordnung über `mix-matrix`, S-Kurven
  über eine Control-Binding; REST unter `…/blocks/{id}/show_sound`. Bis
  16 Zonen (wie lmix); Mono ohne Zuordnung auf alle Kanäle der ersten
  Zone. Ein Klang endet erst, wenn sein Ende an den Zonen-Mischern
  ankommt (vorher am Dekoder: eine Datei von 1 s war nicht zu hören).
- lmix-types: Cue-Arten `sound` (`SoundCue`: Datei, Pegel, Zuordnung
  Dateikanal → Zone/Kanal mit Pegel, Ein- und Ausblendzeit) und `fade`
  (`FadeCue`: Ziel Cue oder Zone, Pegel, danach behalten/stoppen, Dauer
  = `duration_ms`). Aktionen `play_sound`, `fade_sound`, `stop_sound`,
  `set_zone_level` (Gruppe „sound“, Recht `control`).
- lmix-core: ein Flow „lmix sound“ aus den Zonen der Show und den
  Ausgängen dieses Rechners (`ZonePatch`: Gerät, Kanäle, Pegel). Je
  Ausgang `audioconvert` (Zonen- → Gerätekanäle) → `volume` →
  `pulsesink device=…` (leer: `autoaudiosink`); eine Zone ohne Ausgang
  läuft in einen `fakesink`. Geänderte Zonen oder Patches bauen den Flow
  neu (laufende Klänge enden dabei). Der Cue-Runner spielt den Klang mit
  der Cue-ID; ohne `duration_ms` endet der Cue mit der Datei (strom
  meldet das Ende nicht selbst, lmix-core fragt alle 100 ms, solange
  etwas spielt). Stopp blendet mit der Ausblendzeit des Cues aus, ein
  Cue mit „andere stoppen“ mit seiner Zeit, Panik alle Klänge über die
  Panikzeit, harter Stopp und Reset sofort.

**Geprüft**: Einheitstests (Runner, Prüfung, Flow-Aufbau, Matrix) und
`scripts/test-sound-cues.sh` gegen echtes strom (Zone → Flow läuft;
1-s-Cue endet mit der Datei und `after_end` folgt; Stopp mit Fade; Fade
auf die Zone; Panik; ohne Zonen kein Flow). Hörbar geprüft nur in
strom (`show_sound_test.rs`, Pegel am Zonenausgang); die VM hat keine
Soundkarte.

**Offen**: Vorladen vor GO (SND.12: gemessen 31–32 ms ohne Vorladen,
also im Ziel; strom kann `preload`), Wiederholungen (SND.3), Begrenzer
(SND.14), Geräteausfall zur Laufzeit (SND.16), Zonen-Leiste in der
Console (SUI.8), Geräteliste der Ausgänge, Klang-Cues im Cue-Editor der
Console (bisher nur Anzeige in der Liste). Gerätezuordnung mit
`pulsesink` braucht einen Test auf echter Hardware (HARDWARE-TODO).

## 2026-10-17 — Bühnen und Bild-Cues gebaut (S1 Stufe 3, S-E.13 B)

**Kontext**: S-E.13 B entschieden (User 17.10.): Bild-Cues als Zweige im
selben strom-Block wie die Zonen („Show-Wiedergabe“).

**Gebaut**:
- strom (#26 auf `dev`): Der Block heißt jetzt `builtin.show_playback`
  (vorher `show_sound`), REST unter `…/show_playback` mit Klängen, Zonen,
  Bildern und Bühnen. Je Bühne ein `compositor` auf schwarzem Grund mit
  einer live laufenden Schwarzquelle, die zugleich der Dimmer ist
  (oberste Ebene, Deckkraft = 1 − Helligkeit); Ausgang `stage_out_N`.
  Ein Bild ist ein Zweig Datei → Dekoder [→ Beschnitt für „füllen“]
  [→ `imagefreeze` für Standbilder] → Pad mit Ebene, Deckkraft (S-Kurve)
  und Bühnengröße; ein Video hält am Ende sein letztes Bild oder geht.
  Vorladen wie beim Klang. CPU-`compositor`; GL/CUDA wie beim Mischer
  kommt später.
- lmix-types: `Stage` (Name, Größe) in der Show, `StagePatch` je Rechner
  (Bildschirme: Fenster, Vollbild, DRM wie bei den Display-Ausgängen),
  Cue-Art `picture` (`PictureCue`: Datei, Bühne, Ebene 0–999, Deckkraft,
  Füllmodus einpassen/füllen/strecken, Ende halten/stoppen, Ein- und
  Ausblendzeit). Fade-Cues setzen nur, was angegeben ist: `gain_db` für
  Klang-Cue oder Zone, `opacity` für Bild-Cue, `brightness` für eine
  Bühne (Ziel `stage`). Aktionen `set_stage`, `remove_stage`,
  `set_stage_patch`, `remove_stage_patch`, `show_picture`,
  `fade_picture`, `stop_picture`, `set_stage_level`.
- lmix-core: ein Flow „lmix playback“ für Zonen und Bühnen zusammen
  (vorher „lmix sound“); jede Bühne auf ihre Bildschirme (mehrere über
  `tee`), ohne Bildschirm in einen `fakesink`. Ein Bild-Cue ohne Dauer
  läuft bis zum Stopp (Standbild, gehaltenes Video) oder bis zum Ende
  des Videos (`end: stop`); ein Fade mit „danach stoppen“ beendet ihn.
  Stopp, „andere stoppen“, Panik und harter Stopp wirken auf Bilder wie
  auf Klänge. Ein neues Programmformat baut den Flow nur neu, wenn sich
  die Bildrate ändert (die Bühnen laufen mit ihr).

**Geprüft**: strom-Tests am Bühnenausgang (schwarz ohne Bilder,
vorgeladen sichtbar innerhalb von 200 ms nach `play`, einpassen mit
Rändern, füllen/strecken ohne, Dimmer, Fade halb durch, Video hält oder
geht, Pipeline wird freigegeben); lmix-Tests (Runner, Prüfung,
Flow-Aufbau); `scripts/test-show-playback.sh` gegen echtes strom mit
der Bühne in einem Fenster auf Xvfb.

**Offen**: Mischmodi (VIS.6, braucht eigenes Blending), Bühnen auf der
GPU, freie Geometrie (VIS.9), Video-Wiederholungen, Ton aus Videos in
Zonen (VIS.10), Bühnen-Dialog und Vorschau in der Console (SUI.7),
Vollbild und DRM auf echten Bildschirmen (HARDWARE-TODO H.38).

## 2026-10-17 — Standby-Cues vorgeladen (SND.12)

**Kontext**: SND.12 verlangt GO → hörbar ≤ 50 ms. Gemessen ohne Vorladen
31–32 ms für einen Klang in reinem GStreamer (S-E.3), für Bilder 53–91 ms,
vorgeladen 28–64 ms (S-E.13); dazu kommt in lmix der Weg über REST und
das Dekodieren komprimierter Dateien.

**Entscheidung**: Der Standby-Cue jeder Liste wird vorgeladen, wenn er
einen Klang spielt oder ein Bild zeigt. Der Switcher bestimmt die Menge
(`SetPreload`, nur bei Änderung), die Engine gleicht sie mit strom ab:
neu → `start: preload` (dekodiert, am ersten Puffer gehalten), nicht mehr
Standby → entfernt, geändert → neu geladen. GO auf einen vorgeladenen
Cue gibt ihn nur frei (`…/play`); ohne Vorladen legt GO ihn wie bisher
neu an. Nach einem GO läuft erst dessen Aktion, dann die neue Menge, so
bleibt der gestartete Cue unberührt. Ein neuer Playback-Flow lädt neu.

Nicht vorgeladen: Cues in Gruppen und Cues, die über einen Start-Cue
laufen; sie starten wie vorher.

**Geprüft**: `scripts/test-show-playback.sh` (Standby-Klang und -Bild in
strom „ready“, nach GO „playing“, jedes nur einmal angelegt); Einheits-
test der Menge im Switcher.

## 2026-10-17 — Wiederholungen lückenlos (SND.3)

**Kontext**: SND.3 verlangt n-mal oder endlos, ohne Pause und ohne Klick
an der Naht.

**Entscheidung**: Segment-Seeks in strom (#27). Der erste Seek (mit
Flush) kommt, solange der erste Puffer am Gate wartet; danach folgt an
jedem `SEGMENT_DONE` ein Seek ohne Flush, dessen Segment die laufende
Zeit fortsetzt. Das nächste Abspielen beginnt so genau am Ende des
letzten. Das letzte Abspielen endet mit EOS, und der Cue endet mit ihm
(`after_end` folgt). Der Cue-Pegel sitzt dafür jetzt an den Pads der
Zonen-Mischer (laufende Zeit der Zone); die Zeit des Zweigs beginnt bei
jeder Wiederholung neu, ein Fade über die Naht liefe sonst falsch.
Verworfen: zwei Dekoder im Wechsel (doppelte Last, Genauigkeit hängt an
der gemeldeten Dauer) und Neuanlegen am Ende (Lücke).

In lmix: `sound.repeat` = `"once"` (Standard), `{"times": n}` (1–10 000)
oder `"forever"`.

**Geprüft**: strom-Tests (3 × 1 s ohne einen stillen 10-ms-Puffer,
endlos mit Fade über die Naht, vorgeladen und wiederholt);
`scripts/test-show-playback.sh` (ein zweifach gespielter 1-s-Cue endet
nach dem zweiten Durchgang).

Nachtrag (17.10.): Videos wiederholen sich ebenso (VIS.2, strom #28):
`picture.repeat` wie beim Klang; `end` (halten/stoppen) gilt nach dem
letzten Durchgang, ein Standbild übergeht `repeat`. Die Nahtlogik ist in
strom ein eigenes Modul für Klang und Bild.

## 2026-10-18 — Jeder PR wird von einem neuen Claude-Agenten geprüft

**Kontext**: Der Eigentümer liest keinen Code. PR-Agent scheitert seit
Tagen am Kostenlimit seines Schlüssels; PRs gingen allein mit grüner CI
hinein. Der Eigentümer will, dass jeder PR von einem neuen Claude-Agenten
geprüft wird (18.10.2026).

**Entscheidung**: Vor jedem Merge startet die schreibende Sitzung einen
neuen Agenten ohne ihren Kontext. Er bekommt nur Absicht, Diff,
Repository und `REVIEW.md`, belegt jeden Befund mit Datei, Zeile und
Fehlerfall und stuft ihn als **Wichtig**, **Kleinigkeit** oder
**Bestand** ein. Der Bericht steht unverändert als Kommentar am PR.
Gemergt wird erst, wenn kein **Wichtig** mehr offen ist. Eine Antwort
statt einer Behebung zählt nur, wenn der nächste Agent sie annimmt.
Folgerunden prüfen nur die neuen Commits, nach drei Runden mit
**Wichtig** wird der Ansatz mit dem Eigentümer neu bedacht. Änderungen
an CI, Release, Geheimnissen und an dieser Regel bestätigt der
Eigentümer vor dem Merge in Worten. Vorbild ist Anthropics Beitrag „Solving code review
at Anthropic“: frischer Kontext, geprüfte Befunde, Stufen, `REVIEW.md`,
Rundenbegrenzung.

**Verworfen**: Code Review for Claude Code (die GitHub-App). PRs liegen
auf Gitea, GitHub ist nur Spiegel ohne PRs. Ein Gitea-Workflow mit
Claude Code im Kopflosmodus wäre von der schreibenden Sitzung
unabhängig, braucht aber einen eigenen API-Schlüssel mit Kosten je
Prüfung. Er bleibt als Ausbau, wenn der Eigentümer einen Schlüssel
hinterlegt.

## 2026-10-18 — Entscheidungsseite 18.10. beantwortet

Abgefragt über https://claude.ai/artifact/E2RiT9QzpfU5uhAnRhBLtV (Optionen
mit Vorschlag).

**Entscheidungen** (User):
- **REV.2**: Bei der Review-Regel vom 18.10. bleiben (neuer Agent aus der
  schreibenden Sitzung). Kein Gitea-Workflow mit eigenem Schlüssel.
- **REV.R**: Die Regel steht in Kaiwa (G-11) und in jedem Repo mit
  `agent.md` (Kaiwa, Kante, andon, kurrent, plasmai, shrippen.github.io);
  die übrigen bekommen sie mit dem nächsten PR. Der gemeinsame Block nennt
  dabei: CI grün auf dem neuesten Commit des PR, PR-Agents Status zählt
  nicht, „was veröffentlicht wird“ heißt Release, Tag, neue öffentliche
  Seite oder Paket (ein Neubau des schon Veröffentlichten zählt nicht).
  lmix behält seine Fassung nach den CI-Pfaden.
- **REC**: Alle Aufnahmen (Program und ISO-Spuren) laufen über upstreams
  Live-Recorder (`builtin.liverecorder`, fragmentiertes MP4). Der
  Vorschlag war nur die ISO-Spuren.
- **STG**: Stinger ganz über upstream (#1010). Der Vorschlag war beides.
  Folgen, die der Umbau klären muss: Layouts (PiP) als Ziel und „unter den
  Overlays“ gibt es upstream nicht; sie fallen weg oder kommen in strom.
- **CEF**: Freigegeben: `cefsrc` auf dem 3090-Rechner bauen, Abnahme H.36.
- **RUN.8**: Zeitgeber der Cue-Listen auf eigenem Thread mit
  Echtzeit-Priorität über rtkit; geht das nicht, warnt der Selbsttest.
- **VSP**: Vorschau-Ausgang im strom-Mischer bauen.
- **DEMO**: Demo-Welt-Kopien beim nächsten Release des jeweiligen Projekts
  abgleichen.
- **HW**: Abnahme in einer Sitzung auf dem 3090-Rechner (Tests und
  Selbsttest durch Claude, Steckarbeit durch den User).

**Frage des Users zu CEF** (beantwortet): Es gibt keinen zweiten Weg.
`cefsrc` ist die Engine hinter dem schon gebauten Weg: Die Grafikseite
`/graphics/<n>` (M7 Teil A) und Webseiten laufen über strom
`builtin.html_input`, und der rendert mit Chromium über `cefsrc` (GFX.2,
17.10., strom #22). In der VM gibt es kein `cefsrc`, darum lief der Weg
nie echt. Freigeben heißt, `cefsrc` auf dem 3090-Rechner zu bauen und den
Weg dort abzunehmen (H.36).

## 2026-10-18 — Aufnahmen über den Live-Recorder (REC)

**Kontext**: User-Entscheidung REC (Entscheidungsseite 18.10.): alle
Aufnahmen, Program wie ISO-Spuren, über upstreams Live-Recorder
(`builtin.liverecorder`), nicht nur die ISO-Spuren.

**Entscheidung**: Aufnahme-Ausgänge bauen den Live-Recorder statt
`builtin.recorder`. Gleiche Eigenschaften (Container, Ordner, Präfix, Zahl
der Tonspuren) und Pads, also keine Änderung an Show-Dateien oder API. Die
Dateien heißen `<Ordner>/lmix-<ID8>_<Datum>_<Zeit>_00001.<Endung>`, mit
`<ID8>` den ersten 8 Zeichen der Ausgangs-ID (kein Name).
Neu ist der Anfang der Ausgangs-ID: Sonst schrieben zwei Aufnahmen in
einen Ordner, die in derselben Sekunde starten (Restore nach einem
strom-Neustart), dieselbe Datei.
Ausnahme: MKV vor GStreamer 1.26. Dort ist `matroskamux` noch kein
Live-Muxer, und der Live-Recorder lehnt MKV ab. lmix-core fragt beim
Start strom nach seiner GStreamer-Version (`/api/version`, ohne Antwort
mit Warnung im Log) und nimmt für
MKV dann den älteren Recorder (dessen MKV bleibt nach einem Absturz
ebenfalls lesbar). MP4 und TS gehen immer über den Live-Recorder.
**Geprüft**: Unit-Tests (Block je Container und Version);
`scripts/test-m2b-vm.sh` in der VM (GStreamer 1.24.2): MP4-Aufnahme
abgeschlossen und lesbar, H.265 in MKV über den älteren Recorder,
ISO-Spuren mit und ohne Ton, Multiview-Aufnahme.
**Offen**: Lange Aufnahme auf dem 3090-Rechner (H.24), dort mit
GStreamer 1.28 auch MKV über den Live-Recorder.

## 2026-10-18 — Stinger über upstream (STG)

**Kontext**: User-Entscheidung STG (Entscheidungsseite 18.10.): Stinger
ganz über upstreams Stinger-Übergänge (#1010) statt der eigenen Ebene
(09.10.). Vergleich 17.10.

**Entscheidung**: API und Show-Datei bleiben: Eine Übergangstaste mit
Effekt `stinger` nennt einen Clip-Input und einen Cut-Punkt. lmix-core
baut daraus upstreams Stinger-Bibliothek:
- Mischer-Flow (`lmix mixer 10`): `builtin.media_player` `stinger` mit
  `stinger_mode`, `video_out` → `vm:stinger_in`; am Mischer
  `enable_stinger` und `stinger_preroll_ms` = 80 (upstreams Vorgabe).
- Bibliothek = Playlist des Players. Der Switcher schickt sie als
  `StingerLibrary` (Datei des Clip-Inputs, bei einer Playlist ihr erstes
  Stück, mit dem Cut-Punkt der Taste), wenn sich Tasten, Clip-Inputs oder
  deren Quellen ändern, und bei jedem Neuaufbau. Die Engine gleicht über
  upstreams Clip-API ab (entfernen von hinten, hinzufügen, Einstellungen
  `cut_point_ms`, Layout `auto`, darunter `cut`), jede Änderung mit der
  erwarteten Datei (409 bei fremder Änderung). Die Playlist-Eigenschaft
  neu zu setzen erreicht den laufenden Player nicht; die API schon.
- Auslösen: `Stinger` trägt Datei und Cut-Punkt. Die Engine legt den
  Clip vorher in die Bibliothek, wenn er fehlt oder einen anderen
  Cut-Punkt hat (ein gescheiterter Abgleich heilt so beim nächsten
  Drücken), dann `POST …/stinger/take` mit Index und Datei (strom lehnt
  ab, wenn die Bibliothek sich dazwischen geändert hat). strom schaltet
  PGM bildgenau am Cut-Punkt. Läuft der Stinger nicht (Datei fehlt, strom
  lehnt ab), schneidet die Engine auf das neue Program, wie es der
  Switcher schon zeigt, und meldet den Fehler (wie upstream bei einem
  Clip, der nicht spielt).
- Der Switcher tauscht PVW/PGM beim Drücken wie bisher. Ton (AFV),
  Auto-Play, Auto-Pause und vorbereitete Overlays laufen nach
  Cut-Punkt + 80 ms Preroll (`Command::Wait`), also nach lmix-Uhr, ±1–2
  Bilder neben dem Bildschnitt. Sperre (`stinger_until`): Preroll +
  Cliplänge (wenn der Player sie meldet, sonst die Länge der Taste) +
  300 ms. So lange hält strom den Mischer nach dem Clip meist noch (zwei
  Bilder, dann parkt es den Clip neu). Den Rest wartet die Engine ab:
  Vor jedem Take, Stinger und FTB nach einem Stinger fragt sie alle 20 ms,
  bis strom `running == false` meldet (höchstens Cliplänge + 6 s). Cut,
  Übergänge, T-Bar und FTB lehnt lmix während der Sperre ab
  (`StingerPlaying`); FTB kippt den Zustand dann nicht.
- Ton: `stinger:audio_out` auf einen eigenen Kanal des Audiomischers nach
  den Slots, auf Programm mit 0 dB, ohne Kanalzug.

**Entfallen**: eigene Stinger-Ebene (DSK 0, `STINGER_LAYER`), Ablauf
Stinger an → warten → Cut → warten → Stinger aus, Warten auf die
Ausspielverzögerung, Abbildung Stinger → `cut`. Overlays liegen jetzt auf
DSK 0–7 (`overlay_layer(k) = k`), der Mischer hat 8 DSK-Ebenen; die
strom-Grenze (9) bleibt.
**Lage**: upstream zeichnet den Stinger mit z 92 unter den DSK-Ebenen
(ab z 100), also wie bisher unter den Overlays. Die Tabelle vom 17.10.
(„über allem“) war falsch.

**Lücken**:
- Layouts (PiP) als Quelle oder Ziel: upstream lehnt ab, lmix vorher mit
  `BadStinger` („only between inputs, not with layouts“). Fällt weg.
- Ton: der Kanalzug des Clip-Inputs gilt nicht mehr; der Stinger-Ton hat
  weder Fader noch Mute; lmix setzt keine Aux-Sends.
- Einstellungen hält strom je Datei, nicht je Taste: Zwei Tasten mit
  demselben Clip brauchen denselben Cut-Punkt (sonst `BadStinger`); wird
  derselbe Clip später über eine geänderte Quelle doppelt, gilt die
  erste Taste.
- Der Clip spielt immer ganz; die Länge der Taste gilt nur für die
  Sperre, solange die Cliplänge unbekannt ist.
- Ausblenden nach Schwarz während eines Stingers geht nicht mehr (strom
  lehnt ab, lmix sperrt es); vorher ging es.
- Abgleich: Entfernt der Abgleich einen Clip nicht (z. B. während eines
  Stingers), bleibt er in der Bibliothek liegen, bis sich die Tasten
  wieder ändern oder lmix neu aufbaut; er stört nicht.
- Track-Matte und Nur-Maske nur auf dem GL-Mischer; CPU und CUDA spielen
  klassisch (CUDA ungeprüft).
- Der Clip-Input selbst bleibt angelegt und pausiert; er liefert nur die
  Datei (und die Kachel).

**Update**: Der Mischer-Flow heißt jetzt `lmix mixer 10`. Das erste
Starten nach dem Update baut einen laufenden Mischer neu, das Bild fällt
dabei kurz weg: nicht während einer Show aktualisieren.
**Geprüft**: Unit-Tests (Switcher: Befehle, Sperre, FTB gesperrt,
Layout-Ablehnung, Bibliothek je Datei; Engine gegen nachgebautes strom,
das die Bibliothek wirklich ändert: Index und Datei, Clip nachlegen,
Stinger nach Stinger wartet, Abgleich, Cut ohne Clip; Mischer-Flow); `scripts/test-m2b-vm.sh` (CPU-Mischer):
Clip in upstreams Bibliothek mit Cut-Punkt, Vorschau sofort auf Program,
Cut gesperrt, Clip mit Alpha über dem alten Programm, Ende auf dem neuen,
Take-Bericht von strom, Taste wird Cut und Bibliothek leer, wenn der
Clip-Input geht.
**Offen**: H.27 neu auf CUDA/GL (Track-Matte, Bildgenauigkeit, Ton des
Clips hörbar).

**Nachtrag (18.10., Ende des Stingers)**: User-Entscheidung nach PR #107.
Das Warten der Engine (`stinger_idle`) hielt die Switcher-Sperre bis zu
Clip + 6 s und damit Panik (SAF) und Cue-Zeitgeber auf; abgelehnte Takes
nachträglich zurückzunehmen ging nicht sauber (#107, drei Runden).
Jetzt:
- Die Sperre (`stinger_until`) beginnt beim Drücken wie bisher und endet,
  wenn strom das Ende des Takes meldet: `StingerCompleted` oder
  `StingerFailed` auf dem WebSocket, den lmix-core schon liest
  (`meters::follow`), nur für den Mischer-Flow und den Block `vm`.
  `StingerCompleted` zählt nur für den letzten Take von lmix (`take_id`
  aus der Antwort auf `…/stinger/take`; vor jedem Take vergessen). So
  beendet weder ein älterer Take noch einer aus strom's eigener
  Oberfläche die Sperre. `StingerFailed` kommt, bevor strom auf die
  Anfrage antwortet; lmix kennt die Id dann noch nicht und nimmt es ohne.
  `StingerStarted` braucht lmix nicht.
- Höchstens bis Preroll + Cliplänge (sonst Länge der Taste) + 6 s
  (strom gibt einen hängenden Mischer 1 s + 5 s nach dem Clip auf). Ein
  verlorenes Ereignis sperrt so nicht für immer.
- Während der Sperre lehnt der Switcher mit `StingerPlaying` ab, bevor
  sich etwas ändert: Cut, Übergänge, Stinger, T-Bar, Quick Play, FTB,
  Cut direkt, dazu neu Show öffnen/neu/Paket importieren und das
  Schließen des Inputs auf Program (beide schneiden; vorher wartete die
  Engine sie ab). Diese Erweiterung über die Liste der Entscheidung hinaus
  ist noch nicht vom Eigentümer bestätigt (Frage in #109).
  Paket importieren übernimmt die Ziele der Datei schon vorher; das ist wiederholbar (gleiche Namen werden ersetzt).
- Die Engine wartet nicht mehr: kein Abfragen von `running` vor Take,
  Stinger und FTB. Kein Zurücknehmen.
- Scheitert der Stinger-Befehl (strom lehnt ab, Clip fehlt), endet die
  Sperre sofort im Dienst. strom meldet dann `StingerFailed` (oder
  nichts, wenn es vor dem Take ablehnt). Hat strom selbst geschnitten
  (`program_changed`), findet der Ersatz-Cut der Engine das neue Program
  schon vor und tut nichts; sonst schneidet sie. In beiden Fällen ist
  gesendet, was der Switcher seit dem Drücken zeigt.
- Neuer Mischer (strom-Neustart, Restore, neues Format, Multiview,
  Abtastrate): Die Sperre endet, ein Ende käme nicht mehr. Läuft die
  Engine noch nicht, endet sie gleich.
- Panik wartet auf nichts davon; sie nimmt die Switcher-Sperre nicht
  länger als andere Aktionen.
**Geprüft**: Unit-Tests (Switcher: Sperre bis zum Ende, Grenze, Ablehnung
ohne Änderung, neuer Mischer; Ereignisse in strom-Form, fremder Flow,
Block und Take; Dienst mit NullEngine: Sperre bis zum Ereignis, sofort
frei bei Fehler; Engine: Take und FTB nach einem Stinger ohne Abfrage);
`scripts/test-m2b-vm.sh`: Cut gleich nach strom's Ende angenommen und
auf Sendung.
**Offen**: Ein Take, dessen Grenze abläuft, bevor strom ihn beendet
(hängender Mischer), lässt den nächsten Cut wie bisher an strom
scheitern.

**Nachtrag (18.10., Review #108/#109)**:
- Grenze jetzt Clip + 8 s (`STINGER_FINISH_LIMIT_MS`): strom startet seine
  1 s + 5 s erst nach Cue, Analyse und `play_at` und cued den Clip neu,
  bevor es `running` freigibt. Mit 6 s lief lmix' Grenze bei einem
  hängenden Mischer immer zuerst ab; 2 s Reserve.
- Ein spätes Ende eines älteren Takes gibt die Sperre eines neueren nicht
  mehr frei: `Stingers::starting` zählt jeden Take (Generation), jedes
  `StingerEnd` trägt sie, der Switcher gibt nur bei gleicher Generation
  frei (`stinger_ended`). Vorher konnte `Completed(N)` nach abgelaufener
  Grenze auf den Switcher warten, den Take N+1 hielt, und dann dessen
  Sperre lösen.
- `StingerCompleted` vor der Antwort auf `…/stinger/take` (Id noch
  unbekannt) geht nicht mehr verloren: lmix merkt sich die Id und meldet
  das Ende, sobald die Antwort sie nennt. Vorher hielt die Sperre bis zur
  Grenze.
**Geprüft**: Unit-Tests (Generation im Ende, spätes Ende hält die Sperre,
Ende vor der Id); `scripts/test-m2b-vm.sh` (Stinger gesperrt bis
strom's Ende).


**Nachtrag (19.10., Review von #111, lmix #118)**:
- Die Grenze zählt ab dem Start des Takes durch die Engine neu
  (`stinger_started`), nicht mehr ab dem Drücken: hat die Engine für
  Clip, Vorschau und Take länger gebraucht, hält die Sperre entsprechend
  länger, höchstens Preroll + Cliplänge + 8 s ab dem Start.
- `StingerFailed` mit einer bekannten Id eines anderen Takes beendet die
  Sperre nicht; ohne Id oder solange lmix die Id des eigenen Takes noch
  nicht kennt, beendet es sie wie bisher.
- Verpasste Enden (`Lagged` im Kanal) geben die Sperre nicht mehr frei:
  sie können zu einem älteren Take gehören. Dann gibt die Grenze frei.
## 2026-10-18 — Zeitgeber mit Echtzeit-Priorität (RUN.8)

**Kontext**: User-Entscheidung RUN.8 (Entscheidungsseite 18.10.): Der
Zeitgeber der Cue-Listen bekommt einen eigenen Thread mit
Echtzeit-Priorität über rtkit; geht das nicht, warnt lmix.

**Entscheidung**: `watch_cues` läuft auf dem Thread `lmix-cues` mit eigener
tokio-Laufzeit, die Aktionen der Cues wie bisher gleich danach im selben
Ablauf. Damit bleibt die Reihenfolge: Eine Panik kann keine schon fälligen
Aktionen überholen, und die Verspätung in `/metrics` zeigt weiter, wenn
langsame Aktionen die nächsten Schritte aufhalten. Ein Hilfs-Thread holt
dem Zeitgeber die Priorität: zuerst `SCHED_FIFO` direkt (Priorität 20;
mit `RLIMIT_RTPRIO` oder als Root), sonst über rtkit auf dem
System-D-Bus (`MakeThreadRealtime`, bis zu dessen
`MaxRealtimePriority`), wie PipeWire. Ein hängender D-Bus hält den
Zeitgeber so nicht auf.

**Schutz**: Auf beiden Wegen gilt vorher `RLIMIT_RTTIME`: weich 100 ms,
hart 200 ms (rtkit gewährt nichts ohne Grenze). An der harten Grenze
beendet der Kernel den Prozess. Darum macht das SIGXCPU der weichen
Grenze den Zeitgeber wieder zu einem normalen Thread: Ein Cue-Schritt, der
so lange rechnet (etwa „Show öffnen“ mit großen Datenquellen), kostet die
Priorität für den Rest des Laufs, nicht die Show. Dann steht eine Warnung
im Log, und `/metrics` zeigt `lmix_cue_timer_realtime 0`, ebenso ohne
Echtzeit von Anfang an und wenn der Thread endet. Die Rückstufung behält
`SCHED_RESET_ON_FORK`: Ohne CAP_SYS_NICE lässt der Kernel das Flag nicht
löschen (EPERM, mit einem Prozess ohne Rechte nachgeprüft), und der
Thread bliebe bis zum Abbruch an der harten Grenze in Echtzeit. Scheitert
die Rückstufung trotzdem, merkt sich der Signal-Handler den Fehler, und der
Wächter-Thread schreibt ihn bei seiner nächsten Runde (einmal je Sekunde)
ins Log (19.10., Review #105). Rechnet der Thread bis zur harten Grenze
weiter (weitere 100 ms), beendet der Kernel lmix-core meist vorher, und die
Zeile fehlt; sie erscheint vor allem, wenn der Schritt vorher endet. Ein Unit-Test prüft die Rückstufung, wo der Test
SCHED_FIFO bekommt (in der VM als Root).

**Reichweite**: Wartezeiten und Folgen nach Zeit. Ein GO führt seine
Schritte auf dem Thread des Aufrufers aus (HTTP, MIDI, OSC). Was Aktionen
eines Cues mit `tokio::spawn` starten (Ziele, Datenquellen, verzögerte
Aktionen), läuft auf der Laufzeit des Zeitgebers und endet mit ihr. Eine Folge
nach dem Ende eines Klangs oder Bilds kommt mit der Meldung der Engine,
die strom alle 100 ms abfragt (SND.12). Datei-Schreiben (Autosave) läuft
über tokios Blocking-Threads mit normaler Priorität.

**Grenze**: Der Zeitgeber braucht für jeden Schritt die Sperre des
Switchers. Hält sie gerade ein normaler Thread, den die Last verdrängt
hat, wartet auch der Echtzeit-Thread (Prioritätsumkehr). Hilft Echtzeit
auf dem 3090-Rechner nicht, ist der nächste Schritt ein Zeitgeber, der
seine Fristen ohne diese Sperre kennt.

**Messung** (VM mit 4 Kernen, alle vier mit `yes` belegt, je 3 min,
`scripts/test-cue-timing.sh`, spätester Schritt):

| Fassung | mit Echtzeit | ohne (CAP_SYS_NICE entzogen) |
|---|---|---|
| Aktionen auf dem Zeitgeber, erste Fassung | 19,6 ms; 3,8 ms | 9,0 ms; 7,5 ms |
| Aktionen getrennt (verworfen, siehe unten) | 24,4 ms | – |
| diese Fassung | 8,9 ms | – |

In der VM ist kein Gewinn belegt; die Werte streuen stärker, als
Echtzeit sie ändert. Schlüssig ist erst die Messung auf dem 3090-Rechner
(H.39), wo ohne Echtzeit bis 65,3 ms zustande kamen.

**Verworfen**: Die Aktionen über einen Kanal auf der gemeinsamen Laufzeit
ausführen (Review #105, Runde 2): Schon fällige Aktionen konnten eine
Panik überholen, und die Metrik hätte nur noch den Zeitgeber gemessen.

## 2026-10-18 — Console in zwei Programme geteilt (Mischer, Show)

**Kontext**: Auftrag des Users (18.10.): Die Console wird zu zwei
Oberflächen, eine für den Mischer, eine für die Show. Beide laufen
gleichzeitig und sprechen mit demselben `lmix-core`. Bisher teilten sich
Mischer und Show-Ansicht ein Fenster mit Umschalter (2026-10-16).

**Optionen**: (a) zwei Crates; (b) ein Crate mit zwei Binaries, die
Rust-Brücke (`Core`), Formulare, Portal und das QML-Modul teilen.

**Entscheidung**: (b). `workspace/console-qt` baut `lmix-mixer` und
`lmix-show`; `lmix-console` entfällt. Jedes Programm hat eigenen
Anwendungsnamen (`mixer`, `show`, Organisation `lmix`), eigene
Desktop-Datei und Wayland-App-ID (`lmix-mixer`, `lmix-show`,
`packaging/desktop/`) und eigenes Haupt-QML:

```
AppShell.qml   Fenster: lmix-core, Stil, Datei-Knöpfe (Neu, Öffnen, Zuletzt,
               Speichern, Paket mit Einstellungen exportieren/importieren/
               zurücksetzen), Belegungen (Fenstertasten, globale Tasten,
               Dialog), Hinweis altes Format, Ablehnungen
 ├ MixerMain.qml  Monitore, Übergänge, Inputs, Layouts, Ausgänge, alle
 │                Mischer-Dialoge, „Aus Show…“, Programmformat, Multiview,
 │                44,1 kHz, Latenz, Tasten 1–9
 └ ShowMain.qml   Show-Ansicht (Erweitert, Dunkel), Cue-Dialog („Cues…“
                  steht jetzt in der Show-Ansicht), „Belegungen…“ in der
                  Kopfzeile, Leertaste GO, Escape Panik
```

- **Belegungen in beiden Programmen** (AUT.1–3): Fenstertasten wirken nur
  im Fenster vorne, lösen also nie doppelt aus.
- **Globale Tasten (AUT.12) in beiden, je mit eigenem Schalter**
  (`keys/global` in `mixer.conf` bzw. `show.conf`; Entscheidung des Users
  18.10.2026): ab Werk in der Show an (Panik und GO von überall), im Mischer
  aus. Jeder Prozess hat seine eigene Portal-Sitzung; sind beide an, löst
  ein Druck zweimal aus. Darauf weist der Belegungen-Dialog unter dem
  Schalter hin. Die Show erreicht den Dialog über „Belegungen…“ in der
  Kopfzeile.
- **Fokus in der Show**: Die Knöpfe der Kopfzeile nehmen keinen Fokus; nach
  den Dialogen des Fensters (Datei, Paket, Einstellungen, Neu, Belegungen)
  holt die Show Fenster und Show-Ansicht zurück, damit Leertaste und Escape
  wieder GO und Panik sind (vorher blieb der Fokus am Knopf).
- **Einstellungen** je Programm in `~/.config/lmix/mixer.conf` und
  `show.conf`. Fehlt die eigene Datei und gibt es `console.conf`, kopiert
  das Programm sie einmal (beide Programme; über eine `.tmp`-Datei und
  Umbenennen), so bleiben Stil und Ansicht. `view.showMode` entfällt.
  Stand `keys/global` in der alten Datei auf an, ist es danach in beiden an;
  stand es auf aus (die alte Console schrieb das immer), nimmt die Show es
  nicht mit und bleibt bei ihrer Vorgabe an.
- **Titelleiste** nennt das Programm: „lmix Mischer — <Show>“, „lmix Show —
  <Show>“.
- `--mode simple|advanced` wirkt nur im Mischer; `--core`, `--style`
  (seit 19.10. `--theme`), `--window` in beiden.
- `claude.md` Punkt 7 und SUI.1 neu gefasst (User 18.10.2026): ein Kern,
  zwei Oberflächen; die Desktop-Dateien in `packaging/desktop/` bestätigt.
- Skripte: `run-local.sh` startet den Mischer (`APP=show|both`),
  `test-console-qt.sh` prüft den Mischer und danach die Show am selben
  Core (auch Leertaste GO und Escape Panik nach Kopfzeile und Dialog),
  `test-global-keys.sh` die Vorgaben beider Programme, `CONSOLE_SHOT` den
  Mischer, die Demo macht je ein Bild von Mischer und Show.

**Bestand, behoben 19.10. (#118)**: `--style` erreichte auch Qt, das
daraufhin ein Quick-Controls-Stil-Modul `kante` suchte und das Fenster nicht
lud (so schon bei `lmix-console`). Die Option heißt jetzt `--theme`
(Breaking vor dem ersten Release: `--style` gibt es nicht mehr; BUILD.md,
GUI-SHELL.md und der Demo-Kommentar sind nachgezogen).

## 2026-10-18 — PR-Agent nur auf `/review` des Eigentümers

**Kontext**: PR-Agent lief bei jedem PR und jedem Kommentar. Auf dem einen
Gitea-Runner brach jeder Kommentar den wartenden Lauf ab, sein Schlüssel
hat das Kostenlimit erreicht, und seit jeder PR vom Claude-Review geprüft
wird (Eintrag oben), wartete der Merge zweimal. Der Eigentümer wollte ihn
behalten, aber nur auf Abruf (18.10.2026).

**Entscheidung**: `.gitea/workflows/pr-agent.yml` startet nur, wenn der
Eigentümer einen Kommentar mit `/review` beginnt; kein Lauf bei Push
oder neuem PR. Ein neues `/review` (allein oder mit Optionen nach einem
Leerzeichen) ersetzt einen laufenden, andere Kommentare brechen nichts ab.
Sitzungen schreiben kein `/review`. Ohne `/review` wartet kein Merge auf
PR-Agent; nach einem `/review` wartet der Merge, bis dieser Lauf fertig
ist (ein fehlgeschlagener hält nichts auf), und seine Befunde werden
behoben oder beantwortet. Beantwortete Befunde gehen mit den offenen
**Wichtig** an den nächsten Review-Agenten; eine Antwort zählt erst, wenn
er sie annimmt, sonst entscheidet der Eigentümer in Worten. Ein Kommentar
wie `/reviewed` endet im ersten Schritt, ohne etwas zu installieren. Dieselbe Regel gilt in allen Repos mit
PR-Agent.

**Verworfen**: PR-Agent ganz entfernen. Er bleibt als zweiter Leser mit
einem anderen Modell, wenn der Eigentümer ihn will.

## 2026-10-19 — Hardware-Nacht auf der RTX 3090: Abnahme CUDA und GL, Klang, Bühnen, Zeitgeber

**Stand**: lmix `main` f2136cd, strom `dev` 81cd27a plus Branch `lmix-cuda-hw-fixes`;
RTX 3090, Treiber 615.78, GStreamer 1.28.8, Rust 1.99.1. Die ganze Nacht lief nebenher
eine fremde GPU-Last (100 %, 20–23 GB VRAM, ~350 W); Latenz- und Lastwerte sind
darum Obergrenzen.

**Abnahmetest auf echter Hardware**: `scripts/test-m2b-vm.sh` als Kopie mit
`--mixer cuda` bzw. `gl` (strom auf der Wayland-Sitzung, NVIDIA-GL statt
llvmpipe). Mit den Fixes unten sind auf beiden Mischern alle Prüfungen grün
(Stinger mit Alpha, Overlays, PNG mit Alpha, Layouts mit Zuschnitt, Failover,
Signalverlust, Aufnahme/SRT/RTMP/H.265 über NVENC, MultiCorder, Automation,
Show speichern und Wiederaufnahme nach Neustart von lmix-core; auf GL auch
Looks). Latenz im Test 87 ms (CUDA) bzw. 88 ms (GL) Median unter der fremden
Last. Rot bleibt nur „keine CRITICALs“: 28 kommen aus GStreamers V4L2-Abtastung
der Facecam (`gst-device-monitor-1.0 Video/Source` allein zeigt sie auch).
GL brauchte `SETTLE_S=30`: unter der fremden Last kamen die ersten Bilder
erst nach 10 s.

**Gefunden und in strom behoben** (Branch `lmix-cuda-hw-fixes`, je mit Test,
der ohne den Fix scheitert; die CUDA-Tests sind `#[ignore]`, laufen mit
`cargo test -- --ignored` auf einem CUDA-Rechner):
1. **Alpha hängt nach einer Blende (CUDA)**: `cudaconverter` (1.28.8, auch
   upstream `main`) vergleicht ein neues Alpha mit einem Feld, das er nie
   setzt (immer 1.0). Ein Bruchteil zurück auf genau 1.0 wird überhört: Nach
   einer Blende blieb das neue Program bei ~0,97 (Ball auf Grau) und eine
   später wieder geschnittene Quelle bei ~0,03 (fast schwarz). strom macht an
   CUDA-Mischer-Pads aus 1.0 den Wert 1 − 10⁻¹²: für den Konverter eine
   Änderung, im Kernel (`f32`) genau 1.0. Upstream melden.
2. **Vorschaubilder auf CUDA im Videobereich**: NV12 ging ohne Umrechnung in
   `nvjpegenc`, JPEG ist Vollbereich: Schwarz 16, Weiß 235 in allen
   Program- und Input-Bildern (die Aufnahme selbst stimmte). Jetzt
   `colorimetry=1:4:0:0` vor dem Encoder.
3. **Kein Stinger auf CUDA**: `stinger_pads` suchte das Pad hinter der GL-Queue
   oder dem CPU-Crop, auf CUDA hängt es hinter `cudaupload_stinger`. Jeder
   Stinger wurde abgelehnt („this vision mixer has no stinger input“).
4. **Stinger mit Alpha auf CUDA nicht ausgehandelt**: `videoconvert !
   videocrop ! cudaupload`; `videocrop` nimmt A420/A444_10LE, also reichte der
   Convert sie durch, `cudaupload` nicht. Auf CUDA jetzt Crop, Convert, Upload.
5. **CRITICAL bei jeder langen Stummschaltung**: Der letzte Stützpunkt einer
   dB-Rampe auf 0 lag bei −1,25·10⁻¹⁸ (`(1.0 − 0.95) / 0.05` ist etwas über 1),
   `volume` lehnte ihn ab. Jetzt auf [0, 1] begrenzt.

**Klang-Cues auf echten Geräten (H.37)**, Scarlett Solo auf −120 dB gestellt
(nachts), gemessen am Monitor des Geräts mit `parec`:
- 16 Klänge gleichzeitig: 8 s ohne Lücke, alle Samples da.
- Eine Zone auf zwei Geräten (Scarlett, S/PDIF): beide spielen, 6–8 ms Versatz.
- GO bis erstes Sample am Gerät (vorgeladen, 10 GOs): 110 ms Median (bis
  133 ms). Mit `buffer-time` 20 ms und `latency-time` 5 ms am `pulsesink`
  (lmix, Branch `hw-night-1019`) 77 ms (77–81 ms). Gilt für Zonen mit genanntem Gerät; die Standardausgabe (Gerät leer) bleibt `autoaudiosink` mit seinem Puffer (~110 ms), weil sie ohne Sound-Server auf ALSA zurückfallen muss (SNF.1, Review #116). SND.12 verlangt 50 ms; der
  Rest sind strom's Zonenlatenz (30 ms) und PipeWire.
- **Panik stoppt keinen Klang** (offen): strom meldet einen 5-s-Klang 0,2 s
  nach „plays“ als beendet und baut ihn ab, sein Zustand listet keine Klänge
  mehr, während er zu hören ist. lmix stoppt bei Panik „jeden Klang, den
  strom meldet“, also keinen; der Ton spielt bis zum Ende der Datei. Das EOS
  erreicht die Sonde vor dem Zonenmischer sofort, die Annahme „der Mischer
  nimmt die Puffer, wie er sie spielt“ hält am echten Gerät nicht. Der Puffer
  liegt nicht im Pulse-Stream (Buffer Latency 0), sondern in der Pipeline
  dahinter. In der VM fiel es nicht auf (Fakesink).
  Ursache (`GST_DEBUG=aggregator:6`): Der Zonenmischer (`audiomixer`,
  force-live, aber ohne Live-Eingang) mischte die 125 Puffer der 5-s-Datei
  in 130 ms; den Test-Rig hielt sein `appsink sync=true` im Takt,
  `pulsesink` auf der Monotonic-Uhr nicht. Fix strom #30: jede Zone bekommt
  eine Live-Stille, wie jede Bühne ihr Live-Schwarz. Danach Panik wirksam
  (Spitze danach 0,000 statt 0,300, 3 von 3 Läufen), GO 68–77 ms.
- Gerätekanäle 3/4: kein Mehrkanal-Interface hier. Auf einem 8-Kanal-Gerät
  mit Positionen (virtuell, FL…SR) landet eine Zone auf [3, 4] nicht auf 3/4:
  lmix schickt einen 4-Kanal-Strom ohne Positionen, PipeWire ordnet nach
  Position, nicht nach Index. (Nachts gemessen „auf allen Kanälen stumm“; das
  war die Messung: `parec` ohne `--channel-map` ordnet selbst um. Mit der
  Kanalbelegung des Geräts aufgenommen landet der 4-Kanal-Strom auf RL/RR,
  den Kanälen 5/6.) Gerätekanal N ist so nur auf Geräten mit „AUX“-Kanälen
  (Pro-Audio-Profil) adressierbar. **Entscheidung (User, 19.10.)**: lmix
  schickt den Strom mit Kanalzahl und -belegung des Geräts (aus der
  Geräteliste von strom); Gerätekanal N ist der N-te Kanal des Geräts. Ohne
  Angabe des Geräts wie bisher, mit Warnung.
  **Umgesetzt (19.10. tagsüber)**: strom führt Audio-Ausgänge in der
  Geräteliste, mit `channel_positions` aus PipeWires `audio.position`
  (strom #31). lmix-core holt sie beim Bau des
  Playback-Flows und setzt vor den `pulsesink` `channel-mask` und alle
  Kanäle des Geräts; die `mix-matrix` legt jeden Zonenkanal auf den
  Stromkanal mit der Position seines Gerätekanals (GStreamer ordnet nach
  Position, ALSA-5.1 etwa ist FL FR RL RR FC LFE). Ohne Positionen (Gerät
  unbekannt, AUX-Kanäle, Mono) bleibt der Strom ohne Positionen; liegt ein
  Kanal über 2, steht eine Warnung im Log.
  **Folge für bestehende Einstellungen** (Review #117): Ein Gerät, das seine
  Positionen meldet, bekommt jetzt genau die gewählten Kanäle. Eine
  Mono-Zone auf Kanal 1 eines Stereo-Geräts spielt nur noch links (vorher
  verteilte PipeWire den Mono-Strom auf beide Seiten); eine Stereo-Zone auf
  1/2 eines 5.1/7.1-Geräts nur noch vorne links/rechts (vorher je nach
  Upmix auch hinten). Gewollt nach der Entscheidung: Gerätekanal N ist der
  N-te Kanal. Wer eine Mono-Zone auf beiden Seiten will, legt sie auf 1 und
  2 (zwei Ausgänge) oder macht die Zone zweikanalig. Geprüft auf einem virtuellen
  7.1-Gerät: jedes Paar landet genau auf seinen Kanälen, die übrigen
  bleiben still (`scripts/hw-sound-cues/run.sh`, Schritt „channels 3/4“).
  Offen: ein echtes Mehrkanal-Interface (H.37), Pro-Audio-Profil.

**Bühnen (H.38)**: Vollbild als `waylandsink`, 1080p30-Video über die CPU:
strom 185 % CPU für eine Bühne, 280 % für vier. Sichtprüfung nachts nicht
möglich (Bildschirme gedimmt, Bildschirmfotos leer).

**Kamera (H.7, H.12)**: Elgato Facecam MK.2 in der Geräteliste, als Input
nach 1 s online, Bild auf Program (CUDA).

**Zeitgeber (H.39)**, 10 min mit 24 × `yes`: mit Echtzeit (SCHED_FIFO über
`RLIMIT_RTPRIO`) spätestens 16,6 ms, 10 von 61 215 Schritten über 10 ms; ohne
Echtzeit (rtkit gesperrt) 133,2 ms und 434. Echtzeit hilft deutlich, hält
10 ms unter Volllast aber nicht. 4 h mit Echtzeit, neben den Builds und Tests
dieser Nacht (Builds mit `-j 12`, Tests, Soaks, zeitweise nur 4 GB RAM frei): 1 468 811 Schritte, spätestens 385,7 ms, 226 über 10 ms (0,015 %); die Echtzeit blieb die ganzen 4 h (keine Rückstufung). Zum Vergleich 17.10. ohne Echtzeit unter Builds: 145,6 ms, 694 darüber. Weniger späte Schritte, aber ein schlimmerer Ausreißer (in der ersten Stunde, während der strom-Builds). Über rtkit (`RLIMIT_RTPRIO` 0, „how=rtkit“ im Log) 3 min mit 24 × `yes`: spätestens 1,6 ms, keiner über 10 ms. Ein 4-h-Lauf auf freiem Rechner steht weiter aus.

**strom ignorierte Tests (H.34)**: rot auch auf unverändertem `dev`:
`nvdec_cuda_after_a_converted_format_stays_on_the_gpu` (GBR_10LE nie auf PGM)
und `measure_idle_fx_slot_renders` (GL). `gpu_stingers_land_frame_accurately`
scheitert unter der GPU-Last etwa jedes vierte Mal, auch auf `dev`.

**16 Inputs (H.2, H.10, H.18)**, GPU frei (die fremde Last endete 03:38): CUDA 20 min ohne Drop (Aufnahme 37 038 Bilder in 1 233 s, SRT ohne Verlust), Latenz durchgehend 81 ms, mit 8 Inputs 56 ms wie am 13.10.; GL 15 min, Latenz nach 5 min 56 ms (anfangs 80 ms), strom-RSS 1 084 → 1 149 MB (bekanntes GL-Wachstum). Gegenproben je 3–5 min auf CUDA: 12, 14 und 15 Inputs 56 ms, 16 wieder 83 ms. Die Latenz springt also genau bei 16 Slots (17 Mischer-Eingänge mit dem Schwarz-Input), sie wächst nicht mit der Last; ≤ 2 Frames (NFA.2) hält auf CUDA mit 16 Inputs nicht. Ursache offen.

**Syncthing**: In lmix hatte Syncthing `.git/index` (auch den des
Submoduls) und 264 Objekte überschrieben bzw. verdoppelt; Arbeitsbaum war
unverändert. Index aus HEAD neu gebaut, Konfliktdateien nach `~/.cache`.
Seit 19.10. ignoriert Syncthing `.git`.

## 2026-10-19 — Vorschau-Ausgang im strom-Mischer (VSP)

**Kontext**: Entscheidung 18.10. (VSP): Die Vorschau kommt wie Program aus dem
Mischer. Bis dahin las der Vorschau-Monitor der Console den Input in voller
Größe (`/api/live/inputs/{id}?size=full`, ein Flow je Slot): ein Klick auf eine
Kachel baute eine neue Pipeline (Lücke), und ein Layout auf Vorschau zeigte
gar nichts (die Vorschau war kein Input).

**Entscheidung**:
- strom #32: Eigenschaft `enable_preview_out` am
  Bildmischer, Ausgang `pvw_out` in Programmgröße. Ein dritter Compositor
  (`pvw_comp`) bekommt je Input einen Zweig vom Tee des Inputs und spiegelt
  die PVW-Pads des Multiviews: Jede Änderung (Position, Größe, Alpha,
  Z-Ordnung, Zuschnitt) an einem PVW-Pad landet, von `pvw_rect` auf das ganze
  Bild umgerechnet, am Zwillingspad. So zeigt der Ausgang Inputs und PiPs auf
  Vorschau, Takes und Blenden, ohne dass eine Operation ihn kennen muss.
- Ohne Ränder: Die Zonen-Ränder (Underlays) spiegelt er nicht; ihre Quellen
  hängen an genau einem Compositor-Pad (Neustart, Halten). Folgearbeit.
- lmix: Der Mischer-Flow schaltet den Ausgang ein und legt ihn auf den
  Inter-Kanal `pvw_out`; `/api/live/preview` liefert den Socket, die Console
  zeigt im Vorschau-Monitor immer ihn. `?size=full` und `SlotFull` entfallen
  (vor dem ersten Release).
- Kosten: ein Compositor in Programmgröße mehr; Pads mit Alpha 0 rechnet der
  Compositor nicht.

**Verworfen**: den PVW-Ausschnitt aus dem Multiview schneiden (halbe
Auflösung, Beschriftung und Tally im Bild); einen Input-Selector statt des
Compositors (keine Layouts auf Vorschau).

## 2026-10-19 — Latenz: kein Sprung bei 16 Inputs (H.2, NFA.3)

**Kontext**: In der Hardware-Nacht maß der CUDA-Mischer mit 16 Inputs
durchgehend 81–83 ms, mit 8–15 Inputs 56 ms („Sprung genau bei 16“).

**Messung** (19.10. vormittags, RTX 3090 ohne fremde Last, strom
`lmix-pin-1019`, `scripts/soak-hardware.sh`, je 1 min):
- 15 und 16 Inputs ohne Ausgänge: 56 ms; 16 mit Aufnahme und SRT: 56 ms;
  15 mit Aufnahme und SRT: 56 ms, nach einer Minute 81 ms.
- Fein (15 Inputs mit Ausgängen, alle 10 s, Program wechselt alle 30 s):
  In 1 und 2 je 74 ms, In 3–6 je 60 ms, In 7–9 je 56 ms. Der Wert ändert
  sich nur beim Schnitt und bleibt je Input gleich.

**Ergebnis**: Die Latenz hängt nicht an der Zahl der Inputs. Rund 56 ms
sind fest; darüber kommen ab und zu höhere Werte (60–81 ms), am Vormittag
wechselnd mit dem Input auf Program, mittags je Lauf (siehe Nachmessung).
Ursache offen: Phase der Quellen gegenüber dem Live-Aggregator (er gibt
ein Bild aus, sobald alle aktiven Pads Daten haben) oder etwas je Lauf.
NFA.3 (intern ≤ 2 Bilder = 66,7 ms) hält im Normalfall, nicht immer.

**Nachmessung** (mittags, strom mit #29–#33, eigenes Skript: N Inputs,
jeder 2 × auf Program, je 2 s gemessen): 9 Läufe mit 4–15 Inputs, Schnitt
oder Blende, mit und ohne Aufnahme und SRT: 8-mal überall 56 ms, einmal
überall 65 ms (für den ganzen Lauf, unabhängig vom Input). Die Werte über
56 ms kommen also nur ab und zu, dann je Lauf oder ab einem Schnitt. Im
Normalfall hält NFA.3 (≤ 66,7 ms).

**Mischer-Latenz** (Eigenschaft `latency` des Bildmischers, strom-Standard
20 ms, 16 Inputs mit Aufnahme und SRT, je 3 min): 20 ms 73 ms (ein
erhöhter Lauf), 10 und 15 ms je 56 ms; Bildzahlen gleich (5834/5835
Bilder in Aufnahme, ~5765 über SRT). Der feste Teil kommt also nicht aus
dem Spielraum des Mischers, sondern von davor (Quelle, Inter-Kanal,
`min-upstream-latency`).

**Nächster Schritt**: einen Lauf mit erhöhtem Wert fangen (Skript mit
Wiederholung, bis ein Wert über 56 ms kommt) und dann die Fristen des
Programm-Mischers mit `GST_DEBUG=aggregator:6` ansehen; prüfen, ob der
feste Teil (56 ms) kleiner geht (Mischer-Latenz, Slot-Puffer).

## 2026-10-19 — GO bis hörbar: Zonen-Latenz 15 ms (SND.12)

**Kontext**: SND.12 verlangt GO bis hörbar ≤ 50 ms. Gemessen 63–77 ms
(Scarlett Solo, `hw-sound-cues`). Die Zone wartete die Latenz ihrer
Pipeline: Mischer 30 ms, dazu die Live-Stille (strom #30) mit den
1024 Samples (21 ms) von `audiotestsrc`, die sie als Latenz meldet, und
`min-upstream-latency` 30 ms.

**Entscheidung** (strom #33): Live-Stille in Blöcken von 5 ms (240
Samples), Mischer-Block 5 ms, `min-upstream-latency` gleich dem Block,
Mischer-Latenz 15 ms. Gemessen: GO bis erstes Sample am Gerät 44 ms
Median (43–49 ms), 16 Klänge ohne Lücke. Mischer-Latenz 20 ms: 44–56 ms;
10 ms: 21–35 ms, aber einmal 2 Lücken bei 16 Klängen gleichzeitig (der
erste Puffer eines Klangs kommt zu spät). 15 ms lassen 10 ms mehr Luft
dafür. Die Streuung zwischen Läufen ist groß (±10 ms, Rechner nicht
leer); ein Lauf auf ruhigem Rechner steht aus.
Mit allem zusammen (strom `lmix-pin-1019`: #29–#33) ist
`hw-sound-cues` zum ersten Mal ganz grün: GO 32,9 ms Median (31,1–34,7),
Panik, 16 Klänge, zwei Geräte, Gerätekanäle 3/4.

## 2026-10-19 — Ausgang mit Quelle Vorschau (OUT.2)

**Entscheidung**: Ein Ausgang kann die Vorschau tragen (`source:
{"from": "preview"}`, Console „Inhalt: Vorschau“), seit der Mischer sie
ausgibt (VSP, strom #32): Bild wie die Vorschau des Mischers (Input oder
Layout), Ton von Program wie beim Multiview. Auf CUDA bleibt das Bild für
die Encoder auf der GPU wie bei Program.

**Geprüft**: Aufnahme mit Quelle Vorschau, Program schwarz, Vorschau
Balken: Bild 30 im Mittel 130 (CPU) bzw. 140 (CUDA), schwarz wäre 0.

## 2026-10-19 — T-Bar mit dem Effekt von Taste 1 (SWI.3)

**Kontext**: Der T-Bar blendete immer, gleich welcher Effekt auf Taste 1
lag: lmix setzte nur das Alpha des Vorschau-Pads.

**Entscheidung**: strom hält den Take von Program zur Vorschau an einer
Stelle (`POST …/blocks/vm/tbar {position, transition_type}`, strom #34):
jeder Input-Pad geht direkt auf die Werte des Effekts an dieser Stelle
(Schieben und Push verschieben die Pads, Dip geht in der Mitte durch
Schwarz, Blende mischt), ohne Umweg über die Ruhelage: der Mischer rendert
zwischen zwei Property-Schreibvorgängen, ein Umweg blitzte als Frame auf
(Review strom #34). lmix schickt den Effekt von Taste 1 mit. Am Ende
bleibt es ein Schnitt-Take wie bisher, zurück auf 0 zeigt Program wie
vorher.
- **Als Blende**: Schnitt (hat kein Dazwischen), Wischen (braucht die
  Uniforms der Shader-Engine) und Stinger (ein Clip steht nicht still).
- **Bei Fade to Black gesperrt** (`fade_to_black_on`, Review #123): der
  T-Bar setzt die Pads wie ein Take zurück und brächte Program unter dem
  Schwarz zurück, während FTB noch an ist. strom lehnt dann ebenso ab.
- **Overlay-Blenden laufen weiter**: strom setzt beim Halten nur die
  Input-Pads zurück, nicht die DSK-Ebenen (Review #123).
- Layouts auf Vorschau bleiben beim T-Bar abgelehnt wie bisher, ebenso ein
  Layout auf Program (die Pads gehören dann dem Layout; strom lehnt ab,
  und der Ton blendete sonst ohne Bild; Review #123).
- **Ein Take setzt den T-Bar auf 0**: jede Aktion, die Program wechselt
  (Cut, Übergang, Stinger, Cut direkt), setzt in strom die Pads zurück;
  lmix führt den T-Bar dann wieder ab 0, sonst käme die nächste Vorschau
  an der alten Stelle von selbst auf Sendung (Review #123). Ebenso beim Wiederherstellen
  (Neustart der Engine oder von lmix-core, Format, Show laden): lmix
  schickt den T-Bar immer auf 0, auch wenn es selbst keinen gezogenen kennt
  (ein weiter genutzter Mischer hielte ihn sonst), außer unter Fade to
  Black. Wird das Vorschau-Input entfernt, geht der T-Bar vorher auf 0. Nicht erfasst: strom merkt sich das Halten nicht, ein
  Geometrie-Refresh (ein Input ändert seine Größe) setzt die Pads zurück,
  ohne dass lmix es erfährt; der nächste Zug am T-Bar stellt es wieder her.
- **Gezogener T-Bar und Vorschau** (Review strom #34): ein anderes Input auf
  die Vorschau kommt an der Stelle des T-Bars herein; ein Layout auf die
  Vorschau und Fade to Black stellen ihn erst auf 0 (sonst stünde die alte
  Vorschau halb auf Sendung, unter dem Schwarz sichtbar). Der nächste
  Zug am T-Bar zeigt die Vorschau wieder an der Stelle, an der er steht.
- **Laufende Animation**: strom lehnt den T-Bar ab, solange ein Take oder
  die Aufblende nach Fade to Black die Input-Pads noch bewegt; lmix lehnt
  ihn in dieser Zeit selbst ab (`ActionError::TransitionRunning`, ohne die Stellung zu
  übernehmen, sonst blendete der Ton ohne Bild). Position 0 schickt dann
  nichts, das Ende (1) bleibt ein Schnitt. Der nächste Fader-Schritt
  danach greift.
- **Anderes Seitenverhältnis**: deckt das neue Bild das alte nicht ganz,
  blendet das alte mit dem T-Bar aus, damit am Ende keine Streifen
  verschwinden (wie beim animierten Schieben).

**Geprüft**: strom-Test auf dem CPU-Mischer (rot/blau, Schieben, Push, Dip,
Blende, zurück); lmix gegen strom auf CPU und CUDA: Schieben links bei 0,5
links schwarz, rechts Balken (0 | 221 bzw. 0 | 145), Blende bei 0,5 die
Hälfte der Balken, am Ende der Take.

## 2026-10-22 — T-Bar und Fade to Black folgen strom (SWI.3)

**Kontext**: lmix rechnet die Sperre während eines Takes ab dem Befehl,
strom ab dem Start der Animation (einige zehn ms später). Ein T-Bar-Schritt
in diesem Rest kam bei strom nicht an, lmix übernahm ihn trotzdem und
blendete den Ton (Review #123). Und strom hebt Fade to Black mit jedem
Take auf, lmix blieb auf „an“ und sperrte danach den T-Bar (Review #123,
Bestand).

**Entscheidung**:
- Lehnt strom einen T-Bar-Schritt ab, geht der T-Bar in lmix auf 0. Ton
  und Player folgen Program wie ohne T-Bar; die Ton-Befehle mit der
  Blende gehen gar nicht erst raus, die korrigierten an ihrer Stelle.
  0 stimmt in allen Fällen, in denen strom ablehnt: ein laufender Take
  setzt die Pads ohnehin zurück.
- Jede Aktion, die Program wechselt (Cut, Übergang, Stinger, Cut direkt,
  Layout), setzt `fade_to_black` auf aus, wie strom. Ein Cut direkt auf
  das Input, das schon auf Program liegt, erreicht in strom keinen Take
  (`take_source` lässt ihn aus); das Schwarz bleibt (Review #124).
- Ein Player, den der abgelehnte Schritt auf der Vorschau gestartet hat,
  verhält sich wie beim Zurückziehen auf 0: pausiert, wenn sein Auto-Play
  pausiert, sonst läuft er weiter.

**Geprüft**: `a_refused_t_bar_goes_back_and_keeps_the_sound` (Service,
NullEngine lehnt `TBar` ab), `a_take_lifts_fade_to_black_as_the_engine_does`;
beide scheitern ohne den Fix.

## 2026-10-22 — Latenz mit 8 Inputs: 56 ms, Ausreißer unter Last (NFA.3)

**Kontext**: Am 19.10. lag die Latenz Quelle → PGM meist bei 56 ms, ab und
zu bei 60–81 ms, mal je Input, mal je Lauf. Vermutung danach: Seit strom
inaktive Pads nicht abwartet, wartet der Mischer nur auf das Input auf
Program, und jedes Input läuft in eigener Bildphase (0–33 ms zum Raster
des Mischers).

**Messung** (RTX 3090, CUDA-Mischer, 8 Test-Inputs, strom
`lmix-pin-1019`, je Lauf strom und lmix-core neu gestartet,
`/api/latency/measure`):
- 15 Läufe, Input 1 auf Program, je 3 Messungen à 5 s: 12 Läufe
  56/56/56 ms, drei Läufe 65, 66 und 72–73 ms.
- 8 Läufe, jedes der 8 Inputs nacheinander auf Program, je 4 s: 7 Läufe
  alle 8 Inputs 56 ms; ein Lauf 56–108 ms.
- Alle erhöhten Werte fielen in 18:02–18:05, als daneben strom
  kompiliert und seine GPU-Tests liefen; davor und danach nur 56 ms.

**Ergebnis**: Mit 8 Inputs ohne Ausgänge hat auf freiem Rechner jedes
Input in jedem Lauf 56 ms; erhöhte Werte kamen hier nur mit Last
daneben. Das widerlegt die Bildphase je Input nicht: am 19.10.
vormittags (15 Inputs mit Aufnahme und SRT, ohne fremde Last) lag sie
je Input verschieden (74/60/56 ms). Offen also: dieser Aufbau auf freiem
Rechner mit dem Wiederholungs-Skript, und ob der feste Teil kleiner geht.
NFA.3 (≤ 66,7 ms) hält im Normalfall.

## 2026-10-22 — PR-Agent prüft zuerst, Claude-Agent gleicht ab

**Kontext**: Der Eigentümer hat Guthaben für die Claude-API. PR-Agent läuft
mit `anthropic/claude-opus-5-5`. Seit dem 18.10. legte Gitea 28 für die
Concurrency-Gruppe von `pr-agent.yml` keinen Lauf an (#126 behoben).

**Entscheidung** (Eigentümer, alle shrippen-Repos): Löst „PR-Agent nur auf
`/review` des Eigentümers“ vom 18.10. ab. Die Sitzung kommentiert `/review`
je Runde mit neuem Code oder neuer Basis, der Kommentar von PR-Agent ist
das Review. Ein
neuer Claude-Agent prüft die **Wichtig** früherer Runden (PR-Agent sieht
keine früheren Runden) und reviewt selbst, wenn PR-Agent zweimal scheitert.
Regel: `agent.md`, `REVIEW.md`.

## 2026-10-23 — Mischer-Ausgänge ohne Sync: Program 90 → 0–85 ms (NFA.3)

**Kontext**: Die feste Latenz von 56 ms (22.10.) war nicht die Latenz der
Ausgänge. Bildalter an Messpunkten im Mischer-Flow (Buffer-Age-Probes,
CUDA, Testbilder): Die Inputs kommen mit 0–5 ms am Mischer an, der
Compositor gibt Program mit 4–52 ms aus (je Lauf verschieden: er wartet
auf das nächste Bild des spätesten Inputs, die Bildphase). Danach hielt
die `intersink` von `pgm_out` (strom-Standard `sync=true`) jedes Bild bis
Zeitstempel + Latenz des ganzen Mischer-Flows (sein langsamster Zweig,
107–127 ms) und staute die Queue davor. Gemessen wurde bisher am Eingang
dieser Senke: 56 ms. Der Encoder einer Aufnahme bekam Program in jedem
Lauf mit 90 ms, den Ton mit 129 ms. NFA.3 (≤ 2 Frames, 66,7 ms) hielt für
die Ausgänge also nicht. Die Compositor-Werte `latency` und
`min-upstream-latency` (je 20 ms) ändern daran nichts (auch 0/0 ergab
56 ms an `pgm_out`).

**Optionen**: (a) Pipeline-Latenz des Mischer-Flows senken (Zweige
einzeln kürzen; jede neue Senke kann sie wieder heben); (b) die
inter-Ausgänge des Mischers ohne Sync.

**Entscheidung**: (b). Alle inter-Ausgänge des Mischer-Flows (Program,
Multiview, Vorschau, Programmton, Aux-Busse) mit `sync=false`. Den Takt
geben die Live-Aggregatoren (Bildmischer und Tonmischer laufen mit
`force-live`); die Ausgangs-Flows sind getrennte Pipelines mit
erhaltenen Zeitstempeln (Basiszeit 0), Encoder, Muxer und Senken richten
sich nach den Zeitstempeln. Die Slot-Flows behalten `sync=true`: Dort
taktet die Senke Clips aus Dateien.

**Messung** (RTX 3090, unter fremder Last: Load 7–125 durch eine andere
Sitzung; je Lauf strom und lmix-core neu, MKV-Aufnahme auf Program):
- 2 Inputs, CUDA, 12 Läufe: vorher Encoder immer 90 ms; ohne Sync
  0–62 ms (Mittel ~25 ms).
- 8 Inputs, je Input auf Program: CUDA vorher 90 ms am Encoder, ohne
  Sync 33/61/64 ms (`pgm_out` je Input 28–69 ms); CPU vorher 90 ms, ohne
  Sync 45–85 ms (unter Load 125, CPU-Mischer selbst langsam).
- GL, 4 Inputs, 3 Läufe (Load 132): vorher 90 ms, ohne Sync 9/14/68 ms.
- Abnahmetest `MIXER=cuda scripts/test-m2b-vm.sh` 47/47.

**Bild und Ton**: Vorher lagen sie fest 39 ms auseinander (90/129 ms),
jetzt je Lauf verschieden. Alle Ausgänge mit Bild und Ton paaren nach
Zeitstempeln (Muxer bei Aufnahme, SRT, RTMP; `ndisinkcombiner` bei
NDI); dort wartet das Bild auf den späteren Ton, der Gewinn ist also der
des Tons (an der Aufnahme 128–130 → 44–101 ms über alle Läufe). Voll wirkt er, wo nur Bild ankommt:
Bildschirm-Ausgänge, Live-Bilder der Console. Der Tonweg ist der nächste
Engpass. Die Latenz des Tonmischers (strom-Standard 30 ms) zu senken
bringt nichts: mit 30/20/15/10 ms je ein Lauf, 4 Inputs, 60 s Aufnahme,
Ton an `pgm_audio_out` je 48 ms (einmal 0 ms), Aussetzer gleich (je
einer, wohl Messartefakt). Das Alter ist dort das Zeitraster des
Ausgangs, kein Warten.

**Lippensynchronität** (Clip mit Blitz und Piep zur selben Zeit, jede
Sekunde, als einziger Input auf Program, CUDA, 25 s MKV-Aufnahme, je 3
Läufe): Ton nach dem Bild, `main` 21,5/52,0/54,2 ms, ohne Sync
32,8/40,5/49,0 ms (Median je Lauf). Unverändert; gemessen noch mit dem
alten `av-offset.py`, das bei Lücken um ganze Bilder danebenlag (siehe
„Ton in Aufnahmen hinter dem Bild“).

`/api/latency` misst weiter am Eingang von `pgm_out`; das ist jetzt das
Alter, mit dem die Ausgänge Program bekommen. Offen: die Bildphase
(0–33 ms je Lauf); mit freiem Rechner wiederholen.

## 2026-10-23 — Ton in Aufnahmen hinter dem Bild: AAC-Vorlauf

**Befund**: Der konstante Teil des A/V-Versatzes (DECISIONS 23.10.
„Mischer-Ausgänge ohne Sync“) ist der Vorlauf der AAC-Encoder.
`avenc_aac` stellt 1024 Samples vor den Ton, `fdkaacenc` 2048, stempelt
das erste Paket aber als ersten Ton, ohne Clipping-Meta; kein Muxer
erfährt davon. Gemessen mit einem Clip aus Blitz und Piep (gleichzeitig,
jede Sekunde), nur GStreamer, von FFmpeg und von GStreamer dekodiert:
`avenc_aac` 21,4 ms, `fdkaacenc` 42,7 ms Ton nach Bild, in MKV und MP4
gleich; rohes PCM 0 ms. In lmix: ISO-Aufnahme (Input direkt) 21 ms,
Program 21 ms + Bildphase (0–33 ms), SRT 21–46 ms. Betrifft jede
Aufnahme und jeden Stream mit AAC (lmix `add_aac`, strom SRT, RTMP,
Live-Recorder, Audio-Encoder-Block).

**Ausgleich** (strom #38, `dev` b262c87): jede Flow-Pipeline
verschiebt den Ausgang jedes AAC-Encoders (`avenc_aac`, `fdkaacenc`,
`faac`; `voaacenc` ungemessen, bleibt) um den Vorlauf früher
(`gst_pad_set_offset`, negativ) und meldet die Verschiebung als Latenz,
damit Live-Muxer darauf warten. Angehängt an `deep-element-added`, also
auch für lmix' eigenes `avenc_aac` in den Ausgangs-Flows und für Encoder,
die ein Block erst zur Laufzeit anlegt. Gemessen (`scripts/measure-av.sh`,
CUDA, 4 + 2 Läufe à 25–30 s, unter fremder Last ~115):

| | ohne Ausgleich | mit |
|---|---|---|
| ISO (Input direkt) | 21,0–22,0 ms | −0,3…+0,7 ms |
| Program | 19–39 ms | −31…+22 ms |

Was bei Program bleibt, ist die Bildphase des Compositors (innerhalb
eines Bildes, je Lauf fest). Ein Lauf springt einmal um ein Bild: der
Compositor wiederholt unter Last ein Bild, Ton und Bildzeitstempel
laufen dabei lückenlos durch. Als GObject-Eigenschaft (`pad_properties`
in einem Flow) geht ein negativer Offset nicht (Wertebereich ab 0),
darum in strom selbst.

Die früheren Befunde „Ton ganze Bilder daneben“ (mit Ausgleich −33/−67 ms)
und „ISO springt um ein Bild“ (+22/−12 ms) waren Messfehler: Ohne eigene
Zeitstempel ordnete `scripts/av-offset.py` bei Lücken in einer Aufnahme
Bilder und Töne um ganze Bilder falsch zu (Testdatei mit 300 ms Lücke:
−299…+368 ms statt 0). Seit dem Review von #131 nimmt es die Bildzeiten
aus demselben Dekodierlauf und füllt Lücken im Ton auf. Die Werte oben
und alle früheren (vor 23.10. 09:30) mit dem alten Skript gelten nur
ohne Lücken.

Startet eine Aufnahme mit ihrem Flow, verwirft der Muxer das erste
Paket (den Vorlauf); die ersten 21 ms Ton dekodieren ungenau, vorher war
dort Stille.

**Offen**: SRT und RTMP Ende-zu-Ende messen (ein Mitschnitt mit
`ffmpeg -i srt://… -c copy` streut schon ohne Ausgleich um ±500 ms und
taugt dafür nicht), Meldung an GStreamer (gst-libav, fdkaac).

## 2026-10-23 — Click-Dummy: einmalige Ausnahme von der Clean-Room-Regel

Der Owner hat für einen Durchgang erlaubt, Bildschirmfotos der Vorbilder
anzusehen (sonst nur Handbuchtext, claude.md Entscheidung 8). Angesehen:
das Hauptfenster und der Tonmischer aus der vMix-Hilfe, Arbeitsfläche,
Pult-Raster, Inspector und Licht-Dashboard aus der QLab-5-Doku, Umschalt-
und Tonseite von ATEM Software Control (Herstellerseite), OBS Studio
(Flathub). Übernommen wurden nur Anordnung und Abläufe, keine Gestaltung,
Namen oder Icons; der Dummy bleibt Kante (`docs/PLAN-SEITEN.md`,
„Click-Dummy 2“). Die Bilder liegen nicht im Repo. Was daraus in die
Umsetzung geht, nennt PLAN-SEITEN; neue Anforderungen daraus werden wie
üblich mit Quelle (Handbuchseite) in die Lastenhefte eingetragen.

## 2026-10-23 — Eine App mit Seiten statt zweier Programme (Konzept A)

**Entscheidung (User 23.10.)**: Statt `lmix-mixer` und `lmix-show` ein
Programm `lmix` mit Seiten (Mix, Show, Audio, Outputs, Setup; später Light)
und einer Seitenleiste am Fuß, wie bei Resolve. Weitere Pulte (Ton, Licht)
kommen als Seite, nicht als eigenes Programm. Mehrere Oberflächen greifen
weiter parallel auf `lmix-core` zu, auch übers Netz (Arbeitsplätze mit
Profil). Licht im Kern kommt dazu, aber später (todo). Ersetzt claude.md
Entscheidung 7 („zwei Oberflächen“) und SUI.1 (Lastenhefte und ROADMAP
nachgezogen). Was beschreibt, welche Programme heute laufen (BUILD,
HANDOVER, HARDWARE-TODO, GUI-SHELL, Skripte), ändert sich mit dem Code,
im PR der Shell.

Verworfen: feste Sendeleiste oben mit Arbeitsbereichen links (Konzept B,
kostet dauerhaft Höhe), getrennte Programme je Pult (vier Oberflächen zu
betreuen). Entwürfe und Click-Dummies: Leinwand „lmix Arbeitsbereiche“,
Plan und Reihenfolge in `docs/PLAN-SEITEN.md`.

Für die Shell entschieden (Click-Dummy 1): Panik global in der Leiste und je
Seite für ihren Bereich; globale Tasten immer, Seitentasten nur auf der
sichtbaren Seite; Seiten lösbar auf einen zweiten Bildschirm; im Netz dürfen
alle Arbeitsplätze alles, die Leiste zeigt, wer eine Seite zuletzt bediente.

## 2026-10-23 — Globale Tasten: ab Werk an, ein Fenster bindet

Mit einem Programm `lmix` (Seiten) sind die globalen Tasten (AUT.12) ab Werk
**an**, wie bisher in lmix Show (Owner 18.10.); die Vorgabe „im Mischer
aus“ entfällt mit dem Mischer-Programm. Damit zwei lmix-Fenster einen Druck
nicht doppelt auslösen, bindet nur ein Fenster je Rechner: das erste nimmt
eine Sperre (`portal::claim`, Datei im Laufzeitverzeichnis des Benutzers),
weitere zeigen „ein anderes lmix-Fenster bindet sie“ und übernehmen, sobald
es schließt (Wiederholung alle 3 s). Ersetzt „jedes Programm bindet seine
eigenen“ (2026-10-18). Geprüft mit `scripts/test-global-keys.sh` (Ersatz-
Portal); am echten KDE-Portal H.40.


## 2026-10-23 — Rechnerlast und Cue-Zeitgeber in der Oberfläche (RUN.8)

**Kontext**: User-Entscheidung RUN.8 (18.10.): Bekommt der Zeitgeber keine
Echtzeit-Priorität, warnt lmix. Bisher standen nur eine Zeile im Log und
`lmix_cue_timer_realtime 0` in `/metrics`; wer am Pult sitzt, sah nichts.
Der Branch `s1-rt` (17.10.) hatte dafür schon ein Lastereignis; sein
Zeitgeber mit Warteschlange ist verworfen (Eintrag vom 18.10.), das
Ereignis kommt hier auf den Stand von `main`.

**Entscheidung**: lmix-core meldet alle 2 s `Event::Machine`: CPU-Last
seit der letzten Meldung (`/proc/stat`), Last je Kern (1-Minuten-Mittel aus
`/proc/loadavg` durch die Zahl der Kerne), wie der Zeitgeber läuft
(`realtime`, `normal`, `demoted` nach der weichen RLIMIT_RTTIME-Grenze,
`stopped`) und ob in den letzten 30 s ein Cue-Schritt über seinem Budget
(10 ms) lag. „Ausgelastet“ ab 85 % CPU, ab Last 1,0 je Kern oder bei einem
späten Schritt. Über TCP als Thema `MACHINE` (auch in `ALL`), über
`/api/events` wie jedes Ereignis.

**Oberfläche**: Oben in der Kopfzeile steht gedämpft „CPU 34 %“ (Tooltip:
Zeitgeber und Last). Gibt es etwas zu warnen, steht dort statt dessen eine
Statusmarke: „Cues zu spät“, „Rechner ausgelastet“ oder „Keine Echtzeit“
(Warnung) und „Cue-Zeitgeber steht“ (Fehler); der Tooltip sagt, warum, und
bei „Keine Echtzeit“, wie man sie erlaubt (rtkit, Gruppe `realtime`).
Setup zeigt unter „lmix-core“ die Zeile „Cue-Zeitgeber“ mit demselben
Text. Ohne Verbindung verschwindet die Meldung.

**Grenze**: Die Last gilt für den Rechner von lmix-core, nicht für den der
Oberfläche. Schwellen sind Erfahrungswerte (DECISIONS 17.10.: Ausreißer bis
146 ms unter Volllast); sie liegen in `lmix-types` (`machine.rs`).

## 2026-10-24 — Einzelne Cues ändern: `update_cue` statt der ganzen Liste

**Anlass**: Der Show-Inspector (#143) schickte jede Änderung als
`set_cue_list` mit der ganzen Liste aus dem zuletzt empfangenen Zustand.
Zwei Änderungen kurz nacheinander (ein Name ohne Enter, dann ein Klick auf
einen Folge-Trigger) gingen beide vom selben alten Stand aus; die zweite
brachte den alten Wert der ersten zurück, ein verlorener Folge-Trigger fiele
erst in der Show auf (Review #143, Runde 4). Dasselbe gilt für zwei Clients.

**Entscheidung** (Owner, 24.10.): eine Aktion im Kern. `update_cue` nimmt
eine Liste (sonst die aktive), einen Cue (Nummer oder id, auch in Gruppen)
und `changes`, einen JSON Merge Patch des Cues (RFC 7386): nur die genannten
Felder ändern sich, verschachtelte Objekte (`sound`, `picture`, `fade`,
`stop_others`) werden zusammengeführt, `null` löscht ein Feld. Der Cue wird
mit seiner Liste geprüft wie bei `set_cue_list`; Standby und laufende Cues
bleiben. Die id ändert sich nicht, eine Liste aus einem Skript wird dort
bearbeitet (abgelehnt). Verworfen: eine lokale Kopie der Liste in der
Oberfläche (hilft nicht gegen zwei Clients) und Sperren bis zur Antwort
(spürbar beim schnellen Klicken).

## 2026-10-25 — Mix-Seite dichter: Monitore im Projektformat, Kacheln füllen

**Anlass**: Test der Mix-Ansicht (25.10.): Vorschau und Programm füllten die
halbe Höhe, mit Rand um ein festes 16:9-Bild, und ließen sich nicht
verkleinern; die Kacheln zeigten ein Bild von etwa 70×40 px; die Tonspalte
war fest 12 Rastereinheiten breit.

**Entscheidung**:
- Monitore im Seitenverhältnis des Programmformats (`show.format`), ohne
  Rand, jeder mittig in seiner Hälfte; die Übergangsspalte steht immer in
  der Mitte. Die Bildhöhe stellt eine Ziehleiste darunter
  (`KanteSplitHandle`, Doppelklick: Standard), gespeichert unter
  `view/monitorHeight`; die Breite folgt dem Format, höchstens die Hälfte.
- Kacheln: S/M/L (kleinste Breite 8 / 10,5 / 14,5 Rastereinheiten) in der
  Input-Leiste neben Kacheln/Tasten; so viele je Zeile, wie passen, sie
  teilen den Rest, das Bild im Programmformat. Kompakt (Bild und Name) oder
  Detail (dazu Overlay-Tasten und Tonleiste), `view/detail`. Das Raster
  nimmt die restliche Höhe und scrollt; „Reihen“ im Ansicht-Menü entfällt.
- Tonspalte: Breite per Griff (`view/mixerWidth`); Streifen umbrechen in
  Zeilen, bis vier Zeilen teilen sich die Höhe, ab der fünften scrollt sie.
  Der Master ist der letzte Streifen.

**Bricht**: die Einstellung `view/rows` wird nicht mehr gelesen.

## 2026-10-25 — Busse frei anlegen, Kopfhörer als fester Weg (AUD.2, AUD.4)

**Anlass**: Entwurf der Tonspalte (25.10., Zeichenfläche „Tonspalte: Master,
Kopfhörer und Busse fest“): unten stehen die Ausgangswege fest, Master und
Kopfhörer, dazu die Busse der Show. Sieben feste Busse A–G passten nicht
dazu: eine kleine Show braucht keinen, eine große mehr Namen als Buchstaben.

**Entscheidung**:
- Master und Kopfhörer gibt es immer; eine Show legt bis 16 Busse an
  (`add_bus`, `remove_bus`, `rename_bus`, `set_bus_tap`, `set_bus`,
  `set_bus_listen`). Jeder Bus hat eine feste Id (UUID) und einen Namen
  (1–40 Zeichen, eindeutig ohne Groß-/Kleinschreibung) und nimmt die Sends
  vor oder nach den Fadern. Aktionen nennen einen Bus per Position, Id oder
  Name (`BusRef`). Ausgänge speichern die Id, damit Umbenennen und
  Entfernen anderer Busse nichts verschieben. Gespeicherte Aktionen (Cues,
  Tasten, Auslöser) behalten den Bezug, wie er geschrieben ist, wie bei
  Inputs: eine Taste für alle Shows meint „Bus Stream“ oder „Bus 2“ in
  jeder Show, eine Id gäbe es dort nicht. Die Console schreibt Ids.
- Kopfhörer je Input: `phones { on, level }` (Standard an, 100), gesamt
  `phones { volume, mute, pre_fader }` (`set_phones_send`, `set_phones`,
  `set_phones_tap`). Solo (AFL/PFL) und Abhören eines Busses schalten den
  Kopfhörer exklusiv auf die Solos, wie vorher den Monitor.
- **Zonen (LASTENHEFT-SHOW S-E.4):** eine Zone ist ein logischer Ausgang
  mit Rechner-Zuordnung, ein Bus eine Mischung der Inputs. Darum hat ein
  Bus keine Geräteeinstellung; spielt er später in eine Zone (SND.23
  Live-Ton in Zonen, SND.6 Fade-Ziel Zone), sagt deren Patch
  (`targets.json`), welches Gerät er erreicht. Die feste Id ist dafür die
  Verbindung; 16 Busse decken die 16 Zonen.
- Engine: Aux 1 des strom-Mischers ist der Kopfhörer, Aux k+1 der k-te Bus;
  der Monitor folgt Aux 1 (`monitor_source`, strom #42). Ausgänge
  abonnieren `bus<id>_out`, nicht mehr die Busnummer.

**Bricht** (vor dem ersten Release erlaubt): Show-Datei Version 3.
`aux_buses`, `sends: [u8; 7]`, `monitor_volume` und `add_output … aux`
fallen weg, ebenso `set_aux_send`, `set_aux_bus`, `set_monitor_volume` und
`levels.monitor_peak_db`/`aux_peak_db` (jetzt `phones_peak_db` und
`buses: [{ bus, peak_db }]`). Migration beim Laden: genutzte Busse A–G
(mit Send, Ausgang, geänderter Lautstärke, Stumm oder einer gespeicherten
Aktion, die ihn nennt) werden „Bus A“ … mit A vor
den Fadern; Sends, Ausgänge, `monitor_volume` und gespeicherte Aktionen
(Cues, Tasten, Auslöser) ziehen mit, unbenutzte Busse fallen weg. Die
Tasten für alle Shows (`--shortcuts`) haben keine Version: migriert wird
eine Datei, die eine Aktion von Version 2 enthält (`set_aux_send`,
`set_aux_bus`, `set_monitor_volume`, `add_output … aux`), Aux B wird dort
„Bus B“. Enthält sie nur `fade_volume` auf einen Bus, bleibt die Zahl
stehen und meint nun die Position; eine solche Taste neu setzen. Die
Einstellungsdatei (SHW.8) hat eine Version: Version 1 wird beim Import
vollständig migriert, auch `fade_volume`; sie heißt jetzt Version 2.

**Offen**:
- ~~strom kennt Buszahl und Abgriff nur beim Aufbau~~ gelöst, siehe
  „Busse zur Laufzeit“ unten.
- Bus in den Master (strom routet Aux nicht in den Main), Bus → Zone.
- Tonspalte nach dem Entwurf (feste Wege unten, Höhengriff, Kürzungen,
  KH/S/⋯ je Kanal) als eigener Schritt; dieser Schritt stellt das Bus-Panel,
  die Send-Matrix, den Kanalzug und den Ausgangsdialog um.

**Geprüft**: lmix-types- und lmix-core-Tests (Migration, Busse kommen und
gehen mit neuem Mischer, Ausgang hält seinen Bus, Kopfhörer-Sends und
Bus-Abhören), console-qt-Tests, strom-Mischer-Tests,
`scripts/test-console-qt.sh` und `scripts/test-m2b-vm.sh` lokal mit dem
strom aus #42 (Busse, Kopfhörer, Abhören eines Busses).

## 2026-10-25 — Busse zur Laufzeit (AUD.2)

**Anlass**: Hinzufügen, Entfernen und Abgriffwechsel eines Busses bauten den
ganzen Mischer-Flow neu auf (Program stand kurz, laufende Ausgänge starteten
neu, die Console fragte vorher). Nutzer: „gehe den Neuaufbau des Mischers an“.

**Entscheidung**:
- strom legt Aux-Busse zur Laufzeit an und entfernt sie
  (`POST`/`DELETE /api/flows/{flow}/blocks/{block}/aux_buses`): der Block
  hält `max_aux_buses` Plätze vor, gebaut werden nur die belegten
  (`dynamic_aux_buses`, gespeichert in den Block-Properties, damit ein
  Neuaufbau dieselben Busse baut). Der Abgriff `chN_auxM_pre` schaltet live
  um. Vorgehaltene Plätze kosten nichts: Sends mit Queue entstehen erst
  mit dem Bus.
- lmix: Aux 1 bleibt der Kopfhörer (statisch, `num_aux_buses=1`), jeder
  Bus ist ein dynamischer Aux mit eigenem Kanal `bus<id>_out`; Kapazität
  1 + 16. Ändern sich nur die Busse, patcht `set_format` den laufenden
  Mischer (entfernen, Abgriffe, hinzufügen) statt neu zu bauen. Die
  Rückfrage in der Console entfällt, die Sperre gilt für diese Aktionen
  nicht mehr (sie unterbrechen nichts).

**Offen**: Sperre für den Formatwechsel (todo), Bus in den Master, Bus → Zone.

**Geprüft**: strom-Mischer- und API-Tests (Bus zur Laufzeit an, ab,
Abgriff), lmix-core-Tests (`buses_change_the_running_mixer`),
console-qt-Tests, `scripts/test-m2b-vm.sh` und `scripts/test-console-qt.sh`
mit strom #43 (Busse zur Laufzeit) und #44 (Test-Race), beide gemergt
25.10.; Sends und Abhören einer Busänderung gehen in einem PATCH.

## 2026-10-25 — Fremder Stopp kurz nach einem eigenen Befehl (INP.FB)

**Anlass**: `watch_failures` überging jeden Stopp eines Flows bis 1 s nach
einem eigenen Befehl darauf (`OWN_STOP_WINDOW`): Das ist der Stopp des alten
Flows, den der Befehl ersetzt. Gesehen am 25.10.: Ein Slot-Flow stoppte 0,64 s
nach dem Failback („input back“). Der Eingang blieb „online“, der Flow stand,
und es gab keinen neuen Failback.

**Entscheidung**: Läuft der Befehl noch, bleibt der Stopp übergangen (der
alte Flow läuft aus). Hat er den Flow gestoppt oder den Slot geleert, ist ein
Stopp im Fenster sein eigener (eine neue Show leert Slots, bevor sie sie
wieder baut). Hat er ihn gebaut und ist das Fenster noch offen, fragt lmix-core
die Engine, ob der Flow jetzt läuft (auch READY oder PAUSED, ein Flow im Anlauf) (`Engine::flow_running`, bei strom
`GET /flows/{id}`). Ohne Auskunft gilt er als laufend: eine Engine, die es
nicht weiß, strom mit einem Fehler oder ohne Antwort binnen 2 s
(`FLOW_CHECK_TIMEOUT`; der Watcher wartet darauf). Nur wenn er läuft, war
es der alte Flow. Sonst gilt der Stopp als Ausfall, mit Failback wie sonst.

**Geprüft**: lmix-core-Tests (`a_foreign_stop_just_after_an_own_command_is_a_failure`,
`a_stop_after_an_own_emptying_is_own`, `a_flow_runs_unless_strom_says_otherwise`,
schlägt ohne den Fix fehl; `own_rebuild_stop_is_no_failure`,
`stop_during_a_slow_own_rebuild_is_no_failure` unverändert grün).

## 2026-10-25 — Sperre und Wiederholung beim neuen Mischer (SAF.6, OUT.1)

**Anlass**: Zwei Punkte aus den Reviews von #150/#151 (todo). Format,
Abtastrate und Multiview bauen den Mischer weiter neu auf: Program steht kurz,
laufende Ausgänge starten neu. Die Sperre ließ das zu, obwohl sie das Stoppen
von Ausgängen verbietet. Und scheiterte der Neuaufbau (strom antwortet nicht,
der Flow erreicht PLAYING nicht), galt der Wunsch trotzdem als umgesetzt:
dieselbe Wahl noch einmal tat nichts.

**Entscheidung**:
- Bei Sperre lehnt lmix-core einen neuen Wert für Format, Abtastrate und
  Multiview ab (`locked`); derselbe Wert ist keine Änderung und geht durch, außer ein neuer Mischer steht noch aus (dann gilt er als neu und die Sperre lehnt ihn ab).
  Tooltip und Hinweis der Sperre in der Console nennen die drei.
- Scheitert der Neuaufbau, vergisst `set_format` in der Engine die gebauten
  Busse: Der nächste `SetFormat`, auch mit demselben Wunsch, baut neu. Bis
  dahin lehnt die Engine Befehle an Busse ab (sie kennt sie nicht).
- Der Switcher merkt sich einen neuen Mischer, bis ein `SetFormat` durchging
  (`mixer_pending` ab `restore_commands`, also auch beim Öffnen einer Show und
  beim Restore; der Service meldet `mixer_built`). Bis dahin gilt dieselbe
  Wahl von Format, Rate oder Multiview als neu und schickt alles noch einmal;
  eine Busänderung schickt ebenfalls den ganzen Mischer, denn die Engine baut
  ihn dann. Bei Sperre lehnt lmix-core sie darum in dieser Zeit ab.
- Die Console stellt nach einer Ablehnung den Stand des Kerns wieder her
  (Multiview-Schalter, Abtastrate).

**Geprüft**: lmix-core-Tests (`the_lock_refuses_closing_and_stopping`,
`a_failed_new_mixer_is_built_again`, `a_format_the_engine_refused_is_tried_again`,
`a_bus_change_before_the_new_mixer_is_built_sets_up_all`,
`a_restored_mixer_is_pending_until_built`).

## 2026-10-26 — Neustart von lmix-core übernimmt den Mischer wieder

**Anlass**: Die CI scheiterte nach dem Merge von #152 auf main: Nach dem
Absturztest baute lmix-core den laufenden Mischer neu, der Neubau kam nicht
in PLAYING, Program blieb weg. Lokal nachgestellt: Nach jedem Neustart von
lmix-core startete der Mischer neu, mit und ohne Busse. Der Test prüfte nur,
dass er den Absturz selbst überlebt.

**Ursache**: lmix schreibt ganze Zahlen als `UInt`, strom gibt sie aus seinem
gespeicherten Flow als `Int` zurück. `buses_of` (Kopfhörer- und Bus-Plätze)
und `sample_rate_of` kannten nur `UInt`; die Busse galten als unbekannt, und
das `SetFormat` der Wiederherstellung baute den Mischer neu. Seit den freien
Bussen (25.10.) so.

**Entscheidung**: Beim Zurücklesen gelten `Int` und `UInt` (`whole`, wie
`slot_count` es schon tat). Der VM-Test zählt die Starts des Mischers über den
Neustart.

**Offen**: Warum der Neubau in der CI nicht in PLAYING kam (strom: „Failed
to start flow“, 30 s nach dem Neustart). Ein gewollter Neubau nach einem
Neustart (Mischer von vor den Laufzeit-Bussen, anderes Format) geht denselben
Weg; todo.

**Geprüft**: lmix-core-Test `mixer_publishes_its_buses` (Flow mit
vorzeichenbehafteten Zahlen), Neustart gegen echtes strom mit und ohne Busse
(ein Start vorher, keiner danach), `scripts/test-m2b-vm.sh`.

## 2026-10-26 — Tonspalte: Inputs oben, Ausgangswege fest unten (AUD.2, AUD.4)

**Anlass**: Entwurf der Tonspalte (25.10., Zeichenfläche „Tonspalte: Master,
Kopfhörer und Busse fest“). Die schmale Spalte am Rand der Mix-Seite zeigte
nur Inputs und den Master. Kopfhörer, Busse, Solo und Sends gab es nur auf der
Ton-Seite.

**Entscheidung**:
- Oben scrollen die Inputs, unten stehen die Ausgangswege fest: Master, dann
  Kopfhörer, dann die Busse, „+ Bus“ in ihrer Kopfzeile (der Entwurf hatte ihn
  als Zelle; als Zelle rutschte er in eine eigene Zeile, die man erst
  scrollen musste). Ein Griff dazwischen setzt die Höhe der Ausgangswege,
  mindestens eine Zeile je Teil (`view/mixerPathsHeight`; 0 = so viele
  Zeilen wie nötig, höchstens die halbe Spalte, dahin auch der Doppelklick).
- Je Input: M (stumm), S (Solo, nach dem Fader), KH (auf den Kopfhörer),
  ⋯ (Sends an Busse, Kanalzug, Ton folgt Bild, Ton-Seite) und darunter die
  Lautstärke auf dem Kopfhörer. „Solo aus“ in der Kopfzeile, solange ein Solo
  oder Abhören an ist.
- Je Bus: der Name (ein Klick öffnet sein ⋯-Menü mit dem Namen im Feld;
  Enter oder Schließen benennt um, Esc nicht), darunter, was er
  führt (`3 IN`, vor den Fadern `PRE 3`, höchstens 5 Zeichen), M, ⋯ (Abgriff,
  Ausgänge mit ihm, Bus löschen mit Rückfrage) und S (auf dem Kopfhörer
  abhören). Master (PGM) und Kopfhörer (MON) haben M und ⋯; der Kopfhörer
  wählt dort seinen Abgriff. Der Master hat kein S: lmix-core kennt kein
  Abhören des Masters.
- Zu lange Namen kürzt `forms::short_name` (Rust, getestet, `Core.shortName`):
  feste Kürzel deutsch und englisch (Master → MSTR, Kamera/Camera → CAM …),
  dann Vokale weg, dann erstes Wort gekürzt und Anfangsbuchstaben; Zahlen
  bleiben ganz. Der volle Name steht im Tooltip. Die Zeichenzahl folgt der
  Breite des Streifens.
- Das Namensfeld steht im Menü, nicht im Streifen: im Streifen behielt es
  den Fokus, weil Klicks auf Knöpfe und Fader ihn nicht nehmen, und die
  Tasten der Show (Ziffern für Übergänge) landeten im Busnamen.
  Fokussiert wird das Feld erst in `onOpened`: ein Klick, der das Menü eben
  geschlossen hat, öffnet es nicht wieder, und ein Feld im geschlossenen
  Menü behielte die Tasten. Umbenannt wird beim Schließen, nicht bei jedem
  Fokusverlust (ein Fensterwechsel benannte sonst halb Getipptes um).
- Wechselt der Inhalt des Menüs vor dem Öffnen, maß es die Größe des vorigen
  und legte sich unten über den eigenen Knopf. Behoben in `KantePopover.openAt`
  (Kante #81, legt die Spalte vor dem Messen an); lmix hat Kante 5633df3 (1.45,
  Pegelmesser `showScale` statt `scale`).
- „Ausgang zuordnen“ aus dem Entwurf fehlt: es gibt keine Aktion, die den Bus
  eines bestehenden Ausgangs ändert (todo).

**Geprüft**: console-qt-Tests (`short_names_fit_and_keep_their_numbers`),
`scripts/test-console-qt.sh` (M an Input und Master, S und „Solo aus“, KH,
„+ Bus“, Umbenennen mit Enter und Esc, M, S und Abgriff am Bus, Send im
Menü, Griff der Ausgangswege) und Screenshots auf
Xvfb mit zehn Inputs und zwei Bussen.

## 2026-10-26 — Neubau des Mischers: strom stoppt auch einen laufenden Start

**Anlass**: In der CI kam der neu gebaute Mischer zweimal nicht in PLAYING
(Läufe 40255351441 bei `set_master_format`, 40246938342 bei der
Wiederherstellung nach dem Neustart von lmix-core). Program blieb weg, jeder
weitere Befehl an den Mischer scheiterte. Im strom-Log: `intersink`
`pgm_audio_out` „An active producer already exists“, also lief noch ein
Mischer mit denselben inter-Kanälen.

**Ursache** (in strom): `start_flow` trägt die Pipeline erst nach dem Start
in seine Tabelle ein. Ein DELETE in diesem Fenster fand nichts zu stoppen,
die Pipeline lief ohne Flow weiter. Nachgestellt mit dem Mischer-Flow von
lmix: Start, 50 ms später DELETE, neu anlegen, neu starten; strom meldete in
4 von 4 Runden „Pipeline already running“ (die alte Pipeline mit der alten
Definition), in anderer Reihenfolge scheitert der Start wie in der CI. Wer
in der CI den überlappenden Start auslöste, ist nicht belegt (naheliegend:
ein Start, den das getötete lmix-core noch abgeschickt hatte).

**Entscheidung**: strom #45 hält je Flow eine Sperre über Start, Stopp und
Löschen; ein Löschen wartet den Start ab und stoppt dann, was er gebaut hat.
lmix ändert nichts.

**Geprüft**: strom-Test `test_delete_during_start_stops_the_pipeline` (ohne
den Fix rot), Nachstellung mit dem strom-Binary des Branches (0 von 4), VM-
Test in der CI.

## 2026-10-26 — Ausgang nachträglich einem Bus zuordnen (AUD.2)

**Anlass**: Den Bus eines Ausgangs wählte nur „+ Ausgang“; das Menü ⋯ des
Busses in der Tonspalte nannte darum nur, welche Ausgänge ihn führen.

**Entscheidung**:
- Aktion `set_output_bus` (Gruppe „output“, Recht Bearbeiten): Ton aus dem
  Bus oder, ohne Bus, wieder der eigene der Quelle. Ein Bildschirm hat keinen
  Ton (`no_audio`).
- Ein laufender Ausgang spielt weiter: lmix-core setzt am Inter Input
  `pgm_audio` des Ausgangs-Flows den Kanal (`SetOutputSound`), strom stellt
  den `intersrc` im Betrieb um und speichert den Kanal (strom #46), damit
  ein Neustart des Flows in strom ihn behält. Lehnt strom den Wechsel ab,
  startet lmix-core den Ausgang mit dem Ton der Show neu: auf Sendung liegt
  nie ein anderer Ton als angezeigt. Scheitert auch das, ist der Ausgang
  ausgefallen und der Failback startet ihn wie nach einem Absturz immer
  wieder (auch über einen Neustart der Engine hinweg). Dieser Neustart
  übergeht die Sperre wie jeder Failback. Nur ein Ausgang,
  der dabei einen Tonzweig bekommt oder verliert (ISO eines Inputs ohne Ton),
  startet neu; das lehnt die Sperre ab, den reinen Tonwechsel nicht.
- In der Tonspalte steht im Menü ⋯ des Busses jeder Ausgang mit Ton als
  Haken: an = Ton dieses Busses, aus = eigener Ton der Quelle.

**Geprüft**: Switcher-Test `a_running_output_takes_another_bus_on_air`,
Engine-Test `an_output_takes_another_sound_on_air`, Service-Tests
`an_output_that_keeps_its_old_sound_starts_anew` und
`an_output_that_could_not_take_the_other_sound_fails_back`, strom-Test
`an_inter_input_moves_to_another_channel_while_playing` (hört nach dem
Wechsel den anderen Producer), `scripts/test-console-qt.sh` (Haken im
Bus-Menü), VM-Test (laufender SRT-Ausgang auf den Bus und zurück, Kanal in
strom, kein Neustart). Gemessen statt gehört: „Buswechsel ohne Lücke“ (26.10.).

## 2026-10-26 — Buswechsel ohne Lücke: der Ton eines Ausgangs in einem Segment (AUD.2)

**Anlass**: Hörtest aus #151/#158 (todo): knackt der Abgriff-Wechsel eines
Busses, knackt oder reißt der Buswechsel eines laufenden Ausgangs? Statt
zu hören misst `scripts/measure-switch-gaps.sh`: ein Pattern-Input mit
Sinus (440 Hz) auf Program und auf dem Bus „Stream“, eine Aufnahme nimmt
Program und wechselt zehnmal auf den Bus und zurück, dann wechselt der Bus
zehnmal vor/nach Fader. `scripts/switch-gaps.py` sucht in der Aufnahme
Lücken (≥ 2 ms weit unter dem Ton) und Knackser (Block von 5 ms, der nicht
als ein Sinus weitergeht); eine Sekunde Stummschaltung vorher legt die Uhr
der Aufnahme fest.

**Befund**: Der Abgriff-Wechsel ist sauber (keine Lücke, kein Knackser).
Jeder Buswechsel ließ 19–39 ms Stille mit Knacksern an beiden Rändern; die
Zeitstempel des Tons sprangen dabei um 5–26 ms zurück. Der `intersrc`
bekommt vom anderen Producer (dessen Appsink) dessen Segment und Puffer,
deren Raster bis zu einen Puffer früher liegt; der AAC-Encoder fing auf dem
neuen Segment neu an und schrieb seinen Vorlauf als Stille (die Lücke
besteht aus Nullen). Ein `audiorate` davor half nicht.

**Entscheidung**: Im Ausgangs-Flow geht der Ton hinter dem Inter Input
`pgm_audio` durch ein `identity single-segment=true` (Element `sound`), für
SRT, RTMP, Aufnahme und NDI. Es rechnet die Zeitstempel in Laufzeit um und
schluckt neue Segmente: der Encoder sieht einen Strom. Bild und Ton bleiben
zusammen, weil Muxer nach Laufzeit verschachteln.

**Geprüft**: A/B unter gleicher fremder Last (Load ~40), je drei Läufe mit
21 Wechseln: vorher 14, 17, 13 Lücken, jetzt 0, 0 und einmal 9 (Aussetzer
unter Last auch außerhalb der Wechsel). Ohne fremde Last zweimal 0 Lücken
(Vorversuch, dasselbe Element nur in der Aufnahme). SRT, wie ein Empfänger
es bekommt (`OUTPUT=srt`, ffmpeg kopiert den Strom), je zwei Läufe: vorher
14 und 12 Lücken, jetzt 1 und 0. RTMP und NDI nicht gemessen (kein
Empfänger hier); RTMP kodiert den Ton im Block wie SRT.
Lippensynchronität (`scripts/measure-av.sh`, CPU-Mischer): ISO 0 ms wie
vorher, Program −25…+18 ms je Lauf, im bekannten Bereich der Bildphase.
Engine-Test `every_output_takes_its_sound_in_one_segment`.

**Offen**: Ein kurzer Knackser je Buswechsel bleibt (Phasensprung, Rest
etwa −9 dB gegen den Ton): Program und Bus liegen leicht gegeneinander
versetzt, ein harter Schnitt zwischen ihnen springt. Ob er stört, entscheidet
ein Hörtest (todo).

## 2026-10-27 — SDI-/HDMI-Karte als Input und Ausgang (SRC.1, OUT.4)

**Anlass**: Auf dem Fedora-Rechner steckt eine DeckLink Quad 2 (acht
Anschlüsse, Desktop Video 16.4, GStreamer 1.28.8). Ein Kabel verbindet zwei
Anschlüsse (Loop): was lmix ausgibt, kommt auf dem anderen wieder an.

**Entscheidung**:
- Neuer Input `card` (`device`, `mode` oder automatisch) und neuer Ausgang
  `card` (`device`, `mode`) mit den Blöcken `builtin.decklink_input` bzw.
  `builtin.decklink_output` von strom. Die Modi heißen wie im Treiber
  (`1080p25`, `1080i50`, `720p50` …, `CardMode`). strom listet die Karten
  jetzt in der Geräteerkennung (strom #49); lmix zeigt sie als Geräte der
  Art `card`, die Nummer ist GStreamers `device-number`.
- Input: Bild hinter `deinterlace` (Halbbilder werden Vollbilder), Ton
  S32LE 48 kHz Stereo wie von der Karte.
- Ausgang: `videoconvert` → `videoscale` → `videorate skip-to-first=true` →
  UYVY in Größe und Rate des Modus, bei Halbbild-Modi `interlace
  top-field-first=true field-pattern=1:1` (Standard wäre Telecine 2:3, das
  nimmt die Karte nicht); Ton S32LE 48 kHz. `skip-to-first`: das erste Bild
  trägt die Laufzeit des Mischers (Stunden), `videorate` füllte sonst alles
  ab Segmentanfang mit Kopien auf.
- Die Karte taktet nach der Uhr (`sync` bleibt an). Zwei strom-Änderungen
  (Branch `decklink-output-clock`): der Block belegt beim ersten Bild die
  Kalibrierung der Kartenuhr vor (sonst plante `decklinkvideosink` die
  ersten vier Sekunden Bilder 9 h in die Zukunft, die Warteschlange der
  Karte lief voll, danach lehnte sie jedes Bild ab), und eine Vorlaufzeit
  `processing_deadline_ms`. lmix setzt sie auf 150 ms (`CARD_LEAD_MS`):
  der Ausgangs-Flow sieht über `inter` weniger Latenz als der Mischer hat
  (160 gegen 173 ms), mit strom-Standard 20 ms kam jedes Bild ~20 ms zu
  spät, die Karte zeigte die Hälfte verspätet und verwarf den Rest. Für den
  ersten Start reichten schon 40 ms. Für spätere Starts auf derselben Karte
  (siehe Offen) nicht: mit 60 ms scheiterte einer von zwei Läufen zu je vier
  Starts, mit 100 ms zwei von drei, mit 150 ms bestanden zwei von zwei
  (8 Starts). Das ist eine kleine Stichprobe; der Preis sind 150 ms mehr
  Latenz am Kartenausgang.
- Verworfen: `sync=false` am Sink. Dann plant `decklinkvideosink` jedes
  Bild auf die Kartenzeit „jetzt“, umgerechnet als wäre sie Laufzeit; nach
  der Kalibrierung ist das Zeit 0 und das Bild steht nach wenigen Sekunden.
  `decklinkaudiosink` meldet mit `sync=false` keine Latenz und verwirft
  jeden Tonpuffer als zu spät (die Karte spielte keinen Ton).
- Kartenprofil: Die Anschlüsse 5–8 ließen sich erst nach Umstellen des
  Kartenprofils auf `two-sub-devices-half` (Halbduplex) öffnen; das ist eine
  Einstellung der Karte (bleibt gespeichert), nicht von lmix.

**Geprüft**: `scripts/test-sdi-loop.sh` (Hardware, nicht in CI) mit
CUDA-Mischer: je Modus läuft der Ausgang ohne abgelehntes Bild, der Input
erkennt den Modus, das Bild kommt wie das Programm zurück und bewegt sich,
der Ton kommt mit dem Pegel des Programms zurück (±0,1 dB), nach dem Anlauf
zeigt die Karte kein Bild verspätet und verwirft keines. 1080p25, 1080i50
und 720p50 bestanden.

**Offen**: Startet ein Kartenausgang im selben strom-Prozess ein zweites
Mal (Stopp/Start, Moduswechsel), sind mit System-GStreamer alle Bilder
verspätet. Eine Ursache liegt in GStreamer (`decklinkvideosink`, auch in
main): die Wiedergabe der Karte beginnt bei der Uhr-Epoche (wo die letzte
Wiedergabe endete, irgendwo im Bildraster), die Bildzeiten rundet der Sink
aber auf Vielfache der Bilddauer. Ein Patch, der die Epoche aufs Raster
legt (`gst_decklink_video_sink_start_scheduled_playback`), behebt das in
der Reproduktion ohne strom ganz (150 von 150 Bildern verspätet → 0). Mit
strom und lmix genügt er allein nicht: der Abstand zwischen geplanter
Bildzeit und Kartenuhr schwankt je Start zwischen etwa −6 und +25 ms und
wächst mit dem Vorlauf nicht mit; einzelne Starts scheiterten auch mit
positivem Abstand. Erst mit Patch und 150 ms Vorlauf bestanden alle
Starts. Die übrige Ursache ist ungeklärt (Verdacht: die Kartenuhr setzt
ihren Startpunkt erst beim ersten Lesen nach dem Start). Ohne Patch hilft
bis dahin nur ein Neustart von strom (todo).

**Geprüft danach** (150 ms, Patch): `scripts/test-sdi-loop.sh` mit
`MODES="1080p25 1080i50 1080p25 720p50"` zweimal hintereinander bestanden,
kleinster Abstand 7,9 ms, kein Bild verspätet, 0–3 verworfen beim Anlauf.

## 2026-10-27 — SDI-Karte: 16 Kanäle, Ton-Delay, PsF (SRC.1, OUT.4)

**Entscheidung**:
- Input: `pair` (1–8) wählt das eingebettete Kanalpaar, das der Input
  hört. Die Karte nimmt die wenigsten Kanäle, die es enthalten (2, 8, 16),
  `audioconvert` mit `mix-matrix` greift die zwei heraus.
- Ausgang: `channels` (2, 8, 16). Paar 1 trägt den Ton des Ausgangs,
  `more_pairs` legt je weiteres Paar einen Bus auf (oder Stille). Jedes
  Paar kommt per `mix-matrix` auf seine Kanäle, mehrere summiert ein
  `audiomixer`. Ein Bus auf einem Paar lässt sich nicht löschen, solange
  der Ausgang ihn nimmt (wie beim Ton des Ausgangs).
- Ausgang: `sound_delay_ms` (0–500) spielt den Ton später als das Bild,
  mit strom-Block `builtin.time_offset` (Pad-Offset). Der inter-Puffer des
  Tons wächst um das Delay.
- PsF (`1080psf25`, `1080psf2997`): der Treiber kennt keine PsF-Modi; die
  Karte läuft im Halbbild-Modus derselben Feldrate (`1080i50`). Ausgang:
  `interlace field-pattern=2:2` teilt jedes Vollbild in seine zwei Felder.
  Input mit PsF-Modus: `capssetter` nimmt nur die Kennzeichnung
  „interlaced“ weg, kein Deinterlacer. Auf „Erkennen“ gilt PsF als 1080i.
- Verworfen: `identity ts-offset` für das Delay. Es verschiebt keine
  Zeitstempel, sondern nur die eigene Wartezeit bei `sync=true`
  (gemessen: 0 und 500 ms Delay gleich, 467/463 ms).

**Geprüft**: `scripts/test-sdi-loop.sh` (Loop, CPU-Mischer): `1080psf25`
hin und zurück (Bild wie Program, bewegt, Ton auf Pegel, kein Bild spät);
8 Kanäle mit Bus auf Paar 2 und 16 Kanäle mit Bus auf Paar 7 (Input fest
auf `1080psf25`): Paar stumm ohne Send auf den Bus, mit Send hörbar. Zeit
vom Send bis zum Ton am Input: 431 ms ohne, 1000 ms mit 500 ms Delay.

**Befund nebenbei**: die Kanal-Verzögerung der Inputs (AUD.6, DECISIONS
„Kanalzug“) nutzt dasselbe `identity ts-offset` und ist damit vermutlich
wirkungslos; der VM-Test prüft nur den gesetzten Wert (todo).

## 2026-10-27 — Kanal-Verzögerung wirkte nicht (AUD.6)

**Befund**: Die Verzögerung je Input (`set_channel_strip … delay_ms`) lag
als `identity` mit `ts-offset` vor dem Mischer-Eingang. `ts-offset`
verschiebt keine Zeitstempel; es zählt nur, wenn `identity` selbst
synchronisiert (`sync=true`). Der Ton kam nie später. Der VM-Test prüfte
nur, dass der Wert gesetzt war. Gefunden beim Ton-Delay der Karte
(DECISIONS „SDI-Karte: 16 Kanäle“).

**Entscheidung**: strom-Block `builtin.time_offset` statt `identity`
(`adelay{slot}`). Er setzt einen Pad-Offset: der Mischer sieht die
Puffer später und mischt sie später. `offset_ms` ist live änderbar
(PATCH auf die Block-Eigenschaften). Der strom-Patch `lmix-live-offset`
(`ts-offset` von `identity` live erlaubt) wird in lmix nicht mehr
gebraucht.

**Geprüft**: `scripts/test-channel-delay.sh` (neu, lokal): Program mit
einem Clip, der gleichzeitig blitzt und piept, aufgenommen ohne und mit
300 ms Verzögerung, live umgestellt. Vorher 0 ms Verschiebung, jetzt
300 ms. Der VM-Test liest den Wert jetzt am Block.


## 2026-10-27 — Karten-Schalter

**Befund**: Eine DeckLink-Karte nimmt je Anschluss nur ein Programm.
Läuft lmix, kommt keine andere Software an die Karte, auch wenn die Show
sie gerade nicht braucht.

**Entscheidung**: Ein Schalter für alle Karten (`set_cards { on }`,
`ShowState.cards_on`), in der Werkzeugleiste der Console. Er steht beim
Start des Core immer auf aus und wird nicht gespeichert: die Karten
nimmt lmix nur, wenn jemand es will. Aus heißt:

- Karten-Inputs bekommen statt der Karte einen schwarzen Flow mit dem
  Banner „Card disabled“ und Stille (`Command::CardOff`); der Input
  bleibt in der Show, Layouts und Program sehen schwarz.
- Laufende oder ausgefallene Kartenausgänge stoppen und stehen auf
  `cards_off`; ein Start in diesem Zustand wartet. Beim Einschalten
  starten genau diese Ausgänge, die Inputs nehmen die Karte wieder.
- Der Failback eines Kartenausgangs endet, solange die Karten aus sind.
- Die Sperre (`set_lock`) lehnt das Ausschalten ab, weil es Ausgänge
  stoppt; Einschalten geht. Ein Gruppen-Stopp beendet auch das Warten.
- Ausnahme vom „aus beim Start“: Startet der Core neu, während strom
  noch einen Flow mit Karte laufen hat (Absturz in der Show), übernimmt
  er die Karten als an (`Engine::holds_cards`, vor `restore`). Die Karte
  ist dann ohnehin nicht frei, und ein Neustart soll den Beamer nicht
  abschalten. Auf der Karte geprüft: Core beendet und neu gestartet,
  Ausgang läuft weiter; mit Karten aus bleibt es aus.

Ein Schalter je Karte wäre feiner, aber die Karten hängen an einem
Treiber, und die Frage ist im Betrieb „darf lmix an die Karten“.

**Geprüft**: Test `cards_are_used_only_while_switched_on`;
`scripts/test-sdi-loop.sh` prüft beim Start und nach dem Ausschalten das
schwarze Bild mit Banner und dass einfaches GStreamer auf beiden
Anschlüssen spielt und aufnimmt, danach läuft der Ausgang wieder.
Ausreißer bei späten Bildern und der Delay-Messung treten auch ohne den
Schalter auf (todo „GStreamer-Fehler melden“).

## 2026-10-27 — Wege rein und raus sperren (RUN.9)

**Entscheidung**: `set_io_lock { class, on }` sperrt einen Weg einzeln:
`osc`, `midi`, `tcp` (Steuerung rein) und `targets` (HTTP- und
Home-Assistant-Anfragen an die Ausgabeziele). Gesperrt lehnt lmix ab, was
auf dem Weg kommt (`io_locked`, OSC antwortet `/lmix/error`, TCP `ER`),
außer Panik und harter Stopp, auch als Kurzbefehl, der nur das tut (über
MIDI kommt eine Panik nur so): eine Sperre soll eine Probe ruhig halten,
nicht den Notaus nehmen. Entsperren geht nur über einen anderen Weg
(sonst entsperrte ein fremdes Pult sich selbst). Gesperrte Ziele senden
nichts; das Ereignis meldet `locked`, „Ziele prüfen“ prüft nichts, und
jeder Cue, der an ein Ziel sendet, zeigt die Warnung „network out is
locked“. Wie die Karten ist jede Sperre beim Start des Core aus und steht
nicht in der Show (Probe-Zustand, nicht Show-Inhalt).

HTTP und WebSocket sind nicht sperrbar: die Console spricht darüber, und
lmix unterscheidet die Console nicht von anderen HTTP-Clients. Dafür gibt
es die Zugriffsschlüssel (Rechte je Platz). Ausgehendes OSC (Abos, feste
Empfänger) und Tally (TSL, HTTP) bleiben offen; sie melden nur, sie lösen
nichts aus.

Console: Setup → Steuerung → „Sperren“, ein Schalter je Weg.

**Geprüft**: Tests `a_locked_way_in_refuses_all_but_a_panic`,
`a_midi_panic_pad_goes_through_the_lock`,
`locked_targets_send_nothing_and_warn` (der Empfänger sieht keine
Verbindung).

## 2026-10-27 — Cues pausieren (RUN.5)

**Entscheidung**: `cue_pause` und `cue_resume` wählen wie `cue_stop`
einen Cue (Nummer oder Id), alle Cues einer Liste oder alle Cues; ein
Cue nimmt die Cues mit, die er gestartet hat (Gruppen). Pausiert stehen
seine Zeiten still: Vorwartezeit, Dauer, Nachwartezeit und Folge kommen
nach dem Fortsetzen um die Pausenlänge später (der Runner verschiebt
alle Zeitpunkte des Laufs). Ein Klang- oder Bild-Cue pausiert dazu sein
Medium (`pause_sound`/`pause_picture`, strom #51): der Klang wird still,
ein Video behält sein Bild auf der Bühne, beide laufen an der Stelle
weiter. Ein Cue in der Vorwartezeit hat noch kein Medium, er wartet nur.

- Eine laufende Blende (Fade-Cue, Ausblendung beim Stopp) läuft in der
  Pause weiter; strom führt sie in der Zeit der Zonen. Eine Blende
  anzuhalten bräuchte einen eigenen Weg in strom, und im Betrieb ist
  eine Pause mitten in einer Blende selten.
- Stopp, Panik und Reset erreichen pausierte Cues wie laufende.
- Ein neues GO während einer Pause startet normal; „Alle pausieren“ ist
  kein Zustand der Show, nur eine Aktion auf das, was gerade läuft.
- Console: in der Seitenleiste der Show je Cue ⏸/▶ und oben „Alle
  pausieren“ (bzw. „Alle fortsetzen“, sobald etwas pausiert ist); ein
  pausierter Cue zeigt „pausiert“ und seine Zeit steht. OSC
  `/lmix/cue/pause` und `/lmix/cue/resume` (Liste oder alle).

**Geprüft**: Tests `a_paused_wait_holds_its_time_and_follow`,
`a_paused_sound_cue_pauses_its_sound`, `a_group_pauses_with_its_children`,
`a_paused_cue_stops`; in strom `a_paused_sound_resumes_where_it_was`
und `a_paused_video_holds_its_frame_and_resumes` (beide scheitern ohne
die Sperre).

## 2026-10-27 — Pads (RUN.12)

**Entscheidung**: Pads sind eine Cue-Liste mit `pads: true`
(`set_cue_list_pads`, mit der Show gespeichert), kein eigenes Objekt: so
gelten Bearbeiten, Farben, Stopp und Panik wie für jede Liste. Auch
als Notfall-Liste (SAF.4) laufen Pads mit: die Panik startet ihre Cues
direkt, ohne GO.
Eine Pad-Liste hat kein GO (abgelehnt) und ist nie die aktive Liste:
`set_active_cue_list` lehnt sie ab, und ohne Wahl ist die erste Liste
aktiv, die keine Pads ist. Ihr Standby bleibt unberührt.

Ein Pad ist ein Knopf: `cue_toggle` startet den Cue wie `cue_start` oder
stoppt ihn, solange er läuft (mit seiner Ausblendung). Companion braucht
dafür kein Modul: `/lmix/cue/toggle <Liste> <Cue>` (neu, ebenso
`/lmix/cue/start`), die Rückmeldung „läuft“ kommt wie bisher über einen
OSC-Abonnenten (CTL.4).

Console: unter der Show-Ansicht ein Raster je Pad-Liste (bei mehreren eine
Auswahl), ein Knopf je Cue der obersten Ebene, laufende hervorgehoben, die
Cue-Farbe als Streifen darunter. Im Listen-Dialog der Schalter „Pads“;
GO, Zurück und Standby fehlen dort für Pads. Die Auswahl der Liste in der
Show-Ansicht zeigt nur Listen mit GO.

**Geprüft**: `pads_take_no_go_and_toggle`, `a_list_becomes_pads`, OSC-Test
für `/lmix/cue/toggle` und `/lmix/cue/start`.

## 2026-10-27 — Cues markieren und „gelaufen“ (SUI.12)

**Entscheidung**: Eine Markierung ist ein Feld des Cues (`flagged`), gesetzt
und gelöscht über `update_cue` (`{"flagged": true}`) und mit der Show
gespeichert: wer in der Probe einen Cue zum Nacharbeiten markiert, findet
ihn morgen wieder. Ohne Markierung steht das Feld nicht in der Datei.
Listen aus einem Skript werden dort bearbeitet und lassen sich daher in
der Console nicht markieren.

„Gelaufen“ ist dagegen Laufzeit: der Runner merkt sich jeden Cue, der
gestartet wurde (auch Cues in Gruppen), bis zum Reset (RUN.11) oder zur
nächsten Show; `ShowState.ran_cues`. Ein laufender Cue zählt als
laufend, nicht als gelaufen.

Console: in der Cue-Liste ein ⚑ je Zeile (beim Bearbeiten umschaltbar, im
Show-Modus nur sichtbar, wenn gesetzt); gelaufene Zeilen bekommen einen
leichten Hintergrund (Kante `tint1Color`), Standby und Auswahl behalten
ihre Farben.

**Geprüft**: `started_cues_count_as_ran_until_a_reset`,
`a_cue_is_flagged_and_unflagged`.

## 2026-10-27 — decklink-Plugin mit Patch, Vorlauf 60 ms mit ihm

**Befund**: Der Patch für die Epoche (oben, „SDI-/HDMI-Karte“, Offen)
reichte allein nicht; spätere Starts zeigten mit ihm noch 150–205
verspätete Bilder. Zwei weitere Ursachen in `decklinkvideosink`, beide
mit `GST_DEBUG=decklinkvideosink:6` belegt:

- Die Uhr des Sinks zählt erst ab ihrem ersten Lesen nach dem Start der
  Karte (`gst_decklink_clock_get_internal_time`). Kommt das Lesen später
  als `StartScheduledPlayback`, hinkt die Uhr der Wiedergabe der Karte
  hinterher (gemessen 38 ms), jedes Bild wird so viel zu spät geplant.
- `render()` rundet die Bildzeit auf das Bildraster ab. Damit frisst
  das Runden bis zu eine Bilddauer (40 ms bei 25 fps) des Vorlaufs,
  den das Warten davor lässt; ein Bild, gedacht für 0,193 s, geplant
  für 0,160 s, kam um 0,163 s bei der Karte an.

**Entscheidung**: lmix liefert GStreamers decklink-Plugin mit einem
Patch aus drei Teilen aus (`packaging/gstreamer/decklink-start-on-frame-grid.patch`):
Epoche aufs Raster, Uhr beim Start verankern, auf das nächste Raster
aufrunden. `scripts/decklink-plugin.sh` baut nur `sys/decklink` aus
dem gst-plugins-bad-Release, das zur installierten GStreamer-Version
passt (Prüfsumme gegen Übertragungsfehler; sie kommt vom selben Server,
ist also keine Herkunftsprüfung), und legt es in ein Verzeichnis
für `GST_PLUGIN_PATH`; es baut neu, wenn sich Version oder Patch ändern.
`run-local.sh`, `test-sdi-loop.sh` und `measure-sdi-latency.sh` nehmen
es (`DECKLINK_PLUGIN=system` lässt das installierte), die systemd-Unit
von strom zeigt mit `GST_PLUGIN_PATH` auf `~/.local/share/lmix/gstreamer`.
Der Vorlauf sinkt mit dem Patch auf 60 ms; ohne bleibt er bei 150 ms,
denn beide Ursachen treffen auch den ersten Start (bis 38 + 40 ms). Weil
lmix-core nicht sieht, welches Plugin strom lädt, setzt ihn
`--card-lead-ms` (`LMIX_CARD_LEAD_MS`): Die Skripte setzen 60, sobald
sie das Plugin gebaut haben, in der systemd-Einrichtung steht er in
`core.env` (Review #169).

Verworfen: strom neu starten, wenn ein Kartenausgang neu startet (nimmt
alle Flows mit), und ein höherer Vorlauf (half gegen die Epoche nicht,
kostet Latenz). Die Meldung an GStreamer (`docs/upstream/gstreamer-decklink-restart.md`,
jetzt mit allen drei Teilen) bleibt beim Owner; mit dem Fix upstream
entfällt das eigene Plugin.

**Geprüft**: `scripts/test-sdi-loop.sh` mit allen vier Modi im selben
strom (je Lauf 4 Starts): mit Patch und 150 ms 3 von 3 Läufen ohne
verspätetes oder verworfenes Bild, mit 60 ms ebenso 3 von 3. Nur mit
dem Epochen-Teil: 153–205 verspätete Bilder bei einem Teil der Starts.

## 2026-10-27 — Zonen-Leiste (SUI.8)

**Entscheidung**: Jede Zone bekommt im Wiedergabe-Flow hinter ihrem
Ausgang einen Meter-Block von strom (`builtin.meter`, `zone_meter_<i>`),
also nach der Zonenlautstärke: die Leiste zeigt, was zu hören ist, auch
bei einer Zone, die auf diesem Rechner nirgends spielt. Die Pegel stehen
in `/api/audio/levels` unter `zones` (je Name, in der Reihenfolge der
Show).

Lautstärke und Stumm einer Zone merkt sich der Core jetzt (`gain_db`,
`mute` in `ShowState.zones`): `set_zone_level` und Blenden-Cues auf eine
Zone setzen den Zielwert, die Leiste zeigt ihn sofort, nicht den Weg der
Blende. strom kennt Stumm nur als Lautstärke 0 und behält ohne Pegel die
aktuelle Lautstärke; darum schickt lmix bei jeder Änderung Pegel und
Stumm zusammen: eine Blende hält eine stumme Zone still (der neue Pegel
wird gemerkt), Entstummen bringt den gemerkten Pegel zurück. Vorher
machte eine Blende eine stumme Zone hörbar, und Entstummen ohne Pegel
ließ sie still. Wie ein Fader gehört er nicht zur Show: eine andere Show beginnt
bei 0 dB, eine entfernte Zone vergisst ihren Wert. Baut lmix den Flow
neu (Zonen geändert, strom neu gestartet), setzt es die gemerkten Werte
sofort wieder; vorher fing ein neuer Flow still bei 0 dB an.

Console: `ZoneBar` mit Meter, Regler (−60 bis +12 dB, wie Klang-Cues)
und Stumm je Zone, unter der Show-Ansicht und im Reiter Zonen der
Audio-Seite.

**Geprüft**: `a_zone_keeps_its_level_in_the_state_and_a_new_flow`,
`a_zone_change_sends_its_whole_level`, `zones_are_metered_by_index`, `zones_are_levelled_by_name`, der Flow-Test;
`scripts/test-show-playback.sh` mit echtem strom (Zone hat Pegel, solange
ein Klang spielt; nach der Blende steht −20 dB im Zustand).

## 2026-10-29 — Plugins: Steuerung als WASM, Companion, fremde GStreamer-Plugins

**Kontext**: Die Show soll mit Geräten und Programmen sprechen, die wir nicht
alle vorhersehen. Recherche mit Stichprobe von 11 Companion-Modulen und
Auszählung von 600 ausgelieferten (`bitfocus/companion-bundled-modules`):
nur 12 haben native Teile, 599 nutzen noch Base-API 1.x, die Repos meist
schon 2.x. Bericht: https://claude.ai/artifact/RdoFocHWJ6FKVfXihcD3Cn

**Entscheidung, Steuer-Plugins**:
- Eigenes Format `lmix-plugin/1`: WebAssembly-Komponenten (Component Model,
  WIT-Welt `lmix:plugin@1`) in einem eigenen Prozess `lmix-plugin-host`
  (wasmtime, ein Store je Verbindung). Der Kern spricht JSON-RPC mit dem
  Host; ein Plugin-Fehler lässt nur seine Aktion fehlschlagen.
- Netz über ein eigenes `lmix:net` (TCP, UDP, TLS, WebSocket) statt
  `wasi:sockets`, dazu `lmix:timer`, `lmix:serial`, `wasi:http`. Rechte im
  Manifest hängen an der Konfiguration (`$config.host`), der Host prüft jede
  Verbindung. Ereignisgetrieben, weil Imports aus JS unter WASI 0.2
  synchron sein müssen.
- `exec` (eigener Prozess, gleicher JSON-RPC) bleibt als Ausnahme mit Warnung:
  USB-HID, Hersteller-SDKs, externe Programme, GPL-Plugins Dritter, Python.
- Sandbox: Landlock, `no_new_privs`, cgroup für Host und exec-Plugins.
- SDKs: Rust und JS (ComponentizeJS) zuerst, Python nur über `exec`.
- Verteilung: von Hand und über einen Index auf GitHub mit
  minisign-Signaturen. GPL-Plugins Dritter nur als getrennte Programme, nie
  im lmix-Paket. Eine Show bringt nie Code mit.
- Reihenfolge: der Plugin-Host kommt nach K1 (OSC-Cue, lmix-Modul für
  Companion aus S2).

**Entscheidung, Companion**: erst koppeln (K1), dann ein Prototyp (K2), der
Module in GitHub-CI automatisch nach WASM übersetzt (esbuild mit Shims für
`@companion-module/base` 1.x/2.x und Node-APIs, ComponentizeJS). Node nur in
dieser CI, nie zur Laufzeit von lmix. Übersetzte Module mit freier Lizenz
(MIT, ISC, BSD, Apache-2.0) kommen mit ihrer Lizenz und Prüfstatus in den
Index.

**Entscheidung, Bild und Ton**: kein eigenes Plugin-Format. Lokale Quelle
und Senke zuerst über PipeWire, später unixfd. Fremde GStreamer-Plugins
sind erlaubt: lmix führt eine Liste getesteter Plugins mit Versionsnummer;
alles, was nicht darauf steht, warnt beim Installieren und beim Hinzufügen
der Quelle („kann Performance- und Stabilitätseinbußen mit sich bringen“).
Nach einem Absturz von strom ordnet lmix ihn einem Plugin zu (Crash-Handler
in strom schreibt Stack und Speicherbelegung, lmix-core löst sie beim
Neustart über die Registry auf) und schlägt vor, es abzuschalten. Der
Crash-Handler kommt in den strom-Fork und später als Vorschlag zu Eyevinn.

**Verworfen**: Prozess je Plugin als einziger Weg (WASM bietet Isolation
ohne Prozess je Plugin), Extism und Javy (keine echten Imports, keine
Timer), Companion-Module mit Node zur Laufzeit hosten, native Plugins im
Kern.

## 2026-10-29 — Live-Kacheln aus einem Bild auf der GPU

**Entscheidung**: Die Kacheln der Console kommen nicht mehr als ein Strom je
Slot (CPU: Download, `videoscale`, `unixfdsink`, `glupload`), sondern als ein
Bild: strom hat am Bildmischer einen Ausgang `tiles_out` (eigener Fork), der
jeden Input auf der GPU des Mischers in seine Zelle eines Rasters setzt
(Slot n in Zelle n, 640×360, Spalten von lmix-core vorgegeben). Mit dem
CUDA-Mischer geht dieses Bild, wie Programm und Vorschau, per
`cudaipcsink`/`cudaipcsrc` (`ipc-mode=mmap`; im Legacy-Modus kann die Console
das zweite Bild aus derselben Engine nicht öffnen) zur Console und verlässt
die GPU nie. Die Console zeichnet es einmal und schneidet die Zellen aus.
Mit GL- oder CPU-Mischer läuft dasselbe Bild über `unixfdsink`.

Dazu: Desktop-OpenGL statt GLES (Qt nahm auf Wayland mit NVIDIA GLES, und
`cudadownload` kopiert nur mit Desktop-GL in eine Textur) und der threaded
Render-Loop (Qt nahm hier den basic Loop, der jedes Bild im GUI-Thread zeichnet).

**Gemessen** (`scripts/measure-tiles.sh`, 31 Inputs, CUDA, RTX 3070 Ti,
Mix-Seite in kopflosem KWin):

| | vorher | jetzt | ohne Fenster |
|---|---|---|---|
| strom | 508 % | 374–385 % | 369 % |
| lmix | 149 % (GUI-Thread 39) | 41 % (GUI-Thread 3) | – |
| GPU | 37 % | 25 % | 14 % |

Die Kacheln kosten damit rund 57 % CPU statt 290 %; der zusätzliche
Compositor im Mischer ist ohne Console nicht messbar. Ein Versuch mit einem
Shader je Kachel auf einer gemeinsamen Textur statt `ShaderEffectSource` je
Kachel brachte nichts Messbares und wurde verworfen.

**Nachtrag, am selben Tag:** Das Fenster zeichnete 60 Bilder/s bei 30 fps
Quellen. Nur das Kachelbild allein ergab schon 60: Jede `ShaderEffectSource`
(auch mit `live: false` und `scheduleUpdate()`) fordert nach einem Wechsel
ihrer Quelle einen zweiten Frame an. Darum zeichnet jetzt jede Kachel ihren
Ausschnitt der Textur selbst (`AtlasCell`, C++, ein `QSGImageNode`); das Bild
kommt über einen `appsink`, der es für GL abbildet (erst dann füllt
`cudadownload` die Textur) und drei Bilder hält. Der `qml6glsink` von
`TilesAtlas` bleibt nur für den geteilten GL-Kontext und bekommt keine
Bilder. Dazu: strom liefert das Kachelbild in RGBA (`tiles_format`, eigener
Fork), `glcolorconvert` läuft durch; die Bridge hält das jeweils erste von
Programm und Kachelbild bis zu 4 ms an, damit beide in einen Frame fallen
(`frame_gate`); die verbundene Pille „lmix-core“ atmet nicht mehr (ihre
Animation hielt das Fenster immer auf 60 Bildern/s).

| 31 Inputs, CUDA | vorher | jetzt |
|---|---|---|
| lmix | 42–46 % | 28–30 % |
| davon Render-Thread | 25–26 % | 12–16 % |
| davon GL-Thread (gstglcontext) | 10 % | 6–7 % |
| Frames/s des Fensters | 60 | 43–51 |
| GL-Mischer: lmix | 79 % | 67 % |

RGBA allein brachte 3–4 Punkte (gstglcontext 10 → 7 %). Die Schranke allein
nicht messbar, mit `AtlasCell` 60 → 43–51 Frames/s. Die übrigen Frames
kommen von Bildern außer Takt (Pegel 10/s, Ton-Seite). Lastenheft: Das
Programmbild wartet höchstens 4 ms (0,12 Frames) und kommt meist als
letztes, also ohne Wartezeit (NFA.3: ≤ 3 Frames hinter Program, gemessen
72 ms Bildalter am Sink); Kacheln weiter mit voller Bildrate (OUT.11), alles
auf der GPU (NFA.1, G.2).

## 2026-10-29 — Entfernen mit Rückfrage und Rückgängig

Regel 1 aus `docs/DESIGN-DIALOGE.md`. Inputs, Layouts, Busse und Ausgänge
fragen vor dem Entfernen (roter Balken, die Frage als Titel, der Knopf mit
Verb und Objekt; Enter behält). Danach bietet ein Toast „Rückgängig“ an,
Ctrl+Z ebenso.

- **Ein Stapel im Kern:** lmix-core hält die letzten zehn Entfernungen
  aller vier Arten in einem Stapel. `restore` (vorher `restore_input`, nur
  für Inputs) holt die oberste zurück, an ihren alten Platz und mit ihren
  Einstellungen. Ein Bus kommt mit den Sends der Inputs zurück. Hat ein
  anderer Bus inzwischen seinen Namen, heißt er „Name (2)“.
- **Was sich inzwischen geändert hat, gilt:** Ein Layout kommt ohne die
  Inputs zurück, die seither gegangen sind, und auf einem freien
  Bild-im-Bild-Platz. Ein Ausgang kommt gestoppt zurück, ohne einen
  inzwischen entfernten Bus.
- **Abgelehnt bleibt es liegen:** Sind alle Layouts belegt oder alle Busse,
  lehnt `restore` ab, und die Entfernung bleibt die oberste.
- `restorable` im Zustand nennt jetzt Art und Name (`{kind, name}`); der
  Toast der Console bietet Rückgängig nur, solange es seine Entfernung ist.
- **Bricht:** `restore_input` heißt `restore`, `restorable` ist ein Objekt
  statt eines Namens. Gespeicherte Aktionen (Show, Einstellungen,
  Kurzbefehle für alle Shows) liest lmix-core unter dem neuen Namen
  (`RENAMED_ACTIONS` in show.rs, ohne Alias im Typ); OSC-Aufrufe mit
  `restore_input` gehen nicht mehr (vor dem ersten Release).
- Nicht im Stapel: Zonen, Ziele, Cues, Auslöser, Belegungen und Vorlagen
  (Regel 1 nennt sie auch; sie folgen), sowie Stoppen und Werkseinstellungen
  (nicht zurückzuholen, nur mit Rückfrage).

## 2026-10-30 — Inputnamen eindeutig

Aktionen wählen Inputs auch per Name; zwei „Cam 1“ trafen immer den
ersten. Bisher machte nur der Import aus einer anderen Show den Namen
eindeutig. Entscheidung des Users: wie bei Bussen (INP.2).

- **Eingetippt wird abgelehnt:** Anlegen mit Namen (Console-Feld
  „optional“, API, OSC) und Umbenennen auf einen vergebenen Namen lehnt
  lmix-core ab (`duplicate_input_name`), Groß-/Kleinschreibung egal. Die
  Console zeigt es als Toast wie beim Bus. Der eigene Name in anderer
  Schreibung bleibt erlaubt.
- **Selbst vergeben wird nummeriert:** Name aus der Quelle (Anlegen ohne
  Namen), Rückgängig nach Entfernen und Import heißen „Name (2)“,
  „Name (3)“ …
- **Suche wie Eindeutigkeit:** Aktionen, Show-Prüfung und Rückmeldungen
  (Tally/Feedback) finden einen Input per Name ohne Groß-/Kleinschreibung
  („cam 1“ findet „Cam 1“). Layouts, Ausgänge, Cue-Listen und Kurzbefehle
  suchen eigens und bleiben, wie sie sind.
- **Bricht:** Shows mit doppelten Namen benennt das Laden einmal um; der
  erste behält den Namen (ihn trafen Aktionen schon), die späteren werden
  nummeriert. Eine Aktion, die einen späteren Doppelgänger in anderer
  Schreibung nannte („cam 1“ neben „Cam 1“), trifft danach den ersten.

## 2026-10-30 — Kartenausgang über decklink2sink (OUT.4)

**Anlass**: GStreamer hat auf die Meldung zum verspäteten zweiten Start
(work item 5391, `docs/upstream/gstreamer-decklink-restart.md`)
geantwortet: `decklink2sink` (neues Plugin `decklink2`, seit 20.10. auf
`main`, ab GStreamer 1.30) ist der Weg, `decklinkvideosink` wird künftig
fallengelassen und bekommt den Fix nicht. Am 29.10. bestand decklink2,
gegen 1.28.8 gebaut, 10 von 10 Läufen und 12 Starts mit Moduswechsel.

**Entscheidung**:
- strom-Block `builtin.decklink_output` spielt über `decklink2sink`. Der
  Sink nimmt Ton nur an den Bildern (als Meta), darum laufen Bild und Ton
  mit Ton durch `decklink2combiner` (Pads `video`, `audio`; 2, 8 oder 16
  Kanäle S32LE wie bisher); Bild allein geht direkt in den Sink, Ton
  allein bekommt schwarze Bilder. Die Uhr-Vorbelegung für
  `decklinkvideosink` (27.10.) ist entfernt: decklink2 bietet keine
  eigene Uhr und plant die Bilder nach eigener Zählung.
- Fehlt das Plugin, scheitert der Kartenausgang mit „decklink2sink not
  found: … GStreamer 1.30 … GST_PLUGIN_PATH“; lmix-core hängt „build it
  with scripts/decklink-plugin.sh“ an.
- `scripts/decklink-plugin.sh` baut `sys/decklink2` aus einem festen
  GStreamer-`main`-Commit (Commit-Id geprüft, Sparse-Checkout per git)
  gegen die installierte GStreamer nach `~/.local/share/lmix/gstreamer`
  (`LMIX_PLUGIN_DIR`); `gst_plugin_set_static_features_flag()` (erst 1.29)
  wird für ältere GStreamer per Makro übergangen. Hat das System
  decklink2 schon, baut es nichts. Der Patch für `decklinkvideosink` und
  `DECKLINK_PLUGIN=system` sind weg (vor dem ersten Release ohne
  Übergang); ein altes `libgstdecklink.so` im Plugin-Ordner löscht das
  Skript.
- Karten-Inputs bleiben auf `decklinkvideosrc`: dort ist kein Fehler
  bekannt, und Geräteerkennung und Modus-Erkennung hängen daran. Umstellen
  auf `decklink2src`, wenn GStreamer den alten Teil fallen lässt. Die
  Geräte von `decklink2deviceprovider` (Klasse `Video/Audio/…`) landen in
  strom unter „Other“ und doppeln die Karten in lmix nicht.
- Vorlauf (`CARD_LEAD_MS`, `--card-lead-ms`) 20 ms statt 150 (60 mit
  Patch), GStreamers Standard: decklink2 hält die Bilder in einer
  Warteschlange auf der Karte (Vorlauf 7 Bilder, nachgefüllt durch
  Wiederholen, wenn sie unter 3 fällt) statt sie auf Uhrzeiten zu legen;
  ein über inter spätes Bild wartet dort, statt zu spät zu kommen.
  Gemessen gaben 0 und 20 ms dieselbe Latenz und kein spätes Bild.
- Nebenbei behoben: Der Mischer der Kanalpaare (`card_pairs`,
  `audiomixer`) begann bei Laufzeit 0 und füllte erst die Stunden bis zur
  Laufzeit des Mischers auf; ein Bus auf Paar 2 kam darum 24 s und mehr
  zu spät (todo 27.10., nicht Karte oder Treiber, wuchs mit der Laufzeit
  des Rechners). Mit decklink2 hielt das sogar das Bild an, weil der
  Combiner auf den Ton wartet. Jetzt `start-time-selection=first`.

**Geprüft** (Quad 2, Desktop Video 16.4, GStreamer 1.28.8, Fedora 44,
CUDA-Mischer; andere Agenten bauten parallel, Last 1–27):
`scripts/test-sdi-loop.sh` mit allen vier Modi und Paaren dreimal
bestanden (kein Bild spät, verworfen, wiederholt oder übersprungen;
Paar 2 nach 0,7–1,0 s; Ton-Delay 200 ms als 136–329 ms gemessen, die
Messung ist grob). Skript aus der Meldung mit `decklink2sink`, je 12
Starts mit Moduswechsel (1080p25, 1080i50, 720p50) in einem Prozess:
bei Last 0,4 und 20 kein spätes, verworfenes oder doppeltes Bild, bei
Last 42 zwei Starts mit solchen Bildern (Einzelheiten nicht
festgehalten; das Testbild selbst kam unter der Last vermutlich zu
spät). Latenz SDI-Loop 1080p25
(`scripts/measure-sdi-latency.sh`, Last unter 1,1): 433, 400, 433 ms mit
20 ms Vorlauf, 433, 366, 400 ms mit 0 ms; vorher (decklinkvideosink,
150 ms) 266–300 ms.

**Nachtrag (Review strom #53)**: Endet oder stockt der Ton, hielt der
Combiner das Bild an (auf der Karte 11 statt 97 Bilder in 4 s). Der Ton
geht jetzt über einen `audiomixer` mit stiller Live-Quelle in den
Combiner. Die Stille übernimmt Format, Rate und Kanäle des ersten Tons,
der Mixer reicht den Ton also unverändert durch; spätere Formatwechsel
wandelt er, Kanal- oder Ratenwechsel brechen ab (in lmix kommen sie
nicht vor: `card_sound` legt die Caps fest, eine andere Kanalzahl baut
den Flow neu). Eine Eigenschaft für die Kanalzahl braucht der Block
nicht. Kommen 500 ms nach dem ersten Bild keine Caps am Ton (etwa ein
inter-Kanal ohne Produzent), startet die Stille selbst, in S32LE, 48 kHz
und den Kanälen, die `card_sound` anbietet: Das Bild läuft weiter, statt
ohne Meldung schwarz zu bleiben. Hinter dem Mixer hält ein Capsfilter
den Ton auf S32LE. Ihre Puffer sind 480 Samples (10 ms) lang: der
Mixer kostet dann laut Latenz-Query 10 ms (sein Ausgabepuffer), mit den
1024 Samples von `audiotestsrc` waren es 21 ms. Der Ausgang bietet nur
noch die Formate an, die `decklink2sink` kennt (kein 12-bit-RGB),
Unbekanntes scheitert mit Meldung statt Panic. Danach
`test-sdi-loop.sh` bestanden, Latenz 400, 434, 400 ms (Last 1,0 bis 1,6). Der Loop misst nur auf
etwa 33 ms genau (die Werte springen in diesen Schritten zwischen 366,
400 und 433 ms); die 10 ms des Mixers zeigt er nicht, er sieht nur, dass
nichts um einen ganzen Schritt dazukam.

**Nachtrag (Puffer der Karte)**: decklink2 war so etwa 130 ms langsamer,
weil es 7 Bilder vorhält (280 ms bei 25p, 140 ms bei 50p). Der strom-Block
hat jetzt `buffered_frames` (2–12, Standard 7 wie GStreamer; zwei unter dem `max-buffered-frames` 14 des Sinks, über dem er verwirft): Er setzt
`n-preroll-frames`, `min-buffered-frames` folgt zwei darunter (1 bis 3,
GStreamers 3 bei 7). lmix stellt 3 ein (`CARD_FRAMES`, `--card-frames`).
Mit 2 Bildern verwarf oder wiederholte die Karte schon ohne strom in 2
von 6 Starts Bilder. Geprüft unter Last (Load 32–35: `soak-hardware.sh`
mit 16 Inputs, CUDA, Aufnahme, ISO und 2 SRT-Streams, dazu 22 CPU-Brenner
und Speicherkopien): `test-sdi-loop.sh` dreimal bestanden, 18 Starts in
vier Modi ohne spätes, verworfenes oder wiederholtes Bild („Underrun“
zählt mit); Latenz SDI-Loop 1080p25 267, 300, 267 ms (auf leerem Rechner
267, 234, 267 ms), mit 7 Bildern 400–433 ms. Der Soak lief dabei ohne
Fehler weiter (Latenz Quelle → Program 26–28 ms).
