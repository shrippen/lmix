# Build

Reproduzierbare Build-/Run-Kommandos für unseren Anwendungsfall: Server headless
auf 127.0.0.1, nativer egui-Client als eigener Prozess. Für alles, was hier
nicht steht (Docker, PostgreSQL, TLS, ...), siehe `workspace/strom/docs/`.

## Systemabhängigkeiten

Rust-Toolchain: `workspace/strom/rust-toolchain.toml` pinnt 1.97.1, wird aber
nur über `rustup` automatisch angezogen. Ohne `rustup` (z. B. Arch-Paket
`rust`) baut der Workspace auch mit einer neueren Version (getestet: 1.98.1) —
`cargo build`/`cargo run` funktionieren; nur `cargo clippy` kann durch neue
Lints in der falschen Version rot werden (siehe Kommentar in
`rust-toolchain.toml`).

GStreamer + Cairo + graphviz + libnice (Debian/Ubuntu, aus
`workspace/strom/docs/DEVELOPMENT.md`):

```bash
sudo apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  libgstreamer-plugins-bad1.0-dev gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly gstreamer1.0-libav \
  gstreamer1.0-tools libnice-dev gstreamer1.0-nice \
  libcairo2-dev graphviz
```

**Nicht nötig**: `wasm32-unknown-unknown`-Target, `trunk`. Die Web-GUI ist für
uns kein Ziel (Entscheidung 3). Fehlt `trunk`, fällt `backend/build.rs`
automatisch auf eine Platzhalter-Seite zurück und kompiliert normal weiter —
kein Fehler, keine Warnung, die man beheben müsste.

**Nicht nötig**: `cmake`/`libclang-dev` — nur für das optionale `efp`-Feature,
das wir nicht aktivieren.

## Bauen

```bash
cd workspace/strom

# Backend, mit dem no-gui-Feature (siehe unten, "GUI/Server-Trennung")
cargo build --features no-gui

# Nativer Client, eigenes Binary im selben Workspace
cargo build -p strom-frontend
```

`cargo build` ohne `-p` baut wegen `default-members = ["backend"]` in der
Workspace-`Cargo.toml` nur das Backend (inkl. Platzhalter-Frontend-Assets
für die eingebettete Web-GUI). Für den nativen Client ist `-p strom-frontend`
nötig — das weicht bewusst von der Konvention aus `workspace/strom/CLAUDE.md`
("never use the -p flag") ab, weil unser Prozessmodell den Client als eigenes
Binary braucht, nicht als eingebettete GUI im Backend-Prozess.

## GUI/Server-Trennung (Entscheidung 4, hart)

Der `strom`-Serverprozess kann standardmäßig selbst ein natives GUI-Fenster
im selben Prozess öffnen (`backend/src/gui.rs` ruft
`strom_frontend::run_native_gui(...)` — das Backend hängt direkt von
`strom-frontend` ab, siehe `backend/Cargo.toml`). Das verletzt unsere
Prozesstrennung, wenn man es nicht abschaltet.

Wir bauen das Backend deshalb **immer** mit dem `no-gui`-Feature
(compile-time, nicht nur `--headless` zur Laufzeit — das ist robuster gegen
versehentliches Vergessen des Flags):

```bash
cargo run --features no-gui -- --host 127.0.0.1 --port 8080 --no-auto-restart
```

Der native Client läuft als komplett separater Prozess:

```bash
cargo run -p strom-frontend
```

Er verbindet sich fest gegen `strom_types::DEFAULT_PORT` (8080) auf
`localhost` (`frontend/src/main.rs`) — für M0 reicht das; ein konfigurierbarer
Zielport für `app-desktop` ist ein M1-Thema.

## `--host` / `STROM_HOST` (unser Patch, `dev`-Branch)

Upstream bindet immer auf `0.0.0.0` (kein `--host`-Schalter existierte). Wir
haben `--host`/`STROM_HOST` ergänzt (Default weiterhin `0.0.0.0`, keine
Verhaltensänderung für bestehende Deployments). Siehe
`docs/DECISIONS.md` für die Begründung und den Commit auf `workspace/strom`
(`dev`-Branch, `git log main..dev`).

**Bekannte Lücke**: der REST/WS-API-Server bindet dank des Patches korrekt
auf `127.0.0.1`. Ein separater RTSP-Discovery-Server
(`backend/src/rtsp_server.rs`, Port 8554) bindet weiterhin hart auf
`0.0.0.0` — das ist ein Media-Plane-Dienst (RTSP-Streaming für entdeckte
Geräte), kein Control-Plane-API-Endpunkt, und war von Entscheidung 4 vermutlich
nicht gemeint. Trotzdem als offener Punkt in `docs/DECISIONS.md` festgehalten.

