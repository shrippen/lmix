#!/usr/bin/env bash
# Stage (show mode S1 step 3, VIS): picture cues as their own pipeline
# each (a, like an lmix input, through intervideo) or as branches of one
# stage pipeline (b, a compositor pad per cue). Pure GStreamer on the CPU
# (compositor), no strom, no display (fakesink, clocked at 30 fps).
#
#   scripts/bench-stage/run.sh   -> a JSON line per mode and count
set -euo pipefail
cd "$(dirname "$0")"
MEDIA="$PWD/media"
PY="$(command -v python3.12 || command -v python3)"
if [[ ! -f "$MEDIA/v8.mp4" ]]; then
    mkdir -p "$MEDIA"
    # Eight 1080p30 H.264 clips of 20 s at 8 Mbit/s (openh264, BSD), one
    # pattern each,
    # and a still.
    patterns=(smpte ball colors circular gamut spokes pinwheel smpte75)
    for i in $(seq 1 8); do
        gst-launch-1.0 -q videotestsrc pattern="${patterns[$((i - 1))]}" num-buffers=600 ! \
            video/x-raw,width=1920,height=1080,framerate=30/1 ! videoconvert ! \
            openh264enc rate-control=bitrate bitrate=8000000 max-bitrate=12000000 ! h264parse ! mp4mux ! filesink location="$MEDIA/v$i.mp4"
    done
    gst-launch-1.0 -q videotestsrc pattern=smpte num-buffers=1 ! \
        video/x-raw,width=1920,height=1080 ! pngenc ! filesink location="$MEDIA/still.png"
fi
for mode in a b p; do
    for n in 1 4 8; do timeout 90 "$PY" bench.py "$mode" "$n"; done
done
