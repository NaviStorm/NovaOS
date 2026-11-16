#!/bin/sh
set -eu
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp}" TMPDIR="${NOVAOS_TMPDIR:-/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp}/"
case "${1:-}" in
--macOS) target=GNU ;;
--novaOS) target=NovaOS ;;
-h|--help) echo 'tester-options-uniq.sh --macOS|--novaOS [--large] [--output FICHIER]'; exit 0 ;;
*) exit 2 ;;
esac
shift
exec python3 -B "$(dirname "$0")/lab/options.py" "$target" "$@"
