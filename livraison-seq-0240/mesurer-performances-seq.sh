#!/bin/sh
set -eu
cd "$(dirname "$0")"
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd ../.. && pwd)/tmp/seq-arc027}"
export TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$TMPDIR"
exec python3 -B lab/performance.py "$@"
