#!/usr/bin/env python3
"""Frames of lmix and of program around a fader drag (measure-gui-frames.sh).

    gui-frames.py WORK

WORK holds lmix.log (Qt's render loop timing next to lmix's own log),
phases (`<name> <unix time>` lines: idle1, fader, idle2, end), threads
(CPU of lmix's threads per phase) and rec/ (a recording of program).
Prints percentiles per phase and exits 1 when a budget is exceeded:

    GAP_P95_MS   frame gap of the window while the fader moves (default 50)
    GAP_MAX_MS   longest frame gap while the fader moves (default 250)
    GUI_CPU_MAX  CPU of the GUI thread while the fader moves, % (default 35)
    RECOVER      frames per second after the drag, share of before (default 0.8)
    program: no picture the same as the one before (the ball moves every frame)
"""
import datetime
import glob
import os
import re
import subprocess
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")
QT = re.compile(r"^\s*([\d.]+) qt\.scenegraph\.time\.renderloop .*syncAndRender: frame rendered")
QT_ANY = re.compile(r"^\s*([\d.]+) ")
RUST = re.compile(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+)Z ")


def env(name, default):
    return float(os.environ.get(name, default))


def percentile(values, q):
    values = sorted(values)
    return values[min(len(values) - 1, int(len(values) * q))] if values else 0.0


def gui_frames(log):
    """Unix times of the frames lmix drew. Qt stamps its lines with time
    since the process started; lmix's own lines carry UTC. A Qt line just
    before a lmix line gives the offset."""
    offset, last_qt, frames = None, None, []
    with open(log, errors="replace") as f:
        for raw in f:
            line = ANSI.sub("", raw)
            m = RUST.match(line)
            if m and last_qt is not None and offset is None:
                utc = datetime.datetime.fromisoformat(m.group(1)).replace(tzinfo=datetime.timezone.utc)
                offset = utc.timestamp() - last_qt
                continue
            m = QT_ANY.match(line)
            if m:
                last_qt = float(m.group(1))
            m = QT.match(line)
            if m:
                frames.append(float(m.group(1)))
    if offset is None:
        sys.exit("no lmix log line to place Qt's times (RUST_LOG must log lmix at debug)")
    return [offset + t for t in frames]


def program_frames(work, start):
    """(unix time, picture hash) of the recording's frames, from the time
    the output started."""
    files = sorted(glob.glob(f"{work}/rec/*"))
    if not files:
        return []
    out = subprocess.run(["ffmpeg", "-v", "error", "-i", files[0], "-map", "0:v", "-f", "framemd5", "-"],
                         capture_output=True, text=True, check=True).stdout
    tb = re.search(r"#tb 0: (\d+)/(\d+)", out)
    scale = int(tb.group(1)) / int(tb.group(2)) if tb else 0.001
    rows = [line.split(",") for line in out.splitlines() if line and not line.startswith("#")]
    if not rows:
        return []
    first = int(rows[0][2]) * scale
    return [(start + int(r[2]) * scale - first, r[5].strip()) for r in rows]


def main():
    work = sys.argv[1]
    phases = {}
    for line in open(f"{work}/phases"):
        name, value = line.split()[:2]
        try:
            phases[name] = float(value)
        except ValueError:
            pass
    spans = [("idle1", "fader"), ("fader", "idle2"), ("idle2", "end")]
    failures = []

    frames = gui_frames(f"{work}/lmix.log")
    fps = {}
    print("window (frames lmix drew)")
    for a, b in spans:
        t = [x for x in frames if phases[a] + 0.5 <= x < phases[b]]
        gaps = [(y - x) * 1000 for x, y in zip(t, t[1:])]
        fps[a] = len(t) / (phases[b] - phases[a] - 0.5)
        print(f"  {a:6} {fps[a]:6.1f} fps  gap p50 {percentile(gaps, .5):6.1f}  p95 {percentile(gaps, .95):6.1f}"
              f"  p99 {percentile(gaps, .99):6.1f}  max {max(gaps, default=0):7.1f} ms")
        if a == "fader":
            if percentile(gaps, .95) > env("GAP_P95_MS", 50):
                failures.append(f"window frame gap p95 {percentile(gaps, .95):.0f} ms while the fader moves")
            if max(gaps, default=1e9) > env("GAP_MAX_MS", 250):
                failures.append(f"window frame gap max {max(gaps, default=1e9):.0f} ms while the fader moves")
    if fps["idle2"] < env("RECOVER", 0.8) * fps["idle1"]:
        failures.append(f"window draws {fps['idle2']:.0f} fps after the drag, {fps['idle1']:.0f} before")

    print("GUI thread")
    for line in open(f"{work}/threads"):
        phase, rest = line.split(" ", 1)
        m = re.search(r"\blmix\(\d+\)=(\d+)%", rest)
        if not m:
            failures.append(f"no GUI thread in the {phase} measurement")
            continue
        cpu = int(m.group(1))
        print(f"  {phase:6} {cpu:3d} %")
        if phase == "fader" and cpu > env("GUI_CPU_MAX", 35):
            failures.append(f"GUI thread at {cpu} % while the fader moves")

    rec = program_frames(work, float(open(f"{work}/rec-start").read().split()[1]) + 1.0)
    print("program (recording, start placed to about ±1 s)")
    if not rec:
        failures.append("no recording of program")
    for a, b in spans:
        idx = [i for i, (t, _) in enumerate(rec) if phases[a] + 1.5 <= t < phases[b] - 1.0]
        frozen = sum(1 for i in idx if i and rec[i][1] == rec[i - 1][1])
        print(f"  {a:6} {len(idx):4d} frames, {frozen} the same as the one before")
        if frozen:
            failures.append(f"program froze {frozen} times in {a}")

    for f in failures:
        print(f"OVER BUDGET: {f}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
