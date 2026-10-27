# Build

Reproduzierbare Build-/Run-Kommandos für unseren Anwendungsfall: Server headless
auf 127.0.0.1, native Qt-Console als eigener Prozess. Für alles, was hier
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

**Fedora 44** (KDE, geprüft 2026-10-27; Rust aus dem Fedora-Paket, 1.99, die
Pin-Version wird ignoriert):

```bash
sudo dnf install rust cargo clippy rustfmt rust-std-static gcc-c++ clang mold cmake \
  gstreamer1-devel gstreamer1-plugins-base-devel gstreamer1-plugins-bad-free-devel \
  gstreamer1-plugins-bad-free-extras libnice-devel libnice-gstreamer1 graphviz \
  qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qttools-devel qt6-linguist \
  kf6-kirigami-devel gstreamer1-plugins-good-qt6 \
  xdotool ImageMagick jq zstd
```

`gstreamer1-plugins-bad-free-extras` bringt `srtsink` und die DeckLink-Elemente.
Für den CUDA-Mischer braucht Fedoras nvcodec-Plugin `libnvrtc` aus NVIDIAs
CUDA-Repo (`cuda-nvrtc-13-2`, das Repo für fedora43 geht auch auf 44). Sie liegt
nicht im Bibliothekspfad, und GStreamer sucht sie ohne Versionsnummer; ohne sie
fehlen `cudacompositor` und `cudaconvertscale`. Einmalig:

```bash
echo /usr/local/cuda-13.2/targets/x86_64-linux/lib | sudo tee /etc/ld.so.conf.d/cuda-nvrtc.conf
sudo ldconfig
sudo dnf install cuda-nvrtc-devel-13-2   # libnvrtc.so ohne Versionsnummer
```

Ohne Root reicht für eine Sitzung
`LD_LIBRARY_PATH=/usr/local/cuda-13.2/targets/x86_64-linux/lib GST_CUDA_NVRTC_LIBNAME=libnvrtc.so.13`.
Danach einmal `~/.cache/gstreamer-1.0/registry.x86_64.bin` löschen. Kennt die
Registry `cudacompositor` aus einem Lauf mit dieser Umgebung, fehlt sie aber
im nächsten, schlägt der strom-Test `test_compositors_ignore_inactive_pads` fehl.

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
Zielport für die Console war ein M1-Thema.

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

## Selbsttest und M1b-Abnahme

Immer Release-Builds: Debug-Builds sind für Live-Video zu langsam.

```bash
scripts/acceptance-m1b.sh [N] [SEK]     # M1b-Abnahme, eigenes Backend auf 8199
```

Der Selbsttest wählt das Mixer-Backend nach der GPU der Engine
(`gpu_profile.rs`): NVIDIA → `cuda`, AMD/Intel → `gl` (unverifiziert), ohne
Hardware-GPU Abbruch. Übersteuern mit `--mixer-backend cuda|gl|cpu`.

Selbsttest (kopflos, eigenes Binary `lmix-selftest` in `workspace/selftest`)
gegen ein laufendes Backend, wichtigste Schalter:

```bash
cd workspace/selftest && cargo run --release -- --server http://127.0.0.1:8099 --patterns 8
```

