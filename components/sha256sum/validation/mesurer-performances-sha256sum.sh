#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "${1:---novaOS}" in
    --help) echo 'mesurer-performances-sha256sum.sh [--novaOS|--macOS] [--large] [--above-ram]'; echo 'Sans option de taille : trois répétitions de 1, 16 et 64 Mio, puis 1 000 petits fichiers.'; exit 0 ;;
    --macOS) bin=/opt/homebrew/bin/gsha256sum; shift ;;
    --novaOS) bin="$here/sha256sum"; [ -x "$bin" ] || bin="$here/../target/release/sha256sum"; if [ "$#" -gt 0 ]; then shift; fi ;;
    *) exit 2 ;;
esac
: "${TMPDIR:=${NOVAOS_TMPDIR:-$(getconf DARWIN_USER_TEMP_DIR)}}"
export TMPDIR
exec python3 -B "$here/performances.py" "$bin" "$@"
