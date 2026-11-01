Title: Inter Input: switch channel while the flow plays

## What is missing

An Inter Input's `channel` is fixed when the flow is built. Giving a running output another source (another mix's sound, a different program) means rebuilding the flow and restarting the stream, which drops the connection of an RTMP or SRT output.

## Proposed direction

Map `channel` to the `intersrc`'s `producer-name` as a live property: the element unsubscribes from the old producer and subscribes to the new one. The value is stored as before, so a restart keeps the new channel.

## Questions for the maintainers

1. Is a live `channel` change acceptable for the Inter Input contract (the caps of the new producer may differ; the block then renegotiates, or fails like a channel that changes caps today)?
2. Should switching be glitch-free (a crossfade or a cut on a keyframe), or is a plain switch enough for a first step?

## Test case

Two Inter Outputs on channels `a` (440 Hz) and `b` (880 Hz) and one flow with an Inter Input on `a` into an appsink: while it plays, set `channel` to `b`; within a second the appsink gets 880 Hz without the flow restarting (same pipeline, no state change), and the stored property reads `b`.
