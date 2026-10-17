#!/usr/bin/env python3
"""S-E.3: request to first audible sample at the zone mixer's output, for a
sound as its own pipeline (a, through interaudio) or as a branch (b)."""
import struct
import sys
import threading
import time

import gi

gi.require_version("Gst", "1.0")
from gi.repository import GLib, Gst  # noqa: E402

import bench  # noqa: E402

Gst.init(None)
FORMAT = "audio/x-raw,rate=48000,channels=2,format=S16LE,layout=interleaved"


def audible(buf):
    ok, info = buf.map(Gst.MapFlags.READ)
    try:
        return any(struct.unpack_from("<%dh" % (info.size // 2), info.data))
    finally:
        buf.unmap(info)


def once(mode):
    loop = GLib.MainLoop()
    threading.Thread(target=loop.run, daemon=True).start()
    pipe = Gst.parse_launch(
        "audiotestsrc wave=silence is-live=true ! mix. "
        "audiomixer name=mix ! audio/x-raw,rate=48000,channels=2,format=S16LE ! fakesink name=out sync=true"
    )
    mix, out = pipe.get_by_name("mix"), pipe.get_by_name("out")
    heard = threading.Event()
    at = []

    def probe(_pad, info):
        if not heard.is_set() and audible(info.get_buffer()):
            at.append(time.monotonic())
            heard.set()
        return Gst.PadProbeReturn.OK

    out.get_static_pad("sink").add_probe(Gst.PadProbeType.BUFFER, probe)
    pipe.set_state(Gst.State.PLAYING)
    time.sleep(0.5)
    t0 = time.monotonic()
    branch = bench.sound_branch(bench.os.path.join(bench.MEDIA, "s1.flac"))
    if mode == "b":
        pipe.add(branch)
        branch.get_static_pad("src").link(mix.request_pad_simple("sink_%u"))
        branch.sync_state_with_parent()
        extra = []
    else:
        p = Gst.Pipeline.new("sound")
        sink = Gst.ElementFactory.make("interaudiosink")
        sink.set_property("channel", "s")
        caps = Gst.ElementFactory.make("capsfilter")
        caps.set_property("caps", Gst.Caps.from_string(FORMAT))
        p.add(branch)
        p.add(caps)
        p.add(sink)
        branch.link(caps)
        caps.link(sink)
        src = Gst.ElementFactory.make("interaudiosrc")
        src.set_property("channel", "s")
        conv = Gst.ElementFactory.make("audioconvert")
        pipe.add(src)
        pipe.add(conv)
        src.link_filtered(conv, Gst.Caps.from_string(FORMAT))
        conv.get_static_pad("src").link(mix.request_pad_simple("sink_%u"))
        conv.sync_state_with_parent()
        src.sync_state_with_parent()
        p.set_state(Gst.State.PLAYING)
        extra = [p]
    heard.wait(5)
    for x in extra + [pipe]:
        x.set_state(Gst.State.NULL)
    loop.quit()
    return round((at[0] - t0) * 1000) if at else None


if __name__ == "__main__":
    mode = sys.argv[1]
    print(mode, [once(mode) for _ in range(5)])
