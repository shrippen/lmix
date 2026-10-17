#!/usr/bin/env bash
# Script import (show mode, IMP): lmix-script against lmix-core with the
# null engine. Problems with file and line, load, GO, reload with
# standby kept on its cue, watching the folder, a list removed with its
# file, a list of the operator not overwritten. No strom needed.
#
#   scripts/test-script.sh
#   KEEP_WORK=1 scripts/test-script.sh   # keep logs
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT=8897
CORE="http://127.0.0.1:${PORT}"
WORK="$(mktemp -d)"
DIR="$WORK/Kampagne"
PIDS=()
cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT
fail() {
    echo "FAIL: $*"
    KEEP_WORK=1
    exit 1
}
pass() { echo "ok   $*"; }
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/api/actions" >/dev/null; }
state() { curl -sf "$CORE/api/state" | jq -r "$1"; }
up() { for _ in $(seq 1 60); do curl -sf "$CORE/health" >/dev/null && return 0; sleep 0.5; done; return 1; }
wait_for() {
    local tries=$1
    shift
    for _ in $(seq 1 "$tries"); do "$@" && return 0; sleep 0.25; done
    return 1
}

(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
(cd "$ROOT/workspace/lmix-script" && cargo build --release >/dev/null 2>&1)
TARGET="${CARGO_TARGET_DIR:-}"
CORE_BIN="${TARGET:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
SCRIPT_BIN="${TARGET:-$ROOT/workspace/lmix-script/target}/release/lmix-script"
export LMIX_CORE="$CORE"

"$CORE_BIN" --engine none --listen "127.0.0.1:${PORT}" --autosave "$WORK/autosave.lmix" \
    --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" >>"$WORK/core.log" 2>&1 &
PIDS+=($!)
up || fail "lmix-core did not come up"
act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Bars"}'
act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}'
id_of() { state ".inputs[] | select(.name==\"$1\") | .id"; }
bars="$(id_of Bars)"
ball="$(id_of Ball)"

mkdir -p "$DIR/.obsidian" "$DIR/Sessions"
cat >"$DIR/Sessions/Session 3.md" <<'EOF'
---
lmix-list: Session 3
---
## Der Wald

```lmix-cue
number = "1"
name = "Wald"
actions = [{ action = "cut_direct", input = "Bars" }]
follow = "after_wait"
post_wait_ms = 300
```

```lmix-cue
number = "2"
name = "Ball"
actions = [{ action = "cut_direct", input = "Ball" }]
```

## Die Höhle

```lmix-cue
number = "3"
name = "Höhle"
actions = [{ action = "cut_direct", input = "Bars" }]
```
EOF
cat >"$DIR/Twice.md" <<'EOF'
# Doppelt

```lmix-cue
number = "2"
name = "Twice"
actions = [{ action = "cut" }]
```
EOF
cat >"$DIR/Typo.md" <<'EOF'
```lmix-cue
kind = "wait"
duraton_ms = 100
```
EOF
echo "# Nur Notizen" >"$DIR/Notizen.md"
program_is() { [[ "$(state .program)" == "$1" ]]; }
last_cue_is() { [[ "$(state '.cue_lists[0].cues[-1].name')" == "$1" ]]; }

# --- check: every problem with file and line -------------------------------
out="$("$SCRIPT_BIN" check "$DIR" 2>&1)" && fail "check passed a broken script"
grep -q '^Sessions/Session 3.md:14: error: cue number 2 is also at Twice.md:3' <<<"$out" \
    || fail "duplicate number not reported at its line: $out"
grep -q '^Twice.md:3: error: cue number 2 is also at Sessions/Session 3.md:14' <<<"$out" \
    || fail "duplicate number not reported at both places: $out"
grep -q '^Typo.md:2: error: unknown field `duraton_ms`' <<<"$out" || fail "typo not reported at its line: $out"
pass "check: duplicate number at both places and unknown field, with file and line"

