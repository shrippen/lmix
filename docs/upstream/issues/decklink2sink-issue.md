Title: DeckLink Output: frames late after the second start in one process; move to decklink2sink?

## What is missing

`decklinkvideosink` shows every frame late once an output starts a second time in the same process (GStreamer work item 5391, gst-plugins-bad). GStreamer's answer was that `decklink2sink` replaces it and the old sink will not get the fix. In Strom this hits every flow restart: the second start of a DeckLink Output plays late.

## Proposed direction

Build the DeckLink Output block on `decklink2sink`, with a `decklink2combiner` in front when sound comes along (the sink takes sound only attached to frames); audio-only mode drives the combiner with black frames. External pads point at the combiner's video/audio pads, or at the sink for picture only. The card clock seed goes away: `decklink2sink` has no clock of its own and schedules frames by its own count. A missing plugin fails the block with a message naming the GStreamer version it needs. A `processing_deadline_ms` property (bounded) sets the sink's `processing-deadline`, so a busy pipeline gets that much time before a frame is due at the card instead of having it dropped as late.

## Questions for the maintainers

1. `decklink2` ships with GStreamer 1.30, above the 1.28 target. Keep `decklinkvideosink` as the default and use `decklink2sink` when available (runtime check), or wait until the target moves?
2. This overlaps with the recent DeckLink Output changes (#1131, #1168); how should the two sinks share the adapted video input?

## Test case

A DeckLink Output started, stopped and started again in one process: the second run's frames reach the card on time (frame timing from the sink's statistics, or a loop back into a DeckLink input). Needs a card; the block construction (combiner present with audio, absent without; clear failure without the plugin) is testable without one.
