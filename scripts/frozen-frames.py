#!/usr/bin/env python3
"""Frames that repeat the one before in a recording of a moving picture
(scripts/measure-mixer-growth.sh): a stalled program repeats its last
frame, the moving ball never does.

    frozen-frames.py <media file>

Decodes the first video track to 160 × 90 grey. A frame whose largest
pixel change against the one before stays under 5 (of 255) is a repeat:
the encoder makes a near copy of it. The ball moves by more, at least 11
where it turns in a corner. The first and last second are left out
(encoder start and end).

Prints one line per run of repeats, `frozen <t> <frames>` (t in s from
the file's start), then `summary <runs> <frames> <seconds>`.
"""
import subprocess
import sys

import numpy as np

W, H = 160, 90
STILL = 5
SKIP_S = 1.0


def main():
    path = sys.argv[1]
    rate = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
         "stream=avg_frame_rate", "-of", "csv=p=0", path],
        check=True, stdout=subprocess.PIPE, text=True).stdout.strip()
    num, _, den = rate.partition("/")
    fps = float(num) / float(den or 1)
    raw = subprocess.run(
        ["ffmpeg", "-v", "fatal", "-i", path, "-map", "0:v:0", "-vf", f"scale={W}:{H}",
         "-pix_fmt", "gray", "-f", "rawvideo", "-"],
        check=True, stdout=subprocess.PIPE).stdout
    frames = np.frombuffer(raw, dtype=np.uint8).reshape(-1, H, W).astype(np.int16)
    change = np.abs(np.diff(frames, axis=0)).reshape(len(frames) - 1, -1).max(axis=1)
    skip = int(SKIP_S * fps)
    runs, total, start = 0, 0, None
    for i in range(skip, len(change) - skip):
        still = change[i] < STILL
        if still and start is None:
            start = i
        if not still and start is not None:
            print(f"frozen {(start + 1) / fps:.3f} {i - start}")
            runs += 1
            total += i - start
            start = None
    print(f"summary {runs} {total} {len(frames) / fps:.1f}")


if __name__ == "__main__":
    main()
