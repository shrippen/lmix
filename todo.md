# Todo

Erledigtes abhaken (`[x]`, Datum) und nach einer Weile löschen; Details stehen in den verlinkten Dokumenten.

## Für dich

- [ ] **S-E.3 entscheiden** (Klang-Cues: Abspieler je Zone oder verdeckte Inputs), mit Messung: Entscheidungsseite https://claude.ai/artifact/DAzFNWyRUKGiu4x5WUvFPo, „An Claude übergeben“. Danach S1 Stufe 2 (Zonen, Klang-Cues).
- [ ] **PRs vom 16./17.10. ansehen und mergen**, Reihenfolge in `docs/HANDOVER.md`: #45, #51, #39, dann der Stapel #38 → #52.
- [ ] Cue-Zeitgenauigkeit: 4-h-Lauf `DURATION_S=14400 scripts/test-cue-timing.sh` lief in der Nacht; Ergebnis in DECISIONS (oder neu starten, falls nicht eingetragen).
- [x] PRs vom 16.10. gemergt (#28–#36, 2026-10-16); lmix #15–#25 und strom #7–#14 geschlossen, Inhalt war schon in `main` bzw. `dev`.
- [ ] PR-Agent antwortet nur „Failed to review PR“: der OpenRouter-Schlüssel hat sein Monatslimit erreicht (5,49 $ von 5 $, Reset 1.11.). Limit anheben oder anderen Schlüssel/Modell setzen (`PR_AGENT_LLM_KEY`, `PR_AGENT_MODEL`); deine lokale Änderung in `.gitea/pr-agent/` (Claude, adaptive thinking) ist noch nicht committet. Bis dahin genügt grüne CI (`agent.md`).
- [ ] M7.0 (CEF gegen WPE): `cef` 152 ist installiert; `gstcefsrc` bauen lädt gstcefsrc und CEF 139 von außen und wurde heute nicht freigegeben. Freigeben, dann baue ich `cefsrc` nach `~/.local/lib/gstreamer-1.0`.
- [ ] Gitea-Actions: `concurrency.group` mit `${{ github.ref }}` trennte auf Gitea 1.27.3 die Refs offenbar nicht (Läufe 2509/2510, 2532). lmix prüft jetzt auf GitHub; betrifft noch die privaten Repos mit Gitea-CI.
- [x] strom PR #3 (`lmix-player-sysmem`) gemergt.
- [ ] CI läuft auf GitHub (`.github/workflows/ci.yml`, strom öffentlich, Paket `ghcr.io/shrippen/lmix-strom`): Secret `REGISTRY_TOKEN` auf Gitea löschen; in lokalen Kopien `git submodule sync workspace/strom` (Submodul-URL jetzt GitHub). strom `dev` hat `04b87d1` (DSK-Blende); dein lokales strom-`dev` mit `7615d6f` (NVDEC-Deinterlace) ist darauf noch nicht nachgezogen.
- [ ] Demo-Welt-Kopien in 15 anderen Projekten veraltet (neuer Abschnitt `graufeld.stream`): `python3 shrippen.github.io/demo/tools/sync-demo.py`, dann je Projekt committen — soll ich?
- [ ] Vorschau-Lücke ansehen (`scripts/run-local.sh`, Kachel anklicken) und entscheiden: Vorschau-Ausgang im strom-Mischer oder alte Ansicht halten, bis die neue ein Bild hat.
- [ ] Entscheiden: S-E.3 Klang-Cues (#39) und S-E.13 Bühnen (#60), je A oder B; Vorschlag beide B, als ein strom-Block. Danach baue ich S1 Stufe 2 (Ton geht ohne GPU) und Stufe 3.
- [ ] PRs vom 17.10. (#56–#61) liegen auf dem S1-Stapel über #54; Reihenfolge in `docs/HANDOVER.md`.
- [ ] Breeze-Bug melden (siehe unten).
- [ ] Hardware mit dir: H.13 Kopfhörer, H.20 Vollbild auf zweitem Monitor/Beamer, H.21 Kabel ziehen (`docs/HARDWARE-TODO.md`).
- [x] NFA.7 8-h-Lauf (2026-10-12): ohne Ausfall, Latenz stabil; 48-h-Lauf nach dem Speicher-Fix (H.24).
- [x] CI für die Tests (2026-10-16, PR #28): Jobs `core` und `console` in `.github/workflows/ci.yml` mit cargo test, Headless, Titeln und Webseiten; Console-, Global-Keys- und GL-Test bleiben lokal.
- [ ] M7.0 Benchmark CEF gegen WPE: braucht freie GPU und `cefsrc`/`wpesrc` (beide fehlen auf dem Rechner, Installation mit sudo); danach rendert strom den Grafikkanal (GFX.2).

### Breeze-Bug „MenuItem RegExp“ melden

Wo: https://bugs.kde.org, Produkt **qqc2-breeze-style** (oder Merge Request auf https://invent.kde.org/plasma/qqc2-breeze-style, eine Zeile). Vorher prüfen, ob `/usr/lib/qt6/qml/org/kde/breeze/MenuItem.qml:75` in der aktuellen Version noch so aussieht. Ein Minimalbeispiel mit dem `qml`-Werkzeug löste den Fehler nicht aus; aufgetreten ist er in der Qt-Console (cxx-qt, Kirigami, Kante).

**Title:** MenuItem.qml: `RegExp(...)` lookup fails in Repeater delegates ("Property 'RegExp' of object QQmlDMListAccessorData is not a function")

**Versions:** qqc2-breeze-style 6.7.5, Kirigami 6.30.0, Qt 6.12.0, Arch Linux.

**What happens:** MenuItems created by a `Repeater` whose model is a JavaScript array of objects (`[{value, label}]`) print, per item:

```
org/kde/breeze/MenuItem.qml:75: TypeError: Property 'RegExp' of object QQmlDMListAccessorData(0x…) is not a function
```

and the mnemonic `Shortcut` binding breaks.

**Cause:** line 75 looks up `RegExp` unqualified:

```qml
enabled: !(RegExp(/\&[^\&]/).test(control.text))
```

In a delegate the context chain holds the model's accessor object, which answers the lookup before the global object.

**Fix:** use the regex literal; `RegExp(/…/)` is redundant anyway:

```qml
enabled: !(/\&[^\&]/.test(control.text))
```

**Workaround (apps):** create the MenuItems with `Component.createObject`, or use a `ListModel`.

## Offen (Claude)

- [ ] `scripts/test-titles.sh` schlug am 12.10. einmal fehl (direkt nach `test-web.sh`, Ausgabe nicht erhalten), danach 19-mal grün, am 17.10. in der Cloud 8-mal direkt nach `test-web.sh` grün; nicht nachstellbar. Bei der nächsten Wiederholung die Ausgabe sichern (`KEEP_WORK=1`) und die Ursache beheben.
- [ ] Cue-Zeitgeber unter Last bis 146 ms spät (RUN.8, DECISIONS 17.10.): Echtzeit-Priorität per rtkit für den Zeitgeber-Thread, dann 4 h unter Last messen; zusätzlich 4 h auf freiem Rechner.

- [x] Demo-Modus mit Studio-Weber-Daten (2026-10-09, `demo/start.sh`).
- [ ] Release-Build (wenn es einen gibt): `demo/` ausschließen und prüfen.
- [ ] Andere dekodierende Quellen (SRT, NDI, Gerät) auf GPU-Speicher im Slot-Kanal prüfen (DECISIONS „Hardware-Decoder“).
- [x] egui-Console entfernen (2026-10-09): Selbsttest als `lmix-selftest`, VM-Test und Screenshot über die Qt-Console (Cloud-VM: Qt 6 aus KDE neon).
- [x] strom-Speicher wuchs ~31 MB/h (H.24, 2026-10-13): ISO über `autovideoconvert` → NVENC, Fix strom #11, 3-h-Lauf flach, 8-h-Lauf ~7,7 MB/h in Stufen; offen: 48 h.
- [x] Ton eines pausierten Clips (2026-10-13): Programm-Audio unverändert 80 ms, kein Fix nötig.
- [x] CUDA-Latenz 106 ms statt 56 ms (2026-10-12): leere DSK-Ebenen seit `dde8237`, Fix strom `lmix-ignore-inactive-pads` (DECISIONS „CUDA-Latenz 106 ms“).
- [x] GL-Mischer: SRT-Ausgang ohne Video (2026-10-13: behoben mit strom #11, 30 fps über 1 h).
- [ ] GL-Mischer: strom-Speicher wächst auch ohne Ausgänge (~70 MB/h, 2 h, 2026-10-16; mit Aufnahme ~50 MB/h, mit SRT ~110 MB/h): Ursache im Mischer-Flow, nicht im Encoder (DECISIONS „GL-Mischer: Speicherwachstum ohne SRT“). In strom den Mischer-Flow zerlegen (ohne `gl_download`, ohne Kachel-Taps), je 1 h mit freier GPU.
- [x] GL-Mischer: Layout-Rahmen fehlten (H.29): udmabuf-Upload auf NVIDIA, Fix strom #14.
- [x] Aktion in den ersten 2 s nach Engine-Neustart antwortet 502 (2026-10-09: neue Engine erkannt, Aktion geht in die Show, Restore sofort).
- [ ] Konstantes Bildalter 54 ms bei einigen Kacheln klären.
- [x] strom meldete in jedem VM-Lauf zwölfmal „Pipeline 'lmix slot N': leaked element …“: Ursache war der Standbild-Thread eines pausierten Clips (hielt Haupt-Pipeline und appsrc über seinen Schlaf fest). Fix strom #15 (`lmix-still-release`), Submodul zeigt darauf.
- [ ] M7 Rest (DECISIONS „M7 Teil A“): Grafikkanal als Input und Overlay (GFX.2, GFX.17, nach M7.0), Overlay wartet auf die Ausblendung (GFX.4), Titel als Stinger (GFX.11), Beamer für Textfolien (GFX.21, mit GFX.2), M7-Abnahme mit 5 Personas.
- [ ] M9 Rest (DECISIONS „M9 Anfang“): PTZ-Positionen als virtuelle Inputs mit Standbild (PER.3, B38), PTZ und TSL an echter Hardware prüfen, Key/Effekte/Position/Ebenen in den Input-Einstellungen (UI.8, braucht Engine), HID/Gamepad/Stream Deck direkt (AUT.1).
- [x] Lastenheft-Stand der M6/M8-Punkte (AUT.*, API.*, WEB.*, OPS.*) nachgetragen (2026-10-11).
- [x] Lastenheft-Stand aller Bereiche gegen den Code geprüft (2026-10-17): 68 Einträge, vor allem M2b–M5 (INP, SRC, SWI, AUD, OUT, REC, STR), die noch auf dem Stand vom 27.09. waren. NFA hat keine Stand-Spalte; Messwerte stehen in `LATENCY.md` und `HARDWARE-TODO.md`.
