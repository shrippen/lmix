// A state from lmix-core (--engine none): four pattern inputs, program 1,
// preview 2, the bus "Stream". Made with add_input, cut_direct,
// preview_input, add_bus and GET /api/state.
.pragma library

var state = {
  "api_version": 1,
  "path": null,
  "inputs": [
    {
      "id": "6af732e0-3db1-443b-a9f0-c679c9cebc6f",
      "number": 1,
      "name": "Ball",
      "source": {
        "kind": "pattern",
        "pattern": "ball"
      },
      "slot": 0,
      "status": "online",
      "color": null,
      "audio": {
        "on": true,
        "follow_video": false,
        "volume": 100
      },
      "has_audio": true,
      "ingest": null,
      "signal_loss": "hold",
      "auto_play": {
        "play": false,
        "restart": false,
        "pause": false
      },
      "at_end": "loop",
      "click": "preview_input",
      "go": "quick_play",
      "listen": "off",
      "strip": {
        "gain_db": 0.0,
        "pan": 0.0,
        "hpf": {
          "on": false,
          "hz": 80.0
        },
        "eq": {
          "on": false,
          "gain_db": [
            0.0,
            0.0,
            0.0,
            0.0
          ]
        },
        "gate": {
          "on": false,
          "threshold_db": -40.0
        },
        "compressor": {
          "on": false,
          "threshold_db": -20.0,
          "ratio": 4.0
        },
        "delay_ms": 0
      },
      "phones": {
        "on": true,
        "level": 100
      },
      "sends": {},
      "look": {
        "brightness": 0.0,
        "contrast": 1.0,
        "saturation": 1.0,
        "hue": 0.0,
        "gamma": 1.0,
        "temperature": 0.0,
        "tint": 0.0
      },
      "triggers": []
    },
    {
      "id": "041a6709-a4de-4ae3-be1a-42caa4609cff",
      "number": 2,
      "name": "Bars",
      "source": {
        "kind": "pattern",
        "pattern": "bars"
      },
      "slot": 1,
      "status": "online",
      "color": null,
      "audio": {
        "on": true,
        "follow_video": false,
        "volume": 100
      },
      "has_audio": true,
      "ingest": null,
      "signal_loss": "hold",
      "auto_play": {
        "play": false,
        "restart": false,
        "pause": false
      },
      "at_end": "loop",
      "click": "preview_input",
      "go": "quick_play",
      "listen": "off",
      "strip": {
        "gain_db": 0.0,
        "pan": 0.0,
        "hpf": {
          "on": false,
          "hz": 80.0
        },
        "eq": {
          "on": false,
          "gain_db": [
            0.0,
            0.0,
            0.0,
            0.0
          ]
        },
        "gate": {
          "on": false,
          "threshold_db": -40.0
        },
        "compressor": {
          "on": false,
          "threshold_db": -20.0,
          "ratio": 4.0
        },
        "delay_ms": 0
      },
      "phones": {
        "on": true,
        "level": 100
      },
      "sends": {},
      "look": {
        "brightness": 0.0,
        "contrast": 1.0,
        "saturation": 1.0,
        "hue": 0.0,
        "gamma": 1.0,
        "temperature": 0.0,
        "tint": 0.0
      },
      "triggers": []
    },
    {
      "id": "e54ef114-ef16-4552-8bdc-31c8435de2e3",
      "number": 3,
      "name": "Snow",
      "source": {
        "kind": "pattern",
        "pattern": "snow"
      },
      "slot": 2,
      "status": "online",
      "color": null,
      "audio": {
        "on": true,
        "follow_video": false,
        "volume": 100
      },
      "has_audio": true,
      "ingest": null,
      "signal_loss": "hold",
      "auto_play": {
        "play": false,
        "restart": false,
        "pause": false
      },
      "at_end": "loop",
      "click": "preview_input",
      "go": "quick_play",
      "listen": "off",
      "strip": {
        "gain_db": 0.0,
        "pan": 0.0,
        "hpf": {
          "on": false,
          "hz": 80.0
        },
        "eq": {
          "on": false,
          "gain_db": [
            0.0,
            0.0,
            0.0,
            0.0
          ]
        },
        "gate": {
          "on": false,
          "threshold_db": -40.0
        },
        "compressor": {
          "on": false,
          "threshold_db": -20.0,
          "ratio": 4.0
        },
        "delay_ms": 0
      },
      "phones": {
        "on": true,
        "level": 100
      },
      "sends": {},
      "look": {
        "brightness": 0.0,
        "contrast": 1.0,
        "saturation": 1.0,
        "hue": 0.0,
        "gamma": 1.0,
        "temperature": 0.0,
        "tint": 0.0
      },
      "triggers": []
    },
    {
      "id": "0e8edf53-b873-4924-af67-524ece8f218e",
      "number": 4,
      "name": "Checkers",
      "source": {
        "kind": "pattern",
        "pattern": "checkers"
      },
      "slot": 3,
      "status": "online",
      "color": null,
      "audio": {
        "on": true,
        "follow_video": false,
        "volume": 100
      },
      "has_audio": true,
      "ingest": null,
      "signal_loss": "hold",
      "auto_play": {
        "play": false,
        "restart": false,
        "pause": false
      },
      "at_end": "loop",
      "click": "preview_input",
      "go": "quick_play",
      "listen": "off",
      "strip": {
        "gain_db": 0.0,
        "pan": 0.0,
        "hpf": {
          "on": false,
          "hz": 80.0
        },
        "eq": {
          "on": false,
          "gain_db": [
            0.0,
            0.0,
            0.0,
            0.0
          ]
        },
        "gate": {
          "on": false,
          "threshold_db": -40.0
        },
        "compressor": {
          "on": false,
          "threshold_db": -20.0,
          "ratio": 4.0
        },
        "delay_ms": 0
      },
      "phones": {
        "on": true,
        "level": 100
      },
      "sends": {},
      "look": {
        "brightness": 0.0,
        "contrast": 1.0,
        "saturation": 1.0,
        "hue": 0.0,
        "gamma": 1.0,
        "temperature": 0.0,
        "tint": 0.0
      },
      "triggers": []
    }
  ],
  "preview": "041a6709-a4de-4ae3-be1a-42caa4609cff",
  "program": "6af732e0-3db1-443b-a9f0-c679c9cebc6f",
  "transitions": [
    {
      "slot": 1,
      "effect": "fade",
      "duration_ms": 500
    },
    {
      "slot": 2,
      "effect": "cut",
      "duration_ms": 500
    },
    {
      "slot": 3,
      "effect": "dip_to_black",
      "duration_ms": 500
    },
    {
      "slot": 4,
      "effect": "wipe_left",
      "duration_ms": 500
    }
  ],
  "quick_play": {
    "effect": "fade",
    "duration_ms": 500
  },
  "fade_to_black": false,
  "slots": 16,
  "outputs": [],
  "overlays": [
    null,
    null,
    null,
    null,
    null,
    null,
    null,
    null
  ],
  "overlay_last": [
    null,
    null,
    null,
    null,
    null,
    null,
    null,
    null
  ],
  "overlay_channels": [
    {
      "effect": "fade",
      "duration_ms": 500
    },
    {
      "effect": "fade",
      "duration_ms": 500
    },
    {
      "effect": "fade",
      "duration_ms": 500
    },
    {
      "effect": "fade",
      "duration_ms": 500
    },
    {
      "effect": "fade",
      "duration_ms": 500
    },
    {
      "effect": "fade",
      "duration_ms": 500
    },
    {
      "effect": "fade",
      "duration_ms": 500
    },
    {
      "effect": "fade",
      "duration_ms": 500
    }
  ],
  "overlay_preview": [
    null,
    null,
    null,
    null,
    null,
    null,
    null,
    null
  ],
  "t_bar": 0.0,
  "buses": [
    {
      "id": "ff32e69b-98c9-4f1c-8477-334d922aadfd",
      "name": "Stream",
      "pre_fader": false,
      "volume": 100,
      "mute": false,
      "listen": false
    }
  ],
  "master": {
    "volume": 100,
    "mute": false
  },
  "format": {
    "width": 1920,
    "height": 1080,
    "framerate": "30/1"
  },
  "multiview": {
    "program": "right",
    "meters": "shown"
  },
  "sample_rate": 48000,
  "phones": {
    "volume": 100,
    "mute": false,
    "pre_fader": false
  },
  "default_delay_ms": 0,
  "looks_available": false,
  "layouts": [],
  "layout_templates": [],
  "shortcuts": [],
  "cue_lists": [],
  "running_cues": [],
  "go_guard_ms": 300,
  "go_rearm": "time",
  "panic_ms": 1000,
  "script_folder": null,
  "older_format": null,
  "cue_warnings": [],
  "targets": [],
  "feeds": [],
  "zones": [],
  "stages": [],
  "locked": false,
  "io_locked": [],
  "cards_on": false,
  "midi_tbar": null,
  "restorable": null,
  "graphics": [],
  "datasets": [],
  "data_sources": [],
  "dynamic_inputs": [
    null,
    null,
    null,
    null
  ],
  "dynamic_values": [
    "",
    "",
    "",
    ""
  ]
}
;
