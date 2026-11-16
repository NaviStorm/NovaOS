#!/bin/sh
set -eu
exec python3 -B "$(dirname "$0")/lab/bench.py" "$@"
