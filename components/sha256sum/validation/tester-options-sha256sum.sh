#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "${1:---novaOS}" in
    --help) echo 'tester-options-sha256sum.sh [--novaOS|--macOS] : comparer le Mach-O du projet ou GNU à GNU 9.12.'; exit 0 ;;
    --macOS) bin=/opt/homebrew/bin/gsha256sum ;;
    --novaOS) bin="$here/sha256sum"; [ -x "$bin" ] || bin="$here/../target/release/sha256sum" ;;
    *) exit 2 ;;
esac
: "${TMPDIR:=$here/../../../../tmp/}"
export TMPDIR
exec python3 -B "$here/options.py" "$bin"
