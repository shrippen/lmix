pragma Singleton
import QtQuick

/**
 * Every text the Console shows for a value from Rust or lmix-core: list
 * entries, link health, latency, refusals. Here, so qsTr can translate them
 * (i18n/qml_<lang>.ts); Rust passes values and numbers only.
 */
QtObject {
    /** Text of one value of a form list (Core.choices). */
    /** Aux buses A–G (AUD.2): A takes its sends before the faders. */
    readonly property int auxBuses: 7
    function auxName(index) {
        var letter = String.fromCharCode(65 + index)
        return index === 0 ? qsTr("Aux %1 (before the faders)").arg(letter)
            : qsTr("Aux %1 (after the faders)").arg(letter)
    }
    function auxNames() {
        var names = []
        for (var i = 0; i < auxBuses; i++)
            names.push(auxName(i))
        return names
    }

    function label(list, value) {
        switch (list) {
        case "sources":
            switch (value) {
            case "pattern": return qsTr("Test pattern")
            case "color": return qsTr("Colour")
            case "image": return qsTr("Image")
            case "file": return qsTr("File")
            case "playlist": return qsTr("Playlist")
            case "url": return qsTr("URL")
            case "srt": return qsTr("SRT")
            case "ndi": return qsTr("NDI")
            case "whip": return qsTr("WHIP (browser, OBS)")
            case "web": return qsTr("Web page")
            case "graphics": return qsTr("Graphics channel")
            case "device": return qsTr("Camera / capture")
            case "audio_device": return qsTr("Audio (mic, line)")
            }
            break
        case "patterns":
            switch (value) {
            case "bars": return qsTr("Bars")
            case "ball": return qsTr("Ball")
            case "black": return qsTr("Black")
            case "snow": return qsTr("Snow")
            case "checkers": return qsTr("Checkers")
            }
            break
        case "effects":
            switch (value) {
            case "cut": return qsTr("Cut")
            case "fade": return qsTr("Fade")
            case "dip_to_black": return qsTr("Dip")
            case "slide_left": return qsTr("Slide ◀")
            case "slide_right": return qsTr("Slide ▶")
            case "push_left": return qsTr("Push ◀")
            case "push_right": return qsTr("Push ▶")
            case "wipe_left": return qsTr("Wipe ◀")
            case "wipe_right": return qsTr("Wipe ▶")
            case "stinger": return qsTr("Stinger")
            }
            break
        case "presses":
            switch (value) {
            case "preview_input": return qsTr("Into preview")
            case "cut_direct": return qsTr("Cut to program")
            case "quick_play": return qsTr("Quick play")
            case "none": return qsTr("Nothing")
            }
            break
        case "colors":
            switch (value) {
            case "red": return qsTr("Red")
            case "orange": return qsTr("Orange")
            case "yellow": return qsTr("Yellow")
            case "green": return qsTr("Green")
            case "cyan": return qsTr("Cyan")
            case "blue": return qsTr("Blue")
            case "purple": return qsTr("Purple")
            }
            break
        case "containers":
            switch (value) {
            case "mkv": return "MKV"
            case "mp4": return "MP4"
            case "ts": return "MPEG-TS"
            }
            break
        case "outputs":
            switch (value) {
            case "srt": return qsTr("SRT stream")
            case "rtmp": return qsTr("RTMP stream (YouTube, Twitch …)")
            case "record": return qsTr("Recording")
            case "ndi": return qsTr("NDI")
            case "window": return qsTr("Screen: window")
            case "fullscreen": return qsTr("Screen: fullscreen (Wayland)")
            case "drm": return qsTr("Screen: connector (no desktop)")
            }
            break
        case "durations":
            return qsTr("%1 ms").arg(value)
        }
        return String(value)
    }

    /** Short kind of a source on a tile (`SourceSpec.kind`). */
    function sourceBadge(kind) {
        switch (kind) {
        case "pattern": return qsTr("PAT", "source badge: test pattern")
        case "file": return qsTr("FILE", "source badge")
        case "url": return "URL"
        case "srt": return "SRT"
        case "device": return qsTr("CAM", "source badge: camera")
        case "color": return qsTr("COL", "source badge: colour")
        case "image": return qsTr("IMG", "source badge: image")
        case "playlist": return qsTr("LIST", "source badge: playlist")
        case "ndi": return "NDI"
        case "whip": return "WHIP"
        case "web": return "WEB"
        case "graphics": return qsTr("GFX", "source badge: graphics channel")
        case "audio_device": return qsTr("MIC", "source badge: microphone, line in")
        }
        return ""
    }

    /** `[{value, label}]` of a form list, from the values Core.choices gives. */
    function choices(list, valuesJson) {
        return JSON.parse(valuesJson).map(v => ({ value: v, label: label(list, v) }))
    }

    /** Toolbar text of a latency measurement (Core.latency). */
    function latency(json) {
        var l = json.length > 0 ? JSON.parse(json) : null
        if (!l || l.median_ms === null || l.median_ms === undefined)
            return qsTr("PGM latency –")
        return qsTr("PGM %1 ms (p95 %2 ms)").arg(l.median_ms).arg(l.p95_ms)
    }

    /** Core.machine as an object, or null while lmix-core sent none. */
    function machine(json) {
        try {
            return json.length > 0 ? JSON.parse(json) : null
        } catch (e) {
            return null
        }
    }

    /** The machine's load in a few words (header). */
    function machineLoad(m) {
        return m ? qsTr("CPU %1 %").arg(m.cpu_percent) : ""
    }

    /** How the cue timer runs (MachineLoad.cue_timer). */
    function cueTimer(m) {
        switch (m ? m.cue_timer : "") {
        case "realtime": return qsTr("Cue timer at real-time priority")
        case "normal": return qsTr("Cue timer at normal priority")
        case "demoted": return qsTr("Cue timer lost its real-time priority")
        case "stopped": return qsTr("Cue timer stopped")
        }
        return qsTr("Cue timer –")
    }

    /**
     * What may keep cue times from holding (RUN.8): `{ level, text, detail }`,
     * level 0 nothing, 1 a warning, 2 an error; `text` fits a status pill.
     * The first that applies wins: a stopped timer, late cues, a loaded
     * machine, no real time.
     */
    function machineWarning(m) {
        if (!m) {
            return { level: 0, text: "", detail: "" }
        }
        var load = qsTr("CPU %1 %, load %2 per core (%3 cores)").arg(m.cpu_percent)
            .arg(Number(m.load_per_core).toLocaleString(Qt.locale(), "f", 1)).arg(m.cores)
        if (m.cue_timer === "stopped") {
            return { level: 2, text: qsTr("Cue timer stopped"),
                detail: qsTr("Waits and follows of the cue lists no longer run. Restart lmix-core.") }
        }
        if (m.cues_late) {
            return { level: 1, text: qsTr("Cues late"),
                detail: qsTr("A cue step ran late in the last 30 s. %1.").arg(load) }
        }
        if (m.high) {
            return { level: 1, text: qsTr("Machine loaded"),
                detail: qsTr("Cues may run late. %1.").arg(load) }
        }
        if (m.cue_timer === "demoted") {
            return { level: 1, text: qsTr("No real time"),
                detail: qsTr("A cue step computed too long without a pause; the cue timer runs at normal priority until lmix-core restarts, and cues may run late on a busy machine.") }
        }
        if (m.cue_timer === "normal") {
            return { level: 1, text: qsTr("No real time"), detail: timerHelp() }
        }
        return { level: 0, text: "", detail: load }
    }

    /** How to give the cue timer real-time priority. */
    function timerHelp() {
        return qsTr("The cue timer got no real-time priority, so cues may run late on a busy machine. Allow it with rtkit (running for the desktop session), or with a real-time limit for this user (group realtime, RLIMIT_RTPRIO).")
    }

    /**
     * Short link state on an output button: rate, or that nobody receives.
     * "Mbit": Kante writes button text in capitals, and "MB/S" would read as bytes.
     */
    function linkRate(stats) {
        if (!stats.connected)
            return qsTr("no receiver")
        return qsTr("%1 Mbit/s").arg(Number(stats.kbps / 1000).toLocaleString(Qt.locale(), "f", 1))
    }

    /** Link details for the tooltip. */
    function linkDetail(stats) {
        var rtt = stats.rtt_ms === null || stats.rtt_ms === undefined ? qsTr("RTT –") : qsTr("RTT %1 ms").arg(stats.rtt_ms)
        return qsTr("%1 · lost %2 · dropped %3 · %4").arg(linkRate(stats)).arg(stats.packets_lost)
            .arg(stats.packets_dropped).arg(rtt)
    }

    /** A refusal or error (Core.message) as a sentence; empty when there is none. */
    function message(json) {
        if (json.length === 0)
            return ""
        var m
        try {
            m = JSON.parse(json)
        } catch (e) {
            return json
        }
        var e = m.error || {}
        switch (e.error) {
        case "unknown_output": return qsTr("No output matches %1").arg(e.reference)
        case "unknown_input": return qsTr("No input matches %1").arg(e.reference)
        case "slots_full": return qsTr("All %1 mixer slots are in use").arg(e.slots)
        case "bad_transition_slot": return qsTr("Transition slot %1 does not exist (1–%2)").arg(e.slot).arg(e.max)
        case "no_preview": return qsTr("Nothing in preview")
        // Clicking what is on program into preview does nothing; no
        // message for it (owner, 23.10.2026).
        case "on_program": return ""
        case "bad_position": return qsTr("Position %1 is outside 1–%2").arg(e.number).arg(e.max)
        case "no_video": return qsTr("%1 has no picture (audio input)").arg(e.reference)
        case "no_transport": return qsTr("%1 has no transport (not a file, URL or playlist)").arg(e.reference)
        case "bad_color": return qsTr("Colour %1 is not #rrggbb").arg(e.color)
        case "bad_volume": return qsTr("Volume %1 is outside 0–%2").arg(e.volume).arg(e.max)
        case "no_show_path": return qsTr("The show has no file yet; give a path")
        case "show_file": return qsTr("Show file: %1").arg(e.message)
        case "engine": return qsTr("Engine: %1").arg(e.message)
        case "bad_ptz": return qsTr("PTZ: %1").arg(e.reason)
        case "bad_dynamic": return qsTr("Dynamic slot %1 does not exist (1–%2)").arg(e.slot).arg(e.max)
        case "unknown_graphics": return qsTr("No graphics channel matches %1").arg(e.reference)
        case "graphics_full": return qsTr("All %1 graphics channels are in use").arg(e.max)
        case "unknown_template": return qsTr("No template %1").arg(e.template)
        case "bad_title": return qsTr("Title: %1").arg(e.reason)
        case "unknown_dataset": return qsTr("No data set %1").arg(e.name)
        case "unknown_data_source": return qsTr("No data source matches %1").arg(e.reference)
        case "bad_data_source": return qsTr("Data source: %1").arg(e.reason)
        }
        return m.message || json
    }
}
