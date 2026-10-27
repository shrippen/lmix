**Title:** decklinkvideosink: every frame late after the second start on the same device (playback starts off the frame grid)

**Component:** gst-plugins-bad, `sys/decklink/gstdecklinkvideosink.cpp`

### Describe the bug

The second time a `decklinkvideosink` on the same device starts scheduled playback in one process, the card reports every frame as late (`ScheduledFrameCompleted` with `bmdOutputFrameDisplayedLate`).

`render()` schedules each frame on a multiple of the frame duration in the output clock's internal time:

```c
    running_time = gst_util_uint64_scale (running_time, 1, frame_duration);
    running_time = gst_util_uint64_scale_ceil (running_time, frame_duration, 1);
```

`start_scheduled_playback()` starts the card at the clock's internal time, `StartScheduledPlayback (start_time, GST_SECOND, 1.0)`. On the first start that is 0, on the grid. The internal time then carries on from `clock_epoch`, which collects `clock_last_time` of every earlier playback (in `start()`, `READY_TO_PAUSED`, `PAUSED_TO_READY`). That is wherever the last playback stopped, anywhere between two frames. The card's frame slots then sit between the times the frames are scheduled for, and each frame misses its slot.

### How to reproduce

GStreamer 1.28.8, DeckLink Quad 2, Desktop Video 16.4, Fedora 44. The code is the same on `main` (checked 2026-10-27).

```python
import gi, sys, time
gi.require_version("Gst", "1.0")
from gi.repository import Gst
Gst.init(None)
for run in (1, 2):
    p = Gst.parse_launch(
        "videotestsrc is-live=true pattern=ball "
        "! video/x-raw,format=UYVY,width=1920,height=1080,framerate=25/1 "
        "! decklinkvideosink device-number=0 mode=1080p25")
    p.set_state(Gst.State.PLAYING)
    time.sleep(6)
    p.set_state(Gst.State.NULL)
```

With `GST_DEBUG=decklinkvideosink:5`, counting `Late Frame` and `Dropped Frame` per `Starting scheduled playback`:

```
run 1: playback starts at 0:00:00.000000000, 0 late, 0 dropped
run 2: playback starts at 0:00:05.960102995, 150 late, 0 dropped   <- every frame
```

### Expected behavior

The second run plays like the first.

### Suggested fix

Put the epoch on the frame grid before the start time is read, in `gst_decklink_video_sink_start_scheduled_playback()`:

```diff
@@ -2404,6 +2404,21 @@
         "Elements are not set to PAUSED yet");
     return;
   }
+  // Frames are scheduled on multiples of the frame duration in the
+  // clock's internal time (see render()). Playback has to start on that
+  // grid too, or every frame falls between two of the card's frame slots.
+  // The epoch carries on from the previous playback and is anywhere.
+  {
+    GstClockTime frame_duration =
+        gst_util_uint64_scale_int (GST_SECOND, self->output->mode->fps_d,
+        self->output->mode->fps_n);
+    GstClockTime now =
+        self->output->clock_epoch + self->output->clock_last_time;
+    GstClockTime aligned =
+        gst_util_uint64_scale_ceil (now, 1, frame_duration) * frame_duration;
+    self->output->clock_epoch += aligned - now;
+  }
+
   // Need to unlock to get the clock time
   g_mutex_unlock (&self->output->lock);
 
```

With it, the same script gives:

```
run 1: playback starts at 0:00:00.000000000, 0 late, 4 dropped (first frames)
run 2: playback starts at 0:00:06.000000000, 0 late, 1 dropped
```

In lmix (strom flows with direct media timing, `scripts/test-sdi-loop.sh` on an SDI loop) every start after the first plays all frames late without the fix. With the fix, later starts there still failed now and then until the sink's `processing-deadline` was raised to 150 ms; that part may come from our pipeline and is not part of this report.

<!-- Note for lmix (not for upstream): the reproduction above is the clean case. Before filing, re-run restart.py on the current GStreamer. -->

### Workaround

None found from outside the element: the epoch and the rounding are internal. Restarting the process resets the epoch.