# --- load: the good file is loaded, the broken one refused -------------------
rm "$DIR/Twice.md"
"$SCRIPT_BIN" load "$DIR" >"$WORK/load.log" 2>&1 && fail "load reported success with a broken file"
[[ "$(state '[.cue_lists[].name] | join(",")')" == "Session 3" ]] || fail "lists after load: $(state '[.cue_lists[].name]')"
[[ "$(state '.cue_lists[0].source')" == "script:Kampagne/Sessions/Session 3.md" ]] || fail "list has no script source"
[[ "$(state '[.cue_lists[0].cues[].name] | join("|")')" == "Der Wald|Wald|Ball|Die Höhle|Höhle" ]] \
    || fail "cues: $(state '[.cue_lists[0].cues[].name]')"
pass "load: script list with source, scene marks as note cues, broken file refused"

# --- GO on the scene mark starts the scene ----------------------------------
act '{"action":"cue_go"}'
[[ "$(state .program)" == "$bars" ]] || fail "GO on the scene mark did not start Wald"
wait_for 20 program_is "$ball" || fail "Ball did not follow Wald"
[[ "$(state '.cue_lists[0].standby')" == 4 ]] || fail "standby not on the next scene: $(state '.cue_lists[0].standby')"
pass "GO: scene mark, Wald, Ball after 300 ms; standby on the next scene"

# --- reload: a cue before standby, standby stays on its cue -----------------
rm "$DIR/Typo.md"
sed -i 's/^## Die Höhle$/```lmix-cue\nkind = "note"\nname = "Pause"\n```\n\n## Die Höhle/' "$DIR/Sessions/Session 3.md"
"$SCRIPT_BIN" load "$DIR" >"$WORK/load2.log" 2>&1 || fail "reload failed: $(cat "$WORK/load2.log")"
grep -q 'reloaded "Session 3"' "$WORK/load2.log" || fail "not reloaded: $(cat "$WORK/load2.log")"
[[ "$(state '.cue_lists[0].cues | length')" == 6 ]] || fail "new cue not in the list"
[[ "$(state '.cue_lists[0].cues[.cue_lists[0].standby - 1].name')" == "Die Höhle" ]] \
    || fail "standby moved off its cue: $(state '.cue_lists[0].standby')"
pass "reload: standby stays on its cue after a cue came before it"

# --- watch: a change is loaded by itself -----------------------------------
"$SCRIPT_BIN" load "$DIR" --watch >"$WORK/watch.log" 2>&1 &
PIDS+=($!)
wait_for 40 grep -q watching "$WORK/watch.log" || fail "watch did not start"
sed -i 's/^name = "Höhle"$/name = "Tiefe Höhle"/' "$DIR/Sessions/Session 3.md"
wait_for 40 last_cue_is "Tiefe Höhle" || fail "change not loaded: $(cat "$WORK/watch.log")"
pass "watch: a saved change is loaded"

# --- a list of the operator with the same name is not overwritten ----------
kill "${PIDS[-1]}"
act '{"action":"add_cue_list","name":"Live","cues":[{"name":"Mine","actions":[{"action":"cut"}]}]}'
printf -- '---\nlmix-list: Live\n---\n```lmix-cue\nname = "Theirs"\nactions = [{ action = "cut" }]\n```\n' >"$DIR/Live.md"
"$SCRIPT_BIN" load "$DIR" >"$WORK/load3.log" 2>&1 && fail "load overwrote a list of the operator"
grep -q 'exists that is not from this script' "$WORK/load3.log" || fail "no reason given: $(cat "$WORK/load3.log")"
[[ "$(state '.cue_lists[] | select(.name=="Live") | .cues[0].name')" == "Mine" ]] || fail "operator's list changed"
rm "$DIR/Live.md"
pass "an operator's list of the same name stays"

# --- a file gone: its list goes ---------------------------------------------
rm "$DIR/Sessions/Session 3.md"
"$SCRIPT_BIN" load "$DIR" >"$WORK/load4.log" 2>&1 || fail "load of an empty script failed"
[[ "$(state '[.cue_lists[].name] | join(",")')" == "Live" ]] || fail "list of the removed file still there"
pass "a removed file removes its list, the operator's stays"

# --- schema -----------------------------------------------------------------
"$SCRIPT_BIN" schema | jq -e '.properties.kind and .properties.follow and .properties.children' >/dev/null \
    || fail "schema lacks the cue fields"
pass "schema: the cue block's fields (IMP.8)"
