**Title:** cudaconverter: alpha set back to 1.0 after a fraction is ignored (`priv->alpha` never updated)

**Component:** gst-plugins-bad, `gst-libs/gst/cuda/gstcudaconverter.cpp` (seen through `cudacompositor`)

### Describe the bug

The `alpha` property setter of `GstCudaConverter` compares the new value with `priv->alpha`, but it never assigns `priv->alpha`:

```c
    case PROP_ALPHA:
    {
      auto alpha = g_value_get_double (value);
      if (priv->alpha != alpha) {
        priv->update_const_buf = TRUE;
        priv->const_buf->alpha = (float) alpha;
      }
      break;
    }
```

`priv->alpha` stays at its initial 1.0. Any value other than 1.0 is applied. A value of exactly 1.0 compares equal to the stale field, so it is dropped. A sink pad of `cudacompositor` that was faded to 0.5 and then set back to 1.0 stays at 0.5. The getter returns the stale `priv->alpha`: it reads 1.0 while the picture is drawn at 0.5.

A fade from one source to another ends with the new source at the last in-between alpha (here about 0.97, so the background shows through). A source faded out and cut back in later stays at about 0.03.

### How to reproduce

GStreamer 1.28.8, NVIDIA RTX 3090, driver 615.78. The code is unchanged on `main` (checked 2026-10-19).

```python
import gi, time
gi.require_version("Gst", "1.0")
from gi.repository import Gst
Gst.init(None)
p = Gst.parse_launch(
    "videotestsrc pattern=white is-live=true ! video/x-raw,width=64,height=64,framerate=30/1 "
    "! cudaupload ! cudacompositor name=mix ! cudadownload "
    "! video/x-raw,format=NV12 ! appsink name=sink max-buffers=1 drop=true sync=false")
pad = p.get_by_name("mix").get_static_pad("sink_0")
sink = p.get_by_name("sink")
p.set_state(Gst.State.PLAYING)

def luma():
    time.sleep(0.5)
    for _ in range(5):
        sink.emit("pull-sample")
    s = sink.emit("pull-sample")
    ok, m = s.get_buffer().map(Gst.MapFlags.READ)
    y = m.data[32 * 64 + 32]
    s.get_buffer().unmap(m)
    return y

for alpha in (1.0, 0.5, 1.0):
    pad.set_property("alpha", alpha)
    print(f"alpha {alpha}: Y {luma()}")
p.set_state(Gst.State.NULL)
```

Output:

```
alpha 1.0: Y 235
alpha 0.5: Y 117
alpha 1.0: Y 117      <- expected 235
```

### Expected behavior

Alpha 1.0 after 0.5 gives Y 235 again.

### Suggested fix

Store the value in the setter:

```c
      if (priv->alpha != alpha) {
        priv->alpha = alpha;
        priv->update_const_buf = TRUE;
        priv->const_buf->alpha = (float) alpha;
      }
```

### Workaround

Strom works around it on its CUDA mixer pads. When a pad's alpha is set to exactly 1.0 (on `notify::alpha`), it sets `1.0 - 1e-12` instead. That differs from the stale field, and it is still exactly `1.0f` once converted for the kernel.
