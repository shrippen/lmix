Title: Vision Mixer: fade DSK layers in and out on request

## What is missing

`DskToggleRequest` switches a DSK layer on or off instantly. A graphic (a logo, a lower third rendered by an HTML input) cuts in and out hard; the only way to fade it is to drive the layer's pad alpha from outside, which races with the mixer's own pad handling.

## Proposed direction

An optional `duration_ms` in `DskToggleRequest` (default 0: instant, as today). A non-zero duration animates the layer's compositor pad alpha with the same ease-in-out keyframes as transitions and fade to black, then neutralizes the control binding so later toggles stick. The field is optional, so existing clients and the WebSocket contract are unaffected; `openapi.json` gains the field.

A follow-up fix belongs to it: a layer switched on while it is still fading out must end up on (the fade-out's last keyframe must not switch it off again).

## Questions for the maintainers

1. A duration per request, or a per-layer default property (`dskN_fade_ms`) that the toggle uses?
2. Should the fade use the transition engine's clock (like takes) or the pad's control binding as proposed?

## Test case

A running mixer with one DSK layer fed a solid colour: toggle on with `duration_ms=500`; sampled program frames go from 0 to full alpha over about 500 ms; toggle off likewise. Toggle on with `duration_ms=0` is instant, as before. Toggle off with 500 ms and on again after 200 ms: the layer ends fully on.