## Smoke-Test

```bash
curl -s http://127.0.0.1:8080/health          # -> OK
curl -s http://127.0.0.1:8080/api/flows       # -> {"flows":[]}
```

Flow mit zwei Testquellen anlegen und starten (siehe
`docs/STROM-ANATOMY.md#flow-lifecycle` für die Feldbedeutung):

```bash
FLOW_ID=$(python3 -c "import uuid; print(uuid.uuid4())")
curl -s -X POST http://127.0.0.1:8080/api/flows \
  -H "Content-Type: application/json" \
  -d "{\"id\":\"$FLOW_ID\",\"name\":\"Smoke Test\"}"

curl -s -X PUT http://127.0.0.1:8080/api/flows/$FLOW_ID \
  -H "Content-Type: application/json" \
  -d "{\"id\":\"$FLOW_ID\",\"name\":\"Smoke Test\",\"elements\":[
    {\"id\":\"$(python3 -c 'import uuid; print(uuid.uuid4())')\",\"element_type\":\"videotestsrc\",\"position\":[0,0],\"properties\":{\"pattern\":\"smpte\"}},
    {\"id\":\"$(python3 -c 'import uuid; print(uuid.uuid4())')\",\"element_type\":\"videotestsrc\",\"position\":[0,200],\"properties\":{\"pattern\":\"ball\"}}
  ],\"blocks\":[],\"links\":[]}"

curl -s -X POST http://127.0.0.1:8080/api/flows/$FLOW_ID/start   # -> gst_state: "Playing"
curl -s -X POST http://127.0.0.1:8080/api/flows/$FLOW_ID/stop
curl -s -X DELETE http://127.0.0.1:8080/api/flows/$FLOW_ID
```

Interaktive API-Doku: `http://127.0.0.1:8080/swagger-ui` (nur erreichbar,
solange der Prozess mit `--host 0.0.0.0` oder von localhost aus getestet
wird — mit `--host 127.0.0.1` nur vom selben Host).

Nativer Client (separates Terminal, braucht eine laufende Desktop-Session):

```bash
cargo run -p strom-frontend
```

Erwartet: natives Fenster "Strom" öffnet sich, verbindet per WebSocket
(`ws://localhost:8080/api/ws`) und REST, zeigt die Elementpalette und die
oben angelegten Flows.

## lmix Console und Selftest (Stand M1b)

Immer Release-Builds: Debug-Builds sind für Live-Video zu langsam.

```bash
./run.sh                                # Backend (127.0.0.1:8099) + Console
scripts/acceptance-m1b.sh [N] [SEK]     # M1b-Abnahme, eigenes Backend auf 8199
```

Die Console wählt das Mixer-Backend nach der GPU der Engine (`gpu_profile.rs`):
NVIDIA → `cuda`, AMD/Intel → `gl` (unverifiziert), ohne Hardware-GPU Abbruch.
Übersteuern mit `--mixer-backend cuda|gl|cpu`.

Selftest (headless) gegen ein laufendes Backend, wichtigste Schalter:

| Schalter | Wirkung |
|---|---|
| `--selftest-patterns N` | N Testbild-Inputs (mit Testton) |
| `--selftest-file PFAD` | Datei-Input (NVDEC), wiederholbar |
| `--selftest-srt URI` | SRT-Input (MPEG-TS), wiederholbar |
| `--selftest-encode` | PGM + Multiview → HW-Encoder; misst fps/Alter je Input und Ausgang, prüft Bildinhalt |
| `--selftest-secs S` | Dauer der Lastphase |
| `--mixer-backend B` | Backend erzwingen |

Kein `STROM_FORCE_SOFTWARE_CONVERT` mehr nötig (auch nicht für `cpu`).
Messwerte und Methode: `docs/LATENCY.md`.

## lmix-core (Steuerprozess)

```bash
cd workspace/lmix-core
cargo test                       # Switcher, Show-Datei, API
cargo run -- --engine none       # ohne Engine, nur Zustand und API
cargo run -- --strom http://127.0.0.1:8099   # gegen laufendes strom
```

API auf `127.0.0.1:8200`: `GET /api/state`, `POST /api/actions`,
`GET /api/actions` (Katalog als JSON-Schema), `GET /api/tally`,
WebSocket `GET /api/events`.

### strom fertig gebaut (CI)

