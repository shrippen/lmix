# Drehbuch: Cue-Listen aus Markdown

Für den Show-Modus (`LASTENHEFT-SHOW.md` §3.7, IMP): Cues stehen im
Drehbuch neben dem Text, z. B. in einem Obsidian-Vault. `lmix-script`
liest den Ordner und legt die Cue-Listen über die API in `lmix-core` an;
`lmix-core` selbst kennt kein Markdown (S-E.2).

```text
lmix-script check <ordner>            Fehler mit Datei und Zeile
lmix-script load <ordner> [--watch]   in lmix-core laden, mit --watch bei jeder Änderung neu
lmix-script schema                    JSON-Schema eines Cue-Blocks (IMP.8)
```

`load` spricht `http://127.0.0.1:8200` an (`--core` oder `LMIX_CORE`),
mit Passwort über `--password` oder `LMIX_PASSWORD`.

**Ohne Terminal**: In der Show-Ansicht der Console „Drehbuch…“ wählen
(Aktion `set_script_folder`). Die Show merkt sich den Ordner, und
`lmix-core` hält `lmix-script load <ordner> --watch` dafür am Laufen, auch
nach einem Neustart (`--script-tool`, Standard: neben `lmix-core`, sonst
aus `PATH`). Was das Werkzeug meldet, steht im Log von `lmix-core`.

## Format

- **Eine Datei = eine Cue-Liste**, sofern sie mindestens einen
  `lmix-cue`-Block hat. Name aus dem Frontmatter `lmix-list:` oder dem
  Dateinamen. Unterordner zählen mit, versteckte (`.obsidian`) nicht.
- **Ein `lmix-cue`-Block = ein Cue**, in TOML, mit den Feldern des
  API-Cues (`lmix-script schema`). Nur `kind` hat einen Standard
  (`actions`); unbekannte Felder sind ein Fehler, damit Tippfehler
  auffallen.
- **Überschriften vor einem Cue werden Notiz-Cues** (Szenenmarken). Sie
  folgen in den Cue danach: GO auf „## Der Wald“ startet die Szene.
  Überschriften nach dem letzten Cue entfallen.
- **Nummern** (`number`) sind die feste Zuordnung (IMP.6); der Import
  nummeriert nicht. Eine Nummer zweimal in der Show ist ein Fehler an
  beiden Stellen, keine der Listen wird geladen.
- **Dauer eines Aktions-Cues** (`duration_ms`): so lange gilt er als
  laufend, z. B. die Länge des Clips, den er startet; `follow =
  "after_end"` wartet dann darauf. Ohne Dauer endet er sofort.
- **Medienpfade** (`path`, `paths` einer Quelle) zählen ab dem Ordner.
  Eine fehlende Datei ist eine Warnung mit Zeile; die Liste lädt trotzdem,
  der Cue meldet den Fehler, wenn er läuft (CUE.19).

````markdown
---
lmix-list: Session 3
---
## Der Wald

Die Gruppe erreicht die Lichtung.

```lmix-cue
number = "12"
name = "Wald"
actions = [{ action = "cut_direct", input = "Wald" }]
follow = "after_wait"
post_wait_ms = 2000
```

```lmix-cue
number = "13"
kind = "group"
name = "Gewitter"
mode = "timeline"

[[children]]
actions = [{ action = "overlay_in", channel = 1, input = "Blitz", duration_ms = 0 }]

[[children]]
kind = "wait"
pre_wait_ms = 500
duration_ms = 3000
```
````

## Klang und Bild

Klang-, Bild- und Fade-Cues (Show-Modus SND, VIS) haben dieselben Felder
wie in der API. Die Datei (`file`) zählt ab dem Ordner wie die übrigen
Medienpfade; Zonen und Bühnen werden mit ihrem Namen genannt (sie stehen
in der Show, die Geräte und Bildschirme dazu je Rechner).

````markdown
## Gewitter zieht auf

```lmix-cue
number = "20"
kind = "sound"
sound = { file = "ton/regen.flac", gain_db = -6, fade_in_ms = 3000, fade_out_ms = 2000, repeat = "forever" }
```

```lmix-cue
number = "21"
kind = "picture"
picture = { file = "bild/wolken.mp4", stage = "Hinten", layer = 1, fit = "fill", fade_in_ms = 1000 }
follow = "after_wait"
```

```lmix-cue
number = "22"
kind = "fade"
duration_ms = 5000
fade = { target = { cue = "20" }, gain_db = -60, then = "stop" }
```
````

`repeat` ist `"once"` (Standard), `{ times = 3 }` oder `"forever"`; die
Durchgänge folgen ohne Lücke. Ein Fade setzt nur, was er nennt: `gain_db` für einen Klang-Cue oder eine
Zone, `opacity` für einen Bild-Cue, `brightness` für eine Bühne
(`target = { stage = "Hinten" }`).

## Neu laden

`load` ersetzt eine Liste, die es schon geladen hat (erkannt an ihrer
Quelle `script:<ordner>/<datei>`). Standby bleibt auf demselben Cue,
laufende Cues laufen weiter (IMP.5): Ids entstehen aus Liste und Nummer,
bei Cues ohne Nummer aus dem Namen. Eine Datei mit Fehlern behält die
zuletzt geladene Fassung. Ist eine Datei weg oder ohne Cues, verschwindet
ihre Liste. Eine Liste gleichen Namens, die nicht aus dem Drehbuch kommt,
bleibt unangetastet (Fehler statt Überschreiben). In der Console sind
Listen aus dem Drehbuch nicht bearbeitbar (IMP.9).
