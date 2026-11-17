#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
case "${1:---help}" in
--help|-h) echo "Usage : $0 --rapide|--complet [rapport.json | --novaos]"; exit 0;;
--rapide|--complet) ;;
*) exit 2;;
esac
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$ici")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/mktemp-arc027/"
mkdir -p "$TMPDIR"
if [ "${2:-}" = --novaos ]; then exec "$ici/qualifier-qemu.sh" "$1"; fi
python3 -B "$ici/lab/options.py" "${1#--}" "${2:-$TMPDIR/mktemp-options.json}"
