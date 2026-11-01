Title: Device discovery: list audio outputs with their channel positions

## What is missing

A client that plays on one channel of a multichannel output device has to send a stream with the device's channel positions: PipeWire routes by position, not by index. Device discovery lists audio sources, but not audio sinks with their positions, so a client cannot know them.

There is also a small inconsistency: the two places that turn a `gst::Device` into a discovered device differ, and the hot-plug path keeps only 7 of the 21 properties the initial scan keeps.

## Proposed direction

- Device discovery also watches `Audio/Sink`, and puts PipeWire's `audio.position` into a new `DeviceResponse::channel_positions` (list of position names, empty when the device does not say).
- Both conversion paths share one function, so a hot-plugged device carries the same properties as one found at startup.

`openapi.json` updated for the new field.

## Questions for the maintainers

1. Are audio sinks wanted in device discovery (they are not sources a block can read from; they feed output blocks)?
2. Position names as PipeWire spells them (`FL`, `FR`, `AUX0`...), or mapped to GStreamer's channel positions?

## Test case

A unit test feeds a fake device with `audio.position = "[ FL, FR, AUX0, AUX1 ]"` through the conversion: `channel_positions` is `["FL","FR","AUX0","AUX1"]`; a device without it gives an empty list. A hot-plugged device and one from the initial scan convert to the same properties.
