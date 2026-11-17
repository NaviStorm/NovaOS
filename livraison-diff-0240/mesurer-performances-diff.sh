#!/bin/sh
set -eu
ici=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
: "${NOVAOS_TMPDIR:=$(dirname "$(dirname "$ici")")/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
exec python3 -B "$ici/performances.py" "$@"
