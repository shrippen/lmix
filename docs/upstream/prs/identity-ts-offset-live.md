Title: fix(properties): let identity's ts-offset change while playing

## Problem

Strom refuses a property change on a running flow unless GStreamer flags the property `MUTABLE_PLAYING` (or controllable). `identity`'s `ts-offset` lacks the flag, but it only shifts buffer timestamps and is safe to change live. A delay built with it, for example an audio delay for lip sync that an operator adjusts while watching, could only be changed by stopping the flow.

## Change

A short allowlist `LIVE_SAFE_PROPERTIES` of (element type, property) pairs next to the mutability check in `validate_property_mutability`, with `("GstIdentity", "ts-offset")` as its only entry. Everything else keeps the flag-based check.

Rejected: dropping the check for all properties (unsafe for many), or a dedicated block for a delay (a plain element in a flow is enough here).

## Evidence

- `ts-offset` can be changed on a running flow: `gst::pipeline::tests::identity_ts_offset_changes_while_playing` starts videotestsrc ! identity ! fakesink through `PipelineManager`, sets `ts-offset` to 40 ms and reads it back. With the allowlist check disabled it fails with `PropertyNotMutable { element: "delay", property: "ts-offset", state: Playing }`.

## Tests

Ran on Linux (Fedora 44, GStreamer 1.28, x86_64), on current `main` (4479d8c):
- `cargo test --lib identity_ts_offset`: 1 passed (and failed with the fix disabled, see above).
- `cargo fmt --all --check`: clean.

Not run: the full suite.

## For the reviewer

The allowlist is the only way past the check without the GStreamer flag. It is meant for properties that only change values the element applies to each buffer anyway.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
