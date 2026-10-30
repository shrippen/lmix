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

GStreamer 1.28.8, DeckLink Quad 2, Desktop Video 16.4, Fedora 44. The code is the same on `main` (checked 2026-10-27; reproduced again 2026-10-28).

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

With `GST_DEBUG=decklinkvideosink:5`, counting `Late Frame` and `Dropped Frame` per `Starting scheduled playback`. In 8 of 10 runs of the script, the second playback has every frame late:

```
run 1: playback starts at 0:00:00.000000000, 0 late, 1 dropped
run 2: playback starts at 0:00:05.968715445, 150 late, 0 dropped   <- every frame
```

In the other 2 runs the second playback had 1 late frame (start at 5.941 s and 5.960 s); whether it fails depends on where between two frames the first playback stopped.

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

With it (and the two changes below), the second playback starts on the grid. In 5 of 5 runs of the script:

```
run 1: playback starts at 0:00:00.000000000, 0 or 1 late, 0 dropped
run 2: playback starts at 0:00:06.000000000, 0 late, 0 dropped     (once 5.960, also on the grid)
```

### Two more causes of late frames

With only the change above, later starts in a real pipeline (strom flows with direct media timing on an SDI loop, `scripts/test-sdi-loop.sh` in lmix) still showed 150–205 late frames now and then. Two more things in the sink make frames late; both show on the first start too, but there the margin usually hides them.

**The clock starts counting late.** `gst_decklink_clock_get_internal_time()` sets `clock_start_time` at its first reading after the card started, not when playback starts. If that reading comes tens of milliseconds after `StartScheduledPlayback()`, the internal time lags the card's playback by as much (38 ms measured), and every frame is scheduled that much too late. Anchoring the clock where playback starts fixes it:

```diff
@@ -2437,6 +2455,21 @@
       "Starting scheduled playback at %" GST_TIME_FORMAT,
       GST_TIME_ARGS (start_time));
 
+  // The clock counts from its first reading after the start (see
+  // gst_decklink_clock_get_internal_time()), which may come tens of
+  // milliseconds after the card starts, and then lags the card's playback
+  // by as much: frames arrive late. Anchor it here, where playback starts
+  // at start_time.
+  {
+    BMDTimeValue now;
+    if (self->output->output->GetHardwareReferenceClock (GST_SECOND, &now,
+            NULL, NULL) == S_OK && now >= 0) {
+      GstClockTime last = self->output->clock_last_time;
+      self->output->clock_start_time =
+          (GstClockTime) now > last ? (GstClockTime) now - last : 0;
+    }
+  }
+
   res =
       self->output->output->StartScheduledPlayback (start_time,
       GST_SECOND, 1.0);
```

**`render()` rounds down.** It schedules a frame on the frame slot at or before its running time. The wait before it leaves the frame 30 ms ahead of its time (`processing-deadline`); rounding down takes up to one frame duration of that (40 ms at 25 fps), so the frame reaches the card after its slot. Logged on one late frame: running time 0.193 s, scheduled for 0.160 s, handed to the card at 0.163 s. Rounding up keeps the margin:

```diff
@@ -2234,8 +2234,11 @@
     frame_duration =
         gst_util_uint64_scale_int (GST_SECOND, self->output->mode->fps_d,
         self->output->mode->fps_n);
-    running_time = gst_util_uint64_scale (running_time, 1, frame_duration);
-    running_time = gst_util_uint64_scale_ceil (running_time, frame_duration, 1);
+    // Up to the next frame slot: rounded down, a frame lands up to one
+    // frame duration before its time, which eats the 30 ms the wait above
+    // leaves, and it reaches the card late.
+    running_time = gst_util_uint64_scale_ceil (running_time, 1, frame_duration);
+    running_time = gst_util_uint64_scale (running_time, frame_duration, 1);
```

With all three changes, three runs of four starts each (1080p25, 1080i50, 1080psf25, 720p50) in one process showed no late or dropped frame. The full patch is `packaging/gstreamer/decklink-start-on-frame-grid.patch` in lmix.

### Workaround

None found from outside the element: the epoch and the rounding are internal. Restarting the process resets the epoch.
