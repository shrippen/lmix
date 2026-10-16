#!/usr/bin/env python3
"""lmix demo: the Sendehaus stream of "Graufeld nach Mitternacht" from the
Studio Weber world (demo/world.json, graufeld.stream; copied in by
shrippen.github.io/demo/tools/sync-demo.py, never edit it here).

Renders a synthetic clip per input (videotestsrc pattern, the input's name,
a tone), starts strom, lmix-core and the Qt Console on this machine, builds
the show through the API and puts it on air. Nothing leaves the machine.

    demo/start.sh [de|en]          interactive; closing the Console ends it
    SHOT_DIR=… SHOT_PLAN=… demo/start.sh de
                                   screenshots on a virtual display
                                   (shrippen.github.io/demo/tools/screenshots.py)

Internal tool for screenshots: never part of a release.
"""
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORLD = json.loads((ROOT / "demo" / "world.json").read_text(encoding="utf-8"))
STREAM = WORLD["graufeld"]["stream"]

STROM_PORT = 8510
CORE_PORT = 8610
SRT_PORT = 9941
CORE = f"http://127.0.0.1:{CORE_PORT}/api"
CLIP_SECONDS = 10
CLIP_RATE = 25
# About -26 dBFS per input, so the program sum stays out of the red.
TONE_VOLUME = 0.05
SHOT_DISPLAY = ":96"
SHOT_SIZE = "1440x900x24"
# Until every live picture shows and the meters move.
SHOT_SETTLE_S = 15
LOCALES = {"de": "de_DE.UTF-8", "en": "en_US.UTF-8"}
# Kante follows the platform's brightness: a dark scheme gives Gruvbox,
# none (a bare virtual display) gives Leinen.
DARK_SCHEME = Path("/usr/share/color-schemes/BreezeDark.colors")


def text(value, lang):
    """A world text: {de, en} or a plain name."""
    return value[lang] if isinstance(value, dict) else value


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def build():
    """Engine and our two binaries; returns their paths."""
    strom = run([str(ROOT / "scripts" / "fetch-strom.sh")], capture_output=True, text=True).stdout.strip()
    target = os.environ.get("CARGO_TARGET_DIR")
    bins = {}
    for crate, binary in (("lmix-core", "lmix-core"), ("console-qt", "lmix-console")):
        run(["cargo", "build", "--release", "-q"], cwd=ROOT / "workspace" / crate)
        base = Path(target) if target else ROOT / "workspace" / crate / "target"
        bins[crate] = str(base / "release" / binary)
    return strom, bins["lmix-core"], bins["console-qt"]


def render(item, lang, folder):
    """A looping clip of one input: pattern with its name and a tone (none: audio only)."""
    frames = CLIP_SECONDS * CLIP_RATE
    # 40 ms of sound per buffer.
    audio = (f"audiotestsrc num-buffers={frames} samplesperbuffer=1920 "
             + (f"freq={item['tone']} volume={TONE_VOLUME}" if item["tone"] else "wave=silence"))
    if not item["pattern"]:
        path = folder / f"{item['id']}.ogg"
        run(["gst-launch-1.0", "-q", *audio.split(), "!", "opusenc", "!", "oggmux", "!",
             "filesink", f"location={path}"])
        return path
    path = folder / f"{item['id']}.webm"
    run(["gst-launch-1.0", "-q",
         "videotestsrc", f"pattern={item['pattern']}", f"num-buffers={frames}", "!",
         f"video/x-raw,width=1280,height=720,framerate={CLIP_RATE}/1", "!",
         "textoverlay", f"text={text(item['name'], lang)}", "valignment=bottom", "halignment=left",
         "font-desc=Bold 36", "!", "videoconvert", "!", "vp8enc", "deadline=1", "!", "webmmux", "name=mux", "!",
         "filesink", f"location={path}",
         *audio.split(), "!", "opusenc", "!", "mux."])
    return path


