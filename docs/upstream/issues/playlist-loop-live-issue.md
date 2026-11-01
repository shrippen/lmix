Title: Media Player: switch playlist looping while it plays

## What is missing

`loop_playlist` is not a live property. An operator who decides per clip whether it loops or holds its last frame at the end has to change the property, which rebuilds the flow and restarts the clip.

## Proposed direction

`SetPlaylistRequest` takes an optional `loop_playlist` that updates both the stored property and the running player. A player that has already stopped at the end of a clip only starts over when it gets other files, so switching looping on for a clip that holds its end leaves it there. Optional field, so existing clients are unaffected; `openapi.json` updated.

## Questions for the maintainers

1. In the playlist request, or as a live block property (a property change without a rebuild)?
2. When looping is switched on for a clip that already holds its end, should it start over at once?

## Test case

A Media Player with a short clip, `loop_playlist` off: while it plays, send the playlist request with `loop_playlist: true`; at the end the clip starts over (position goes back to 0) without the flow restarting. Switch it off again: at the next end it holds the last frame.
