# Todo

Erledigtes abhaken (`[x]`, Datum) und nach einer Weile löschen; Details stehen in den verlinkten Dokumenten.

## Für dich

- [ ] **Hardware-Nacht 19.10.** (DECISIONS 19.10.): strom #29 (CUDA-Mischer auf echter Hardware: Alpha nach Blende, Stinger, Vorschaubilder; Rampen-CRITICAL) und #30 (Panik erreicht Klänge) ansehen und mergen, danach lmix #116 (Submodul, `pulsesink`-Puffer, Doku). Mit dir: H.38 Bühne am Projektor ansehen (nachts waren die Bildschirme gedimmt), H.13, H.20, H.21.
- [ ] **Entscheiden: Gerätekanäle einer Zone** (H.37): Auf Geräten mit Kanalpositionen (FL, FR, …) landet eine Zone auf Kanal 3/4 nirgends, PipeWire ordnet nach Position. Kanalzahl und -belegung des Geräts mitschicken (Gerätekanal N = N-ter Kanal des Geräts), oder Gerätekanäle nur für Pro-Audio-Profile (AUX) anbieten?
- [ ] Vorschau-Ausgang im strom-Mischer (VSP, DECISIONS „Entscheidungsseite 18.10.“). REC (#104), STG (#106, #109, #111) und RUN.8 (#105) sind gemergt (2026-10-18).
- [ ] **Stinger-Sperre, Umfang (#109):** Darf lmix während eines Stingers auch Show öffnen/neu, Paket importieren und das Entfernen des Eingangs auf Program ablehnen? So ist es gebaut; Antwort steht aus.
- [ ] Nachzügler aus dem Review von #111 (Stinger-Sperre, Tastentest): Liste in `docs/HANDOVER.md`, „Nächste Schritte“.
- [ ] Review-Regel (REV.R): Repos ohne `agent.md` bekommen sie mit ihrem nächsten PR (Block wie in Kaiwa `agent.md`, eigenes `REVIEW.md`). Demo-Welt-Kopien (DEMO) beim nächsten Release je Projekt abgleichen (`sync-demo.py`).
- [ ] Sitzung auf dem 3090-Rechner (HW, CEF): `cefsrc` bauen (H.36), Hardware-Abnahme H.13, H.20, H.21, H.37, H.38, GPU-Pfade, VA. Checkliste in `docs/HARDWARE-TODO.md`.
- [ ] **S-E.3 entscheiden** (Klang-Cues: Abspieler je Zone oder verdeckte Inputs), mit Messung: Entscheidungsseite https://claude.ai/artifact/DAzFNWyRUKGiu4x5WUvFPo, „An Claude übergeben“. Danach S1 Stufe 2 (Zonen, Klang-Cues).
- [ ] **PRs vom 16./17.10. ansehen und mergen**, Reihenfolge in `docs/HANDOVER.md`: #45, #51, #39, dann der Stapel #38 → #52.
- [x] Cue-Zeitgenauigkeit 4 h (2026-10-19, DECISIONS „Hardware-Nacht“): mit Echtzeit, neben Builds und Tests, 226 von 1 468 811 Schritten über 10 ms, spätestens 385,7 ms. Ein Lauf auf freiem Rechner steht aus (eine Nacht ohne andere Arbeit).
- [x] PRs vom 16.10. gemergt (#28–#36, 2026-10-16); lmix #15–#25 und strom #7–#14 geschlossen, Inhalt war schon in `main` bzw. `dev`.
- [ ] PR-Agent antwortet nur „Failed to review PR“: der OpenRouter-Schlüssel hat sein Monatslimit erreicht (5,49 $ von 5 $, Reset 1.11.). Limit anheben oder anderen Schlüssel/Modell setzen (`PR_AGENT_LLM_KEY`, `PR_AGENT_MODEL`). PR-Agent läuft nur auf `/review` des Eigentümers, kein Merge wartet sonst auf ihn (`agent.md`).
- [ ] M7.0 (CEF gegen WPE): `cef` 152 ist installiert; `gstcefsrc` bauen lädt gstcefsrc und CEF 139 von außen und wurde heute nicht freigegeben. Freigeben, dann baue ich `cefsrc` nach `~/.local/lib/gstreamer-1.0`.
- [ ] Gitea-Actions: `concurrency.group` mit `${{ github.ref }}` trennte auf Gitea 1.27.3 die Refs offenbar nicht (Läufe 2509/2510, 2532). lmix prüft jetzt auf GitHub; betrifft noch die privaten Repos mit Gitea-CI.
- [x] strom PR #3 (`lmix-player-sysmem`) gemergt.
- [ ] CI läuft auf GitHub (`.github/workflows/ci.yml`, strom öffentlich, Paket `ghcr.io/shrippen/lmix-strom`): Secret `REGISTRY_TOKEN` auf Gitea löschen; in lokalen Kopien `git submodule sync workspace/strom` (Submodul-URL jetzt GitHub). strom `dev` hat `04b87d1` (DSK-Blende); dein lokales strom-`dev` mit `7615d6f` (NVDEC-Deinterlace) ist darauf noch nicht nachgezogen.
- [ ] Demo-Welt-Kopien in 15 anderen Projekten veraltet (neuer Abschnitt `graufeld.stream`): `python3 shrippen.github.io/demo/tools/sync-demo.py`, dann je Projekt committen — soll ich?
- [ ] Vorschau-Lücke ansehen (`scripts/run-local.sh`, Kachel anklicken) und entscheiden: Vorschau-Ausgang im strom-Mischer oder alte Ansicht halten, bis die neue ein Bild hat.
- [ ] Entscheiden: S-E.3 Klang-Cues (#39) und S-E.13 Bühnen (#60), je A oder B; Vorschlag beide B, als ein strom-Block. Danach baue ich S1 Stufe 2 (Ton geht ohne GPU) und Stufe 3.
- [ ] PRs vom 17.10. (#56–#84) liegen auf dem S1-Stapel über #54; Reihenfolge in `docs/HANDOVER.md`. strom #15–#17 zuerst (Submodul ab lmix #70).
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

- [ ] **Panik und Stopp erreichen laufende Klänge nicht** (H.37, DECISIONS 19.10.): Fix strom #30 (Live-Stille je Zone) ansehen und mergen, danach Submodul nachziehen und `QUIET=1 scripts/hw-sound-cues/run.sh` wiederholen.
- [ ] GO bis hörbar 77 ms statt ≤ 50 ms (SND.12, H.37): Rest ist strom's Zonenlatenz (30 ms) und PipeWire; prüfen, ob die Zonenlatenz kleiner gehen kann.
- [ ] CUDA-Mischer mit 16 Slots: Latenz 81–83 ms statt 56 ms (H.2, NFA.2 „≤ 2 Frames“). Mit 8, 12, 14, 15 Slots 56 ms: Sprung genau bei 16, nicht mit der Last (GL mit 16: 56 ms nach 5 min). Mit dem Schwarz-Input sind das 17 statt 16 Mischer-Eingänge (`black_input`). strom hat dort keine feste Grenze (nur `pick_grid` begrenzt Spalten auf 16); 25 ms sind kein ganzes Bild, eher eine verpasste Frist des CUDA-Mischers oder des Multiview. Mit `GST_DEBUG=aggregator:5` am `vm:mixer` die Fristen bei 16 und 17 Eingängen vergleichen.
- [ ] `cudaconverter`-Alpha-Bug upstream melden (GStreamer, `gstcudaconverter.cpp`: der Setter setzt `priv->alpha` nicht); den Workaround in strom #29 entfernen, sobald die Distributionen den Fix haben.
- [ ] strom-Tests auf der RTX 3090 rot, auch auf `dev`: `nvdec_cuda_after_a_converted_format_stays_on_the_gpu`, `measure_idle_fx_slot_renders`; `gpu_stingers_land_frame_accurately` flakig unter GPU-Last (H.34).
- [ ] Abnahmetest `test-m2b-vm.sh` für echte Hardware öffnen (`MIXER`, Strom-Umgebung, Looks auf GL, Bildschirm-Ausgang auf Wayland): in dieser Nacht als Kopie gelaufen (`~/.cache/lmix-hwnight/m2b/test.sh`). Die 28 CRITICALs der Facecam (GStreamer-V4L2-Abtastung) dort aus der Zählung nehmen.
- [ ] `scripts/test-titles.sh` schlug am 12.10. einmal fehl (direkt nach `test-web.sh`, Ausgabe nicht erhalten), danach 19-mal grün, am 17.10. in der Cloud 8-mal direkt nach `test-web.sh` grün; nicht nachstellbar. Bei der nächsten Wiederholung die Ausgabe sichern (`KEEP_WORK=1`) und die Ursache beheben.
- [ ] Cue-Zeitgeber unter Last (RUN.8): Echtzeit gebaut (#105), auf der RTX 3090 gemessen (DECISIONS 19.10.): 10 min Volllast 16,6 ms (ohne 133 ms), über rtkit 1,6 ms; 4 h neben Builds ein Ausreißer von 385,7 ms. Offen: 4 h auf freiem Rechner; hält 10 ms nicht, dann Zeitgeber ohne Switcher-Sperre (DECISIONS 18.10. „Grenze“).

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
- [x] VM-Check: `picture()` brach bei leerer Antwort sofort ab (behoben #87: in `wait_for` ein Fehlversuch) („no picture inputs/Orange“ im Signalverlust-Schritt, während der Slot neu startet); in `wait_for` sollte ein fehlendes Bild nur ein Versuch sein.
- [ ] VM-Check hing einmal (17.10., Branch `aud2-seven-aux`, 1 von 2 Läufen) nach „color tag“: lmix-core schickte 37 min nichts mehr an strom. Danach 3 Läufe am Stück grün, nicht nachstellbar. Befund: Engine-Befehle laufen unter dem Switcher-Lock, und der HTTP-Client zu strom hatte kein Zeitlimit; ein hängender strom-Aufruf friert so ganz lmix-core ein. Seit #80 Zeitlimit 60 s; seit #76 sichert der VM-Check bei einem Hänger die Threads von lmix-core und strom.
- [ ] Selten (etwa 1 von 3 VM-Läufen nach strom #15): ein gestoppter Slot-Flow bleibt kurz übrig, Standbild beim Leeren der Show oder Muster beim Failover-Stopp (4–8 Elemente, `src`/`file`, `caps`, `tap:tee`, `out:intersink`). Einzeln nicht nachstellbar; vermutlich hält ein Streaming-Thread kurz eine Referenz, wenn strom direkt nach dem Stopp prüft. Bisher als „beim Leeren der Show bleibt ein Standbild-Slot-Flow übrig“ notiert („leaked element 'file'/'decode'/'freeze'/'out:intersink'“, 8 Elemente). Einzeln (PNG/JPG anlegen, als Overlay, entfernen) nicht nachstellbar; vermutlich ein Wettlauf beim Stoppen von `imagefreeze`.
- [ ] M7 Rest (DECISIONS „M7 Teil A“): Grafikkanal als Input und Overlay (GFX.2, GFX.17, nach M7.0), Overlay wartet auf die Ausblendung (GFX.4), Titel als Stinger (GFX.11), Beamer für Textfolien (GFX.21, mit GFX.2), M7-Abnahme mit 5 Personas.
- [ ] M9 Rest (DECISIONS „M9 Anfang“): PTZ-Positionen als virtuelle Inputs mit Standbild (PER.3, B38), PTZ und TSL an echter Hardware prüfen, Key/Effekte/Position/Ebenen in den Input-Einstellungen (UI.8, braucht Engine), HID/Gamepad/Stream Deck direkt (AUT.1).
- [x] Lastenheft-Stand der M6/M8-Punkte (AUT.*, API.*, WEB.*, OPS.*) nachgetragen (2026-10-11).
- [x] Lastenheft-Stand aller Bereiche gegen den Code geprüft (2026-10-17): 68 Einträge, vor allem M2b–M5 (INP, SRC, SWI, AUD, OUT, REC, STR), die noch auf dem Stand vom 27.09. waren. NFA hat keine Stand-Spalte; Messwerte stehen in `LATENCY.md` und `HARDWARE-TODO.md`.
