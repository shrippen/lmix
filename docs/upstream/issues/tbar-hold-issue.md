Title: Vision Mixer: hold a take at a T-bar position, with its transition

## What is missing

A T-bar can only be built from outside by setting the preview pad's alpha, which always gives a fade, whatever transition the operator chose. Slides, pushes and dips cannot be held part-way.

## Proposed direction

`POST /api/flows/{flow}/blocks/{block}/tbar` with `{position, transition_type}`: the pads rest as for a classic take and show the transition at that point: slide and push move the pads, dip to black goes through black at half, fade (and, without the FX engine's uniforms, wipes and master FX) blends. The take at the end is an ordinary transition request. A PiP take after a held fade or dip brings program fully on before the bar's preview goes off.

## Questions for the maintainers

1. Wipes and master FX fall back to a blend while held, because the FX engine's uniforms are driven by its own clock. Acceptable, or should the bar drive the uniforms too?
2. Should the position be streamed over the WebSocket instead of one POST per step for a hardware fader?

## Test case

- Unit tests on the frame for a bar position: a fade shows the new source at the bar's alpha, a slide brings it in from its side, a push moves both sources, a dip goes through black at half.
- Integration: two inputs of different colours into a running mixer, the bar held at a position per transition type; the program frame shows the expected bands of each source (sampled by column).
