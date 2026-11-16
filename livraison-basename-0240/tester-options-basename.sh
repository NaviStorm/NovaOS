#!/bin/sh
set -eu
here=$(CDPATH='' cd "$(dirname "$0")" && pwd -P)
root=$(dirname "$here")
. "$here/lab/environnement.sh"
exec python3 -B "$here/lab/options.py" "$@"
