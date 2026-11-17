#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
root=$(dirname "$here")
. "$here/lab/environnement.sh"
if [ "${1:-}" = "seuil" ]; then
    shift
    exec python3 -B "$here/lab/seuil.py" "$@"
fi
exec python3 -B "$here/lab/mesures.py" "$@"
