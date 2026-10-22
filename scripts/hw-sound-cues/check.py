#!/usr/bin/env python3
"""Sound cues on a real device (HARDWARE-TODO H.37), against a running
lmix-core with strom. Listens on the device's monitor source with `parec`.

  check.py <lmix-core url> <work dir>

Environment: DEVICE (a PipeWire/Pulse sink), DEVICE2 (optional second sink
for one zone on two devices), MULTI (optional sink with more than four
positioned channels). Checks, each printed as ok/FAIL:
  - GO to the first sample at the device, 10 preloaded GOs (SND.12: 50 ms);
  - a panic fades a playing sound out within its time (SAF.3);
  - 16 sounds at once without a gap (SND.4);
  - with DEVICE2: one zone plays on both devices (SND.10);
  - with MULTI: a stereo zone on device channels 3 and 4 plays there and
    nowhere else (SND.10).
"""
import array
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request

CORE, WORK = sys.argv[1], sys.argv[2]
DEVICE = os.environ["DEVICE"]
DEVICE2 = os.environ.get("DEVICE2", "")
MULTI = os.environ.get("MULTI", "")
RATE = 48000
GO_BUDGET_MS = 50
failed = False


def act(body):
    req = urllib.request.Request(CORE + "/api/actions", json.dumps(body).encode(),
                                 {"content-type": "application/json"})
    try:
        urllib.request.urlopen(req, timeout=30).read()
    except urllib.error.HTTPError as e:
        raise SystemExit(f"FAIL {body['action']} refused: {e.code} {e.read()[:300]!r}")


def result(ok, what):
    global failed
    print(("ok   " if ok else "FAIL ") + what, flush=True)
    failed |= not ok


def tone(name, freq, seconds, volume=0.3):
    path = f"{WORK}/{name}.wav"
    subprocess.run(["gst-launch-1.0", "-q", "audiotestsrc", f"freq={freq}", f"volume={volume}",
                    f"num-buffers={seconds * 100}", "samplesperbuffer=480", "!",
                    f"audio/x-raw,rate={RATE},channels=2", "!", "wavenc", "!",
                    "filesink", f"location={path}"], check=True)
    return path


class Recording:
    """The device's monitor from now on; `at(t)` is the sample index of
    monotonic time t (parec's own start lag makes latencies a little short)."""

    def __init__(self, sink, name, channel_map="front-left,front-right"):
        # Without the sink's own map, the server remaps by position and a
        # channel can look silent that is not.
        self.channels = channel_map.count(",") + 1
        self.path = f"{WORK}/{name}.raw"
        self.proc = subprocess.Popen(["parec", "-d", sink + ".monitor", "--format=float32le",
                                      f"--rate={RATE}", f"--channels={self.channels}",
                                      f"--channel-map={channel_map}", "--latency-msec=5", self.path])
        self.t0 = time.monotonic()

    def stop(self):
        self.proc.terminate()
        self.proc.wait()
        self.samples = array.array("f", open(self.path, "rb").read())

    def frame(self, t):
        return int((t - self.t0) * RATE)

    def first_loud(self, after, threshold=0.01):
        s = self.samples
        for k in range(2 * max(self.frame(after), 0), len(s), 2):
            if abs(s[k]) > threshold or abs(s[k + 1]) > threshold:
                return k // 2
        return None

    def peak(self, start, end):
        s = self.samples[2 * max(self.frame(start), 0):2 * max(self.frame(end), 0)]
        return max((abs(x) for x in s), default=0.0)

    def channel_peaks(self):
        n = self.channels
        return [max((abs(x) for x in self.samples[c::n]), default=0.0) for c in range(n)]


def channel_map(sink):
    sinks = json.loads(subprocess.run(["pactl", "-f", "json", "list", "sinks"], check=True,
                                      capture_output=True, text=True).stdout)
    return next(s["channel_map"] for s in sinks if s["name"] == sink)


def zone(*sinks, channels=(1, 2)):
    act({"action": "set_zone_patch", "patch": {"zone": "Hall", "outputs": [
        {"device": s, "channels": list(channels)} for s in sinks]}})
    time.sleep(3)


def cue_list(name, files, first):
    act({"action": "add_cue_list", "name": name, "cues": [
        {"number": str(first + i), "kind": "sound", "sound": {"file": f}} for i, f in enumerate(files)]})
    act({"action": "set_active_cue_list", "list": name})
    time.sleep(1.5)


