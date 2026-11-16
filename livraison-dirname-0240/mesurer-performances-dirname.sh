#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$ici")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/dirname-arc027/" PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR"
exec python3 "$ici/lab/performances.py" "${1:-court}" "${2:-$TMPDIR/perf-${1:-court}}"
