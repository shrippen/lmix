Title: A paused Media Player makes live aggregators downstream wait for their deadline

## What is broken

Pausing a Media Player stops its internal pipeline, so the main pipeline's video appsrc gets no more buffers. A live aggregator downstream (a Vision Mixer input, also across Inter Output / Inter Input) counts that pad as active but starving and waits for its deadline on every output frame. One paused clip anywhere on a mixer, for example a clip parked for its next use, added about 30 ms to program latency: 86 instead of 56 ms, measured once by hand on a GPU mixer (no automated measurement yet). The same happens at the end of a playlist without looping, and after Stop.

`ignore-inactive-pads` on the consumer does not help: it skips only pads that never had a buffer, and a paused player's pad had buffers.

## Proposed direction

Keep the paused player's video output fed: repeat the last decoded frame at the frame rate (40 ms when the caps carry none), stamped with the main pipeline's running time, while paused, stopped or at the end of the playlist; play, resume and file changes end it.
- Raw video only (`video/x-raw`, any memory). In passthrough mode (`decode=false`) the player emits encoded frames; repeating a P or B frame without DTS would break decoders and muxers behind it, so nothing is repeated there.
- The stored frame is dropped on play, on every file change and on shutdown, so a new file paused before its first frame shows nothing rather than the old file's frame. Next, previous and goto play the new file, so the output stays fed. Stop keeps the frame it stopped on.
- The hold at the end of the playlist starts under the player's control lock, and a seek on a player that is not paused (a finished playlist played again) ends it, so a repeat never runs next to playback.
- The repeat holds only weak references between pushes, so removing the flow still frees the pipeline (the lifecycle leak check stays clean).

## Questions for the maintainers

1. Default for every Media Player, or a property (for example "When paused: hold last frame | send nothing")? It changes what every consumer sees:
   - a recorder behind a paused player keeps writing the still frame, and the file grows;
   - an encoder with an SRT or RTMP output keeps sending (usually better for live receivers, but different);
   - Stop shows the frame it stopped on, not black.
2. Or solve it at the consumer: let a live aggregator treat a pad that has stopped delivering as inactive after a while (related: the vision mixer's handling of inactive pads, and #1188, an open PR of ours, for a stalled multiview).
3. Cost: the paused player's branch runs at full frame rate again (conversion, upload at the mixer input, thumbnail tap, an encoder if there is one), about as much as during playback. Negligible for one player, it adds up with many paused players compared to today's zero frames. Acceptable?
4. Audio: the audio appsrcs still go silent, and a live `audiomixer` behind them waits for its timeout the same way. Fill with silence as well, or leave it?

## Test case

- A paused player keeps feeding its output: play a clip through a real flow, pause, count buffers at the flow's sink for 2 s. Today: 0 frames; with the change: the full rate.
- Passthrough: a paused player with `decode=false` sends no repeated encoded frames.
- Paused on a new file that has shown no frame yet (one without video), the player holds nothing rather than the old file's frame.
- Next, previous and goto while paused feed the output at the full rate.
- A seek after the end of the playlist plays the file again without a repeat running next to it (a repeat left running would double the frame rate).
- Removing a paused player's flow frees its pipeline (`pipeline_lifecycle_test`).

A branch with this exists on top of current `main` and can become a PR once the direction is agreed.
