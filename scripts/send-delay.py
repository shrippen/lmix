#!/usr/bin/env python3
"""Reads a stream's sound as it arrives (mono s16le at 8 kHz on stdin, from
ffmpeg) and prints the wall-clock time (Unix seconds) of the first tone,
`tone <time>`, and of every switch from tone to silence, `silence <time>`. scripts/measure-send-delay.sh compares it
with the time the tone was muted. Needs nothing beyond Python 3."""
import array
import math
import sys
import time

RATE = 8000
CHUNK = RATE // 100  # 10 ms
QUIET = 300  # RMS below: silence (the tone is at full scale)

# None until the first tone: the silence before it marks nothing.
loud = None
read = sys.stdin.buffer.read
while True:
    data = read(CHUNK * 2)
    if len(data) < CHUNK * 2:
        break
    now = time.time()
    samples = array.array("h", data)
    rms = math.sqrt(sum(s * s for s in samples) / len(samples))
    is_loud = rms >= QUIET
    if is_loud and loud is None:
        print(f"tone {now:.3f}", flush=True)
    elif loud and not is_loud:
        print(f"silence {now:.3f}", flush=True)
    if loud is not None or is_loud:
        loud = is_loud
