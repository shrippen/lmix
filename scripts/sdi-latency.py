#!/usr/bin/env python3
"""Loop latency of a card output, from a multiview recording made by
`scripts/measure-sdi-latency.sh`: program flashes white once a second,
and the card input showing it back from the cable flashes later in its
own tile. Prints the delay of the later tiles behind the earlier ones in
ms (median, min, max over the flashing cells, count of cells).

    scripts/sdi-latency.py multiview.mkv
"""
import re
import subprocess
import sys

import numpy as np

# Analysis grid: each cell is one value per frame.
W, H = 64, 36
# A cell flashes when it goes from below to above this (black is 16).
BRIGHT = 128
# Cells need this many flashes to count.
MIN_FLASHES = 3
# Two tiles' delays differ by more than this: one frame at 25 fps.
TILE_GAP_S = 0.035


def frames(path):
    """Grey levels of every frame and their times (showinfo)."""
    run = subprocess.run(
        ["ffmpeg", "-v", "info", "-i", path, "-map", "0:v:0", "-fps_mode", "passthrough",
         "-vf", f"scale={W}:{H},format=gray,showinfo", "-f", "rawvideo", "-"],
        capture_output=True, check=True)
    levels = np.frombuffer(run.stdout, np.uint8).reshape(-1, W * H).astype(np.int16)
    times = np.array([float(t) for t in re.findall(rb"pts_time:\s*(-?[\d.]+)", run.stderr)])
    if len(levels) != len(times):
        sys.exit(f"{path}: {len(levels)} frames but {len(times)} timestamps")
    return levels, times


def onsets(column, times):
    bright = column > BRIGHT
    return times[bright & ~np.r_[False, bright[:-1]]]


def main():
    levels, times = frames(sys.argv[1])
    cells = [o for o in (onsets(levels[:, c], times) for c in range(W * H)) if len(o) >= MIN_FLASHES]
    if len(cells) < 2:
        sys.exit("fewer than two flashing cells")

    # Each cell's delay behind the busiest one, wrapped into ±0.5 s (one
    # flash a second).
    reference = max(cells, key=len)
    delays = []
    for cell in cells:
        nearest = reference[np.argmin(np.abs(reference[:, None] - cell[None, :]), axis=0)]
        wrapped = (cell - nearest + 0.5) % 1.0 - 0.5
        delays.append(np.median(wrapped))
    delays = np.sort(np.array(delays))

    # Two groups, split at the widest gap: program's tiles, the card's.
    gaps = np.diff(delays)
    split = int(np.argmax(gaps)) + 1
    if gaps[split - 1] < TILE_GAP_S:
        sys.exit(f"no tile flashes later (delays {delays.min():.3f}-{delays.max():.3f} s)")
    early, late = delays[:split], delays[split:]
    behind = (late - np.median(early)) * 1000
    print(f"{np.median(behind):.0f} {behind.min():.0f} {behind.max():.0f} {len(late)}")


if __name__ == "__main__":
    main()
