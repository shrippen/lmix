#!/usr/bin/env python3
"""Lip sync of a recording made from `scripts/measure-av.sh`'s clip.

The clip flashes white for one frame and beeps at the same time, once a
second. Prints the sound onset minus the flash, in ms (median, min, max
over all flashes); positive means the sound comes after the picture.

    scripts/av-offset.py recording.mkv
"""
import subprocess
import sys

import numpy as np

RATE = 48000
# Analysis size of a frame: enough to tell white from black.
W, H = 64, 36
# A beep further than this from a flash belongs to another one.
PAIR_S = 0.5


def probe(path, stream, entry):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", stream,
         "-show_entries", entry, "-of", "csv=p=0", path],
        capture_output=True, text=True, check=True).stdout
    return [x.strip(",") for x in out.split()]


def flashes(path):
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-vf", f"scale={W}:{H},format=gray",
         "-f", "rawvideo", "-"], capture_output=True, check=True).stdout
    level = np.frombuffer(raw, np.uint8).reshape(-1, W * H).mean(axis=1)
    times = np.array([float(t) for t in probe(path, "v:0", "frame=best_effort_timestamp_time")])
    n = min(len(level), len(times))
    bright = level[:n] > 128
    return times[:n][bright & ~np.r_[False, bright[:-1]]]


def beeps(path):
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(RATE),
         "-f", "f32le", "-"], capture_output=True, check=True).stdout
    loud = np.abs(np.frombuffer(raw, np.float32)) > 0.2
    onsets = np.flatnonzero(loud & ~np.r_[False, loud[:-1]])
    if len(onsets) == 0:
        return onsets
    # One onset per beep: the sine crosses the threshold many times.
    onsets = onsets[np.r_[True, np.diff(onsets) > RATE // 10]]
    start = float(probe(path, "a:0", "stream=start_time")[0])
    return onsets / RATE + start


def main():
    path = sys.argv[1]
    seen, heard = flashes(path), beeps(path)
    offsets = [(heard[np.argmin(np.abs(heard - t))] - t) * 1000
               for t in seen if len(heard) and np.min(np.abs(heard - t)) < PAIR_S]
    if not offsets:
        sys.exit(f"{path}: no flash with a beep ({len(seen)} flashes, {len(heard)} beeps)")
    print(f"{np.median(offsets):.1f} {min(offsets):.1f} {max(offsets):.1f} {len(offsets)}")


if __name__ == "__main__":
    main()
