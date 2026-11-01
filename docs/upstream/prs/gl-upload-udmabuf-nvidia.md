Title: fix(vision mixer): keep GL inputs off glupload's udmabuf path where it loses the picture

Closes #<ISSUE>

## Problem

On GStreamer 1.28, `glupload` proposes a udmabuf allocator to its upstream and imports the frames as DMABuf into EGL. It does so only when the process can open `/dev/udmabuf`: on a desktop typically through a logind ACL (`uaccess`) for the logged-in user; a system service without a session, or a Docker container without `/dev/udmabuf` mapped, never gets the udmabuf method. With NVIDIA's proprietary driver those imports come out as empty textures. Wherever a system-memory producer feeds `glupload` directly, the GL Vision Mixer composites black:
- an input whose source links straight into the mixer (with a `tee` in between, the proposal does not reach the source and it works);
- the layout underlays, whose capsfilter feeds `glupload` without a queue (the borders around boxes disappear);
- an adapter that `gl_input_front` splices in front of `glupload` (`videoconvert` or `cudadownload`);
- the stinger input when its `videoconvert` converts the clip's format (it allocates from the proposal; a white UYVP frame reads back as `00 88 00 ff`);
- the Compositor block with the OpenGL backend (`queue ! glupload`, or its input straight into `glupload`).

Plain `gst-launch-1.0` shows the same (RTX 3070 Ti, driver 615.78.08, GStreamer 1.28.8):

```
gst-launch-1.0 videotestsrc pattern=white num-buffers=1 ! video/x-raw,format=RGBA,width=64,height=64 \
  ! glupload ! gldownload ! video/x-raw,format=RGBA ! filesink location=out.raw
```

`out.raw` is all zeros; with `tee ! queue !` before `glupload` it is white.

## Change

- `gpu::udmabuf_upload_broken()`, decided once per process in `detect_gpu_capabilities()` (later calls keep the result) by a functional probe (`gpu::upload_keeps_pixels`): one white 64x64 RGBA frame through `glupload ! gldownload ! appsink`, bounded by 5 s and ended early by a bus error; broken when it does not come back white. Logged with the GL vendor and version; no answer (error, timeout) is logged as a warning and leaves the workaround off. No vendor string check: the probe follows the driver version, `/dev/udmabuf` access and containers, and switches the workaround off by itself once the import works.
- `gl_input_front::keep_own_memory(pad)`: where the probe found the path broken, it installs `hide_system_memory_allocation(pad)`, a QUERY_DOWNSTREAM probe that drops ALLOCATION queries at `pad` whose caps are raw video in system memory, so everything upstream keeps its own memory and `glupload` does a plain texture upload. GL, CUDA and DMABuf producers keep their allocation. It fires per allocation query, never per buffer.
- Applied wherever system memory feeds `glupload`: each Vision Mixer input queue's src pad (`install_input_front`), the underlay capsfilter, a spliced adapter's src pad, the stinger's `videoconvert` src pad, and the Compositor block's `glupload` sink pad.
- `tests/common::init_gl` now runs `gpu::detect_gpu_capabilities()`, as the server does at start. Without it the GL integration tests never ran the probe, kept the workaround off, and composited black on an affected host.

Rejected: a vendor check (`GL_VENDOR` contains "NVIDIA"): it hits every driver version, also without `/dev/udmabuf`, and would stay on after a fix. Always dropping the allocation query: it takes the zero-copy path from drivers where it works. A `tee`/queue in front of every input: a plain queue forwards the query, and the underlay, stinger and spliced adapters would still need the workaround.

**Depends on #1199** (GL renderer probe on GStreamer 1.28). This branch is stacked on it: the upload probe runs where the renderer probe found GL, which on 1.28 needs #1199. Please review the last commit only.

## Evidence