Ein kalter Release-Build von strom dauert 20–25 min. Die CI
(`.gitea/workflows/strom.yml`) baut ihn einmal je Submodul-Commit in
`ubuntu:24.04` und legt ihn als Generic-Paket `strom/<sha>` ab.
`scripts/fetch-strom.sh` holt ihn (Sekunden) und baut nur lokal, wenn es
kein Paket gibt oder `GITEA_TOKEN` fehlt.

| Wo | Was | Rechte |
|---|---|---|
| Repo-Secret `REGISTRY_TOKEN` (lmix) | CI: privates Submodul lesen, Paket schreiben | `read:repository`, `write:package` |
| Umgebungsvariable `GITEA_TOKEN` (Cloud-Umgebung) | Klonen, Paket laden | `read:repository`, `read:package` |

Cloud-Umgebung: Inhalt von `scripts/cloud-setup.sh` als Setup-Skript
eintragen. Es installiert GStreamer und Werkzeuge, klont lmix nach
`/root/work/lmix` und lädt strom.

### Hinweis: Swagger UI ohne GitHub

strom lädt beim Bauen `swagger-ui` von GitHub. Ist GitHub gesperrt, das
gleiche Paket von npm holen und als Zip anbieten:

```bash
curl -sSL -o sud.tgz https://registry.npmjs.org/swagger-ui-dist/-/swagger-ui-dist-5.17.14.tgz
mkdir -p z/swagger-ui-5.17.14/dist && tar xzf sud.tgz -C z && cp -r z/package/* z/swagger-ui-5.17.14/dist/
(cd z && zip -qr ../swagger-ui-5.17.14.zip swagger-ui-5.17.14)
export SWAGGER_UI_DOWNLOAD_URL=file://$PWD/swagger-ui-5.17.14.zip
```

### M2b-Test ohne GPU

```bash
scripts/test-m2b-vm.sh   # strom (CPU-Mischer) + lmix-core, Testbilder
CONSOLE_SHOT=console.png scripts/test-m2b-vm.sh   # plus Console auf Xvfb
```

Prüft Bildinhalt (Helligkeit, Bewegung) über ImageMagick `convert`.

## Console als Client von lmix-core

```bash
cd workspace/app-desktop
cargo run -- --core http://127.0.0.1:8200    # oder LMIX_CORE=…
```

Ohne `--core` spricht die Console wie in M1b direkt mit strom.

## Qt-Console (`workspace/console-qt`, Prototyp)

Systempakete: Qt 6 (Quick, QuickControls2, Dialogs) mit Entwicklungsdateien,
Kirigami 6, das GStreamer-Plugin `qml6` und gstreamer-rs-Abhängigkeiten.

```bash
# Arch
sudo pacman -S qt6-declarative kirigami gst-plugin-qml6
# Ubuntu 26.04 LTS / Kubuntu (Plasma 6)
sudo apt-get install qt6-declarative-dev qml6-module-org-kde-kirigami \
  qml6-module-qtquick-dialogs gstreamer1.0-qt6
```

Ubuntu 24.04 hat noch Plasma 5 und damit kein Kirigami 6: dort baut die
Qt-Console nicht ohne Backports (offen, siehe HANDOVER).

```bash
scripts/sync-kante.sh                 # Kante-QML ins Crate kopieren (nach Kante-Änderungen)
cd workspace/console-qt
cargo run --release -- --core http://127.0.0.1:8200 [--style kante|kante-light|system]
```

`cxx-qt-build` findet Qt über `qmake6` (bzw. `QMAKE=…`). Die Console muss auf
dem Rechner der Engine laufen: Live-Bilder kommen über Unix-Sockets in
`$XDG_RUNTIME_DIR/lmix/live` (`lmix-core --live-dir`). Messwerte je Ansicht:
`RUST_LOG=lmix_console=debug`. Läuft die Console ohne Terminal, landen
Qt-Meldungen im Journal; `QT_FORCE_STDERR_LOGGING=1` holt sie nach stderr.

Automatischer Test (Xvfb, CPU-Mischer, keine GPU nötig):
`scripts/test-console-qt.sh` prüft bewegte Monitore und Kacheln, Klick =
Vorschau, Cut und dass QML keine Warnungen schreibt.

## Dauertest auf Hardware

```bash
scripts/soak-hardware.sh                       # 60 min, CUDA-Mischer
MINUTES=10 MIXER=gl scripts/soak-hardware.sh   # AMD/Intel
```

Misst alle 5 min die Latenz Quelle → PGM, CPU von strom, GPU-Last und den
SRT-Leitungszustand, schaltet alle 30 s das Programm um und prüft am Ende:
Latenz nicht gewachsen, Ausgänge nie ausgefallen, Aufnahme und empfangener
SRT-Strom lückenlos (Bilder ≥ 98 % von 30 fps × Laufzeit).
