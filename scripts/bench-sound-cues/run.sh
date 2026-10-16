#!/usr/bin/env bash
# S-E.3: sound cues as their own pipeline each (a, like an lmix input,
# through interaudio) or as branches of one player per zone (b).
# Pure GStreamer, no strom, no GPU, no sound device (fakesink, clocked).
#
#   scripts/bench-sound-cues/run.sh   -> a JSON line per mode and count,
#                                        then request-to-audible latency
set -euo pipefail
cd "$(dirname "$0")"
MEDIA="$PWD/media"
if [[ ! -f "$MEDIA/s10.flac" ]]; then
    mkdir -p "$MEDIA"
    # Ten 30 s stereo sines, one pitch each.
    for i in $(seq 1 10); do
        gst-launch-1.0 -q audiotestsrc wave=sine freq=$((200 + i * 40)) num-buffers=1300 \
            samplesperbuffer=1024 ! audio/x-raw,rate=44100,channels=2 ! audioconvert ! \
            flacenc ! flacparse ! filesink location="$MEDIA/s$i.flac"
    done
fi
for mode in a b; do
    for n in 1 5 10; do timeout 60 python3 bench.py "$mode" "$n"; done
done
for mode in a b; do timeout 120 python3 latency.py "$mode"; done
