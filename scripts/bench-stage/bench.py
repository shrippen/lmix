#!/usr/bin/env python3
"""Stage (VIS): picture cues as one pipeline per cue (A, like an lmix
input) or as branches of one stage pipeline (B).

  bench.py a|b|p N -> one JSON line: start per cue (request to the first
                      stage output frame that shows it), RSS and threads
                      added, CPU while N play for 10 s, frames the stage
                      output dropped (late at the clocked sink), its
                      frame rate while they play.

Stage    videotestsrc black (live) ! compositor ! 1080p30 ! fakesink sync
A        per cue  filesrc ! decodebin ! convert ! scale ! intervideosink
         stage    intervideosrc ! convert ! compositor pad
B        per cue  filesrc ! decodebin ! convert ! scale ! queue ! compositor pad,
                  added to the running stage
P        B, the cue loaded before GO (decoded, held at its pad)
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
CAPS = "video/x-raw,width=1920,height=1080,framerate=30/1"
# A cue covers a tile of the stage; its centre is where it shows.
TILE_W, TILE_H = 480, 270


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


def tile(i):
    """Top-left of cue i's tile (4 x 4 grid)."""
    return (i % 4) * TILE_W, (i // 4) * TILE_H


class Watch:
    """Times at which the stage output first shows each tile lit."""

    def __init__(self, pad):
        self.wanted = {}
        self.lock = threading.Lock()
        pad.add_probe(Gst.PadProbeType.BUFFER, self.probe)

    def expect(self, i):
        event = threading.Event()
        with self.lock:
            self.wanted[i] = [event, None]
        return event

    def at(self, i):
        return self.wanted[i][1]

    def probe(self, _pad, info):
        with self.lock:
            open_ = [i for i, (_, at) in self.wanted.items() if at is None]
        if not open_:
            return Gst.PadProbeReturn.OK
        buf = info.get_buffer()
        ok, m = buf.map(Gst.MapFlags.READ)
        if not ok:
            return Gst.PadProbeReturn.OK
        try:
            now = time.monotonic()
            for i in open_:
                x, y = tile(i)
                # I420: the luma plane comes first, one byte per pixel; a
                # 5 x 5 grid over the tile, as patterns have dark parts.
                luma = max(
                    m.data[(y + TILE_H * r // 6) * 1920 + x + TILE_W * c // 6]
                    for r in range(1, 6)
                    for c in range(1, 6)
                )
                if luma > 40:
                    with self.lock:
                        self.wanted[i][1] = now
                    self.wanted[i][0].set()
        finally:
            buf.unmap(m)
        return Gst.PadProbeReturn.OK


def stage():
    pipe = Gst.parse_launch(
        f"videotestsrc pattern=black is-live=true ! {CAPS},format=I420 ! mix.sink_0 "
        f"compositor name=mix background=black ! {CAPS},format=I420 ! "
        "fakesink name=out sync=true qos=true"
    )
    return pipe, pipe.get_by_name("mix"), pipe.get_by_name("out")


def cue_branch(path):
    """filesrc ! decodebin ! videoconvert ! videoscale ! caps in a bin with
    a ghost src pad; the tile's size, so the stage only places it."""
    b = Gst.Bin.new(None)
    src = Gst.ElementFactory.make("filesrc")
    src.set_property("location", path)
    dec = Gst.ElementFactory.make("decodebin")
    conv = Gst.ElementFactory.make("videoconvert")
    scale = Gst.ElementFactory.make("videoscale")
    caps = Gst.ElementFactory.make("capsfilter")
    caps.set_property(
        "caps", Gst.Caps.from_string(f"video/x-raw,format=I420,width={TILE_W},height={TILE_H}")
    )
    queue = Gst.ElementFactory.make("queue")
    for e in (src, dec, conv, scale, caps, queue):
        b.add(e)
    src.link(dec)
    conv.link(scale)
    scale.link(caps)
    caps.link(queue)
    dec.connect("pad-added", lambda _d, pad: pad.link(conv.get_static_pad("sink")))
    b.add_pad(Gst.GhostPad.new("src", queue.get_static_pad("src")))
    return b


def place(pad, i):
    x, y = tile(i)
    pad.set_property("xpos", x)
    pad.set_property("ypos", y)
    pad.set_property("zorder", i + 1)


def run_b(pipe, mix, watch, n):
    starts = []
    for i in range(n):
        seen = watch.expect(i)
        t0 = time.monotonic()
        branch = cue_branch(os.path.join(MEDIA, f"v{i + 1}.mp4"))
        pipe.add(branch)
        pad = mix.request_pad_simple("sink_%u")
        place(pad, i)
        # The clip starts now, not at the stage's running time 0 (as strom's
        # lmix-live-offset does for the mixer).
        clock = pipe.get_clock()
        branch.get_static_pad("src").set_offset(clock.get_time() - pipe.get_base_time())
        branch.get_static_pad("src").link(pad)
        branch.sync_state_with_parent()
        seen.wait(5)
        starts.append(round((watch.at(i) - t0) * 1000) if watch.at(i) else None)
    return [pipe], starts


def run_p(pipe, mix, watch, n):
    """B with the cues loaded before GO: decoded and held at their pad."""
    ready = []
    for i in range(n):
        branch = cue_branch(os.path.join(MEDIA, f"v{i + 1}.mp4"))
        out = branch.get_static_pad("src")
        held = threading.Event()

        def hold(_pad, _info, held=held):
            held.set()
            return Gst.PadProbeReturn.OK

        block = out.add_probe(Gst.PadProbeType.BLOCK_DOWNSTREAM, hold)
        pipe.add(branch)
        branch.sync_state_with_parent()
        held.wait(5)
        ready.append((branch, out, block))
    starts = []
    for i, (branch, out, block) in enumerate(ready):
        seen = watch.expect(i)
        t0 = time.monotonic()
        pad = mix.request_pad_simple("sink_%u")
        place(pad, i)
        out.set_offset(pipe.get_clock().get_time() - pipe.get_base_time())
        out.link(pad)
        out.remove_probe(block)
        seen.wait(5)
        starts.append(round((watch.at(i) - t0) * 1000) if watch.at(i) else None)
    return [pipe], starts


def run_a(pipe, mix, watch, n):
    starts = []
    pipes = [pipe]
    for i in range(n):
        seen = watch.expect(i)
        t0 = time.monotonic()
        # The cue's own pipeline, like an lmix input flow.
        p = Gst.Pipeline.new(f"cue{i}")
        branch = cue_branch(os.path.join(MEDIA, f"v{i + 1}.mp4"))
        out = Gst.ElementFactory.make("intervideosink")
        out.set_property("channel", f"c{i}")
        p.add(branch)
        p.add(out)
        branch.link(out)
        # The stage's end of it.
        src = Gst.ElementFactory.make("intervideosrc")
        src.set_property("channel", f"c{i}")
        conv = Gst.ElementFactory.make("videoconvert")
        caps = Gst.ElementFactory.make("capsfilter")
        caps.set_property(
            "caps",
            Gst.Caps.from_string(
                f"video/x-raw,format=I420,width={TILE_W},height={TILE_H},framerate=30/1"
            ),
        )
        for e in (src, conv, caps):
            pipe.add(e)
        src.link(conv)
        conv.link(caps)
        pad = mix.request_pad_simple("sink_%u")
        place(pad, i)
        caps.get_static_pad("src").link(pad)
        for e in (caps, conv, src):
            e.sync_state_with_parent()
        p.set_state(Gst.State.PLAYING)
        seen.wait(5)
        starts.append(round((watch.at(i) - t0) * 1000) if watch.at(i) else None)
        pipes.append(p)
    return pipes, starts


def main():
    mode, n = sys.argv[1], int(sys.argv[2])
    loop = GLib.MainLoop()
    threading.Thread(target=loop.run, daemon=True).start()
    base_rss = rss_mb()
    pipe, mix, out = stage()
    watch = Watch(out.get_static_pad("sink"))
    pipe.set_state(Gst.State.PLAYING)
    pipe.get_state(Gst.CLOCK_TIME_NONE)
    time.sleep(0.5)
    rss0, th0 = rss_mb(), threads()
    run = {"a": run_a, "b": run_b, "p": run_p}[mode]
    pipes, starts = run(pipe, mix, watch, n)
    stats0 = out.get_property("stats")
    dropped0, rendered0 = stats0.get_value("dropped"), stats0.get_value("rendered")
    c0, w0 = cpu_s(), time.monotonic()
    time.sleep(PLAY_S)
    played = time.monotonic() - w0
    cpu = (cpu_s() - c0) / played * 100
    stats = out.get_property("stats")
    print(
        json.dumps(
            {
                "mode": mode,
                "cues": n,
                "start_ms": starts,
                "rss_mb": round(rss_mb() - rss0, 1),
                "rss_stage_mb": round(rss0 - base_rss, 1),
                "threads": threads() - th0,
                "cpu_pct": round(cpu, 1),
                "dropped": stats.get_value("dropped") - dropped0,
                "fps": round((stats.get_value("rendered") - rendered0) / played, 1),
                "pipelines": len(pipes),
            }
        )
    )
    for p in pipes:
        p.set_state(Gst.State.NULL)
    loop.quit()


if __name__ == "__main__":
    main()
