#!/usr/bin/env python3
"""Lip sync of a recording made from `scripts/measure-av.sh`'s clip.

The clip flashes white for one frame and beeps at the same time, once a
second. Prints the sound onset minus the flash, in ms (median, min, max
over all flashes); positive means the sound comes after the picture.

    scripts/av-offset.py recording.mkv
"""
import re
import subprocess
import sys

import numpy as np

RATE = 48000
# Analysis size of a frame: enough to tell white from black.
W, H = 64, 36
# A beep further than this from a flash belongs to another one.
PAIR_S = 0.5


def flashes(path):
    """Start times of the white frames, from one decoding pass: frame
    timestamps come from showinfo, so a gap in the picture keeps them."""
    run = subprocess.run(
        ["ffmpeg", "-v", "info", "-i", path, "-map", "0:v:0", "-fps_mode", "passthrough",
         "-vf", f"scale={W}:{H},format=gray,showinfo", "-f", "rawvideo", "-"],
        capture_output=True, check=True)
    level = np.frombuffer(run.stdout, np.uint8).reshape(-1, W * H).mean(axis=1)
    times = np.array([float(t) for t in re.findall(rb"pts_time:\s*(-?[\d.]+)", run.stderr)])
    if len(level) != len(times):
        sys.exit(f"{path}: {len(level)} frames but {len(times)} timestamps")
    bright = level > 128
    return times[bright & ~np.r_[False, bright[:-1]]]


def beeps(path):
    """Start times of the beeps. aresample fills every gap in the sound
    (over 1 ms) with silence from time 0, so sample index / rate is the
    time; by default it would close small gaps only slowly."""
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-map", "0:a:0", "-ac", "1",
         "-af", f"aresample={RATE}:async=1:min_hard_comp=0.001:first_pts=0", "-f", "f32le", "-"],
        capture_output=True, check=True).stdout
    loud = np.abs(np.frombuffer(raw, np.float32)) > 0.2
    onsets = np.flatnonzero(loud & ~np.r_[False, loud[:-1]])
    if len(onsets) == 0:
        return onsets / RATE
    # One onset per beep: the sine crosses the threshold many times.
    onsets = onsets[np.r_[True, np.diff(onsets) > RATE // 10]]
    return onsets / RATE


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
