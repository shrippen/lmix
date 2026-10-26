#!/usr/bin/env python3
"""Gaps and clicks in a recorded steady sine (scripts/measure-switch-gaps.sh).

    switch-gaps.py <media file> <freq Hz> [<switch s> ...]

Decodes the first audio track to mono 48 kHz. A gap is a stretch of 2 ms or
more far below the tone's level. A click is a block of 5 ms (each block
overlaps the next by half) where the tone does not go on as one sine: the
block is fitted with a sine of the tone's frequency (amplitude and phase
free), and the residual is compared with the tone's level. AAC at 128 kb/s
alone leaves about -70 dB; a phase jump of 0.3 rad -16 dB, a gap more.

Prints one line per event, `gap <t> <ms>` or `click <t> <residual dB>`
(t in s from the file's start), with the switch time nearest before it when
switch times are given, then `summary <gaps> <clicks> <seconds>`.
"""
import subprocess
import sys

import numpy as np

RATE = 48000
BLOCK = RATE // 200          # 5 ms
GAP_DB = -30                 # below the tone's level
CLICK_DB = -18               # residual against the tone's level
SKIP_S = 1.0                 # the encoder's start and end


def decode(path):
    raw = subprocess.run(
        ["ffmpeg", "-v", "fatal", "-i", path, "-map", "0:a:0", "-ac", "1", "-ar", str(RATE),
         "-f", "f32le", "-"],
        check=True, stdout=subprocess.PIPE).stdout
    return np.frombuffer(raw, dtype=np.float32).astype(np.float64)


def main():
    path, freq = sys.argv[1], float(sys.argv[2])
    switches = sorted(float(s) for s in sys.argv[3:])
    x = decode(path)
    skip = int(SKIP_S * RATE)
    x = x[skip:len(x) - skip]
    if len(x) < RATE:
        sys.exit(f"{path}: less than a second of sound")
    level = np.sqrt(np.mean(x ** 2))
    if level < 1e-4:
        sys.exit(f"{path}: no tone")

    def at(i):
        return (i + skip) / RATE

    def before(t):
        earlier = [s for s in switches if s <= t + 0.05]
        return f" after switch at {earlier[-1]:.2f}" if earlier else ""

    events = []
    # Gaps: 1 ms windows far below the tone's level, joined.
    win = RATE // 1000
    n = len(x) // win
    rms = np.sqrt(np.mean(x[:n * win].reshape(n, win) ** 2, axis=1))
    quiet = rms < level * 10 ** (GAP_DB / 20)
    i = 0
    while i < n:
        if quiet[i]:
            j = i
            while j < n and quiet[j]:
                j += 1
            if j - i >= 2:
                events.append((at(i * win), f"gap {at(i * win):.3f} {j - i} ms"))
            i = j
        else:
            i += 1
    # Clicks: per block, the residual of the best sine at the tone's frequency.
    t = np.arange(BLOCK) / RATE
    basis = np.stack([np.sin(2 * np.pi * freq * t), np.cos(2 * np.pi * freq * t)], axis=1)
    pinv = np.linalg.pinv(basis)
    # Blocks overlap by half: a jump on one block's edge is inside the next.
    hop = BLOCK // 2
    blocks = np.lib.stride_tricks.sliding_window_view(x, BLOCK)[::hop]
    fit = blocks @ pinv.T @ basis.T
    resid = np.sqrt(np.mean((blocks - fit) ** 2, axis=1)) / level
    loud = resid > 10 ** (CLICK_DB / 20)
    for k in np.flatnonzero(loud):
        if k > 0 and loud[k - 1]:
            continue
        events.append((at(k * hop), f"click {at(k * hop):.3f} {20 * np.log10(resid[k]):.1f} dB"))
    events.sort()
    for when, line in events:
        print(line + before(when))
    gaps = sum(1 for _, line in events if line.startswith("gap"))
    print(f"summary {gaps} {len(events) - gaps} {len(x) / RATE:.1f}"
          f" (residual median {20 * np.log10(np.median(resid)):.1f} dB)")


if __name__ == "__main__":
    main()
