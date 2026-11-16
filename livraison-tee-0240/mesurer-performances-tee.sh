#!/bin/sh
set -eu
cd "$(dirname "$0")"
: "${NOVAOS_TMPDIR:=$(cd ../.. && pwd)/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/tee-arc027/" PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR"
exec python3 outils/mesures.py "$@"
