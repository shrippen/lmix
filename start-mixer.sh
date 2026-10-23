#!/usr/bin/env bash
# Starts lmix on this machine, on the Mix page (engine, lmix-core, test inputs), see
# scripts/run-local.sh for the settings (MIXER, INPUTS, KEEP_WORK, ...).
# Arguments go to the program, e.g. ./start-mixer.sh --window maximized
set -euo pipefail
mkdir -p "$HOME/.cache/lmix-local"
export TMPDIR="${TMPDIR:-$HOME/.cache/lmix-local}"
APP=mix exec "$(dirname "$0")/scripts/run-local.sh" "$@"
