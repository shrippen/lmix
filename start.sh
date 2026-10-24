#!/usr/bin/env bash
# Starts lmix on this machine (engine, lmix-core, test inputs); closing the
# window stops everything. The page is the first argument, without one lmix
# opens the page that was open last. scripts/run-local.sh has the settings
# (MIXER, INPUTS, KEEP_WORK, ...); further arguments go to lmix.
#
#   ./start.sh                 # the page open last
#   ./start.sh show            # mix, show, audio, outputs or setup
#   ./start.sh both            # two windows, Mix and Show
#   ./start.sh mix --window maximized
#   MIXER=gl INPUTS=8 ./start.sh
set -euo pipefail
case "${1:-}" in
mix | show | audio | outputs | setup | both) export APP="$1"; shift ;;
-* | "") ;;
*) echo "usage: $0 [mix|show|audio|outputs|setup|both] [lmix arguments]" >&2; exit 1 ;;
esac
mkdir -p "$HOME/.cache/lmix-local"
export TMPDIR="${TMPDIR:-$HOME/.cache/lmix-local}"
exec "$(dirname "$0")/scripts/run-local.sh" "$@"
