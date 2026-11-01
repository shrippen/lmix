Title: Vision Mixer: a CUDA compositor backend that keeps NVDEC frames in CUDA memory

## What is missing

On NVIDIA hosts the decoders (NVDEC) and encoders (NVENC) work in CUDA memory, but the Vision Mixer composites either in OpenGL or on the CPU. Every NVDEC input therefore crosses into GL (or is downloaded), and program crosses back for NVENC. With many inputs that crossing dominates the cost.

Measured with 16 x 1080p30 H.264 file inputs on an RTX 3090 (4x4 grid, 1080p30 out):

| Backend | Backend CPU | Output |
|---|---|---|
| `cudacompositor` (CUDA memory end to end) | 6 % | 30 fps |
| NVDEC -> GL -> `glvideomixer` | 108 % | 25 fps |
| CPU `compositor` | 240 % | 30 fps |

24 x 1080p30 NVDEC inputs plus program, multiview and 2 x NVENC held 30 fps on every input and output at 25 % backend CPU.

## Proposed direction

A fourth value `cuda` for `compositor_preference`, building the mixer on `cudacompositor`. It can reuse the CPU builder's topology (crop through `videocrop` with crop meta, `cudaupload` in place of `videoconvert`, passthrough for CUDA input). Pieces that make it work in practice, each a separate PR:

1. The backend itself (`cudacompositor`, CUDA caps, `cudaupload` in front of each pad).
2. One CUDA context shared across all pipelines: each flow and each Media Player's internal pipeline otherwise creates its own. With 16 NVDEC inputs into a CUDA mixer: 9.6 GB VRAM / 4.1 GB RSS with one context per pipeline, 4.6 GB / 1.0 GB with a shared one, backend CPU 69 % -> 17 %.
3. Alpha formats in front of the CUDA mixer: `cudaupload` takes neither `A444_10LE` (ProRes 4444) nor `A420` (VP9 with alpha), so stingers with alpha need a `videoconvert` first.
4. Thumbnail taps on CUDA memory: scale with `cudaconvertscale` and download only the small frame; with `nvjpegenc`, encode the JPEG on the GPU (18 % vs 39 % backend CPU with 16 inputs at 10 thumbnails/s; noise at the default 1/s).
5. Interlaced NVDEC frames: no CUDA deinterlacer exists, so RGBA in CUDA, `cudadownload` into GL and `gldeinterlace` (one 1080i SRT input: 50 % -> 23 % backend CPU; keeps the frame rate).
6. Fixes found on real hardware: alpha after a fade, stingers, full-range thumbnails.

## Questions for the maintainers

1. Do you want a CUDA backend in the Vision Mixer at all, given that CI has no NVIDIA runner? The tests that need CUDA are `#[ignore]`d and run by hand.
2. Should `auto` ever pick CUDA, or only an explicit `cuda`?
3. The shared CUDA context touches every pipeline (set at startup, answered on NEED_CONTEXT). Acceptable as a global, or per flow?
4. One issue with a series of PRs in the order above, or separate issues per piece?

## Test case

16 NVDEC file inputs into a 4x4 Vision Mixer with `compositor_preference=cuda`, program into NVENC: all inputs and outputs at 30 fps, program shows every input, backend CPU in the single digits. The same flow on `gpu` (OpenGL) as the comparison.

We run this on a branch and can split it into PRs on top of current `main` once the direction is agreed.
