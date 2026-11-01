Title: Block statistics: Inter Input queue fill and drops, Video Encoder load

## What is missing

A client that shows an output's health wants four numbers: bitrate, dropped frames, buffer fill and encoder load. Strom's block statistics (`GET /api/flows/{id}/rtp-stats`) give the first for RTP blocks, but nothing for the other three on the usual output path (Inter Input -> Video Encoder -> output):

- The Inter Input's `appsrc` drops its oldest buffers silently once it holds `max-time`. Neither how full it is nor how much it has dropped is visible anywhere.
- Nothing measures how long an encoder takes per frame, so a software encoder that cannot keep up only shows as drops further up, with no hint where.

## Proposed direction

Extend the existing block statistics; no new endpoint.

- **Inter Input** (`InterInputStats` in `strom-types`): `queued_buffers`, `queued_time_ns`, `max_time_ns`, `dropped_buffers`. Read from the `appsrc` properties only on request, nothing per buffer. `appsrc`'s `dropped` counter exists from GStreamer 1.28 on; below that (floor 1.22) the drops would be counted from the `dropped-buffer` element message the inter consumer posts for each drop (only when a drop happens).
- **Video Encoder** (`VideoEncoderStats`): `frames` and summed `encode_time_ns`, plus `hardware` (from `Hardware` in the element klass, as for decoders). Two BUFFER probes, on the encoder's sink and src pad, note a frame's running time going in and coming out, matched in running time (x264enc moves its output segment by 1000 h, so PTS alone does not match). Atomics and a fixed ring only, no lock, allocation or formatting per buffer, as `CLAUDE.md` asks for buffer probes. The ring is cleared on SEGMENT and FLUSH_STOP so a restarted running time cannot pair with an old frame. Two readings give encode time per frame and the busy share.
- The frontend shows both like the RTP statistics.

## Questions for the maintainers

1. Are two BUFFER probes per Video Encoder acceptable for this, or should timing be opt-in (a block property) or left out, keeping only the Inter Input part?
2. A hardware encoder with several frames in flight reports its latency as encode time, which overstates its load. The `hardware` flag lets a client tell the two apart. Is that enough, or should hardware encoders not be timed at all?
3. Inter Input drops before GStreamer 1.28 from the bus message: worth it, or report the drop count only on 1.28+ and leave it at 0 / absent below?

## Test case

Two integration tests that build the blocks through their builders:
- a Video Encoder (x264enc) fed a fixed number of frames to EOS reports exactly that many frames, a positive encode time, and `hardware: false`;
- an Inter Input with a small `max_time` whose consumer never pulls keeps what fits and drops the rest: `queued_buffers > 0`, `dropped_buffers > 0`, `queued_time_ns <= max_time_ns`, and queued plus dropped equals what was published (on 1.28 from `appsrc`, below 1.28 through the block's bus handler).

We have this working on a branch on top of current `main` (about 900 lines with tests, types and frontend) and can open it as a PR, or split it (Inter Input first), once the direction is agreed.
