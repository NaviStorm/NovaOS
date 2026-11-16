#!/bin/sh
set -eu
base=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$base")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
exec python3 -B "$base/lab/options.py" "$@"
