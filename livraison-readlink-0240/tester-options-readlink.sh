#!/bin/sh
set -eu
base=$(CDPATH= cd "$(dirname "$0")" && pwd)
case "${1:---help}" in
--rapide|--complet) mode=${1#--};;
--help|-h) echo 'tester-options-readlink.sh --rapide|--complet [rapport.json]'; exit 0;;
*) exit 2;;
esac
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$base")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/readlink-arc027/"
mkdir -p "$TMPDIR"
exec python3 -B "$base/lab/options.py" "$mode" "${2:-$TMPDIR/options-$mode.json}"
