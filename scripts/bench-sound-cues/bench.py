#!/usr/bin/env python3
"""S-E.3: sound cues as one pipeline per sound (A, like an lmix input)
or as branches of one player pipeline per zone (B).

  bench.py a|b N   -> one JSON line: start per sound (request to first
                      decoded buffer: a at its interaudiosink, b at the
                      mixer pad), RSS and threads before/after, CPU while
                      N play for 10 s. To audible: latency.py.

A: per sound  filesrc ! decodebin ! convert ! resample ! interaudiosink
   mixer      interaudiosrc x N ! audiomixer ! fakesink sync
B: one        audiomixer ! fakesink sync, a branch per sound added live
"""
import json
import os
import resource
import sys
import threading
import time

import gi

gi.require_version("Gst", "1.0")
from gi.repository import GLib, Gst  # noqa: E402

Gst.init(None)
MEDIA = os.path.join(os.path.dirname(os.path.abspath(__file__)), "media")
PLAY_S = 10


def rss_mb():
    with open("/proc/self/status") as f:
        for line in f:
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) / 1024
    return 0.0


def threads():
    return len(os.listdir("/proc/self/task"))


def cpu_s():
    r = resource.getrusage(resource.RUSAGE_SELF)
    return r.ru_utime + r.ru_stime


class FirstBuffer:
    """Time of the first buffer through a pad."""

    def __init__(self, pad):
        self.at = None
        self.event = threading.Event()
        pad.add_probe(Gst.PadProbeType.BUFFER, self.probe)

    def probe(self, _pad, _info):
        if self.at is None:
            self.at = time.monotonic()
            self.event.set()
        return Gst.PadProbeReturn.OK


def mixer_pipeline(sources):
    # A zone runs all the time: silence keeps the mixer going.
    pipe = Gst.parse_launch(
        "audiotestsrc wave=silence is-live=true ! mix. "
        "audiomixer name=mix ! audio/x-raw,rate=48000,channels=2 ! fakesink sync=true"
    )
    return pipe, pipe.get_by_name("mix")


def sound_branch(path):
    """filesrc ! decodebin ! audioconvert ! audioresample ! volume in a bin
    with a ghost src pad."""
    b = Gst.Bin.new(None)
    src = Gst.ElementFactory.make("filesrc")
    src.set_property("location", path)
    dec = Gst.ElementFactory.make("decodebin")
    conv = Gst.ElementFactory.make("audioconvert")
    res = Gst.ElementFactory.make("audioresample")
    vol = Gst.ElementFactory.make("volume")
    for e in (src, dec, conv, res, vol):
        b.add(e)
    src.link(dec)
    conv.link(res)
    res.link(vol)
    dec.connect("pad-added", lambda _d, pad: pad.link(conv.get_static_pad("sink")))
    b.add_pad(Gst.GhostPad.new("src", vol.get_static_pad("src")))
    return b


def run_b(n):
    pipe, mix = mixer_pipeline(n)
    pipe.set_state(Gst.State.PLAYING)
    pipe.get_state(Gst.CLOCK_TIME_NONE)
    rss0, th0 = rss_mb(), threads()
    starts = []
    for i in range(n):
        t0 = time.monotonic()
        branch = sound_branch(os.path.join(MEDIA, f"s{i + 1}.flac"))
        pipe.add(branch)
        sink = mix.request_pad_simple("sink_%u")
        first = FirstBuffer(sink)
        branch.get_static_pad("src").link(sink)
        branch.sync_state_with_parent()
        first.event.wait(5)
        starts.append(round((first.at - t0) * 1000, 1) if first.at else None)
    return pipe, [pipe], starts, rss0, th0


def run_a(n):
    mixer = Gst.Pipeline.new("mixer")
    mix = Gst.ElementFactory.make("audiomixer")
    caps = Gst.ElementFactory.make("capsfilter")
    caps.set_property("caps", Gst.Caps.from_string("audio/x-raw,rate=48000,channels=2"))
    sink = Gst.ElementFactory.make("fakesink")
    sink.set_property("sync", True)
    silence = Gst.ElementFactory.make("audiotestsrc")
    silence.set_property("wave", "silence")
    silence.set_property("is-live", True)
    for e in (silence, mix, caps, sink):
        mixer.add(e)
    silence.link(mix)
    mix.link(caps)
    caps.link(sink)
    mixer.set_state(Gst.State.PLAYING)
    mixer.get_state(Gst.CLOCK_TIME_NONE)
    rss0, th0 = rss_mb(), threads()
    starts = []
    pipes = [mixer]
    for i in range(n):
        t0 = time.monotonic()
        # The sound's own pipeline, like an lmix input flow.
        p = Gst.Pipeline.new(f"sound{i}")
        branch = sound_branch(os.path.join(MEDIA, f"s{i + 1}.flac"))
        out = Gst.ElementFactory.make("interaudiosink")
        out.set_property("channel", f"s{i}")
        p.add(branch)
        p.add(out)
        branch.link(out)
        # The mixer's end of it.
        src = Gst.ElementFactory.make("interaudiosrc")
        src.set_property("channel", f"s{i}")
        mixer.add(src)
        pad = mix.request_pad_simple("sink_%u")
        src.get_static_pad("src").link(pad)
        # First buffer with sound at the sound's sink (the mixer side
        # only ever sees silence first: interaudiosrc fills gaps).
        first = FirstBuffer(out.get_static_pad("sink"))
        src.sync_state_with_parent()
        p.set_state(Gst.State.PLAYING)
        first.event.wait(5)
        starts.append(round((first.at - t0) * 1000, 1) if first.at else None)
        pipes.append(p)
    return mixer, pipes, starts, rss0, th0


def main():
    mode, n = sys.argv[1], int(sys.argv[2])
    loop = GLib.MainLoop()
    threading.Thread(target=loop.run, daemon=True).start()
    base_rss, base_th = rss_mb(), threads()
    _, pipes, starts, rss0, th0 = (run_a if mode == "a" else run_b)(n)
    c0, w0 = cpu_s(), time.monotonic()
    time.sleep(PLAY_S)
    cpu = (cpu_s() - c0) / (time.monotonic() - w0) * 100
    print(
        json.dumps(
            {
                "mode": mode,
                "sounds": n,
                "start_ms": starts,
                "rss_mb": round(rss_mb() - rss0, 1),
                "rss_base_mb": round(rss0 - base_rss, 1),
                "threads": threads() - th0,
                "cpu_pct": round(cpu, 1),
                "pipelines": len(pipes),
            }
        )
    )
    for p in pipes:
        p.set_state(Gst.State.NULL)
    loop.quit()


if __name__ == "__main__":
    main()
