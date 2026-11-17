#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
case "${1:---help}" in
--help|-h) echo "Usage : $0 --rapide|--complet [rapport.json]"; exit 0;;
--rapide|--complet) ;;
*) exit 2;;
esac
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$ici")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/mktemp-arc027/"
mkdir -p "$TMPDIR"
python3 -B "$ici/lab/performance.py" "${1#--}" "${2:-$TMPDIR/mktemp-performances.json}"