def call(path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(f"{CORE}{path}", data=data, headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read() or b"null")


def act(action):
    return call("/actions", action)


def wait_up(url, what):
    for _ in range(240):
        try:
            urllib.request.urlopen(url, timeout=2)
            return
        except (urllib.error.URLError, ConnectionError):
            time.sleep(0.5)
    sys.exit(f"{what} did not come up")


def mixer():
    """CUDA where NVIDIA is, GL elsewhere; the CPU on a virtual display."""
    if os.environ.get("MIXER"):
        return os.environ["MIXER"]
    if os.environ.get("SHOT_DIR"):
        return "cpu"
    return "cuda" if shutil.which("nvidia-smi") else "gl"


def titles(lang):
    """A graphics channel with the night programme's titles: the show as a
    lower third, the guest with their role, a clock (all from the world)."""
    show = WORLD["graufeld"]["radio_show"]
    guest = WORLD["graufeld"]["contact"]
    act({"action": "add_graphics", "name": show["name"]})
    act({"action": "title_load", "graphics": 1, "layer": 1, "template": "lmix/lower-third",
         "data": {"headline": show["name"], "text": f"{show['slot']} · {text(show['about'], lang)}"}})
    act({"action": "title_load", "graphics": 1, "layer": 2, "template": "lmix/name-role",
         "data": {"name": guest["name"], "role": text(guest["role"], lang)}, "play": True})
    act({"action": "title_load", "graphics": 1, "layer": 8, "template": "lmix/clock", "play": True})


def stage(lang, work):
    """The show from the world: inputs, colours, buses, outputs."""
    ids = {}
    for item in STREAM["inputs"]:
        clip = render(item, lang, work)
        state = act({"action": "add_input", "source": {"kind": "file", "path": str(clip)},
                     "name": text(item["name"], lang)})
        ids[item["id"]] = state["inputs"][-1]["id"]
        act({"action": "set_input_color", "input": ids[item["id"]], "color": item["color"]})
    time.sleep(3)
    act({"action": "cut_direct", "input": ids[STREAM["on"]["program"]]})
    act({"action": "preview_input", "input": ids[STREAM["on"]["preview"]]})

    (work / "rec").mkdir()
    for n, out in enumerate(STREAM["outputs"]):
        if out["kind"] == "srt":
            spec = {"kind": "srt", "uri": f"srt://:{SRT_PORT + n}?mode=listener"}
        else:
            spec = {"kind": "record", "folder": str(work / "rec"), "container": "mkv"}
        action = {"action": "add_output", "spec": spec, "name": text(out["name"], lang)}
        if out.get("input"):
            action["source"] = {"from": "input", "input": ids[out["input"]]}
        act(action)
        if out["running"]:
            act({"action": "start_output", "output": text(out["name"], lang)})
    titles(lang)
    act({"action": "save_show", "path": str(work / STREAM["file"])})


def main():
    lang = (sys.argv[1] if len(sys.argv) > 1 else os.environ.get("DEMO_LANG", "de"))
    if lang not in LOCALES:
        sys.exit("usage: demo/start.sh [de|en]")
    shots = os.environ.get("SHOT_DIR")
    strom, core, console = build()

    work = Path(tempfile.mkdtemp(prefix="lmix-demo-"))
    live = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp")) / "lmix-demo"
    env = dict(os.environ)
    # Own settings, so the demo neither reads nor changes the user's; the
    # colour scheme by DEMO_THEME (dark, light) or the user's.
    config = work / "config"
    config.mkdir()
    env["XDG_CONFIG_HOME"] = str(config)
    theme = os.environ.get("DEMO_THEME") or ("dark" if shots else "")
    user_scheme = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "kdeglobals"
    if theme == "dark" and DARK_SCHEME.exists():
        shutil.copy(DARK_SCHEME, config / "kdeglobals")
        env["QT_QPA_PLATFORMTHEME"] = "kde"
    elif not theme and user_scheme.exists():
        shutil.copy(user_scheme, config / "kdeglobals")
    procs = []
    if shots:
        procs.append(subprocess.Popen(["Xvfb", SHOT_DISPLAY, "-screen", "0", SHOT_SIZE],
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        env.pop("WAYLAND_DISPLAY", None)
        env.update(DISPLAY=SHOT_DISPLAY, QT_QPA_PLATFORM="xcb", GST_GL_WINDOW="x11")

    def stop(*_):
        for proc in reversed(procs):
            proc.terminate()
        for proc in procs:
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
        if os.environ.get("KEEP_WORK") == "1":
            print(f"work: {work}", flush=True)
        else:
            shutil.rmtree(work, ignore_errors=True)
        shutil.rmtree(live, ignore_errors=True)

    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    try:
        log = open(work / "engine.log", "w")
        procs.append(subprocess.Popen([strom, "--host", "127.0.0.1", "--port", str(STROM_PORT),
                                       "--no-auto-restart", "--data-dir", str(work / "strom-data")],
                                      cwd=work, env=env, stdout=log, stderr=log))
        wait_up(f"http://127.0.0.1:{STROM_PORT}/health", "strom")
        procs.append(subprocess.Popen([core, "--strom", f"http://127.0.0.1:{STROM_PORT}",
                                       "--listen", f"127.0.0.1:{CORE_PORT}", "--mixer", mixer(),
                                       "--slots", str(len(STREAM["inputs"])),
                                       "--autosave", str(work / "autosave.lmix"),
                                       # Not the operator's shortcuts for all shows
                                       # and layout templates.
                                       "--shortcuts", str(work / "shortcuts.json"),
                                       "--layout-templates", str(work / "layout-templates.json"),
                                       "--live-dir", str(live)],
                                      env=env, stdout=log, stderr=log))
        wait_up(f"{CORE}/state", "lmix-core")
        stage(lang, work)

        env.update(LMIX_CORE=f"http://127.0.0.1:{CORE_PORT}", LANGUAGE=lang, LANG=LOCALES[lang],
                   LC_ALL=LOCALES[lang])
        console_proc = subprocess.Popen([console, "--style", "kante"], env=env, stdout=log, stderr=log)
        procs.append(console_proc)
        if not shots:
            console_proc.wait()
            return
        time.sleep(float(os.environ.get("DEMO_SETTLE_S", SHOT_SETTLE_S)))
        plan = json.loads(os.environ.get("SHOT_PLAN", "[]")) or [{"name": "console"}]
        for shot in plan:
            run(["import", "-display", SHOT_DISPLAY, "-window", "root", str(Path(shots) / f"{shot['name']}.png")])
            print(f"shot {shot['name']}", flush=True)
    finally:
        stop()


if __name__ == "__main__":
    main()
