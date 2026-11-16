#!/bin/sh
set -eu
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
case "${1:-}" in
--macOS) target=GNU ;;
--novaOS) target=NovaOS ;;
-h|--help) echo 'mesurer-performances-uniq.sh --macOS|--novaOS [--large] [--output JSON]'; exit 0 ;;
*) exit 2 ;;
esac
shift
exec python3 -B "$(dirname "$0")/lab/bench.py" mesures "$target" "$@"
