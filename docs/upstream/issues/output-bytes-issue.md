Title: Outputs: report the bytes sent or written (RTMP, Live Recorder)

## What is missing

A client that shows an output's bitrate has only the bitrate it configured, except for SRT, whose element reports statistics. For RTMP and the Live Recorder there is no way to see what actually leaves: a stream that stalls or a recorder that stops writing looks the same as a healthy one.

## Proposed direction

`GET /api/flows/{id}/output-bytes`: the bytes each output element has sent or written since start, keyed by element name. A client takes the rate from two readings.

- RTMP (`rtmp2sink`): `out-bytes-total` from its `stats` structure, the bytes on the socket including RTMP framing. Read only on request, nothing per buffer.
- Live Recorder: its file sink adds each write (headers included) to an `AtomicU64` that the API reads without the state lock.
- The `splitmuxsink` Recorder keeps no counter and is not covered.

New types in `strom-types`, `utoipa` annotation, registered in `openapi.rs`, `openapi.json` updated.

## Questions for the maintainers

1. A separate endpoint, or part of the existing block statistics (`/flows/{id}/rtp-stats`), which would then cover outputs too (see also the block statistics issue for Inter Input and Video Encoder)?
2. Element names as keys, or block ids (an output block can contain several sinks)?
3. Should the splitmuxsink Recorder count its writes too (a probe on its sink pad would be per buffer)?

## Test case

- Live Recorder: record over at least one split; the counter grows while recording and equals the sum of the written file sizes.
- RTMP: the sink reports a network counter (0 before a connection), the muxer none.
