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
letzten zehn, solange die Show offen ist). `set_lock` sperrt Schließen von
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
