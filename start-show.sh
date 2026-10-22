#!/usr/bin/env bash
# Starts lmix Show on this machine (engine, lmix-core, test inputs), see
# scripts/run-local.sh for the settings (MIXER, INPUTS, KEEP_WORK, ...).
# Arguments go to the program, e.g. ./start-show.sh --window maximized
set -euo pipefail
mkdir -p "$HOME/.cache/lmix-local"
export TMPDIR="${TMPDIR:-$HOME/.cache/lmix-local}"
APP=show exec "$(dirname "$0")/scripts/run-local.sh" "$@"
