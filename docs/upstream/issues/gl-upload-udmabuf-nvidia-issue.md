Title: GL Vision Mixer composites black on NVIDIA when glupload takes its udmabuf path

## What is broken

On GStreamer 1.28, `glupload` proposes a udmabuf allocator to its upstream and imports the frames as DMABuf into EGL. With the proprietary NVIDIA driver (GL vendor "NVIDIA Corporation") those imports come out as empty textures. Wherever a system-memory producer feeds `glupload` directly, the picture is black:
- a Vision Mixer input whose source links straight into the mixer's input queue;
- the layout underlays, whose capsfilter feeds `glupload` without a queue (the borders around boxes disappear);
- an adapter that `gl_input_front` splices in front of `glupload` (`videoconvert`, `cudadownload`);
- the Vision Mixer's stinger input when its `videoconvert` converts the clip's format (`videoconvert` then allocates its output from the udmabuf proposal);
- the Compositor block with the OpenGL backend (`queue ! glupload`, or its input straight into `glupload`).

It is not specific to Strom. Reproduction without Strom (RTX 3070 Ti, driver 615.78.08, GStreamer 1.28.8):

```
gst-launch-1.0 videotestsrc pattern=white num-buffers=1 ! video/x-raw,format=RGBA,width=64,height=64 \
  ! glupload ! gldownload ! fakesink dump=true
```

gives RGBA 0,0,0,0 for every pixel, and the glupload debug log shows the udmabuf method. With `tee ! queue !` between `videotestsrc` and `glupload` (the allocation proposal no longer reaches the source) the frame is white; a plain `queue` does not help, it forwards the query. Other formats come out wrong rather than empty: a white UYVP frame through `tee ! queue ! videoconvert ! glupload ! glcolorconvert ! gldownload` reads back as RGBA `00 88 00 ff`.

**Condition:** `glupload` offers the udmabuf allocator only when the process can open `/dev/udmabuf`. On a desktop that is typically granted to the logged-in user through a logind ACL (`uaccess`); a system service without a session, or a Docker container without `/dev/udmabuf` mapped, never gets the udmabuf method. That is probably why Strom's Docker deployments on NVIDIA nodes do not show this: it hits desktop and native GUI setups. Whether the GitHub runners can open `/dev/udmabuf` is unverified, and so is whether the import fails there at all (they use Mesa, not the NVIDIA driver).

Effect in Strom's tests on such a host (`STROM_REQUIRE_GL=1`): `vision_mixer_source_resize_test` 8 of 8 fail, `vision_mixer_underlay_upload_test` both GPU tests, `vision_mixer_overlay_upload_test` `tally_follows_a_cut_on_the_same_frame_with_late_inputs`, `vision_mixer_fx_test` `wipe_between_letterboxed_sources_animates`, `vision_mixer_fx_passthrough_test` `idle_fx_slots_pass_buffers_through_and_effects_still_apply`. All pass with the workaround below.

## Proposed direction

A branch implements this (stacked on #1199, which the probe needs on 1.28); I would open it as a PR referencing this issue.

1. **A functional probe at startup, not a vendor check:** next to the existing GL renderer probe, run `videotestsrc pattern=white num-buffers=1 ! video/x-raw,format=RGBA,width=64,height=64 ! glupload ! gldownload ! appsink` once and check that the frame comes back white. It detects the bug itself, independent of driver version, Mesa vs. proprietary, `/dev/udmabuf` access and containers, and switches the workaround off once NVIDIA or GStreamer fix the import. Keying it on vendor plus driver version (as `demote_qsv_encoders`, #1163) would hit hosts without `/dev/udmabuf` access, which work, and would need a new driver range for every release. Only RGBA is probed; a driver that breaks only YUV imports would pass.
2. **Where the probe fails:** in front of each `glupload` fed from system memory (Vision Mixer input queue, underlay capsfilter, a spliced adapter, the stinger's `videoconvert`, the Compositor block's `glupload`), a QUERY_DOWNSTREAM probe drops the ALLOCATION query, limited to raw video in system memory, so upstream keeps its own memory and `glupload` does a plain texture upload. Per allocation query, never per buffer. GL, CUDA and DMABuf producers keep their own allocation.
3. **Cost on an affected host:** the lost zero-copy upload (one copy per frame for system-memory sources). The dropped query also takes `glupload`'s `VideoMeta` and overlay-composition meta proposals with it: a decoder with padded strides then copies, and `textoverlay` and similar blend in software.
4. Report the driver behaviour to GStreamer (gitlab.freedesktop.org) with the one-liner above and link it, so it is clear when the workaround can go.

## Question for the maintainers

Should the workaround be on by default where the probe fails, or behind a setting?

## Test case

The integration tests above, run on an affected host with `STROM_REQUIRE_GL=1`, fail without the workaround and pass with it. In CI (Mesa, surfaceless) the probe should report working, so the workaround stays off there; for CI the branch adds a test that hides the ALLOCATION query for system memory and lets it pass for GL memory, and a test that a converted frame stays white with the query hidden.
