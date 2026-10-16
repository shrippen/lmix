# Todo

Erledigtes abhaken (`[x]`, Datum) und nach einer Weile löschen; Details stehen in den verlinkten Dokumenten.

## Für dich

- [x] strom PR #3 (`lmix-player-sysmem`) gemergt.
- [ ] CI `strom.yml`/`vm-check.yml` seit Einführung rot (403 beim Submodul `strom`): neuen Gitea-Token mit `read:repository` + `write:package` als Secret `REGISTRY_TOKEN` hinterlegen, dann Lauf von PR #14 (`ci-submodule-token`, `persist-credentials: false`) neu starten und bei grün mergen. Der jetzige Token kommt an (Lauf 1859), darf `strom` aber nicht lesen.
- [ ] Demo-Welt-Kopien in 15 anderen Projekten veraltet (neuer Abschnitt `graufeld.stream`): `python3 shrippen.github.io/demo/tools/sync-demo.py`, dann je Projekt committen — soll ich?
- [ ] Kante PR #36 mergen (Farb-Labels, `KanteLevelMeter`), danach in `shrippen.github.io` `./build.sh` committen (liegt schon gebaut, uncommittet).
- [ ] Vorschau-Lücke ansehen (`scripts/run-local.sh`, Kachel anklicken) und entscheiden: Vorschau-Ausgang im strom-Mischer oder alte Ansicht halten, bis die neue ein Bild hat.
- [ ] Breeze-Bug melden (siehe unten).
- [ ] Hardware mit dir: H.13 Kopfhörer, H.20 Vollbild auf zweitem Monitor/Beamer, H.21 Kabel ziehen (`docs/HARDWARE-TODO.md`).
- [x] NFA.7 8-h-Lauf (2026-10-12): ohne Ausfall, Latenz stabil; 48-h-Lauf nach dem Speicher-Fix (H.24).
- [ ] CI für die Tests: `cargo test` (lmix-core, lmix-types, console-qt), `scripts/test-titles.sh` und `scripts/test-web.sh` laufen in keiner CI (nur `vm-check`). Nach dem Token-Fix (lmix-core braucht das Submodul `strom` für `strom-types`) einen Workflow `tests.yml` anlegen, in `.gitea/` und `.github/`, Chromium über Playwright.
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

- [ ] `scripts/test-titles.sh` schlug am 12.10. einmal fehl (direkt nach `test-web.sh`, Ausgabe nicht erhalten), danach 19-mal grün; bei der nächsten Wiederholung die Ausgabe sichern und die Ursache beheben.

- [x] Demo-Modus mit Studio-Weber-Daten (2026-10-09, `demo/start.sh`).
- [ ] Release-Build (wenn es einen gibt): `demo/` ausschließen und prüfen.
- [ ] Andere dekodierende Quellen (SRT, NDI, Gerät) auf GPU-Speicher im Slot-Kanal prüfen (DECISIONS „Hardware-Decoder“).
- [x] egui-Console entfernen (2026-10-09): Selbsttest als `lmix-selftest`, VM-Test und Screenshot über die Qt-Console (Cloud-VM: Qt 6 aus KDE neon).
- [x] strom-Speicher wuchs ~31 MB/h (H.24, 2026-10-13): ISO über `autovideoconvert` → NVENC, Fix strom #11, 3-h-Lauf flach, 8-h-Lauf ~7,7 MB/h in Stufen; offen: 48 h.
- [x] Ton eines pausierten Clips (2026-10-13): Programm-Audio unverändert 80 ms, kein Fix nötig.
- [x] CUDA-Latenz 106 ms statt 56 ms (2026-10-12): leere DSK-Ebenen seit `dde8237`, Fix strom `lmix-ignore-inactive-pads` (DECISIONS „CUDA-Latenz 106 ms“).
- [x] GL-Mischer: SRT-Ausgang ohne Video (2026-10-13: behoben mit strom #11, 30 fps über 1 h).
- [ ] GL-Mischer: strom-Speicher wächst mit SRT ~110 MB/h (2026-10-13 ruhig gemessen, 90 min, 1 Input: 672 → 813 MB ab Minute 10, 30 fps sauber; CUDA im 8-h-Lauf ~7,7 MB/h). Ursache suchen: GL→CUDA-Übergabe vor NVENC (`cudaupload` aus GL-Speicher) oder SRT; Gegenprobe GL mit Aufnahme statt SRT.
- [x] GL-Mischer: Layout-Rahmen fehlten (H.29): udmabuf-Upload auf NVIDIA, Fix strom #14.
- [x] Aktion in den ersten 2 s nach Engine-Neustart antwortet 502 (2026-10-09: neue Engine erkannt, Aktion geht in die Show, Restore sofort).
- [ ] Konstantes Bildalter 54 ms bei einigen Kacheln klären.
- [ ] M7 Rest (DECISIONS „M7 Teil A“): Grafikkanal als Input und Overlay (GFX.2, GFX.17, nach M7.0), Overlay wartet auf die Ausblendung (GFX.4), Titel als Stinger (GFX.11), Beamer für Textfolien (GFX.21, mit GFX.2), M7-Abnahme mit 5 Personas.
- [ ] M9 Rest (DECISIONS „M9 Anfang“): PTZ-Positionen als virtuelle Inputs mit Standbild (PER.3, B38), PTZ und TSL an echter Hardware prüfen, Key/Effekte/Position/Ebenen in den Input-Einstellungen (UI.8, braucht Engine), HID/Gamepad/Stream Deck direkt (AUT.1).
- [x] Lastenheft-Stand der M6/M8-Punkte (AUT.*, API.*, WEB.*, OPS.*) nachgetragen (2026-10-11).
