Title: Mixers: add and remove inputs, channels and aux buses while the flow runs

## What is missing

The Vision Mixer's inputs and the Audio Mixer's channels and aux buses are fixed when the flow is built. A production that brings sources in and out (guests joining a call, cameras added on site) has to restart the flow, which takes program off air, or pre-build every slot it might ever need and feed black or silence into the unused ones.

## Proposed direction

A capacity property per mixer (`max_inputs`, `max_channels`, `max_aux_buses`) above the built count; the free slots take inputs at runtime, everything else plays on. In steps, each its own PR:

1. **Vision Mixer PadMap** (groundwork, no behaviour change): every compositor sink pad index (inputs, DSKs, border underlays, stinger, multiview thumbnails, PGM, PVW, PiP tiles, overlay) comes from one `strom_types::vision_mixer::PadMap` instead of index arithmetic spread over builders and runtime operations. The map takes a pad capacity; with capacity == `num_inputs` it names exactly today's pads. `num_inputs` above the maximum fails the build instead of being clamped silently.
2. **Vision Mixer inputs at runtime** (CPU backend; `max_inputs` up to 128): `GET/POST/DELETE .../blocks/{block}/inputs`; an added input plays an inter channel inside the mixer into reserved pads of every compositor (program, multiview, PiP), without rebuilding the mixer, and a `VisionMixerInputsChanged` event announces it. The GL mixer names its pads in link order, so it cannot reserve pads; with free slots `auto` would have to fall back from OpenGL, which is only acceptable with shader FX off. With free slots a mixer can be built with one input. A benchmark (ignored test) measured the cost of empty reserved slots: with 8, 16 and 32 slots, the CPU load of slots built but unlinked, against every slot fed.
3. **Audio Mixer aux buses at runtime**, and the monitor following an aux bus (`monitor_source`).
4. **Audio Mixer channels at runtime**: `GET/POST .../blocks/{block}/channels`, `DELETE .../channels/{channel}`; a dynamic channel plays an inter channel through an `intersrc`, built from the same strip as a static one so every `chN_` property reaches it, and is rebuilt on restart. Every channel also gets a live delay (`chN_delay`). The bus capsfilters pin the sample format as well as the rate, so a channel added later can join a bus that started unfed.

Dynamic slots are stored in the flow's properties (`dynamic_channels`, dynamic aux buses), so a restart rebuilds them; copying a block drops them.

## Questions for the maintainers

1. Capacity property plus runtime endpoints, or should the block be rebuilt in place (a flow update) with a glitch-free handover instead?
2. Sources for runtime slots are inter channels only. Enough, or should a runtime input accept any block output?
3. GL: is falling back from OpenGL for a mixer with free slots acceptable, or should dynamic inputs wait for a way to reserve GL mixer pads?
4. Do you want the PadMap refactor on its own first (it is useful without dynamic inputs: one place for pad indices, and the GL mixer's link-order naming checked by a test)?

## Test case

- Vision Mixer with `num_inputs=2`, `max_inputs=4`, running: POST an input from an inter channel carrying a red pattern; a take to input 3 shows red on program while inputs 1-2 kept running without a gap (frame counter on program); DELETE it, program falls back without the flow stopping.
- Audio Mixer with `max_channels` above `num_channels`: POST a channel from an inter channel with a tone; the main bus level rises; DELETE it; the stored `chN_` properties are gone.
