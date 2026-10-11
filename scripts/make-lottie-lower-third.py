#!/usr/bin/env python3
"""Writes the built-in Lottie lower third:
    scripts/make-lottie-lower-third.py workspace/lmix-core/titles/lmix/lottie-lower-third/animation.json
Colours come from Kante at run time ("kante:<token>" layer names); the
values here are only placeholders."""
import json, sys

FR = 50
IN_END = 30
OUT_AT, OUT_DR = 120, 20
OP = OUT_AT + OUT_DR

def ease(dims):
    # Kante's --ease-out: cubic-bezier(.2,.8,.2,1)
    return {"o": {"x": [0.2] * dims, "y": [0.8] * dims}, "i": {"x": [0.2] * dims, "y": [1] * dims}}

def anim(frames):
    """frames: [(t, value_list)] -> animated property."""
    keys = []
    for n, (t, v) in enumerate(frames):
        key = {"t": t, "s": v}
        if n < len(frames) - 1:
            key.update(ease(len(v)))
        keys.append(key)
    return {"a": 1, "k": keys}

def still(v):
    return {"a": 0, "k": v}

def transform(p, scale=None, opacity=None):
    return {
        "o": opacity or still(100),
        "r": still(0),
        "p": still(p + [0]),
        "a": still([0, 0, 0]),
        "s": scale or still([100, 100, 100]),
    }

def wipe(start, end):
    """Scale x 0 -> 100 in, 100 -> 0 out (anchor at the left edge)."""
    return anim([(start, [0, 100, 100]), (end, [100, 100, 100]),
                 (OUT_AT, [100, 100, 100]), (OUT_AT + OUT_DR - 4, [0, 100, 100])])

def fade(start, end):
    return anim([(start, [0]), (end, [100]), (OUT_AT, [100]), (OUT_AT + 8, [0])])

def rect(ind, name, x, y, w, h, scale):
    return {
        "ddd": 0, "ind": ind, "ty": 4, "nm": name, "sr": 1,
        "ks": transform([x, y], scale=scale), "ao": 0,
        "shapes": [{"ty": "gr", "nm": "box", "it": [
            {"ty": "rc", "d": 1, "s": still([w, h]), "p": still([w / 2, h / 2]), "r": still(0), "nm": "rect"},
            {"ty": "fl", "c": still([0.5, 0.5, 0.5, 1]), "o": still(100), "r": 1, "nm": "fill"},
            {"ty": "tr", "p": still([0, 0]), "a": still([0, 0]), "s": still([100, 100]), "r": still(0),
             "o": still(100), "sk": still(0), "sa": still(0)},
        ]}],
        "ip": 0, "op": OP, "st": 0, "bm": 0,
    }

def text(ind, name, x, y, size, font, tracking, opacity):
    return {
        "ddd": 0, "ind": ind, "ty": 5, "nm": name, "sr": 1,
        "ks": transform([x, y], opacity=opacity), "ao": 0,
        "t": {"d": {"k": [{"s": {"s": size, "f": font, "t": " ", "j": 0, "tr": tracking,
                                  "lh": size * 1.2, "ls": 0, "fc": [0.5, 0.5, 0.5]}, "t": 0}]},
              "p": {}, "m": {"g": 1, "a": still([0, 0])}, "a": []},
        "ip": 0, "op": OP, "st": 0, "bm": 0,
    }

doc = {
    "v": "5.12.2", "fr": FR, "ip": 0, "op": OP, "w": 1920, "h": 1080, "nm": "lmix lower third", "ddd": 0,
    "assets": [],
    "fonts": {"list": [
        {"fName": "Rajdhani-Bold", "fFamily": "Rajdhani", "fStyle": "Bold", "ascent": 70},
        {"fName": "JetBrainsMono-Medium", "fFamily": "JetBrains Mono", "fStyle": "Medium", "ascent": 75},
    ]},
    "layers": [
        text(1, "kante:fg0 name", 132, 905, 64, "Rajdhani-Bold", 20, fade(8, 24)),
        text(2, "kante:yellow role", 124, 976, 24, "JetBrainsMono-Medium", 140, fade(16, IN_END)),
        rect(3, "kante:yellow bar", 96, 838, 8, 92, still([100, 100, 100])),
        rect(4, "kante:bg0 panel", 104, 838, 620, 92, wipe(0, 22)),
        rect(5, "kante:bg-void band", 96, 938, 420, 54, wipe(10, IN_END)),
    ],
    "markers": [
        {"tm": 0, "cm": "in", "dr": IN_END},
        {"tm": OUT_AT, "cm": "out", "dr": OUT_DR},
    ],
}
# The bar wipes with the panel: same scale animation, own width.
doc["layers"][2]["ks"]["s"] = wipe(0, 6)
json.dump(doc, open(sys.argv[1], "w"), separators=(",", ":"))
