# Projekt: vMix-Alternative für Linux als native Desktop-App

## Kontext
Ziel: eine native Linux-Desktop-App, die als Software-Vision-Mixer (PGM/PVW-Schalter)
ca. auf vMix-Niveau funktioniert. Medienkern = Strom (Eyevinn, Rust + GStreamer,
Lizenz MIT oder Apache-2.0, https://github.com/Eyevinn/strom).

Projektebene: Früh-Prototyp aus Mache, der aber auf echtes Produkt hinausläuft.
Kommuniziere auf Deutsch, Code/Commits auf Englisch.

## Feststehende Entscheidungen (nicht neu verhandeln, nicht "alternativ bewerten")
1. Basis = STROM-Workspace (Crate: strom, strom-frontend, strom-types, strom-mcp-server).
2. Open Live (Eyevinn open-live / open-live-studio) wird NICHT verwendet: kein Node/TS,
   keine CouchDB, kein K8s. Dieses Repo ignorieren vollständig.
3. GUI = NATIV (kein Web-Interface als Produktziel). strom-frontend ist egui-basiert
   ("Native or Web"); wir bauen auf der nativen egui-Variante auf.
4. Prozessmodell (hart): strom läuft als eigenständiger Headless-Server auf
   127.0.0.1; GUI = separater Prozess; Kommunikation ausschliesslich über
   REST + WebSocket, Typen aus strom-types. NICHTS in-Process: kein Linken des
   Server-Crates in die GUI, keine GUI-Logik in Server-Crate, keine gemeinsamen
   globalen Zustände. GUI-Crash darf die Engine nie mitnehmen.
5. OMT (openmediatransport) als Input: entweder (a) ein OmtBlock in der Strom-
   Block-Registry (Rust + C-FFI gegen die libomt-C-API) oder (b) ein externes
   Adapter-Repo (omtpw-bridge: OMT-Receive → NDI- oder SRT-Out → bestehender
   NDI/SRT-Block). Entscheidung erst in M2 nach Aufwand-Vergleich.
   HARD CONSTRAINT: Das OMT-Protokoll wird NICHT neu implementiert, immer gegen libomt.
6. Lizenzkompatibilität: Projekt muss MIT-kompatibel bleiben (kommerzielle Nutzung
   möglich). Kein GPL-Code (weder 2 noch 3) in das Repo. Jede neue Abhängigkeit
   prifen, Lizenz in der Commit-Meldung dokumentieren.

## Repo-Struktur
workspace/
  strom/          ← Upstream-Klon (eigene Branch "dev", nur minimaler Diff erlaubt)
  app-desktop/    ← UNSER Client (Fork/Neuaufbau auf Basis strom-frontend, native egui)
  omtpw-bridge/   ← nur falls Options (b) für OMT gewählt wird (eigenes Repo, stabile API)
  docs/           ← Build-Doku, Architektur-Anatomie, Latenz-Messungen, Entscheide-Log

## Arbeitsregeln
- Der Code ist die Quelle der Wahrheit (die Grundregel des Strom-Projekts selbst):
  Vor JEDEM Ändern eines Moduls: bestehende Module, Tests und das Block-Registry-
  Pattern lesen und im Commit auf die Verwiesen werden.
- API-Kontrakt-first: Neue/veränderte API-Flächen zuerst in strom-types, dann Server,
  dann Client.
- Commits: klein, thematisch, 1 Feature = 1 Commit-Stack. Nie Mega-Diffs.
- Unser Diff gegen strom/ halten = minimal: alles was nur kann, gehört in
  app-desktop/ oder einen OmtBlock. Nur wenn ein Feature ohne Server-Änderung
  nicht machbar ist: Server-Änderung, und dann ein eigener, sauberer, upstream-
  tauglicher Commit dafür (mit Begründung im Commit, PR-fähig).
- Persona-Durchläufe (`docs/PERSONAS.md`, `docs/LASTENHEFT.md` §9): jede neue
  oder geänderte Funktion gegen die betroffenen Personas prüfen; jede
  Meilenstein-Abnahme spielt mindestens 5 Personas durch, Befunde in PERSONAS.md.
- Alle Meilensteine enden in einem lauffähigen, vom User abnahmefähigen Zustand
  mit dokumentierten Akzeptanzbefehlen. Nach jeder Meile: STOPP und auf User-
  Freigabe warten.
- Logge alle signifikanten Entscheidungen in docs/DECISIONS.md (Kontext, entgegengesetzte
  Optionen, Entscheidung, mit Datum).

## Meilensteine

### M0 — Lokaler Build + Anatonie (Priorität 1, allein)
- strom workspace klonen und COMPLETT nativ lauffähig machen: Server (Headless,
  Loopback) + nativer egui-Client. Falls System-Pakete (GStreamer-Elemente,
  wgpu/winit-Abhängigkeiten etc.) fehlen: in die Doku.
- Deliverables:
  * docs/BUILD.md — exakte, reproduzierbare Build-Kommandos für Server und Client.
  * docs/STROM-ANATOMY.md — für den User verständlich: Flow-Lifecycle,
    Block-Registry-Mechanik (wie ein Block registriert wird: Code-Beispiel),
    Vision-Mixer-Flow-Typ (PVW/PGM-Struktur), Audio-Mixer, vollständiges
    REST-+WS-API (inkl. Events/State-Diffing, Swagger referenzieren),
    Storage-Modell (JSON vs. PostgreSQL), Property-System.
  * docs/OMT-PLAN.md — Aufwand-Schätzung Options (a) vs. (b) für OMT (siehe oben),
    inkl. libomt-Build-Anleitung (buildlinuxx64.sh, .NET 8 SDK nur für Kompierten,
    ldd --verify der AOT-Abhängigkeiten auf unserem Host).
- Akzeptanz: (1) nativer Client öffnet sich und spricht den Server; (2) ich kann per
  App einen Vision-Mixer-Flow mit zwei Quelle (ffmpeg testsrc oder v4l2) anlegen
  und sehen; (3) dieselben Aktionen per curl gegen die API aufweisbar;
  (4) User hat die drei Doku gelesen und freigegeben.

### M1 — Schalter-Console (GUI, der Kern des Produkts)
- Neues "Console"-Panel in app-desktop (vMix-Workflow, nicht Node-Editor):
  * PGM/PVW-Grid mit allen Quellen (Miniaturframes over WS), 
  * take-cut (PGM/PVW-Swap), dip & wipe via bestehende Vision-Mixer-Transitions,
  * Audio-Strip mit Fadern/Metering gegen den Audio-Mixer,
  * Multimember (PGM/Preview/alle Quellen).
  * Hotkeys auf Programm-Quelle (Taste 1-8), wie bei vMix.
- HARD: nur bestehende Strom-API verwenden; keine Server-Änderung in M1.
- Wenn egui an UX-Grenzen stösst (Dichte, Multi-Display, Frame-Refresh): NICHT
  erzwungen, sondern in docs/DECISIONS.md dokumentieren — Entscheidung Qt/Tauri-Shell
  spätestens in M3.
- Akzeptanz: Ich schneide aus lauffenden Quellen in der Console genauso schnell
  wie im Node-Editor; Hotkeys functional; Konsole zeigt Live-State korrekt auch
  bei paralleler API-Operation (z. B. Quelle weg während Transition).

### M2 — OMT-Input
- Umsetzung von Options (a) oder (b) aus M0/OMT-PLAN-Entscheidung.
  Falls (b): omtpw-bridge als eigenes Repo: OMT-Receive-Loop (libomt C-API,
  SPSC-Queue, Frame-Drop bei Backpressure) → NDI-Send (bevorzugt, wegen
  Latenz) oder SRT-Video (Fallback).
- Akzeptanz: OMT-Loopback-Quelle (omtsendtest aus libomtnet/Examples) erscheint
  als Quellen in der Console, ist schaltbar auf PGM; End-to-End-Latenz gemessen
  und in docs/LATENCY.md dokumentiert (Zielbudget: < 5 Frames OMT-Rx → PGM-Ausgabe;
  messen mit getreuen Zeitmarker-Bildern, keine Schätzung).

### M3 — Output-Matrix + Robustheit-Basis
- Min. 2 simultane Outputs (z. B. RTMP + SRT, oder SRT + MP4-Recording),
  unabhängig schaltbare Record pro Quelle (MultiCorder-Prinzip).
- Failover-Tests: Quelle verliert Mid-Transition / Stream bricht mid-Stream →
  Program bleibt up (kein Crash, kein Black-Frame-Stau); Failback Test.
- Latenz-Regression-Test in CI (testsrc-Zeitmarker, automatisierte Messung).
- Entscheidung: GUI-Shell-Strategie (weiter egui vs. Qt/Tauri-Wrapper)
  auf Basis der M1-M1-M1-Erfahrungen.

### M4 & M5 — später, nach Freigabe
- M4: Instant Replay (Ringbuffer), Lower Thirds/DSK — so weit der
  Vision-Mixer/FX-Framework schon trägt; fehlende Features als gap im
  docs/DECISIONS.md, nicht hacken.
- M5: Robustness: 8h-Soak-Test mit Mixed-Load, Crash-Isolation-Test
  (GUI kill -9 → Engine unverändert weiter), vollständige CI-Grüne.

## Exkludierte Ziele (explizit NICHT in diesem Scope)
- Open Live, Node/TS, CouchDB, Kubernetes, Web-UI als Haupt-GUI
  (Web-Fernbedienung ist erlaubt, `docs/LASTENHEFT.md` E.1)
- Windows/macOS
- XAML-Titel-Engine, OBS-Plugin-Ökosystem
- GPU-FX-Neuentwicklung jenseits des Bestehenden
- Aufgehoben 2026-10-08 (E.1): Control-Surfaces und 4K sind wieder im Scope

## Sofort-Start-Weisung
Beginne AUSCHLIESSLICH mit M0. Beende jede Meile mit einer klaren
Abschlussmeldung: was läuft, welche Akzeptanzbefehle ich ausführen soll,
welche offenen Risiken und welches die nächste Meile ist.
