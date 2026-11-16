#!/bin/sh
set -eu
: "${NOVAOS_TMPDIR:=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
mkdir -p "$NOVAOS_TMPDIR"
exec python3 -B "$(dirname "$0")"/outils/mesures.py "$@"
