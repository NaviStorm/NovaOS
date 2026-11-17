#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
exec python3 -B "$ici/performances.py" "$@"
