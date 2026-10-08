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
