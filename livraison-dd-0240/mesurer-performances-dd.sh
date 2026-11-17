#!/bin/sh
set -eu
cd "$(dirname "$0")"
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd ../.. && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/dd-arc027/" PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR"
case "${1:-}" in
 --help|-h) echo "mesurer-performances-dd.sh --rapide|--complet [rapport.json] | --invite aarch64|x86_64 512|4096 dossier-neuf";;
 --rapide|--complet) exec python3 performances.py "$1" "${2:-${TMPDIR}performances-$1.json}";;
 --invite) shift; exec python3 invite.py "$@";;
 *) exit 2;;
esac
