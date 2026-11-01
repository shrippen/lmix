Title: Flows: stop with EOS first, so muxers can finish their files

## What is missing

Stopping a flow sets the pipeline straight to NULL. A muxer in it never sees EOS: a recording without robust muxing loses its trailer (an MKV without its index, an MP4 without `moov` is unreadable), and an RTMP output disconnects without unpublishing.

## Proposed direction

A flow property `eos_on_stop` (default off: stopping stays immediate for every other flow). With it on, stop sends EOS to the pipeline and waits until it has passed every sink, at most 5 s, then tears the pipeline down as today (the bounded NULL transition stays). A flow that does not drain within the deadline is stopped anyway, with a warning.

## Questions for the maintainers

1. A per-flow property, or the default for flows that contain a recorder or a streaming output?
2. 5 s fixed, or configurable?
3. Recorders already use robust muxing where they can; is the remaining gap (other muxers, RTMP unpublish) worth a property?

## Test case

A flow with a test source into an MP4 muxer and a file sink, `eos_on_stop` on: stop it after 2 s; the file is readable (`gst-discoverer` reports its duration). With `eos_on_stop` off the same file has no `moov` (today's behaviour). A flow whose sink never passes EOS stops after 5 s.
