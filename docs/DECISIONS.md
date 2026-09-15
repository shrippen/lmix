# Entscheidungen

Format: Datum, Kontext, Optionen, Entscheidung.

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

## 2026-09-15 — Repo-Hosting und Push-Verhalten

**Kontext**: Wohin soll das lmix-Repo, wann pushen?

**Entscheidung** (User): Remote `https://git.arianw.de/shrippen/lmix`. Push
nur nach Abschluss eines Meilensteins oder auf explizite Anweisung — nie
automatisch nach einzelnen Commits während der Arbeit an einer Meile.