def main():
    one = tone("one", 440, 1)
    five = tone("five", 440, 5)
    many = [tone(f"m{i}", 200 + i * 37, 12, volume=0.04) for i in range(16)]
    act({"action": "set_go_guard", "ms": 0})
    act({"action": "set_zone", "zone": {"name": "Hall", "channels": 2}})
    zone(DEVICE)

    # GO to the first sample at the device.
    cue_list("Go", [one] * 10, 101)
    rec = Recording(DEVICE, "go")
    time.sleep(1)
    gos = []
    for _ in range(10):
        gos.append(time.monotonic())
        act({"action": "cue_go"})
        time.sleep(1.6)
    rec.stop()
    lat = sorted((rec.first_loud(g) - rec.frame(g)) * 1000 / RATE for g in gos if rec.first_loud(g) is not None)
    median = lat[len(lat) // 2] if lat else float("inf")
    result(len(lat) == 10 and median <= GO_BUDGET_MS,
           f"GO to the device: median {median:.1f} ms, {lat[0] if lat else 0:.1f}-{lat[-1] if lat else 0:.1f} ms "
           f"({len(lat)} of 10 heard, budget {GO_BUDGET_MS} ms)")

    # A panic fades a playing sound out.
    cue_list("Panic", [five], 201)
    rec = Recording(DEVICE, "panic")
    time.sleep(0.5)
    act({"action": "cue_go"})
    time.sleep(1.2)
    panic = time.monotonic()
    act({"action": "cue_panic", "ms": 500})
    time.sleep(3)
    rec.stop()
    after = rec.peak(panic + 1.0, panic + 2.5)
    result(after < 0.001, f"panic of 500 ms: peak {after:.3f} from 1.0 to 2.5 s after it "
                          f"(before: {rec.peak(panic - 0.5, panic):.3f})")

    # 16 sounds at once, no gap.
    cue_list("Many", many, 301)
    rec = Recording(DEVICE, "many")
    for _ in range(16):
        act({"action": "cue_go"})
        time.sleep(0.12)
    time.sleep(0.5)
    start = time.monotonic()
    time.sleep(8.5)  # parec delivers a little late
    rec.stop()
    s = rec.samples[2 * rec.frame(start):2 * rec.frame(start + 8)]
    gaps, run = 0, 0
    for k in range(0, len(s) - 1, 2):
        if s[k] == 0.0 and s[k + 1] == 0.0:
            run += 1
        else:
            gaps += run >= RATE // 500  # 2 ms of exact silence
            run = 0
    result(gaps == 0 and len(s) // 2 >= 8 * RATE * 99 // 100,
           f"16 sounds at once: {gaps} gaps in {len(s) // 2} frames")
    act({"action": "cue_panic", "ms": 0})
    act({"action": "cue_panic", "ms": 0})

    # One zone on two devices.
    if DEVICE2:
        zone(DEVICE, DEVICE2)
        cue_list("Two", [one], 401)
        a, b = Recording(DEVICE, "two-a"), Recording(DEVICE2, "two-b")
        time.sleep(0.5)
        act({"action": "cue_go"})
        time.sleep(1.5)
        a.stop()
        b.stop()
        pa, pb = a.peak(a.t0, a.t0 + 2), b.peak(b.t0, b.t0 + 2)
        result(pa > 0.1 and pb > 0.1, f"one zone on two devices: peaks {pa:.2f} and {pb:.2f}")

    # Device channels 3 and 4 of a positioned multichannel device.
    if MULTI:
        zone(MULTI, channels=(3, 4))
        cue_list("Channels", [one], 501)
        rec = Recording(MULTI, "channels", channel_map(MULTI))
        time.sleep(0.5)
        act({"action": "cue_go"})
        time.sleep(1.5)
        rec.stop()
        peaks = rec.channel_peaks()
        loud = [c + 1 for c, p in enumerate(peaks) if p > 0.1]
        quiet = all(p < 0.001 for c, p in enumerate(peaks) if c + 1 not in (3, 4))
        result(loud == [3, 4] and quiet,
               f"zone on device channels 3 and 4 of {len(peaks)}: peaks "
               + " ".join(f"{p:.2f}" for p in peaks))

    sys.exit(1 if failed else 0)


main()
