#!/bin/sh
set -eu
base=$(CDPATH= cd "$(dirname "$0")" && pwd)
case "${1:---help}" in
--rapide|--complet) mode=${1#--};;
--help|-h) echo 'mesurer-performances-readlink.sh --rapide|--complet [dossier]'; exit 0;;
*) exit 2;;
esac
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$base")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/readlink-arc027/"
mkdir -p "$TMPDIR"
exec python3 -B "$base/lab/performances.py" "$mode" "${2:-$TMPDIR/performances-$mode}"