On an affected host (RTX 3070 Ti, driver 615.78.08, GStreamer 1.28.8, Wayland session with `/dev/udmabuf` accessible), `STROM_REQUIRE_GL=1`, `--test-threads=1`, this branch against the same branch with the probe result forced to "working":

| Test file | With the change | Probe forced to "working" |
|---|---|---|
| `vision_mixer_source_resize_test` | 8 passed | 8 of 8 fail (program black) |
| `vision_mixer_underlay_upload_test` | 3 passed | 2 GPU tests fail (no borders) |
| `vision_mixer_overlay_upload_test` | 3 passed | `tally_follows_a_cut_on_the_same_frame_with_late_inputs` fails |
| `vision_mixer_fx_test` | 4 passed | `wipe_between_letterboxed_sources_animates` fails |
| `vision_mixer_fx_passthrough_test` | 1 passed, 1 ignored | `idle_fx_slots_pass_buffers_through_and_effects_still_apply` fails |

Guard for CI: `gl_input_front::tests::hides_the_allocation_query_for_system_memory_only` (no GL needed) installs the probe as on an affected host and checks that an ALLOCATION query for system-memory RGBA (also with overlay-composition meta) does not reach the peer while one for `memory:GLMemory` and one for audio do. It fails when the probe or its caps filter is removed; it does not check that each of the call sites installs the probe.

Stinger and Compositor: `gl_probe_test::a_converted_frame_stays_white_with_the_allocation_query_hidden` sends a white UYVP frame through `tee ! queue ! videoconvert ! glupload ! glcolorconvert ! gldownload` with the query hidden at `videoconvert`'s src pad (as at the stinger) and at `glupload`'s sink pad (as in the Compositor block): white both ways. Without hiding it, this host gives `[0, 136, 0, 255]`. On a host whose upload works (CI, presumably) it only shows that hiding the query does no harm: a demonstration there, not a guard.

The probe itself: `gl_probe_test::the_upload_probe_reads_a_working_upload_as_working` checks that a frame through `tee ! queue ! glupload ! gldownload` comes back white, so the probe cannot call a working host broken.

## Tests

Ran on Linux (Fedora 44, GStreamer 1.28.8, RTX 3070 Ti), on current `main` (4479d8c) plus #1199:
- `gl_probe_test` (3 passed) and the five test files above with the probe; the forced-"working" column is from the same change before the stinger and Compositor additions;
- `stinger_test`, `stinger_software_decode_test`, `stinger_prores_test`, `compositor_cpu_input_memory_test` (they check no pixels, so they show only that nothing breaks);
- `cargo test --lib gl_input_front`;
- `cargo fmt --all --check`: clean;
- `cargo clippy --all-targets`: no warnings in the touched files (three pre-existing warnings from a newer clippy in `vision_mixer/overlay.rs` and `shaders/passthrough.rs`).

Not run: Mesa on AMD/Intel hardware with `/dev/udmabuf` (the probe should report working there and leave the zero-copy path alone; unverified), and a Docker container on NVIDIA.

## For the reviewer

- One more tiny GL pipeline at startup (one 64x64 frame), once per process and only where the renderer probe found GL.
- On an affected host the dropped query also drops `glupload`'s `VideoMeta` and overlay-composition meta proposals: a decoder with padded strides then copies, and `textoverlay` and similar blend in software. Unaffected hosts keep everything.
- `init_gl` now runs GPU detection in every GL integration test, which can change behaviour of other GL tests on affected hosts (it makes them match the server).
- The driver bug itself is not reported to GStreamer yet; the one-liner above is the reproduction.

## Out of scope

- The report to GStreamer/NVIDIA (see the issue).
- YUV-only import bugs: the probe checks RGBA only.
- Unverified platforms: Mesa (AMD/Intel) with `/dev/udmabuf` access, Docker on NVIDIA with `/dev/udmabuf` mapped, macOS and Windows (no udmabuf there; the probe should report working). Whether the GitHub runners can open `/dev/udmabuf` is unverified.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
