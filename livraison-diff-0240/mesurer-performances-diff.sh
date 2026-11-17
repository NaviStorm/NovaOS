#!/bin/sh
set -eu
ici=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
: "${NOVAOS_TMPDIR:=$(dirname "$(dirname "$ici")")/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
case "${1:-}" in
 --novaos) shift; exec "$ici/rejouer-invites.sh" "$@" ;;
 --help|-h) echo "Cibles NovaOS : $0 --novaos --rapide|--complet [répertoire neuf]. Par défaut : Darwin ARM64." ;;
esac
exec python3 -B "$ici/performances.py" "$@"
