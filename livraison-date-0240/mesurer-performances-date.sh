#!/bin/sh
set -eu
here=$(CDPATH= cd "$(dirname "$0")" && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd "$here/../.." && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
case "${1:-}" in
--qemu) shift; exec python3 -B "$here/lab/invite.py" "${1:?architecture requise}" lot --out "${2:-$NOVAOS_TMPDIR/date-arc027/rejeu}" ;;
--help|-h) echo "Usage : $0 --rapide|--complet [--out FICHIER] | --qemu aarch64|x86_64 [DOSSIER]"; exit ;;
esac
exec python3 -B "$here/lab/performance.py" "$@"