| Schalter | Wirkung |
|---|---|
| `--patterns N` | N Testbild-Inputs (mit Testton) |
| `--file PFAD` | Datei-Input (NVDEC), wiederholbar |
| `--srt URI` | SRT-Input (MPEG-TS), wiederholbar |
| `--encode` | PGM + Multiview → HW-Encoder; misst fps/Alter je Input und Ausgang, prüft Bildinhalt |
| `--secs S` | Dauer der Lastphase |
| `--dropout` | Datei-Input stoppt mitten in der Blende |
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
auf dem GitHub-Spiegel (`.github/workflows/ci.yml`) baut ihn einmal je
Submodul-Commit in `ubuntu:24.04` und legt ihn öffentlich als
`ghcr.io/shrippen/lmix-strom:<sha>` ab (OCI-Artefakt, ein Tarball).
`scripts/fetch-strom.sh` holt ihn ohne Anmeldung (Sekunden) und baut nur
lokal, wenn es kein Paket gibt. strom ist öffentlich
(https://github.com/shrippen/strom, Spiegel des Gitea-Forks).

| Wo | Was | Rechte |
|---|---|---|
| Repo-Secret `GITEA_TOKEN` (GitHub-Spiegel lmix) | CI: Status `github / test` an Gitea | Schreibrecht auf das Repo |
| Umgebungsvariable `GITEA_TOKEN` (Cloud-Umgebung) | lmix klonen | `read:repository` |

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

### Titel (M7)

```bash
scripts/test-titles.sh   # lmix-core ohne Engine + Chromium: Vorlagen, Daten, Neuladen
```

Eigene Vorlagen liegen im Ordner `--templates` (Standard
`$XDG_DATA_HOME/lmix/templates`); die eingebauten (Kante) stecken im
Binary (`workspace/lmix-core/titles/`, Kante-Teil über
`scripts/sync-kante.sh`). Ein Grafikkanal ist die Seite
`http://127.0.0.1:8200/graphics/<Nummer>` (`?preview` im Browser: Karo-
Grund, meldet nichts). Eine Vorlage allein im Browser
(`/titles/lmix/<name>/index.html`) zeigt eine Leiste mit Play/Next/Stop.
Gerüst für eigene Vorlagen: `workspace/lmix-core/titles/sdk/skeleton/` in
den Vorlagenordner kopieren.

Braucht node mit playwright (Chromium). Ohne globales playwright geht
auch der Node-Treiber aus Pythons playwright:
`NODE_PATH=<dir mit node_modules/playwright → …/site-packages/playwright/driver/package>`.

### M2b-Test ohne GPU

```bash
scripts/test-m2b-vm.sh   # strom (CPU-Mischer) + lmix-core, Testbilder
CONSOLE_SHOT=console.png scripts/test-m2b-vm.sh   # plus lmix Mischer auf Xvfb (braucht Qt 6, Kirigami)
```

Prüft Bildinhalt (Helligkeit, Bewegung) über ImageMagick `convert`.

## Qt-Console (`workspace/console-qt`, Prototyp)

Ein Programm `lmix` mit Seiten (Mix, Show, Audio, Outputs, Setup;
DECISIONS 2026-10-23, `docs/PLAN-SEITEN.md`), umgeschaltet über die Leiste am
Fuß oder Strg+1…n. Mehrere Fenster laufen auch gleichzeitig am selben
`lmix-core`.

Ziel ist Linux allgemein, keine einzelne Distribution; entwickelt wird auf
Arch. Gebraucht werden:

| Was | Mindestens | Wofür |
|---|---|---|
| Qt 6 (Quick, QuickControls2, Dialogs) mit Entwicklungsdateien | 6.x (entwickelt mit 6.12, ältere ungetestet) | Oberfläche, `cxx-qt-build` findet Qt über `qmake6` bzw. `QMAKE=…` |
| Kirigami (KDE Frameworks 6) | KF6 | Fenster, Kante |
| GStreamer mit Plugin `qml6` (`qml6glsink`) | 1.24 | Live-Bilder |
| Qt Linguist-Werkzeuge (`lrelease`) | – | deutsche Übersetzung; ohne sie baut die Console nur englisch (Build-Warnung) |

```bash
# Arch (Entwicklungssystem, geprüft)
sudo pacman -S qt6-declarative qt6-tools kirigami gst-plugin-qml6
# Debian 13 / Ubuntu ab 25.04 (ungeprüft)
sudo apt-get install qt6-declarative-dev qt6-l10n-tools qml6-module-org-kde-kirigami \
  qml6-module-qtquick-dialogs gstreamer1.0-qt6
```

**Ubuntu 24.04** (geprüft in der Cloud-VM, 2026-10-09): Qt 6 und Kirigami
aus KDE neon, gepinnt, damit nur Qt-, KF6- und ihre neueren
Grundbibliotheken (libwayland, libinput, libpcre2) von dort kommen;
GStreamer bleibt das von Ubuntu (~100 MB Download, ~550 MB Platte):

```bash
sudo curl -sSo /etc/apt/keyrings/neon-archive.asc https://archive.neon.kde.org/public.key
echo "deb [signed-by=/etc/apt/keyrings/neon-archive.asc] https://archive.neon.kde.org/user noble main" \
  | sudo tee /etc/apt/sources.list.d/neon.list
printf 'Package: *\nPin: origin archive.neon.kde.org\nPin-Priority: 100\n\nPackage: qt6-* libqt6* qml6-* kf6-* libkf6* gstreamer1.0-qt6 qmake6* qt6-l10n-tools libwayland* libinput* libpcre2*\nPin: origin archive.neon.kde.org\nPin-Priority: 600\n' \
  | sudo tee /etc/apt/preferences.d/neon-qt6
sudo apt-get update
sudo apt-get install --no-install-recommends qt6-base-dev qt6-declarative-dev qt6-l10n-tools \
  qt6-tools-dev-tools qt6-qpa-plugins qml6-module-org-kde-kirigami kf6-kirigami-dev gstreamer1.0-qt6
PATH=/usr/lib/qt6/bin:$PATH   # lrelease
```

GStreamer vor 1.28: `unixfdsink` kopiert erst ab 1.28 selbst; lmix-core legt
für Programm und volle Vorschau dann einen Kopierschritt davor (DECISIONS).

**Ungeeignet** (zu alt): Debian 12 (kein Kirigami 6, GStreamer 1.22).
Andere Distributionen mit KF6 und GStreamer ≥ 1.24 sollten gehen; wer eine
prüft, trägt sie hier ein.

```bash
scripts/sync-kante.sh                 # Kante-QML ins Crate kopieren (nach Kante-Änderungen)
cd workspace/console-qt
cargo run --release -- --core http://127.0.0.1:8200 [--page mix|show|audio|outputs|setup] [--theme kante|kante-light|system]
```

Oder alles zusammen (strom, lmix-core, lmix mit Testbildern; Schließen
beendet alles): `./start.sh [mix|show|audio|outputs|setup|both]` bzw.
`scripts/run-local.sh` mit `APP=…` (`MIXER=gl`, `INPUTS=8`; ohne Seite die
zuletzt offene, `both` zwei Fenster Mix und Show).

Einstellungen (Stil, Ansicht, Tasten, zuletzt offene Seite) in
`~/.config/lmix/lmix.conf`. Beim ersten Start übernimmt lmix einmal
`mixer.conf` und `show.conf` der zwei Programme bis 23.10.2026 (bei gleichen
Schlüsseln gilt der Mischer, die globalen Tasten die Show), sonst eine
`console.conf` (die eine Console bis 18.10.2026). Startmenü:
`packaging/desktop/lmix.desktop` nach `~/.local/share/applications/`, `lmix`
im `PATH`. Globale Tasten (AUT.12) ab Werk an, abschaltbar unter
Belegungen…

Sprache: Quelltexte englisch, Deutsch als Übersetzung; die Programme folgen der
Systemsprache (`LANGUAGE=en LANG=en_US.UTF-8` erzwingt Englisch). Neue oder geänderte
Texte: `scripts/update-translations.sh` sammelt sie (`lupdate`) und trägt die
deutschen Texte aus `workspace/console-qt/i18n/de.py` ein; fehlende meldet es.

`cxx-qt-build` findet Qt über `qmake6` (bzw. `QMAKE=…`). Der Mischer muss auf
dem Rechner der Engine laufen: Live-Bilder kommen über Unix-Sockets in
`$XDG_RUNTIME_DIR/lmix/live` (`lmix-core --live-dir`). Messwerte je Ansicht:
`RUST_LOG=lmix_mixer=debug`. Läuft ein Programm ohne Terminal, landen
Qt-Meldungen im Journal; `QT_FORCE_STDERR_LOGGING=1` holt sie nach stderr.

Automatischer Test (Xvfb, CPU-Mischer, keine GPU nötig):
`scripts/test-console-qt.sh` prüft im Mischer bewegte Monitore und Kacheln,
Klick = Vorschau, Cut und dass QML keine Warnungen schreibt; danach startet
es die Show am selben `lmix-core` und prüft, dass ihr Fenster zeichnet, ohne
QML-Warnungen, dass die Leertaste GO und Escape Panik auslöst, auch nach
„Speichern unter…“ in der Kopfzeile, das mit Escape geschlossen wird.

## Dauertest auf Hardware

```bash
scripts/soak-hardware.sh                       # 60 min, CUDA-Mischer
MINUTES=10 MIXER=gl scripts/soak-hardware.sh   # AMD/Intel
```

Misst alle 5 min die Latenz Quelle → PGM, CPU von strom, GPU-Last und den
SRT-Leitungszustand, schaltet alle 30 s das Programm um und prüft am Ende:
Latenz nicht gewachsen, Ausgänge nie ausgefallen, Aufnahme und empfangener
SRT-Strom lückenlos (Bilder ≥ 98 % von 30 fps × Laufzeit).

### NVENC fehlt nach einem Treiber-Update

Zeigt das strom-Log `NVENC not available` und `Using software fallback
encoder: x264enc`, obwohl eine NVIDIA-Karte steckt, und brechen alle
Programm-Ausgänge mit `not-negotiated` ab: GStreamer hat beim Update (noch
mit dem alten Kernelmodul) eine Registry ohne NVENC-Encoder gespeichert und
prüft sie nicht neu, solange sich die Plugin-Dateien nicht ändern. Nach dem
Neustart:

```bash
gst-inspect-1.0 nvh264enc >/dev/null || rm ~/.cache/gstreamer-1.0/registry.*.bin
```
