# Todo

Erledigtes abhaken (`[x]`, Datum) und nach einer Weile löschen; Details stehen in den verlinkten Dokumenten.

## Für dich

- [ ] Kante PR #36 mergen (Farb-Labels, `KanteLevelMeter`), danach in `shrippen.github.io` `./build.sh` committen (liegt schon gebaut, uncommittet).
- [ ] Vorschau-Lücke ansehen (`scripts/run-local.sh`, Kachel anklicken) und entscheiden: Vorschau-Ausgang im strom-Mischer oder alte Ansicht halten, bis die neue ein Bild hat.
- [ ] Breeze-Bug melden (siehe unten).
- [ ] Hardware mit dir: H.13 Kopfhörer, H.20 Vollbild auf zweitem Monitor/Beamer, H.21 Kabel ziehen (`docs/HARDWARE-TODO.md`).
- [ ] NFA.7: 8-h- und 48-h-Lauf, nachdem das Speicherwachstum in strom geklärt ist (H.24).

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

- [ ] Demo-Modus mit Studio-Weber-Daten (Welt in `shrippen.github.io/demo/world/` erweitern, `demo/shots.json`, nie in Releases).
- [ ] egui-Console entfernen: Client-Funktionen sind gleichauf (`docs/GUI-SHELL.md`, „Parität“). Vorher Selbsttest als eigenes kopfloses Binary herauslösen und VM-Test/Screenshots auf die Qt-Console umstellen.
- [ ] strom-Speicher wächst ~32 MB/h (H.24, DECISIONS): Ursache eingrenzen.
- [ ] GL-Mischer: SRT-Ausgang ohne Video (Fix in strom, DECISIONS „Hardware-Läufe“).
- [ ] Aktion in den ersten 2 s nach Engine-Neustart antwortet 502.
- [ ] Konstantes Bildalter 54 ms bei einigen Kacheln klären.
