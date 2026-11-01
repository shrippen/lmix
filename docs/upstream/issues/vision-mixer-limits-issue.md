Title: Vision Mixer: raise the input limit to 32 and the DSK layer limit to 9

## What is missing

`MAX_NUM_INPUTS` is 16 and `MAX_DSK_INPUTS` is 4. Both are soft limits: the layout scales, and the hardware is the real ceiling.

- Inputs: with a CUDA compositor (see the CUDA backend issue) an RTX 3090 holds 24 x 1080p30 NVDEC inputs plus program, multiview and 2 x NVENC at 30 fps on every input and output, at 25 % backend CPU. 32 x 1080p30 exceeds NVDEC on that card (inputs at 27.5 fps), so 32 is a sensible cap.
- DSK layers: a production with eight overlay channels plus a layer for stinger clips (a clip with alpha over program while it cuts underneath) needs nine.

## Proposed direction

Raise `MAX_NUM_INPUTS` to 32 and `MAX_DSK_INPUTS` to 9 in `strom-types`, and the enum choices of `num_dsk_inputs` with it. The layers stay plain DSK inputs; nothing else changes. Inputs above the maximum should fail the build with the reason instead of being clamped silently (today a flow with 17 inputs builds 16, and its link to the 17th pad has no target).

## Questions for the maintainers

1. Is 32 / 9 acceptable, or would you rather keep the caps and make them configurable?
2. The multiview layout at 32 inputs gets small tiles; is that fine, or should the multiview cap stay at 16 while program takes 32?

## Test case

A mixer with `num_inputs=32` and `num_dsk_inputs=9` builds on the CPU backend, every input pad and DSK pad links, and a take to input 32 shows it on program. A mixer with `num_inputs=33` fails to build with a message naming the maximum.
