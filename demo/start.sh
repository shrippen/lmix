#!/usr/bin/env bash
# lmix demo with the Studio Weber data (internal, for screenshots; never in a
# release). See demo/run.py.
#   demo/start.sh [de|en]
exec python3 "$(dirname "$0")/run.py" "$@"
