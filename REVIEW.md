# Review guide

Every PR is reviewed by a new Claude agent before it is merged (rule in
`agent.md`, owner 18.10.2026). The agent knows nothing of the session that
wrote the change: it gets the PR's intent, its diff and the repository, and
this file. The owner does not read code; this review is the second reader.

## What failure looks like here

lmix runs shows live. A bug that matters is one the audience sees or hears,
or one that stops the operator:

- **On air:** a black frame, a gap or click in sound, a cue that does not
  fire, fires twice or fires late, a fade that jumps, a picture on the
  wrong output.
- **Stops the show:** a panic, a deadlock, a blocked streaming thread, a
  GO that is refused, a panic button that leaves anything playing.
- **Leaks over a long show:** pipelines, pads, threads or memory that are
  not freed when a cue ends (shows run for hours).
- **Breaks the contract:** the strom API, a show file or the machine
  settings change so that an older file no longer loads (serde defaults).
- **Lies:** docs, DECISIONS, LASTENHEFT status or comments that say
  something the code does not do.

## What to check in particular

- GStreamer: closures hold `WeakRef`s, not strong refs; probes that run
  per buffer are cheap; an event is taken with `Handled` plus
  `info.data = None`, not `Drop` (GStreamer 1.24 CRITICAL); no seek from a
  streaming thread.
- Root cause on bug fixes: does the fix remove the cause, or hide one case
  of it? Is there a test that failed before the fix?
- Tests: is the new behaviour tested where it can be (unit, strom
  integration, `scripts/test-*.sh`)? Say what is only verified on
  hardware later.

## Skip

- `workspace/strom/openapi.json` and other generated files, `Cargo.lock`.
- `demo/` screenshots and images.
- Formatting and style that `cargo fmt` and `clippy` already enforce.

## How to report

- Every finding names file and line and the concrete input or state that
  goes wrong. No guess without evidence; if unsure, say so.
- Each finding gets one label: **Wichtig** (a bug from the list above,
  must be fixed or answered before the merge), **Kleinigkeit** (optional,
  at most five) or **Bestand** (the bug was there before this PR; not
  blocking).
- The report starts with the count, e.g. "2 Wichtig, 1 Kleinigkeit" (the
  session puts the heading above it). With nothing to fix it says so
  plainly: "Nichts zu beheben."
- In a later round: say for each open **Wichtig** of the last round whether
  it is fixed, or whether its answer is accepted, with the reason.
- Findings in German, code names as they are.
