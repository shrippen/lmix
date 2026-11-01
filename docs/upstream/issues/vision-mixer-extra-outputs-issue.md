Title: Vision Mixer: a full-size preview output and a tile output

## What is missing

The mixer has a program and a multiview output.
- A client that wants the preview picture has to cut it out of the multiview (half size, with labels and tallies) or show the raw input, which lacks the PiPs set up on preview.
- A control surface that shows every input live needs one stream per input, each scaled on the CPU somewhere else.

## Proposed direction

Two optional outputs, off by default:

1. **Preview output** (`enable_preview_out` -> `pvw_out`): a third compositor at program size with one branch per input from the input's tee. It mirrors the multiview's PVW pads: every change of position, size, alpha, z-order or crop on a PVW pad lands on its twin, scaled from `pvw_rect` to the full frame (the CPU backend crops with a `videocrop` upstream of the pad, which is mirrored too). Inputs and PiPs on preview, takes and fades then show without any operation knowing about the output. Zone border underlays are mirrored as well.
2. **Tile output** (`enable_tiles_out` -> `tiles_out`): every input scaled on the mixer's own backend into its cell of one grid (`tiles_cell`, `tiles_columns`, `tiles_framerate`, `tiles_format`): input i in cell i, row by row, aspect fitted; empty slots black. One more tee branch per input and one compositor; the cells never move, so no operation has to follow it.

The queues feeding both are leaky like the multiview's (see #1188), so neither can hold up program.

## Questions for the maintainers

1. Mirroring the PVW pads couples the preview output to the multiview's pad handling; would you rather have the take/transition code drive a third compositor explicitly?
2. Are block properties the right switch (they change the block's outputs), or should these be separate blocks fed from the mixer?
3. Is `tiles_format` (fixed pixel format for viewers that draw RGBA) worth carrying?

## Test case

- Preview output: two PiPs with yellow and white borders, take to each in turn; after the second take the preview output shows the white border (pixel check), and after a take of input 2 the preview output shows input 2 full frame.
- Tile output: four test-pattern inputs with distinct colours, `tiles_columns=2`: each cell of the output frame has its input's colour; with three inputs the fourth cell is black.
