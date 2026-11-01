Title: Properties: let identity's ts-offset change while a flow plays

## What is missing

Strom refuses a property change on a running flow unless GStreamer flags the property `MUTABLE_PLAYING` (or controllable). `identity`'s `ts-offset` lacks the flag, although it only shifts the timestamps of each buffer. A delay built with it, for example an audio delay for lip sync that an operator adjusts while watching, can only be changed by stopping the flow.

## Proposed direction

A short allowlist of (element type, property) pairs next to the mutability check in `validate_property_mutability`, starting with `("GstIdentity", "ts-offset")`. Everything else keeps the flag-based check.

Element info has to agree: `gst/discovery.rs` reports `mutable_in_playing` from the GStreamer flags, and the `update_flow` docs and the MCP tool `update_element_property` send clients to that flag. The allowlist would have to be applied there too, or clients are told "not mutable" for a property the write path accepts.

## Questions for the maintainers

1. A global allowlist that overrides GStreamer's flags, or a per-block path like the HTML Input's `url` (`blocks/builtin/html_input.rs`)? The latter keeps the exception where the element is used, the former is shorter for plain elements in a flow.
2. Is changing `ts-offset` live safe enough? A timestamp jump can make sync'ing sinks drop or wait (a gap when the offset grows, late buffers when it shrinks). Unverified so far whether buffers keep flowing after the change.

## Test case

`identity_ts_offset_changes_while_playing`: videotestsrc ! identity ! fakesink through `PipelineManager`, running; set `ts-offset` to 40 ms and read it back. Without the allowlist it fails with `PropertyNotMutable { element: "delay", property: "ts-offset", state: Playing }`. To be added: buffers still reach the sink after the change (count before and after), and element info reports `mutable_in_playing: true` for it.

A branch with the allowlist and this test exists on top of current `main` and can become a PR once the direction is agreed.
