#!/bin/sh
set -eu
here=$(CDPATH= cd "$(dirname "$0")" && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd "$here/../.." && pwd)/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/"
case "${1:-}" in
--qemu) shift; exec python3 -B "$here/lab/invite.py" "$@" ;;
--amont) shift; exec sh "$here/lab/amont.sh" "$@" ;;
--help|-h) echo "Usage : $0 --rapide|--complet [--out FICHIER] | --qemu ARCH sdk|nsh|lot [--out DOSSIER] | --amont GNU_SOURCE"; exit ;;
esac
exec python3 -B "$here/lab/options.py" "$@"
