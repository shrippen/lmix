#!/usr/bin/env bash
# strom Recorder against Live Recorder (upstream #1034) for MultiCorder:
# is the file readable while it records, and after strom is killed?
#
#   scripts/bench-recorders/run.sh path/to/strom
set -u
STROM_BIN="$1"
WORK="$(mktemp -d)"
PORT=18093
API="http://127.0.0.1:$PORT/api"
"$STROM_BIN" --host 127.0.0.1 --port $PORT --no-auto-restart --data-dir "$WORK/data" >"$WORK/strom.log" 2>&1 &
PID=$!
for _ in $(seq 1 60); do curl -sf "$API/../health" >/dev/null 2>&1 && break; sleep 0.5; done

flow() { # name block_definition
  cat <<EOF
{"id":"$(cat /proc/sys/kernel/random/uuid)","name":"$1","elements":[
 {"id":"src","element_type":"videotestsrc","properties":{"is-live":true,"pattern":"ball"},"position":[0,0]},
 {"id":"caps","element_type":"capsfilter","properties":{"caps":"video/x-raw,width=640,height=360,framerate=25/1"},"position":[0,0]},
 {"id":"enc","element_type":"x264enc","properties":{"tune":"zerolatency","key-int-max":25},"position":[0,0]},
 {"id":"parse","element_type":"h264parse","properties":{},"position":[0,0]}
],
"blocks":[{"id":"rec","block_definition_id":"$2","properties":{"output_dir":"$1"},"position":{"x":0,"y":0}}],
"links":[{"from":"src:src","to":"caps:sink"},{"from":"caps:src","to":"enc:sink"},{"from":"enc:src","to":"parse:sink"},{"from":"parse:src","to":"rec:video_in_0"}]}
EOF
}

start() { # name def -> flow id
  local id
  id="$(flow "$1" "$2" | curl -sf -X POST -H 'content-type: application/json' "$API/flows" -d @- | jq -r '.flow.id // .id')"
  curl -sf -X POST "$API/flows/$id/start" >/dev/null || echo "start failed $1" >&2
  echo "$id"
}

probe() { # file -> duration or error
  ffprobe -v error -show_entries format=duration -of csv=p=0 "$1" 2>&1 | head -1
}

A="$(start rec builtin.recorder)"
B="$(start live builtin.liverecorder)"
sleep 8
for d in rec live; do
  f="$(find "$WORK/data" -path "*$d/*" -type f | head -1)"
  echo "while recording, $d: file=${f##*/} size=$(stat -c %s "$f" 2>/dev/null) probe=$(probe "$f")"
done
kill -9 "$PID"; sleep 1
for d in rec live; do
  f="$(find "$WORK/data" -path "*$d/*" -type f | head -1)"
  echo "after kill -9, $d: size=$(stat -c %s "$f" 2>/dev/null) probe=$(probe "$f")"
done
grep -E "ERROR" "$WORK/strom.log" | tail -5
echo "work: $WORK"
