#!/bin/sh
set -eu
cd "$(dirname "$0")"
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd ../.. && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/dd-arc027/" PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR"
case "${1:-}" in
 --help|-h) echo "tester-options-dd.sh --rapide|--complet [rapport.json.gz] | --invite aarch64|x86_64 512|4096 dossier-neuf";;
 --rapide|--complet)
    rapport=${2:-${TMPDIR}options-$1.json.gz}
    python3 options.py "$1" --rapport "$rapport"
    if [ "$1" = --complet ]; then python3 flux-darwin.py "${rapport%.json.gz}.flux.json"; fi;;
 --invite) shift; exec python3 invite.py "$@";;
 *) exit 2;;
esac
