# Working rules for this repository

The rules for all shrippen repositories are in `RULES.md` (writing, code, commits, bug
fixes, repository, GUI). It is a synced copy from the rules repository; do not edit it
here. This file holds only what is particular to this repository. A Claude Code session on
the owner's machine has the rules through the global `CLAUDE.md` already and reads
`RULES.md` only for its GUI rule, before work on a user interface.

## Rules of this project

- Performance in tests: **count, don't time.** CI asserts on counts that do not depend on the
  machine: events, bytes and requests per operator action, state rebuilds, delegate
  rebuilds, frames scheduled, late and dropped (`output-stats`), queue levels. A budget is a
  number in the test, with a comment that says where it comes from.
- Time budgets (frame gaps, latency, CPU, switch gaps) run on real hardware only, not in
  CI: as a script under `scripts/` that prints percentiles and exits non-zero over budget,
  listed in its header with the hardware it needs.
- Hunt crashes actively, don't wait for them: property tests on random action sequences
  (`proptest`; no panic, invariants hold), fuzzing of every parser of outside input (show
  file, action JSON, OSC, TCP, TSL, MIDI), and chaos runs (kill and restart the engine or
  lmix-core mid-flow, drop the connection, rebuild flows in random order). A crash found
  this way gets a regression test with the input that caused it.

## GUI (this project)

### lmix: Stil der Bedienoberfläche

- Stilwahl in den Einstellungen: **Kante** (Standard: Gruvbox dunkel, bei
  hellem Systemthema das helle Schema „Leinen“), **Kante Light** (abgespeckt:
  nur die Formsprache von Kante, ohne Gruvbox-Farben, auf den Farben des
  Systems), **System** (Rückfall: KDE Plasma / Breeze, folgt dem
  Plasma-Farbschema). Wie das `kind` von `KanteStyle` (`qml/Kante`).
- Fehlt Kante für das gewählte GUI-Toolkit (z. B. egui), wird die Anbindung
  zuerst in Kante gebaut (aus `tokens/palette.json` generiert), nicht in lmix.
- Monitore und Bildflächen bleiben in allen Stilen dunkel (Farbbeurteilung).

## Repository (this project)

- CI runs on the GitHub mirror (`.github/workflows/ci.yml`, since 16.10.2026) and reports back to Gitea as the commit status `github / test`. The strom binary goes to `ghcr.io/shrippen/lmix-strom` (public) only.
- The merge waits for: CI is green: the newest `github / test` status on the PR's commits is success, and no commit after the one it is on touches a path of `on.push.paths` in `.github/workflows/ci.yml` (one push of several commits gets one status, on its last commit). Whether CI is needed goes by the paths, not by what status is there: a PR none of whose commits (a merged base included) touches such a path needs no status; any other waits for it, and no status yet means wait, not merge. PR-Agent's status does not count here.
- The owner also confirms: `packaging/`, and the strom image CI pushes for each strom commit counts as rebuilding; rebuilding what is already published does not count.
- Everything else: `RULES.md` (Repository) and the skill `pr-review`.
