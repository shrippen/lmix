Title: Vision Mixer: accept VA memory (AMD/Intel decoders) through vapostproc, on the GL and the CPU backend

## What is missing

`video_adapt` refuses `memory:VAMemory` (`UnsupportedMemory`) for the GL mixer and for system-memory consumers alike: `download_front` downloads GL, CUDA, D3D12 and D3D11 memory, not VA (see `system_memory_other_gpu_memory_is_refused`). A source decoded by VA (AMD or Intel hardware decoders), for example a Media Player re-pushing VA surfaces from its appsrc, therefore can feed neither the GL nor the CPU Vision Mixer; the user has to force software decoding.

## Proposed direction

Let `video_adapt::decide` pick `vapostproc` for VA memory in front of `glupload` (or refuse naming the missing factory), and let `gl_input_front` splice whichever GPU adapter the memory calls for, the same way it splices `cudadownload` for CUDA. `vapostproc` hands the frames on as DMABuf, which `glupload` imports.

For the CPU mixer (and any system-memory consumer), let `download_front` do the same for VA memory: `vapostproc` to system memory in front of the converter, as it already does with `cudadownload` for CUDA and `gldownload` for GL. The CAPS query answer is not widened with VA memory: VA decoders offer DMABuf and system memory themselves.

## Questions for the maintainers

1. This is unverified on AMD/Intel hardware here. Is a change acceptable that is tested only with a `vapostproc` stand-in, or would you want someone with VA hardware to confirm first?
2. One PR for both backends, or the CPU download first (it is the smaller change and helps every system-memory consumer, not only the mixer)?
3. Same for thumbnail taps on VA memory (`vapostproc` scaling, then the system-memory path): include, or separate?

## Test case

`vision_mixer_va_input_test`: the real Media Player into the real GL mixer, with a stand-in element registered as `vapostproc` and caps tagged `memory:VAMemory`; the input negotiates and program shows it. It fails without the change (not-negotiated). It covers the GL mixer only; a test for the CPU mixer (the same stand-in in front of `download_front`, program shows the input) still has to be written.
