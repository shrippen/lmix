# Todo

Erledigtes abhaken (`[x]`, Datum) und nach einer Weile löschen; Details stehen in den verlinkten Dokumenten.

## Für dich

- [ ] strom PR #3 (`lmix-player-sysmem`, Player in Systemspeicher) mergen.
- [ ] Demo-Welt-Kopien in 15 anderen Projekten veraltet (neuer Abschnitt `graufeld.stream`): `python3 shrippen.github.io/demo/tools/sync-demo.py`, dann je Projekt committen — soll ich?
- [ ] Kante PR #36 mergen (Farb-Labels, `KanteLevelMeter`), danach in `shrippen.github.io` `./build.sh` committen (liegt schon gebaut, uncommittet).
- [ ] Vorschau-Lücke ansehen (`scripts/run-local.sh`, Kachel anklicken) und entscheiden: Vorschau-Ausgang im strom-Mischer oder alte Ansicht halten, bis die neue ein Bild hat.
- [ ] Breeze-Bug melden (siehe unten).
- [ ] Hardware mit dir: H.13 Kopfhörer, H.20 Vollbild auf zweitem Monitor/Beamer, H.21 Kabel ziehen (`docs/HARDWARE-TODO.md`).
- [ ] NFA.7: 8-h- und 48-h-Lauf, nachdem das Speicherwachstum in strom geklärt ist (H.24).
- [ ] PRs der Nacht 11./12.10. ansehen und mergen: lmix #15 (H.24-Vergleich, Todo), #16 (Qt-eigene Übersetzungen), Kante #53 (Dialog ohne Hintergrund in Kante Light, danach `scripts/sync-kante.sh` in lmix), lmix #17 (M7 Teil A, gestapelt auf #9, also erst #7–#9 und strom #7/#8).
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

- [x] Demo-Modus mit Studio-Weber-Daten (2026-10-09, `demo/start.sh`).
- [ ] Release-Build (wenn es einen gibt): `demo/` ausschließen und prüfen.
- [ ] Andere dekodierende Quellen (SRT, NDI, Gerät) auf GPU-Speicher im Slot-Kanal prüfen (DECISIONS „Hardware-Decoder“).
- [x] egui-Console entfernen (2026-10-09): Selbsttest als `lmix-selftest`, VM-Test und Screenshot über die Qt-Console (Cloud-VM: Qt 6 aus KDE neon).
- [ ] strom-Speicher wächst ~32 MB/h (H.24, DECISIONS): in der VM eingegrenzt (2026-10-09: kein Leck ohne Ausgänge, SRT-Statistik leckt nicht, nach 12 min flach); offen: Vergleichsläufe auf Hardware (ohne ISO, 8 Inputs, `gl`, `MALLOC_ARENA_MAX=2`).
- [ ] GL-Mischer: SRT-Ausgang ohne Video (Fix in strom, DECISIONS „Hardware-Läufe“); in der VM nicht nachstellbar (2026-10-09, DECISIONS „GL-Mischer und SRT“), braucht Hardware.
- [x] Aktion in den ersten 2 s nach Engine-Neustart antwortet 502 (2026-10-09: neue Engine erkannt, Aktion geht in die Show, Restore sofort).
- [ ] Konstantes Bildalter 54 ms bei einigen Kacheln klären.
- [ ] M7 Rest (DECISIONS „M7 Teil A“): Grafikkanal als Input und Overlay (GFX.2, GFX.17, nach M7.0), Overlay wartet auf die Ausblendung (GFX.4), Titel als Stinger (GFX.11), Textfolien aus Datei (GFX.21), XLSX/ODS und Sheets-API (DAT.1), Zeilen zu Spalten (DAT.3), M7-Abnahme mit 5 Personas.
- [ ] Lastenheft-Stand der M6/M8-Punkte (AUT.*, API.*, WEB.*) nachtragen, steht noch auf „–“.
