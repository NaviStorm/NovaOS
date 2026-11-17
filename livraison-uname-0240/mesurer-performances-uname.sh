#!/bin/sh
set -eu
ici=$(CDPATH= cd "$(dirname "$0")" && pwd)
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(dirname "$(dirname "$ici")")/tmp}"
export TMPDIR="$NOVAOS_TMPDIR/uname-arc027/" PYTHONDONTWRITEBYTECODE=1
mkdir -p "$TMPDIR"
case ${1:---rapide} in
    --help|-h) echo "Usage : $0 [--rapide|--complet] [dossier] | --qemu [dossier]";;
    --rapide|--complet) mode=${1:---rapide}; exec python3 "$ici/lab/performances.py" "${mode#--}" "${2:-$TMPDIR/mesures}";;
    --qemu) exec python3 "$ici/lab/exclusif.py" sh "$ici/lab/qemu.sh" "${2:-$TMPDIR/recette}" sdk;;
    *) exit 2;;
esac
