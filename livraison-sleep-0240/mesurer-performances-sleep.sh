#!/bin/sh
set -eu
here=$(CDPATH= cd "$(dirname "$0")" && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$here")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR"
exec python3 "$here/lab/rejeu.py" mesures "$@"
