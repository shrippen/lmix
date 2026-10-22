# Übergabe: Stand M9 und Show-Modus S1 (in Arbeit), lokale Übernahme

Stand 2026-10-22 abends (`main` mit allen PRs bis #127, strom `dev` mit #29–#37). Die Arbeit läuft lokal weiter (Arch, RTX 3090), Abnahme
auf echter Hardware.

## Für den nächsten Agenten: Stand 22.10.2026 abends

Der Eigentümer hat am 22.10. das Mergen freigegeben: strom #29–#37 sind in
`dev`; in lmix kamen #116, #117, #119, #121–#125 in `main`, danach #118 und #120, zuletzt
der Pin-PR #127: das Submodul zeigt auf strom `dev`. `lmix-pin-1019` (strom) wird damit
nicht mehr gebraucht, ältere lmix-Commits zeigen aber noch auf ihn: nicht
löschen.

### Ergebnisse 22.10. (Details in DECISIONS)

- T-Bar folgt strom (#124): ein abgelehnter Schritt geht auf 0 zurück,
  Ton bleibt; jeder Take hebt Fade to Black auf.
- BiB-Cut ohne Schwarz (strom #35): neue Pads erst sichtbar, dann die
  alten aus.
- GL-Tests auf NVIDIA grün (strom #36): die Tests machen die
  GPU-Erkennung des Servers; eingesetzte `videoconvert`-Adapter bleiben
  ohne udmabuf. Vorschaubilder ohne `cudaconvertscale` (strom #37).
- Abnahmetest prüft die Vorschau als Ausgangsquelle, Console-Test
  „tile 1 still“ behoben (#125).
- Latenz: mit 8 Inputs ohne Ausgänge auf freiem Rechner konstant 56 ms je
  Input und Lauf; mit 15 Inputs und Ausgängen (19.10.) noch je Input
  verschieden, offen.

### Nächste Schritte

Wie unten (19.10.), ohne Punkt 1 und 6; Punkt 4 ist jetzt „fester Teil
der Latenz kleiner“.

## Stand 19.10.2026 abends (überholt)

Kurz: Hardware-Nacht (01–07 Uhr) und ein Arbeitstag (08–20 Uhr) auf dem
3090-Rechner. `main` ist unverändert (f2136cd); alles liegt in PRs, die
der Eigentümer mergt. Die Arbeitsweise unten (Review, CI, Gitea) gilt
weiter.

### Merge-Reihenfolge

**strom** (Gitea `shrippen/strom`, Basis `dev`; #33 auf #30):

| PR | Branch | Inhalt |
|---|---|---|
| #29 | `lmix-cuda-hw-fixes` | CUDA-Mischer auf echter Hardware: Alpha nach Blende (`cudaconverter`-Bug umgangen), Stinger, Vorschaubilder im Vollbereich, Rampen-CRITICAL |
| #30 | `lmix-zone-clock` | Live-Stille je Zone: Panik und Stopp erreichen laufende Klänge |
| #31 | `lmix-device-channels` | Audio-Ausgänge mit Kanalpositionen in der Geräteliste |
| #32 | `lmix-preview-out` | Vorschau-Ausgang `pvw_out` am Bildmischer (VSP), mit Zonen-Rändern |
| #33 | `lmix-zone-latency` | Zone wartet weniger: GO bis hörbar 32,9 ms (SND.12) |
| #34 | `lmix-tbar-effect` | T-Bar hält den Take mit seinem Effekt (SWI.3) |

`lmix-pin-1019` (strom) wächst mit: Er ist der Merge aller strom-PRs
oben (Stand siehe lmix #123). Jeder lmix-PR zeigt auf den Stand, den sein
Text nennt (#121: `a016205` = #29–#33; #122: `fbd3dfb` mit den Rändern
von #32; #123: der jüngste Stand mit #34 nach allen Review-Runden und den
Review-Nachträgen von #29 und #33, Hash im PR-Text). Nach dem Merge in `dev` das Submodul
auf `dev` stellen (kleiner PR).

**lmix**:

| PR | Basis | Inhalt |
|---|---|---|
| #116 | `main` | Hardware-Nacht: Ergebnisse, `pulsesink`-Puffer, `hw-sound-cues` |
| #117 | #116 | Zonen auf Gerätekanälen (Kanalbelegung des Geräts) |
| #119 | #117 | Vorschau-Monitor zeigt den Vorschau-Ausgang des Mischers (VSP) |
| #121 | #119 | Stand 19.10.: Submodul mit #33, Doku, Upstream-Bericht |
| #122 | #121 | Ausgänge mit Quelle Vorschau (OUT.2) |
| #123 | #122 | T-Bar mit dem Effekt von Taste 1 (SWI.3) |
| #118 | `main` | Nachzügler #111, Console `--theme` statt `--style`, Kleinigkeiten #104/#105 |
| #120 | `main` | Abnahmetest mit `MIXER` (cpu, cuda, gl) auf echter Hardware |

`todo.md`, `docs/DECISIONS.md` und dieses Dokument ändern mehrere PRs;
beim Mergen von #118/#120 nach dem Stapel Konflikte dort von Hand lösen
(beide Seiten behalten).

### Ergebnisse 19.10. (Details in DECISIONS)

- Klang-Cues auf echtem Gerät ganz grün (`hw-sound-cues`): GO 32,9 ms,
  Panik, 16 Klänge, zwei Geräte, Gerätekanäle 3/4 (virtuelles 7.1).
- Abnahmetest auf echter GPU: CPU und CUDA je 47/47 (GL nur nachts, er
  öffnet Fenster auf dem Desktop).
- VSP: Vorschau kommt aus dem Mischer, Layouts in der Vorschau sichtbar,
  getestet auf CPU, GL und CUDA.
- Latenz-„Sprung bei 16 Inputs“ war keiner: meist 56 ms, ab und zu
  60–81 ms (mal je Input, mal je Lauf). Ursache offen.
- T-Bar mit dem Effekt von Taste 1 (strom #34, lmix #123): nach vielen
  Review-Runden hält strom jeden Pad direkt auf seinen Werten (kein Frame
  über die Ruhelage), wartet auf laufende Takes, folgt der Vorschau; lmix
  stellt den T-Bar bei Take, Layout, FTB und Wiederherstellen auf 0.
  End to end CPU und CUDA, Abnahmetest 47/47 mit dem Pin.

### Nächste Schritte (Reihenfolge)

1. **Review-Befunde** der PRs oben einarbeiten, sobald der Eigentümer
   Reviews anstößt.
2. **OVL.3 im Vorschau-Bild**: wartet auf die Entscheidung in `todo.md`
   („Für dich“); danach DSK-Ebenen am Vorschau-Compositor in strom.
3. **OVL.5 Clean Feed und OVL.2 Rand um BiB-Overlays**: warten auf die
   Entscheidungen (Entscheidungsseite 19.10., `todo.md`).
4. **Latenz** (todo): einen erhöhten Lauf fangen, dann Aggregator-Fristen.
5. **CEF/H.36**: Bau braucht die Freigabe (todo); dann H.36 und M7.0.
6. Flakes im Console-Test (todo): „tile 1 still“ (abends wieder grün),
   Show-Dialog. strom-GL-Tests `source_resize` und
   `overlay_upload::tally_follows_a_cut…` sind auf dem 3090-Rechner rot,
   auch ohne die PRs (todo).
7. Hardware mit dem Eigentümer: H.38 (Bühne sehen), H.13, H.20, H.21,
   echtes Mehrkanal-Interface (H.37), GL-Abnahme tagsüber mit Freigabe.

## Stand 18.10.2026 abends (überholt, Arbeitsweise gilt weiter)

Kurz: `main` ist sauber (alles bis #114 gemergt, kein eigener PR offen).
Offen sind eine Frage an den Eigentümer, kleine Nachzügler aus Reviews,
VSP und die Abnahme auf der RTX 3090.

### Arbeitsweise (gilt für jeden PR)

- Gitea `git.arianw.de/shrippen/lmix` ist führend, GitHub nur Spiegel.
  Branch → PR auf Gitea → Review → Merge über die Gitea-API.
- Review: ein **neuer** Claude-Agent prüft jeden PR (`agent.md`
  „Repository rule“, `REVIEW.md`). Sein Bericht geht unverändert als
  Kommentar „## Claude-Review (neuer Agent) zu <commit>“ in den PR.
  Merge nur ohne offenes **Wichtig** und mit grüner CI (`github / test`,
  nach den Pfaden von `ci.yml`). Nach 3 Runden mit Wichtig: Eigentümer
  fragen. Bisher so gehandhabt (keine Regel): optionale Befunde mit
  einer Zeile beantworten und in den nächsten PR mitnehmen.
- PRs an CI, Release, Packaging, `agent.md`, `REVIEW.md`, Geheimnissen
  oder Veröffentlichtem bestätigt der Eigentümer vor dem Merge in Worten.
- PR-Agent läuft nur auf `/review` des Eigentümers (#112–#114);
  Sitzungen schreiben kein `/review`.
- Platz auf der Platte ist knapp: nach Builds `target/*/incremental`,
  `target/debug` löschen. Der VM-Gesamttest braucht
  `SWAGGER_UI_DOWNLOAD_URL` (lokale Kopie von swagger-ui 5.17.14), wenn
  GitHub-Downloads gesperrt sind.

### PRs seit #102 (18.10.)

| PR | Branch | Inhalt |
|---|---|---|
| #103 | `decisions-1018` | Entscheidungsseite 18.10. eingetragen, Regel angeglichen |
| #104 | `live-recorder` | Aufnahmen über `builtin.liverecorder`; MKV fällt bei GStreamer < 1.26 auf `builtin.recorder` zurück (REC.3) |
| #105 | `cue-timer-rt` | Cue-Zeitgeber auf eigenem Echtzeit-Thread (rtkit/SCHED_FIFO, SIGXCPU gibt ab), `lmix_cue_timer_realtime` (RUN.8) |
| #106 | `stinger-upstream` | Stinger über die Stinger-Übergänge von strom upstream (STG) |
| #107 | `stinger-wait` | geschlossen, ersetzt durch #109 |
| #108 | `console-split` | Console getrennt in `lmix-mixer` und `lmix-show` (ein Kern, zwei Oberflächen); globale Tasten je Programm, Show ab Werk an, Mischer aus |
| #109 | `stinger-done` | Stinger-Sperre endet mit `StingerCompleted`/`StingerFailed` von strom, Obergrenze; kein Warten unter der Sperre |
| #110 | – | Review-Regel: drei Lücken schließen – geschlossen, nicht gemergt |
| #111 | `review-followups` | Sperre: Grenze Clip + 8 s, Generation je Take, frühes Completed gemerkt |
| #112–#114 | `pr-agent-manual`, `pa-polish`, `pa-last` | PR-Agent nur auf `/review` des Eigentümers |

Was jeweils entschieden wurde und warum: `docs/DECISIONS.md`, Einträge
vom 18.10. (Review-Regel, Entscheidungsseite, REC, STG mit Nachträgen,
RUN.8, „Console in zwei Programme geteilt (Mischer, Show)“, PR-Agent nur auf `/review`).

### Offen beim Eigentümer

- **#109, Umfang der Sperre:** Während eines Stingers lehnt lmix
  zusätzlich ab: Show öffnen/neu, Paket importieren, den Eingang auf
  Programm entfernen (`StingerPlaying`). Das geht über die Liste der
  Entscheidung hinaus; Antwort steht aus. Bis dahin bleibt es so
  (auch in `todo.md` und im STG-Nachtrag vermerkt).

### Nächste Schritte (Reihenfolge)

1. **Nachzügler aus dem Review von #111** (ein PR):
   - `service.rs` `watch_stingers`: bei `Lagged` `continue` statt
     `stinger_finished()`.
   - `meters.rs`: `StingerFailed` gegen die bekannte Take-Id prüfen
     (andere bekannte Id → ignorieren; `None` oder Id noch unbekannt →
     freigeben wie bisher, Failed kommt oft vor der Antwort auf den Take).
   - `switcher.rs`: Grenze in `stinger_started` ab jetzt neu setzen.
   - Bekannte Grenze, keine Frage offen: Paket importieren übernimmt die
     Ziele vor der Ablehnung (wiederholbar, DECISIONS STG). Eine saubere
     Lösung bräuchte eine öffentliche Abfrage am Switcher und damit die
     Zustimmung des Eigentümers.
   - `NullEngine::stinging()` zählt Generationen; Service-Test: altes
     Ende hält, aktuelles gibt frei.
   - `scripts/test-global-keys.sh`: `show.conf` prüfen statt der
     Logzeile „settings taken over“ (bricht mit `RUST_LOG=warn`).
2. **Console `--style`:** clap liest die Option vor Qt
   (`console-qt/src/app.rs`, `config::init`), aber `QGuiApplication::new`
   liest dieselbe argv, sucht ein Quick-Controls-Stilmodul `kante` und lädt
   das Fenster nicht (DECISIONS 18.10. „Console in zwei Programme geteilt (Mischer, Show)“,
   Bestand). Option
   umbenennen oder vor `QGuiApplication::new` aus der argv nehmen.
3. **Weitere Kleinigkeiten** (optional, je mit Quelle):
   - #104, Review Runde 2: `lmix-<Ausgang>` in DECISIONS (REC) und im
     Kommentar an `recording_prefix` (`engine/flows.rs`) liest sich als
     Name; gemeint sind die ersten 8 Zeichen der Ausgangs-ID. Der Test
     dort prüft nur „verschieden“, nicht die Form `lmix-` + 8 Zeichen.
   - #105, Review Runde 4: In DECISIONS (RUN.8) steht der Einschub vor
     „Dann steht eine Warnung im Log …“, so dass „Dann“ falsch bezogen
     wird. Wunsch: Scheitert die Rückstufung in `on_xcpu`
     (`src/realtime.rs`), bleibt keine Spur; ein Merker (etwa ein
     `AtomicBool`, beim nächsten Metrik-Lauf gemeldet) würde helfen.
4. **VSP:** Vorschau-Ausgang im strom-Mischer (noch nicht begonnen).
5. **Hardware (RTX 3090):** H.24, H.27 (Stinger auf CUDA/GL-Mischer),
   H.36–H.39 (H.39: Echtzeit-Zeitgeber, `test-cue-timing.sh` unter Last),
   CEF/`cefsrc`, GPU-Pfade allgemein; H.40 (Console: zwei Portal-Sitzungen
   am KDE-Portal, Fokus nach Dialogen).

### Prüfen

- `cd workspace/lmix-core && CARGO_INCREMENTAL=0 cargo test --bin lmix-core`
  (340), `cargo clippy --all-targets -- -D warnings`, `cargo fmt`.
- `scripts/test-m2b-vm.sh` (47 Prüfungen, zuletzt grün),
  `scripts/test-console-qt.sh` (13), `scripts/test-global-keys.sh` (5),
  `scripts/test-cue-timing.sh`. Unter Last (paralleler Build) scheitert
  der VM-Test gern an der Latenz – dann allein wiederholen.
- Console starten: `scripts/run-local.sh` (`APP=show` oder `APP=both`).

## PRs in Merge-Reihenfolge, Stand 2026-10-18 (bis #102 gemergt)

CI läuft auf GitHub (Status `github / test` am Commit auf Gitea). Seit
18.10.2026 prüft vor jedem Merge ein neuer Claude-Agent (`agent.md`,
`REVIEW.md`). PR-Agent läuft nur noch, wenn der Eigentümer `/review`
kommentiert; sein Schlüssel hat das Kostenlimit erreicht (`todo.md`).

Unabhängig von `main` (gemergt):

| PR | Branch | Inhalt |
|---|---|---|
| #45 | `vm-failover-later` | VM-Check: Quelle stirbt später in der Blende (Failover-Prüfung wackelte) |
| #51 | `console-test-coords` | Lokaler Console-Check: wartet auf das Bild nach dem Cut, ignoriert CRITICALs von strom beim Start |
| #39 | `se3-measure` | S-E.3 gemessen (Klang-Cues), nur Doku und Messskript |

Show-Modus S1 und Rest, in der Reihenfolge des Merges (bis #102 gemergt):

| PR | Branch | Inhalt |
|---|---|---|
| #38 | `s1-cues` | Cue-Modell und Ablauf in lmix-core (Stufe 1) |
| #40 | `s1-script` | Drehbuch-Import `lmix-script` (Stufe 6) |
| #41 | `s1-targets` | Ausgabeziele HTTP, Home Assistant (Stufe 4) |
| #42 | `s1-safety` | Panik, harter Stopp, Notfall-Liste (Stufe 5) |
| #43 | `s1-feeds` | Restzeit als Ereignis, feste OSC-Abonnenten (CTL.2, CTL.4) |
| #44 | `s1-show-view` | Show-Ansicht der Console (Stufe 7) |
| #46 | `s1-timing` | Cue-Zeitgenauigkeit in `/metrics`, Zugangsschlüssel mit Rechten |
| #47 | `s1-checks` | Warnliste, Rechner-Einstellungen im Paket, Persona-Durchlauf 10 |
| #48 | `s1-script-folder` | Show merkt sich ihren Drehbuch-Ordner |
| #49 | `s1-targets-ui` | Ziele und Abonnenten in der Console |
| #50 | `s1-polish` | Dauer an Aktions-Cues, dunkler Raum, OSC-Abo-Ablauf, Übergangsleiste rollt (Fix für `main`) |
| #52 | `m9-settings` | Älteres Show-Format beim Speichern gesichert; Einstellungen ex-/importieren (SHW.7/8) |
| #53 | `s1-zones` | Zonen in der Show, Ausgänge je Rechner (SND.10 ohne Engine); Gruppe endet nach Neuladen |
| #54 | `s1-ctl` | Panik in Zeit x, Cues über `standby`, Listen über `active` (CTL.1) |
| #56 | `s1-release` | GO erst nach Loslassen (RUN.6) |
| #57 | `s1-check` | Nicht erreichbare Ziele in der Warnliste, „Ziele prüfen“ (SUI.5) |
| #58 | `s1-trust` | Vertraute Adressen ohne Schlüssel, `--trust` (CTL.5) |
| #59 | `s1-osc-topics` | OSC-Abo je Thema und Feld (CTL.3); enthält den Fix aus #45 |
| #60 | `se13-stage-measure` | S-E.13 Bühnen gemessen, nur Doku und Messskript |
| #61 | `todo-1017` | Übergabe und Todo nach dem 17.10. |
| #62 | `lastenheft-stand` | Lastenheft: Stand von 68 Zeilen nachgezogen |
| #63 | `aud14-fades` | Pegel blenden: Eingang, Bus, Master über Zeit (AUD.14) |
| #64 | `src4-alpha` | Standbilder mit Alpha (PNG, WebP, TIFF) (SRC.4) |
| #65 | `aud1-stereo` | Pegel je Kanal an der Kachel (AUD.1) |
| #66 | `aud4-monitor` | Abhörpegel, Standardverzögerung neuer Eingänge (AUD.4/AUD.6) |
| #67 | `out2-multiview` | Multiview als Ausgangsquelle (OUT.2) |
| #68 | `out1-format` | Programmformat: Auflösung und Bildrate (OUT.1) |
| #69 | `handover-1017b` | Übergabe: PRs #62–#68 |
| #70 | `strom-still-release` | strom-Pin mit Fix für das Leck pausierter Clips (strom #15) |
| #71 | `ovl1-eight` | Acht Overlay-Kanäle (OVL.1, strom #16); vorweg `cargo fmt` über den Stapel |
| #72 | `str1-streams` | Alle Streams zusammen starten/stoppen (STR.1) |
| #73 | `api4-playback` | Abspielstand im Zustand (API.4) |
| #74 | `inp10-at-end` | Clip am Ende: Schleife oder stehen bleiben (INP.10, strom #17) |
| #75 | `aud2-seven-aux` | Sieben Aux-Busse A–G (AUD.2) |
| #76 | `vm-act-timeout` | VM-Check: Aktionen mit Zeitlimit, Threads von lmix-core bei Hänger |
| #77 | `str3-encoding` | Codec, Rate Control, Keyframes je Ausgang (STR.3) |
| #78 | `swi5-tbar-sound` | Ton folgt der T-Bar (SWI.5) |
| #79 | `handover-1017c` | Übergabe: PRs #69–#78, strom-Stapel |
| #80 | `engine-call-timeout` | Engine-Aufrufe mit Zeitlimit 60 s (Hänger von lmix-core) |
| #81 | `out10-multiview` | Multiview: Program links/rechts, Pegel an/aus (OUT.10) |
| #82 | `lay2-box-visible` | Layout-Box ein-/ausblenden (LAY.2) |
| #83 | `shw3-new-format` | Neue Show mit Formatwahl (SHW.3) |
| #84 | `personas-1017` | Persona-Durchlauf 11, Übergabe bis #84 |
| #85 | `str2-rtmp` | RTMP-Ausgang (STR.2, B31), strom #18 |
| #86 | `strom-upstream-fixes` | strom-Pin mit upstream #874/#788 (strom #19) |
| #87 | `vm-picture-retry` | VM-Check: fehlendes Bild ist ein Fehlversuch |
| #88 | `aud12-meter` | Pegel stumm blass, Spitzenwert 2 s (AUD.12) |
| #89 | `aud17-sample-rate` | Abtastrate 44,1 kHz wählbar (AUD.17) |
| #90 | `strom-upstream-sync` | strom mit ganzem upstream main 0.7.1 (strom #20), Player-Verzögerung 100 ms für Dateien |
| #91 | `decisions-1017` | Entscheidungen 17.10. (S-E.3/S-E.13 B, Merge, GPU später …) |
| #92 | `strom-va-pin` | VA-Brücke in den GL-Mischer (strom #21) |
| #93 | `gfx2-web-inputs` | Webseite und Grafikkanal als Input (GFX.2/GFX.6), Stinger-Schnitt mit dem Clip, Latenz-Perzentil |
| #94 | `eval-stinger-recorder` | Vergleich Stinger und Live-Recorder (Vorschläge offen) |
| #95 | `gfx6-page-control` | JavaScript, CSS, Einfrieren einer Seite (strom #22) |
| #96 | `snd-cues` | Klang- und Fade-Cues in Zonen (strom #23–#25) |
| #97 | `stage-cues` | Bühnen und Bild-Cues, Block „Show Playback“ (strom #26) |
| #98 | `cue-preload` | Standby-Cues vorgeladen (SND.12) |
| #99 | `script-sound-files` | Drehbuch: Klang- und Bilddateien ab dem Ordner |
| #100 | `sound-repeat` | Wiederholungen von Klängen (SND.3, strom #27), Panik lässt Vorgeladenes |
| #101 | `picture-repeat` | Wiederholungen von Videos (VIS.2, strom #28) |
| #102 | `review-rule` | Jeder PR wird von einem neuen Claude-Agenten geprüft (`REVIEW.md`) |

Die wackelige Failover-Prüfung (früher rote Läufe auf `s1-feeds` und
`s1-script-folder`) ist mit #45 behoben.

Breaking (vor dem ersten Release erlaubt, je PR beschrieben): Cue `label`
→ `name`, `cue_back`/`cue_jump` → `cue_standby`, `next` → `standby`,
OSC `/lmix/cue/next` → `/lmix/cue/standby`, OSC-Abos laufen nach 60 s
ohne Lebenszeichen ab; Show-Dateien Version 1 werden beim Öffnen
umgewandelt (alte Datei bleibt als `.v1.bak`).

strom-Upstream (Eyevinn/strom `main`): ganz übernommen als Merge (strom
#20 `lmix-upstream-sync`, upstream 0.7.1 vom 15.10.), lmix #90 pinnt ihn.
Upstream ersetzt unseren Abtastraten-Patch und `gl_bridge.rs`; was es
behält und anpasst, steht in DECISIONS 17.10. „strom mit ganzem upstream
main“. CPU-Pfade sind in der VM geprüft, die GPU-Pfade (CUDA-/GL-Mischer,
NVDEC/NVENC, Thumbnails) brauchen die Abnahme auf der RTX 3090. Neu
verfügbar: HTML-Input (GFX.2), Player für Live-Streams/URLs, upstream-
Stinger, Live-Recorder.

S1 Stufen 2/3 (S-E.3 B, S-E.13 B, entschieden 17.10.): strom-Block
`builtin.show_playback` („Show-Wiedergabe“, strom #23–#26) mit Zonen und
Bühnen in einem Flow „lmix playback“ je Rechner. lmix #96 bringt Klang-
und Fade-Cues, der PR darauf Bühnen und Bild-Cues (DECISIONS 17.10.
„Klang-Cues gebaut“, „Bühnen und Bild-Cues gebaut“); Prüfung
`scripts/test-show-playback.sh`. Offen: Vorladen, Wiederholungen,
Begrenzer, Mischmodi, GPU-Bühnen, Console-Dialoge für Zonen/Bühnen und
ein Cue-Editor für Klang und Bild; Geräte und Bildschirme auf echter
Hardware (HARDWARE-TODO H.37, H.38).

## Branches und PRs (Gitea), Stand 2026-10-09

| PR | Branch | Basis | Inhalt | Stand |
|---|---|---|---|---|
| #1–#3 | `lastenheft`, `m2b`, `m3` | `main` | Lastenheft bis M3 | gemergt |
| #4 | `claude-todo` (mit `console-qt`) | `main` | Restart-502, Selbsttest, Qt-Console statt egui | gemergt |
| #5 | `m4` | `main` | M4: Overlays (4.1), T-Bar (4.3), Kanalzug und Aux-Busse (4.7) | gemergt |
| #6 | `m4-stinger` | `main` | M4: Stinger (4.2) | offen |

strom-Fork auf Gitea (`shrippen/strom`), Ziel `dev`, seit 17.10. gestapelt:
#15 `lmix-still-release` (Leck pausierter Clips) → #16 `lmix-nine-dsk`
(9 DSK-Ebenen) → #17 `lmix-player-loop` (Schleife live) → #18
`lmix-rtmp-output` (RTMP und 4:2:0 aus upstream) → #19
`lmix-upstream-fixes` (Stopp begrenzt, Bus-Watches) → #20
`lmix-upstream-sync` (ganzes upstream main). Das Submodul von lmix #74
zeigt auf #17, #85 auf #18, #86 auf #19, #90 auf #20; nach dem Merge auf
`dev` neu pinnen.
Danach (17.10. abends, je einzeln auf `dev` gemergt): #21 VA-Brücke,
#22 Seitensteuerung, #23 Show-Sound-Block, #24 16 Zonen/Mono, #25 Ende
an den Zonen, #26 Bühnen und Umbenennung in `builtin.show_playback`,
#27 Wiederholungen ohne Lücke (Pegel an den Mischer-Pads), #28 Videos
wiederholen, Nahtlogik als Modul.

strom-Fork (`shrippen/strom`), Ziel `dev`:
- Gemergt: #1 `lmix-start-time`, #2 `lmix-sample-rate`, #3
  `lmix-player-sysmem`, #5 `lmix-live-offset` (enthält #4
  `lmix-dsk-fade`, daher geschlossen).
- Offen: #6 `lmix-stinger-layer` (bis zu 8 DSK-Ebenen, Stinger-Ebene).
  Submodul von `m4-stinger` zeigt hierher.

Offen in dieser Reihenfolge: strom #6, dann lmix #6.

Seit 2026-10-09 auf `console-qt` und `claude-todo`:
- Live-Bilder über Unix-Sockets (`/api/live/…`, damals `?size=full` für
  die Vorschau; seit 19.10. `/api/live/preview`, VSP).
- Qt-Console `workspace/console-qt`: Schritt 1 gemessen, Schritt 2 (Funktionen
  der egui-Console) übertragen, Test `scripts/test-console-qt.sh`.
- lmix-core: CUDA-Programm für Bildschirm/NDI/Live heruntergeladen,
  GL-Mischer gibt Systemspeicher aus, Aktionen vor „Engine bereit“ warten auf
  den Restore, ein neu gestartetes strom bekommt die Show zurück.
- `scripts/soak-hardware.sh`: 60 min CUDA bestanden (DECISIONS,
  „Hardware-Läufe“).

## Lokal einrichten

```bash
git clone https://git.arianw.de/shrippen/lmix.git && cd lmix
git checkout main
git submodule update --init workspace/strom
```

Systempakete wie in `BUILD.md` („Systemabhängigkeiten“) bzw. die
apt-Liste in `scripts/cloud-setup.sh` (Ubuntu 24.04 / Kubuntu).

strom: `scripts/fetch-strom.sh` lädt das CI-Binary (ghcr, ohne Token)
oder baut lokal (`cargo build --release --features
no-gui`, 20–25 min kalt). Mischer-Backend wählt `lmix-core --mixer`
(Standard `cuda` für NVIDIA; ohne NVIDIA `gl` oder `cpu`).

## Starten (drei Prozesse)

```bash
STROM="$(scripts/fetch-strom.sh)"
"$STROM" --host 127.0.0.1 --port 8099 --no-auto-restart &          # Engine
(cd workspace/lmix-core && cargo run --release -- --strom http://127.0.0.1:8099) &   # Steuerung, API :8200
(cd workspace/console-qt && cargo run --release)                   # Qt-Console (Prototyp)
```

Die Qt-Console braucht Qt 6 mit Kirigami und das GStreamer-Plugin `qml6`
(Arch `gst-plugin-qml6`, Ubuntu `gstreamer1.0-qt6`); sie muss auf dem
Rechner der Engine laufen (Live-Bilder über Unix-Sockets in
`$XDG_RUNTIME_DIR/lmix/live`). Kante aktualisieren:
`scripts/sync-kante.sh`.

Kopfhörer für PFL/AFL: `lmix-core --monitor-out auto`. Externe Adresse
für WHIP-Links: `--public-url`.

## Prüfen

1. Ohne GPU zuerst den Gesamttest:
   `scripts/test-m2b-vm.sh` (in der Cloud zuletzt grün, Latenz p95 121 ms).
2. Dann die Hardware-Liste `docs/HARDWARE-TODO.md`, H.1–H.23. Für M3
   zuerst:
   - H.18 NVENC-Ausgänge, 1 h SRT und Aufnahme
   - H.20 Vollbild/DRM auf Beamer
   - H.21 Halten beim echten Kabelziehen
   - H.22 Latenz auf CUDA/GL (`GET /api/latency`, Knopf „Latency“)
   - H.23 SRT-Kamera als Input
3. Messwerte nach `docs/LATENCY.md`, Abgehaktes mit Datum in
   `HARDWARE-TODO.md`.

## Offen beim User

- **Merges** strom #6, lmix #6.

## Bekannte Befunde

- GL-Mischer: SRT-Ausgang ohne Video (DECISIONS, „Hardware-Läufe“).
- Qt-Console: Ziel Linux allgemein; Ubuntu 24.04 über gepinntes KDE neon (BUILD.md), ungeeignet Debian 12.

- SRT-Input-Block in strom decodiert in der VM nicht (DECISIONS,
  „M3 Schritt 4“).
- Halten bei Quellverlust: in der VM teils ein schwarzer Frame (H.21).
- MP4-Aufnahme reserviert ~93 MB moov; MKV ist Standard.
