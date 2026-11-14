# Architecture

How this repository is meant to be built. Where the code differs today is listed under
"Known deviations". Change this file in the PR that changes the structure. The fixed
decisions behind it are in `PROJEKT.md`, the requirements in `docs/`.

## What it does

lmix is a live production program for Linux: a software vision mixer (programme and
preview, transitions, sound, outputs) and a show runner that plays a prepared list of
cues. One window with pages (Mix, Show, Audio, Outputs, Setup) steers a control
process, which steers the media engine strom. Each of the three is a process of its
own, so a crash of the window stops no picture and no cue.

## Building blocks

| Path | Its one task |
|---|---|
| `workspace/strom/` | The media engine (Eyevinn's strom, a submodule on the branch `dev`). Not ours: the smallest possible difference, and every change goes upstream. |
| `workspace/lmix-types/` | The words all parts share: actions, state, events, the parts of a show. No I/O. |
| `workspace/lmix-core/src/main.rs` | The control process: starts and wires the parts below. |
| `…/api.rs`, `osc.rs`, `tcp.rs`, `midi.rs`, `web.rs` with `web/`, `access.rs` | The ways in: HTTP and WebSocket, OSC, text lines over TCP, MIDI, the browser pages; who may use them. |
| `…/tsl.rs`, `tally_http.rs`, `feedback.rs`, `osc_subs.rs`, `feeds.rs`, `metrics.rs`, `patch.rs` | The ways out: tally, lamps on controllers, OSC subscribers, health figures, state changes for clients. |
| `…/service.rs` | The service: one state machine, one engine, many clients. It puts all actions in one order, reads and writes files, and sends the events. |
| `…/switcher.rs`, `switcher/` | The state machine: applies an action to the show's state and decides what the engine must do. No I/O. In the target one part per topic, as `switcher/graphics.rs` is today. |
| `…/cues.rs`, `moods.rs`, `checks.rs`, `playback.rs`, `stages.rs`, `zones.rs`, `clips.rs` | The rules of one topic each, used by the state machine. No I/O. |
| `…/show.rs`, `package.rs`, `settings.rs`, `layouts.rs`, `recent.rs`, `script.rs`, `titles.rs`, `datasource.rs` | The files: the show, its package, this machine's settings, templates, data sources. |
| `…/targets.rs`, `osc_out.rs`, `ptz.rs`, `machine.rs`, `realtime.rs` | This machine and its devices: output targets, OSC out, cameras, load, the cue timer's priority. |
| `…/engine/` | The engine drivers: turn the state machine's commands into calls to strom (`strom.rs`, with the flows it builds), or into nothing (`null.rs`, for tests and for running without an engine). |
| `workspace/console-qt/src/` | The window's Rust side: `bridge.rs` is its only way to lmix-core; forms, state changes for QML, global shortcuts, live pictures. |
| `workspace/console-qt/qml/` | The pages and dialogs. `Kante/` is a synced copy of the design system, never edited here. |
| `workspace/lmix-script/` | Reads a script folder and loads its cue lists into lmix-core through the API. |
| `workspace/selftest/`, `workspace/tools/` | A load and acceptance test straight at strom; a stand-in for the desktop portal in tests. |
| `scripts/`, `packaging/`, `demo/`, `examples/`, `docs/`, `tools/` | Tests and measurements (those on real hardware name it in their header); desktop file and systemd units; the demo start and screenshots; small clients; requirements, plans, decisions; the structure check (synced copy). |

## Layers

```
 lmix window (console-qt)       lmix-script      browser pages, OSC, TCP, MIDI
  QML ─► bridge.rs                  │                      │
        │ HTTP + WebSocket          │ HTTP                 │
        ▼                           ▼                      ▼
 lmix-core:   ways in (api, osc, tcp, midi; access)
                    │
                    ▼
              service ───────────► ways out (tally, lamps, OSC, metrics)
               │    │
               │    └──► files (show, settings, …), devices (targets, ptz, …)
               ▼
              state machine (switcher + topics): no I/O
               │ commands
               ▼
              engine/ (strom driver, null driver)
                    │ REST + WebSocket
                    ▼
 strom (own process, 127.0.0.1)          lmix-types: the words every part uses
```

- Three processes, joined by HTTP and WebSocket only. No part links another; the
  window never talks to strom. The types of the API are in `lmix-types`: a new or
  changed action comes there first, then lmix-core, then the clients.
- What must survive a crash of the window runs in lmix-core: the cue lists, their
  timing, panic. A page shows state and sends actions; it decides nothing about
  the show.
- QML reaches lmix-core through `bridge.rs` only. Which actions exist and what
  their fields are called is defined once, in `lmix-types`; a test holds what QML
  sends against it.
- A way in translates its protocol into an action and hands it to the service. It
  never touches the state machine or the engine.
- The state machine does no I/O and knows no engine: it returns commands, and an
  engine driver carries them out. Files, network and devices are the service's
  business, before or after the state machine's step.
- A rule stands once: how a reference names an input, what an action needs. An
  action is entered at one place per layer.
- A module is named after its one topic; modules do not import in a circle.
- The show names logical things only (zone, stage, output target). Devices,
  addresses and secrets are in this machine's settings, never in the show file.
- lmix-core knows no Markdown; a script comes in through `lmix-script` and the API.
- Only `selftest` and the engine driver talk to strom; strom's types stay in
  `engine/` and what reads its figures.
- Before the first release a changed file format is migrated once, not translated
  at every load.

## Where new things go

| Change | Touch |
|---|---|
| A new action | `lmix-types/src/action.rs`, the state machine's part for its topic, a test there; the right it needs in `access.rs`; then the page in `console-qt/qml/` |
| Something the engine must do | A command of the state machine, both drivers in `engine/` |
| A change to strom | A branch from upstream `main`, merged into `dev`; its entry in `docs/upstream/STROM.md` |
| A new way to steer lmix from outside | A file of its own beside `osc.rs` and `tcp.rs`, started in `main.rs` |
| A new signal to the outside | A file of its own beside `tsl.rs`, fed by the service's events |
| Something saved with the show | `lmix-types` (the type), `show.rs`; machine-bound things in `settings.rs` |
| A new page or dialog | `console-qt/qml/`, a test in `console-qt/tests/qml/`; texts through `scripts/update-translations.sh` |
| A new form | Its fields to actions in `console-qt/src/forms.rs` |
| A time budget | A script in `scripts/` that names its hardware; CI counts, it does not time |
| A design element that is missing | Kante, then `scripts/sync-kante.sh`; never `qml/Kante/` here |

## Known deviations

- Which input a reference means is worked out in three places, differently
  (`AUDIT.md`, finding 1).
- An action with file access is entered at up to five places in the service
  (`AUDIT.md`, finding 2).
- QML writes its actions as free text and no test checks them; the window repeats
  limits of the core (`AUDIT.md`, finding 3).
- Blacking out passwords lives in the module `srt` of `lmix-types`
  (`AUDIT.md`, finding 4).
- No check builds or tests `selftest` (`AUDIT.md`, finding 5).
- The state machine is one file of 14340 lines that holds nearly every topic
  (`AUDIT.md`, finding 6).
- Three versions of the show file and older shortcut files are translated at
  load (`AUDIT.md`, finding 7).
- The state machine uses constants and two functions of `engine/flows.rs`; small
  helpers sit in a way in or in the state machine, so modules import in a circle
  (OSC encoding in `osc.rs`, the input-name rule in `switcher.rs`, `targets.rs`
  and `osc_out.rs`); `api.rs` loads a show file itself; two dialogs send an HTTP
  request from QML themselves (not in `AUDIT.md`).
